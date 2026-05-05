//! gliner2_fastino — fastino-ai GLiNER2 backend (issue #18).
//!
//! **Status:** experimental / WIP. No API stability guarantees in Phase 1.
//!
//! Loads `fastino/gliner2-*` ONNX models (Zaratiana et al. 2025,
//! arXiv:2507.18546). Distinct from `gliner_multitask` (which loads GLiNER v1
//! multi-task models with hardcoded `<<ENT>>=128002` IDs and rejects any
//! `fastino/*` model id at the discovery layer).
//!
//! # Architecture deltas vs `gliner_multitask`
//!
//! - Special-token vocabulary: `[P]`, `[E]`, `[C]`, `[L]`, `[R]`,
//!   `[SEP_STRUCT]`, `[SEP_TEXT]`. IDs read from `tokenizer.json` at load
//!   time; never hardcoded.
//! - Prompt format: `( [P] task_name ( [E] label1 [E] label2 ) ) [SEP_TEXT] tokens...`
//! - Span scoring: dot-product similarity (Eq. 1 of arXiv:2507.18546).
//!
//! # LoRA
//!
//! Phase 1 does **not** support runtime LoRA adapter loading. To use a
//! LoRA-fine-tuned model, merge the adapter into the base weights and
//! re-export to ONNX:
//!
//! ```bash
//! python scripts/gliner2_export_onnx.py \
//!     --base fastino/gliner2-multi-v1 \
//!     --lora-adapter ./my_adapter \
//!     --output ./my_merged.onnx
//! ```
//!
//! Pointing `from_local` at a directory containing `adapter_config.json`
//! returns [`errors::Error::LoraAdapterNotSupported`].
//!
//! # Source attribution
//!
//! `processor.rs` is adapted from SemplificaAI/gliner2-rs (Apache-2.0):
//! <https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/processor.rs>

#![cfg(feature = "gliner2-fastino")]

pub mod errors;
pub(crate) mod config;
pub(crate) mod decoder;
pub(crate) mod nms;
pub(crate) mod pipeline;
pub(crate) mod processor;
pub(crate) mod sessions;

/// fastino-ai GLiNER2 model.
///
/// **Experimental.** API may change without semver bump.
pub struct GLiNER2Fastino {
    pub(crate) tokenizer: tokenizers::Tokenizer,
    pub(crate) special: processor::SpecialTokenIds,
    pub(crate) transformer: processor::SchemaTransformer,
    pub(crate) config: config::FastinoConfig,
    pub(crate) sessions: sessions::Sessions,
    pub(crate) model_id: String,
}

impl std::fmt::Debug for GLiNER2Fastino {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GLiNER2Fastino")
            .field("model_id", &self.model_id)
            .field("hidden_size", &self.config.hidden_size)
            .finish()
    }
}

use std::path::Path;

impl GLiNER2Fastino {
    /// Load a fastino GLiNER2 model from a local directory.
    pub fn from_local(model_dir: &Path) -> crate::Result<Self> {
        if model_dir.join("adapter_config.json").exists() {
            return Err(errors::Error::LoraAdapterNotSupported {
                path: model_dir.to_path_buf(),
            }
            .into());
        }

        // Sessions::from_dir resolves the dtype subdir (fp32_v2/, etc.) and
        // loads all 8 ONNX graphs from it. We use the same subdir for
        // tokenizer.json + config.json since SemplificaAI ships those
        // co-located with the ONNX files.
        let (sessions, subdir) = sessions::Sessions::from_dir(model_dir)?;

        // Tokenizer: prefer subdir, fall back to root for layouts that ship
        // tokenizer at the snapshot root.
        let tokenizer_path = if subdir.join("tokenizer.json").exists() {
            subdir.join("tokenizer.json")
        } else {
            model_dir.join("tokenizer.json")
        };
        if !tokenizer_path.exists() {
            return Err(errors::Error::TokenizerMissing(tokenizer_path).into());
        }
        let tokenizer = crate::backends::hf_loader::load_tokenizer(&tokenizer_path)
            .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: tokenizer: {e}")))?;

        let special = processor::SpecialTokenIds::resolve(&tokenizer)?;
        let transformer = processor::SchemaTransformer::new(tokenizer.clone())?;

        // Same fallback logic for config.json.
        let config_path = if subdir.join("config.json").exists() {
            subdir.join("config.json")
        } else {
            model_dir.join("config.json")
        };
        let config = config::FastinoConfig::from_path(&config_path)?;

        Ok(Self {
            tokenizer,
            special,
            transformer,
            config,
            sessions,
            model_id: model_dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "gliner2_fastino_local".to_string()),
        })
    }

    pub(crate) fn extract_ner(
        &self,
        text: &str,
        types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<crate::Entity>> {
        use pipeline::*;
        if types.is_empty() {
            return Ok(vec![]);
        }
        let labels: Vec<String> = types.iter().map(|s| s.to_string()).collect();
        let task = processor::SchemaTask::Entities(labels.clone());
        let record = self.transformer.transform(text, &[task])?;
        let num_words = record.word_to_char_maps.len();
        if num_words == 0 {
            return Ok(vec![]);
        }

        let enc = run_encoder(&self.sessions, &record)?;
        let tg  = run_token_gather(&self.sessions, &enc, &record)?;
        let sr  = run_span_rep(&self.sessions, &tg, num_words)?;

        let task_map = record.tasks.first().ok_or_else(|| {
            crate::Error::Backend("gliner2_fastino: transformer produced no task mapping".into())
        })?;
        let sg = run_schema_gather(&self.sessions, &enc, task_map)?;
        let pred_count = run_count_pred_argmax(&self.sessions, &sg)?;
        if pred_count == 0 {
            return Ok(vec![]);
        }
        let cl = run_count_lstm_fixed(&self.sessions, &sg)?;
        let scorer_out = run_scorer(&self.sessions, &sr, &cl)?;
        let entities = decode_entities(
            text,
            &record,
            task_map,
            &scorer_out,
            pred_count,
            threshold,
            /* flat_ner = */ false,
        );
        Ok(entities)
    }

    /// Load a fastino GLiNER2 model by Hugging Face model id.
    ///
    /// Downloads `tokenizer.json`, `config.json`, and the 8 v2 ONNX graphs
    /// (encoder, token_gather, span_rep, schema_gather, count_pred_argmax,
    /// count_lstm_fixed, scorer, classifier) from the repo. Tries fp32_v2/
    /// first, falls back to fp16_v2/ per file. Then defers to `from_local`.
    ///
    /// **Phase 3 / experimental.** No retry/backoff on transient HF Hub
    /// failures beyond what `hf-hub` itself provides.
    pub fn from_pretrained(model_id: &str) -> crate::Result<Self> {
        let api = crate::backends::hf_loader::hf_api()
            .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: hf_api: {e}")))?;
        let repo = api.model(model_id.to_string());

        // Tokenizer + config are co-located with the ONNX files in dtype subdirs.
        // Try fp32_v2/ first, fall back to fp16_v2/, then root for backward compat.
        let tokenizer_path = crate::backends::hf_loader::download_model_file(
            &repo,
            &["fp32_v2/tokenizer.json", "fp16_v2/tokenizer.json", "tokenizer.json"],
        )
        .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: download tokenizer: {e}")))?;
        let _config_path = crate::backends::hf_loader::download_model_file(
            &repo,
            &["fp32_v2/config.json", "fp16_v2/config.json", "config.json"],
        )
        .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: download config: {e}")))?;

        // Download the 8 v2 ONNX files. Try fp32_v2 first (clearer dtype
        // semantics for debugging), then fp16_v2 as fallback.
        let bases = [
            "encoder", "token_gather", "span_rep", "schema_gather",
            "count_pred_argmax", "count_lstm_fixed", "scorer", "classifier",
        ];
        for base in &bases {
            let candidates = [
                format!("fp32_v2/{base}_fp32.onnx"),
                format!("fp16_v2/{base}_fp16.onnx"),
            ];
            let candidate_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
            crate::backends::hf_loader::download_model_file(&repo, &candidate_refs)
                .map_err(|e| crate::Error::Backend(
                    format!("gliner2_fastino: download {base}: {e}")
                ))?;
        }

        // Resolve to the snapshot dir and dispatch.
        // tokenizer_path may be at <snapshot>/fp32_v2/tokenizer.json (subdir)
        // or <snapshot>/tokenizer.json (legacy). Walk up until we find a parent
        // containing one of the dtype subdirs.
        let mut snapshot_dir = tokenizer_path.parent().ok_or_else(|| {
            crate::Error::Backend("gliner2_fastino: tokenizer has no parent".into())
        })?;
        loop {
            let has_dtype_subdir = ["fp32_v2", "fp16_v2", "fp32", "fp16"]
                .iter()
                .any(|sub| snapshot_dir.join(sub).is_dir());
            if has_dtype_subdir {
                break;
            }
            match snapshot_dir.parent() {
                Some(p) => snapshot_dir = p,
                None => break, // reached filesystem root; from_local will surface an error
            }
        }
        let mut model = Self::from_local(snapshot_dir)?;
        model.model_id = model_id.to_string();
        Ok(model)
    }
}

use crate::backends::inference::ZeroShotNER;
use crate::{EntityCategory, EntityType, Language};

impl crate::Model for GLiNER2Fastino {
    fn extract_entities(
        &self,
        text: &str,
        _language: Option<Language>,
    ) -> crate::Result<Vec<crate::Entity>> {
        self.extract_ner(text, &["person", "organization", "location", "date"], 0.5)
    }

    fn supported_types(&self) -> Vec<EntityType> {
        vec![
            EntityType::Person,
            EntityType::Organization,
            EntityType::Location,
            EntityType::Date,
            EntityType::custom("misc", EntityCategory::Misc),
        ]
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "GLiNER2Fastino"
    }

    fn description(&self) -> &'static str {
        "fastino-ai GLiNER2 (NER + classification, ONNX, experimental)"
    }

    fn capabilities(&self) -> crate::ModelCapabilities {
        crate::ModelCapabilities {
            zero_shot: true,
            ..Default::default()
        }
    }

    fn as_zero_shot(&self) -> Option<&dyn ZeroShotNER> {
        Some(self)
    }
}

impl ZeroShotNER for GLiNER2Fastino {
    fn default_types(&self) -> &[&'static str] {
        &["person", "organization", "location", "date", "event"]
    }

    fn extract_with_types(
        &self,
        text: &str,
        types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<crate::Entity>> {
        self.extract_ner(text, types, threshold)
    }

    fn extract_with_descriptions(
        &self,
        text: &str,
        descriptions: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<crate::Entity>> {
        // Phase 1: descriptions are treated as simple type labels
        self.extract_ner(text, descriptions, threshold)
    }
}

impl GLiNER2Fastino {
    /// Single-label classification using the dedicated `[L]`-head classifier.
    ///
    /// Returns labels sorted by descending probability (softmax). The
    /// `threshold` parameter is reserved for future multi-label use; in
    /// Phase 3 single-label mode it's ignored.
    ///
    /// Not behind a public trait — see spec §3.
    pub fn classify(
        &self,
        text: &str,
        labels: &[&str],
        _threshold: f32,
    ) -> crate::Result<Vec<(String, f32)>> {
        use pipeline::*;
        if labels.is_empty() {
            return Ok(vec![]);
        }
        let label_strings: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
        let task = processor::SchemaTask::Classifications(
            "classification".to_string(),
            label_strings.clone(),
        );
        let record = self.transformer.transform(text, &[task])?;
        let task_map = record.tasks.first().ok_or_else(|| {
            crate::Error::Backend("gliner2_fastino: transformer produced no task mapping".into())
        })?;

        let enc = run_encoder(&self.sessions, &record)?;
        let sg = run_schema_gather(&self.sessions, &enc, task_map)?;
        let pred_count = run_count_pred_argmax(&self.sessions, &sg)?;
        if pred_count == 0 {
            return Ok(label_strings.into_iter().map(|l| (l, 0.0)).collect());
        }
        let probs = run_classifier(&self.sessions, &sg)?;

        let mut out: Vec<(String, f32)> = label_strings
            .into_iter()
            .zip(probs.into_iter())
            .collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(out)
    }
}

#[cfg(test)]
mod from_local_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn from_local_rejects_lora_adapter_dir() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("adapter_config.json"), "{}").unwrap();

        let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("scripts/gliner2_export_onnx.py"), "missing script path: {msg}");
        assert!(msg.contains("--lora-adapter"), "missing flag: {msg}");
    }

    #[test]
    fn from_local_missing_tokenizer_returns_typed_error() {
        let dir = tempdir().unwrap();
        // Empty directory — no tokenizer.json, no adapter_config.json.
        // With the subdir-first loading order, Sessions::from_dir fires
        // before tokenizer resolution and surfaces a "no complete v2 session
        // set" error. Both session-set and tokenizer errors indicate a
        // missing/incomplete model directory.
        let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("tokenizer") || msg.contains("no complete v2 session set"),
            "got {msg}"
        );
    }

    #[test]
    fn from_local_empty_dir_returns_session_set_error() {
        let dir = tempdir().unwrap();
        // Need at least tokenizer.json to bypass the early-return.
        // Stub one out using the project's own fixture.
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/gliner2_fastino/stub_tokenizer.json");
        fs::copy(&fixture, dir.path().join("tokenizer.json")).unwrap();
        // And a config.json with hidden_size.
        fs::write(
            dir.path().join("config.json"),
            r#"{"hidden_size": 768, "counting_layer": "count_lstm_v2"}"#,
        ).unwrap();

        let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no complete v2 session set"),
            "Phase 3 should report missing sessions, not 'Phase 3 needed'. Got: {msg}"
        );
    }
}
