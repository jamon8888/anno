//! GLiNER poly-encoder ONNX inference engine.
//!
//! Handles model loading from HuggingFace Hub, tokenization, tensor construction,
//! and output decoding. Follows the same patterns as `gliner_onnx::inference`.

use super::*;

/// Special token IDs for GLiNER poly-encoder models.
///
/// These match the standard GLiNER tokenizer vocabulary. If the poly-encoder
/// export uses a different tokenizer, these values may need adjustment.
const TOKEN_START: u32 = 1;
const TOKEN_END: u32 = 2;
const TOKEN_ENT: u32 = 128002;
const TOKEN_SEP: u32 = 128003;

/// Default max span width from GLiNER config.
const MAX_SPAN_WIDTH: usize = 12;

#[cfg(feature = "onnx")]
impl GLiNERPoly {
    /// Create a new poly-encoder GLiNER model from HuggingFace with default settings.
    ///
    /// Downloads the model and tokenizer from the HuggingFace Hub on first use.
    /// Subsequent calls use the cached files.
    ///
    /// # Arguments
    ///
    /// * `model_name` - HuggingFace model ID
    ///   (e.g., `"knowledgator/gliner-bi-large-v1.0"`)
    pub fn new(model_name: &str) -> Result<Self> {
        Self::with_options(model_name, true, 3, 4)
    }

    /// Create a new poly-encoder GLiNER model with explicit options.
    ///
    /// # Arguments
    ///
    /// * `model_name` - HuggingFace model ID
    /// * `prefer_quantized` - Try quantized (INT8) model first for faster CPU inference
    /// * `optimization_level` - ONNX graph optimization (1-3)
    /// * `num_threads` - Intra-op thread count (0 = auto)
    pub fn with_options(
        model_name: &str,
        prefer_quantized: bool,
        optimization_level: u8,
        num_threads: usize,
    ) -> Result<Self> {
        use hf_hub::api::sync::{Api, ApiBuilder};
        use ort::execution_providers::CPUExecutionProvider;
        use ort::session::builder::GraphOptimizationLevel;
        use ort::session::Session;

        // Load .env if present (for HF_TOKEN).
        crate::env::load_dotenv();

        let api = if let Some(token) = crate::env::hf_token() {
            ApiBuilder::new()
                .with_token(Some(token))
                .build()
                .map_err(|e| Error::Retrieval(format!("HuggingFace API with token: {}", e)))?
        } else {
            Api::new().map_err(|e| {
                Error::Retrieval(format!("Failed to initialize HuggingFace API: {}", e))
            })?
        };

        let repo = api.model(model_name.to_string());

        // Download model -- try quantized variants first if preferred.
        let (model_path, is_quantized) = if prefer_quantized {
            if let Ok(path) = repo.get("onnx/model_quantized.onnx") {
                log::info!("[GLiNERPoly] Using quantized model (INT8)");
                (path, true)
            } else if let Ok(path) = repo.get("model_quantized.onnx") {
                log::info!("[GLiNERPoly] Using quantized model (INT8)");
                (path, true)
            } else {
                let path = repo
                    .get("onnx/model.onnx")
                    .or_else(|_| repo.get("model.onnx"))
                    .map_err(|e| {
                        Error::Retrieval(format!("Failed to download model.onnx: {}", e))
                    })?;
                log::info!("[GLiNERPoly] Using FP32 model (quantized not available)");
                (path, false)
            }
        } else {
            let path = repo
                .get("onnx/model.onnx")
                .or_else(|_| repo.get("model.onnx"))
                .map_err(|e| Error::Retrieval(format!("Failed to download model.onnx: {}", e)))?;
            (path, false)
        };

        let tokenizer_path = repo
            .get("tokenizer.json")
            .map_err(|e| Error::Retrieval(format!("Failed to download tokenizer.json: {}", e)))?;

        // Build ONNX session.
        let opt_level = match optimization_level {
            1 => GraphOptimizationLevel::Level1,
            2 => GraphOptimizationLevel::Level2,
            _ => GraphOptimizationLevel::Level3,
        };

        let mut builder = Session::builder()
            .map_err(|e| Error::Retrieval(format!("ONNX session builder: {}", e)))?
            .with_optimization_level(opt_level)
            .map_err(|e| Error::Retrieval(format!("ONNX optimization level: {}", e)))?
            .with_execution_providers([CPUExecutionProvider::default().build()])
            .map_err(|e| Error::Retrieval(format!("ONNX execution providers: {}", e)))?;

        if num_threads > 0 {
            builder = builder
                .with_intra_threads(num_threads)
                .map_err(|e| Error::Retrieval(format!("ONNX thread config: {}", e)))?;
        }

        let session = builder
            .commit_from_file(&model_path)
            .map_err(|e| Error::Retrieval(format!("ONNX model load: {}", e)))?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| Error::Retrieval(format!("Tokenizer load: {}", e)))?;

        log::info!(
            "[GLiNERPoly] Loaded {} (quantized={})",
            model_name,
            is_quantized
        );

        Ok(Self {
            session: Mutex::new(session),
            tokenizer: std::sync::Arc::new(tokenizer),
            model_name: model_name.to_string(),
            is_quantized,
        })
    }

    /// Check if a quantized model was loaded.
    #[must_use]
    pub fn is_quantized(&self) -> bool {
        self.is_quantized
    }

    /// Get the model name.
    #[must_use]
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Extract entities from text using poly-encoder GLiNER zero-shot NER.
    ///
    /// # Arguments
    ///
    /// * `text` - Input text
    /// * `entity_types` - Entity type labels to detect (e.g., `["person", "organization"]`)
    /// * `threshold` - Confidence threshold (0.0-1.0, recommended: 0.5)
    pub fn extract(
        &self,
        text: &str,
        entity_types: &[&str],
        threshold: f32,
    ) -> Result<Vec<Entity>> {
        if text.is_empty() || entity_types.is_empty() {
            return Ok(vec![]);
        }

        let text_words: Vec<&str> = text.split_whitespace().collect();
        let num_text_words = text_words.len();
        if num_text_words == 0 {
            return Ok(vec![]);
        }

        // Encode input following the GLiNER prompt format.
        let (input_ids, attention_mask, words_mask) =
            self.encode_prompt(&text_words, entity_types)?;

        // Build ONNX tensors.
        use ndarray::Array2;

        let seq_len = input_ids.len();

        let input_ids_arr = Array2::from_shape_vec((1, seq_len), input_ids)
            .map_err(|e| Error::Parse(format!("Array: {}", e)))?;
        let attention_mask_arr = Array2::from_shape_vec((1, seq_len), attention_mask)
            .map_err(|e| Error::Parse(format!("Array: {}", e)))?;
        let words_mask_arr = Array2::from_shape_vec((1, seq_len), words_mask)
            .map_err(|e| Error::Parse(format!("Array: {}", e)))?;
        let text_lengths_arr =
            Array2::from_shape_vec((1, 1), vec![num_text_words as i64])
                .map_err(|e| Error::Parse(format!("Array: {}", e)))?;

        let input_ids_t = crate::backends::ort_compat::tensor_from_ndarray(input_ids_arr)
            .map_err(|e| Error::Parse(format!("Tensor: {}", e)))?;
        let attention_mask_t =
            crate::backends::ort_compat::tensor_from_ndarray(attention_mask_arr)
                .map_err(|e| Error::Parse(format!("Tensor: {}", e)))?;
        let words_mask_t = crate::backends::ort_compat::tensor_from_ndarray(words_mask_arr)
            .map_err(|e| Error::Parse(format!("Tensor: {}", e)))?;
        let text_lengths_t =
            crate::backends::ort_compat::tensor_from_ndarray(text_lengths_arr)
                .map_err(|e| Error::Parse(format!("Tensor: {}", e)))?;

        // Run inference.
        let mut session = lock(&self.session);

        // TODO: The exact set of ONNX input names depends on the exported model.
        // The poly-encoder may accept additional inputs (e.g., entity_type_ids,
        // entity_attention_mask) for the cross-attention fusion. Verify against
        // the actual exported model and add/remove inputs as needed.
        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids_t.into_dyn(),
                "attention_mask" => attention_mask_t.into_dyn(),
                "words_mask" => words_mask_t.into_dyn(),
                "text_lengths" => text_lengths_t.into_dyn(),
            ])
            .map_err(|e| Error::Inference(format!("ONNX inference failed: {}", e)))?;

        // Decode output.
        let entities =
            self.decode_output(&outputs, text, &text_words, entity_types, threshold)?;
        drop(outputs);
        drop(session);

        Ok(entities)
    }

    /// Encode prompt following the GLiNER format: word-by-word encoding.
    ///
    /// Structure: `[START] <<ENT>> type1 <<ENT>> type2 <<SEP>> word1 word2 ... [END]`
    fn encode_prompt(
        &self,
        text_words: &[&str],
        entity_types: &[&str],
    ) -> Result<(Vec<i64>, Vec<i64>, Vec<i64>)> {
        let mut input_ids: Vec<i64> = Vec::new();
        let mut word_mask: Vec<i64> = Vec::new();

        // Start token.
        input_ids.push(TOKEN_START as i64);
        word_mask.push(0);

        // Entity types: <<ENT>> type1 <<ENT>> type2 ...
        for entity_type in entity_types {
            input_ids.push(TOKEN_ENT as i64);
            word_mask.push(0);

            let encoding = self
                .tokenizer
                .encode(entity_type.to_string(), false)
                .map_err(|e| Error::Parse(format!("Tokenizer error: {}", e)))?;
            for token_id in encoding.get_ids() {
                input_ids.push(*token_id as i64);
                word_mask.push(0);
            }
        }

        // <<SEP>> token.
        input_ids.push(TOKEN_SEP as i64);
        word_mask.push(0);

        // Text words (word_mask starts counting from 1).
        for (word_idx, word) in text_words.iter().enumerate() {
            let encoding = self
                .tokenizer
                .encode(word.to_string(), false)
                .map_err(|e| Error::Parse(format!("Tokenizer error: {}", e)))?;

            let word_id = (word_idx + 1) as i64;
            for (token_idx, token_id) in encoding.get_ids().iter().enumerate() {
                input_ids.push(*token_id as i64);
                // First subword token gets the word ID, rest get 0.
                word_mask.push(if token_idx == 0 { word_id } else { 0 });
            }
        }

        // End token.
        input_ids.push(TOKEN_END as i64);
        word_mask.push(0);

        let seq_len = input_ids.len();
        let attention_mask: Vec<i64> = vec![1; seq_len];

        Ok((input_ids, attention_mask, word_mask))
    }

    /// Decode model output into entities.
    ///
    /// Supports two output shapes:
    /// - `[batch, num_words, max_width, num_entity_types]` (span-level)
    /// - `[3, batch, num_words, num_entity_types]` (BIO token-level)
    fn decode_output(
        &self,
        outputs: &ort::session::SessionOutputs,
        text: &str,
        text_words: &[&str],
        entity_types: &[&str],
        threshold: f32,
    ) -> Result<Vec<Entity>> {
        let output = outputs
            .iter()
            .next()
            .map(|(_, v)| v)
            .ok_or_else(|| Error::Parse("No output from poly-encoder model".to_string()))?;

        let (_, data_slice) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Parse(format!("Failed to extract output tensor: {}", e)))?;
        let output_data: Vec<f32> = data_slice.to_vec();

        let shape: Vec<i64> = match output.dtype() {
            ort::value::ValueType::Tensor { shape, .. } => shape.iter().copied().collect(),
            _ => return Err(Error::Parse("Output is not a tensor".to_string())),
        };

        log::debug!(
            "[GLiNERPoly] Output shape: {:?}, data len: {}",
            shape,
            output_data.len()
        );

        if output_data.is_empty() || shape.contains(&0) {
            return Err(Error::Inference(
                "GLiNERPoly returned empty/degenerate output tensor. \
                 Check ONNX export compatibility."
                    .to_string(),
            ));
        }

        let num_words = text_words.len();
        let mut entities = Vec::with_capacity(32);

        // Span-level output: [batch, num_words, max_width, num_entity_types]
        if shape.len() == 4 && shape[0] == 1 && shape[1] > 0 && shape[2] > 1 {
            let out_num_words = shape[1] as usize;
            let out_max_width = shape[2] as usize;
            let num_classes = shape[3] as usize;

            if num_classes == 0 {
                return Err(Error::Inference(
                    "GLiNERPoly model produced num_classes=0.".to_string(),
                ));
            }

            for word_idx in 0..out_num_words.min(num_words) {
                for width in 0..out_max_width.min(MAX_SPAN_WIDTH) {
                    let end_word = word_idx + width;
                    if end_word >= num_words {
                        continue;
                    }

                    for class_idx in 0..num_classes.min(entity_types.len()) {
                        let idx = (word_idx * out_max_width * num_classes)
                            + (width * num_classes)
                            + class_idx;

                        if idx < output_data.len() {
                            let logit = output_data[idx];
                            let score = 1.0 / (1.0 + (-logit).exp());

                            if score >= threshold {
                                let (char_start, char_end) =
                                    Self::word_span_to_char_offsets(text, text_words, word_idx, end_word);

                                if char_start == 0 && char_end == 0 && word_idx > 0 {
                                    // Offset lookup failed; skip this span.
                                    continue;
                                }

                                let span_text: String = text
                                    .chars()
                                    .skip(char_start)
                                    .take(char_end.saturating_sub(char_start))
                                    .collect();

                                if span_text.is_empty() {
                                    continue;
                                }

                                let entity_type_str =
                                    entity_types.get(class_idx).unwrap_or(&"OTHER");
                                let entity_type =
                                    crate::schema::map_to_canonical(entity_type_str, None);

                                entities.push(Entity::new(
                                    span_text,
                                    entity_type,
                                    char_start,
                                    char_end,
                                    score as f64,
                                ));
                            }
                        }
                    }
                }
            }
        }
        // BIO token-level output: [3, batch, num_words, num_entity_types]
        else if shape.len() == 4 && shape[0] == 3 && shape[1] == 1 {
            let out_num_words = shape[2] as usize;
            let num_classes = shape[3] as usize;
            let word_class_size = out_num_words * num_classes;

            let b_offset = 0_usize;
            let i_offset = word_class_size;

            for class_idx in 0..num_classes.min(entity_types.len()) {
                let mut current_start: Option<(usize, f32)> = None;

                for word_idx in 0..out_num_words.min(num_words) {
                    let b_idx = b_offset + word_idx * num_classes + class_idx;
                    let i_idx = i_offset + word_idx * num_classes + class_idx;

                    let b_logit = output_data.get(b_idx).copied().unwrap_or(-100.0);
                    let i_logit = output_data.get(i_idx).copied().unwrap_or(-100.0);

                    let b_score = 1.0 / (1.0 + (-b_logit).exp());
                    let i_score = 1.0 / (1.0 + (-i_logit).exp());

                    if b_score >= threshold {
                        // Emit any in-progress entity.
                        if let Some((start_word, avg_score)) = current_start.take() {
                            let end_word = word_idx.saturating_sub(1);
                            let label = entity_types.get(class_idx).unwrap_or(&"OTHER");
                            if let Some(e) = Self::build_bio_entity(text, text_words, label, start_word, end_word, avg_score) {
                                entities.push(e);
                            }
                        }
                        current_start = Some((word_idx, b_score));
                    } else if i_score >= threshold && current_start.is_some() {
                        if let Some((sw, score)) = current_start {
                            current_start = Some((sw, (score + i_score) / 2.0));
                        }
                    } else if let Some((start_word, avg_score)) = current_start.take() {
                        let end_word = word_idx.saturating_sub(1);
                        let label = entity_types.get(class_idx).unwrap_or(&"OTHER");
                        if let Some(e) = Self::build_bio_entity(text, text_words, label, start_word, end_word, avg_score) {
                            entities.push(e);
                        }
                    }
                }

                // Handle entity at end of text.
                if let Some((start_word, avg_score)) = current_start.take() {
                    let end_word = out_num_words.min(num_words).saturating_sub(1);
                    let label = entity_types.get(class_idx).unwrap_or(&"OTHER");
                    if let Some(e) = Self::build_bio_entity(text, text_words, label, start_word, end_word, avg_score) {
                        entities.push(e);
                    }
                }
            }
        } else {
            // TODO: Verify the actual poly-encoder output shape against the exported model.
            // If the shape doesn't match either known layout, return a descriptive error.
            return Err(Error::Inference(format!(
                "Unsupported GLiNERPoly output shape: {:?}. \
                 Expected [1,words,width,classes] (span) or [3,1,words,classes] (BIO).",
                shape
            )));
        }

        // Sort by position, deduplicate.
        entities.sort_unstable_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| b.end.cmp(&a.end))
                .then_with(|| {
                    b.confidence
                        .partial_cmp(&a.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        entities.dedup_by(|a, b| a.start == b.start && a.end == b.end);

        // Strip trailing/leading punctuation.
        let entities: Vec<Entity> = entities
            .into_iter()
            .map(|mut e| {
                while e.text.ends_with(['.', ',', ';', ':', '!', '?']) {
                    e.text.pop();
                    if e.end > e.start {
                        e.end -= 1;
                    }
                }
                while e.text.starts_with(['.', ',', ';', ':', '!', '?']) {
                    e.text.remove(0);
                    e.start += 1;
                }
                e
            })
            .filter(|e| !e.text.is_empty() && e.start < e.end)
            .collect();

        Ok(entities)
    }

    /// Build a BIO-decoded entity if the span is valid, or return `None`.
    #[allow(clippy::manual_map)] // clarity over brevity for offset validation
    fn build_bio_entity(
        text: &str,
        text_words: &[&str],
        entity_type_str: &str,
        start_word: usize,
        end_word: usize,
        score: f32,
    ) -> Option<Entity> {
        let num_words = text_words.len();
        if start_word > end_word || end_word >= num_words {
            return None;
        }

        let span_text = text_words[start_word..=end_word].join(" ");
        let (start, end) = Self::word_span_to_char_offsets(text, text_words, start_word, end_word);

        if start == 0 && end == 0 && start_word > 0 {
            return None; // Offset lookup failed.
        }

        let entity_type = crate::schema::map_to_canonical(entity_type_str, None);
        Some(Entity::new(span_text, entity_type, start, end, score as f64))
    }

    /// Convert word indices to character offsets.
    ///
    /// Correctly handles Unicode text by converting byte offsets to character
    /// offsets using the offset module.
    fn word_span_to_char_offsets(
        text: &str,
        words: &[&str],
        start_word: usize,
        end_word: usize,
    ) -> (usize, usize) {
        if words.is_empty()
            || start_word >= words.len()
            || end_word >= words.len()
            || start_word > end_word
        {
            return (0, 0);
        }

        let mut byte_pos = 0;
        let mut start_byte = 0;
        let mut end_byte = text.len();
        let mut found_start = false;
        let mut found_end = false;

        for (idx, word) in words.iter().enumerate() {
            if let Some(pos) = text.get(byte_pos..).and_then(|s| s.find(word)) {
                let abs_pos = byte_pos + pos;

                if idx == start_word {
                    start_byte = abs_pos;
                    found_start = true;
                }
                if idx == end_word {
                    end_byte = abs_pos + word.len();
                    found_end = true;
                    break;
                }

                byte_pos = abs_pos + word.len();
            }
        }

        if !found_start || !found_end {
            (0, 0)
        } else {
            crate::offset::bytes_to_chars(text, start_byte, end_byte)
        }
    }
}
