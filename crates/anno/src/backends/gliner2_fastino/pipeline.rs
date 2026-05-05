//! Standard-mode 8-session inference pipeline for `gliner2_fastino`.
//!
//! Adapted from SemplificaAI/gliner2-rs (Apache-2.0):
//! https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/lib_v2.rs
//! Specifically: `Gliner2EngineV2::extract_standard` (lines ~660-897).
//! Original: Copyright 2026 Dario Finardi, Semplifica s.r.l.
//!
//! Phase 3 standard mode (this module) does NOT implement IOBinding.
//! The IOBinding-mode pipeline (lib_v2.rs:285-660) keeps tensors in
//! a single ort allocator across session boundaries for 2-3× speedup;
//! that's a Phase 3.5 follow-up.

use crate::backends::gliner2_fastino::errors::Error;
use crate::backends::gliner2_fastino::processor::ProcessedRecord;
use crate::backends::gliner2_fastino::sessions::Sessions;
use ndarray::{Array2, Array3};

/// Maximum span width baked into the v2 export. Spans wider than this
/// can't be scored. Hardcoded in `count_lstm_fixed` and `scorer` ONNX
/// graphs.
pub const MAX_WIDTH: usize = 8;

/// Maximum predicted instance count baked into the v2 export. Used by
/// the scorer's first dimension (struct_proj is `[MAX_COUNT, M, H]`).
pub const MAX_COUNT: usize = 20;

/// Output of the encoder step. Owned f32 ndarray of shape `[1, L, H]`.
pub(crate) struct EncoderOutput {
    pub hidden_states: Array3<f32>,
}

/// Run the encoder graph. Tries output names in priority order
/// (`hidden_states`, `last_hidden_state`, `output`) — different fastino
/// exports use different names.
pub(crate) fn run_encoder(
    sessions: &Sessions,
    record: &ProcessedRecord,
) -> Result<EncoderOutput, Error> {
    let seq_len = record.input_ids.len();
    let input_ids: Array2<i64> = Array2::from_shape_vec(
        (1, seq_len),
        record.input_ids.clone(),
    )
    .map_err(|e| Error::Tokenizer(format!("encoder input_ids reshape: {e}")))?;
    let attn_mask: Array2<i64> = Array2::from_shape_vec(
        (1, seq_len),
        record.attention_mask.clone(),
    )
    .map_err(|e| Error::Tokenizer(format!("encoder attn reshape: {e}")))?;

    let input_ids_t = crate::backends::ort_compat::tensor_from_ndarray(input_ids)
        .map_err(|e| Error::Tokenizer(format!("encoder input_ids tensor: {e}")))?;
    let attn_mask_t = crate::backends::ort_compat::tensor_from_ndarray(attn_mask)
        .map_err(|e| Error::Tokenizer(format!("encoder attn tensor: {e}")))?;

    let hs: ndarray::ArrayD<f32> = sessions.encoder.with_session(
        |s| -> Result<_, Error> {
            let outputs = s
                .run(ort::inputs![
                    "input_ids"      => input_ids_t.into_dyn(),
                    "attention_mask" => attn_mask_t.into_dyn(),
                ])
                .map_err(|e| Error::Tokenizer(format!("encoder run: {e}")))?;

            for name in ["hidden_states", "last_hidden_state", "output"] {
                if let Some(v) = outputs.get(name) {
                    let (shape, cow) = v
                        .try_extract_tensor::<f32>()
                        .map_err(|e| Error::Tokenizer(format!("encoder extract: {e}")))?;
                    let data: Vec<f32> = cow.to_vec();
                    let shape_usize: Vec<usize> = shape.iter().map(|&s| s as usize).collect();
                    return Ok(ndarray::ArrayD::from_shape_vec(shape_usize, data)
                        .map_err(|e| Error::Tokenizer(format!("encoder array reshape: {e}")))?);
                }
            }
            // Fallback: take the first output.
            let first = outputs.values().next().ok_or_else(|| {
                Error::Tokenizer("encoder: no outputs".into())
            })?;
            let (shape, cow) = first
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Tokenizer(format!("encoder extract first: {e}")))?;
            let data: Vec<f32> = cow.to_vec();
            let shape_usize: Vec<usize> = shape.iter().map(|&s| s as usize).collect();
            Ok(ndarray::ArrayD::from_shape_vec(shape_usize, data)
                .map_err(|e| Error::Tokenizer(format!("encoder array reshape: {e}")))?)
        },
    )?;

    // hs is dynamic; convert to fixed [1, L, H] Array3.
    let shape = hs.shape().to_vec();
    if shape.len() != 3 || shape[0] != 1 {
        return Err(Error::Tokenizer(format!(
            "encoder output shape {:?}: expected [1, L, H]",
            shape
        )));
    }
    let hidden_states: Array3<f32> = hs
        .into_dimensionality::<ndarray::Ix3>()
        .map_err(|e| Error::Tokenizer(format!("encoder dim convert: {e}")))?;
    Ok(EncoderOutput { hidden_states })
}

/// Output of token_gather: word-level embeddings extracted from the
/// encoder's hidden states using `word_to_token_maps`.
pub(crate) struct TokenGatherOutput {
    /// Shape: `[1, num_words, H]`
    pub text_embs: Array3<f32>,
}

pub(crate) fn run_token_gather(
    sessions: &Sessions,
    encoder_out: &EncoderOutput,
    record: &ProcessedRecord,
) -> Result<TokenGatherOutput, Error> {
    use ndarray::Array1;

    let num_words = record.word_to_token_maps.len();
    if num_words == 0 {
        return Err(Error::Tokenizer("token_gather: 0 words in record".into()));
    }
    let word_starts: Vec<i64> = record
        .word_to_token_maps
        .iter()
        .map(|&(start, _)| start as i64)
        .collect();
    let word_idx_arr: Array1<i64> = Array1::from_vec(word_starts);

    let hs_t = crate::backends::ort_compat::tensor_from_ndarray(
        encoder_out.hidden_states.clone(),
    )
    .map_err(|e| Error::Tokenizer(format!("token_gather hs tensor: {e}")))?;
    let word_idx_t = crate::backends::ort_compat::tensor_from_ndarray(word_idx_arr)
        .map_err(|e| Error::Tokenizer(format!("token_gather idx tensor: {e}")))?;

    let result: ndarray::ArrayD<f32> = sessions.token_gather.with_session(
        |s| -> Result<_, Error> {
            let outputs = s
                .run(ort::inputs![
                    "last_hidden_state" => hs_t.into_dyn(),
                    "word_indices"      => word_idx_t.into_dyn(),
                ])
                .map_err(|e| Error::Tokenizer(format!("token_gather run: {e}")))?;
            let v = outputs.values().next().ok_or_else(|| {
                Error::Tokenizer("token_gather: no outputs".into())
            })?;
            let (shape, cow) = v
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Tokenizer(format!("token_gather extract: {e}")))?;
            let data: Vec<f32> = cow.to_vec();
            let shape_usize: Vec<usize> = shape.iter().map(|&s| s as usize).collect();
            Ok(ndarray::ArrayD::from_shape_vec(shape_usize, data)
                .map_err(|e| Error::Tokenizer(format!("token_gather array reshape: {e}")))?)
        },
    )?;

    let text_embs: Array3<f32> = result
        .into_dimensionality::<ndarray::Ix3>()
        .map_err(|e| Error::Tokenizer(format!("token_gather dim: {e}")))?;
    Ok(TokenGatherOutput { text_embs })
}

/// Output of span_rep: span-level embeddings.
pub(crate) struct SpanRepOutput {
    /// Shape: `[1, num_spans, H]` where num_spans = num_words * MAX_WIDTH
    pub span_embs: Array3<f32>,
}

/// Build the span-index tensor used by span_rep.
///
/// For each (start_word, width_idx) pair where `width_idx` ∈ 0..MAX_WIDTH,
/// emit (start, start + width_idx). If end exceeds `num_words`, emit
/// `[0, 0]` as zero-padding (matches upstream's behavior — those spans
/// are masked out by the model).
pub(crate) fn build_span_idx(num_words: usize) -> Array3<i64> {
    let num_spans = num_words * MAX_WIDTH;
    let mut data = Vec::with_capacity(num_spans * 2);
    for start in 0..num_words {
        for width in 0..MAX_WIDTH {
            let end = start + width;
            if end >= num_words {
                data.extend_from_slice(&[0_i64, 0_i64]);
            } else {
                data.push(start as i64);
                data.push(end as i64);
            }
        }
    }
    Array3::from_shape_vec((1, num_spans, 2), data)
        .expect("span_idx shape consistent by construction")
}

pub(crate) fn run_span_rep(
    sessions: &Sessions,
    tg_out: &TokenGatherOutput,
    num_words: usize,
) -> Result<SpanRepOutput, Error> {
    let span_idx = build_span_idx(num_words);

    let hs_t = crate::backends::ort_compat::tensor_from_ndarray(
        tg_out.text_embs.clone(),
    )
    .map_err(|e| Error::Tokenizer(format!("span_rep hs tensor: {e}")))?;
    let idx_t = crate::backends::ort_compat::tensor_from_ndarray(span_idx)
        .map_err(|e| Error::Tokenizer(format!("span_rep idx tensor: {e}")))?;

    let result: ndarray::ArrayD<f32> = sessions.span_rep.with_session(
        |s| -> Result<_, Error> {
            let outputs = s
                .run(ort::inputs![
                    "hidden_states" => hs_t.into_dyn(),
                    "span_idx"      => idx_t.into_dyn(),
                ])
                .map_err(|e| Error::Tokenizer(format!("span_rep run: {e}")))?;
            let v = outputs.values().next().ok_or_else(|| {
                Error::Tokenizer("span_rep: no outputs".into())
            })?;
            let (shape, cow) = v
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Tokenizer(format!("span_rep extract: {e}")))?;
            let data: Vec<f32> = cow.to_vec();
            let shape_usize: Vec<usize> = shape.iter().map(|&s| s as usize).collect();
            Ok(ndarray::ArrayD::from_shape_vec(shape_usize, data)
                .map_err(|e| Error::Tokenizer(format!("span_rep array reshape: {e}")))?)
        },
    )?;

    let span_embs: Array3<f32> = result
        .into_dimensionality::<ndarray::Ix3>()
        .map_err(|e| Error::Tokenizer(format!("span_rep dim: {e}")))?;
    Ok(SpanRepOutput { span_embs })
}

/// Output of schema_gather: per-task pc_emb + field_embs.
pub(crate) struct SchemaGatherOutput {
    /// Shape: `[1, H]` — the [P]-token embedding (prompt context).
    pub pc_emb: Array2<f32>,
    /// Shape: `[M, H]` where M = number of fields/labels for this task.
    pub field_embs: Array2<f32>,
}

pub(crate) fn run_schema_gather(
    sessions: &Sessions,
    encoder_out: &EncoderOutput,
    task: &crate::backends::gliner2_fastino::processor::TaskMapping,
) -> Result<SchemaGatherOutput, Error> {
    use ndarray::Array1;

    let mut indices: Vec<i64> = Vec::with_capacity(1 + task.field_tok_indices.len());
    indices.push(task.prompt_tok_idx as i64);
    indices.extend(task.field_tok_indices.iter().map(|&i| i as i64));
    let idx_arr: Array1<i64> = Array1::from_vec(indices);

    let hs_t = crate::backends::ort_compat::tensor_from_ndarray(
        encoder_out.hidden_states.clone(),
    )
    .map_err(|e| Error::Tokenizer(format!("schema_gather hs tensor: {e}")))?;
    let idx_t = crate::backends::ort_compat::tensor_from_ndarray(idx_arr)
        .map_err(|e| Error::Tokenizer(format!("schema_gather idx tensor: {e}")))?;

    type SchemaResult = (ndarray::ArrayD<f32>, ndarray::ArrayD<f32>);
    let (pc, fields): SchemaResult = sessions.schema_gather.with_session(
        |s| -> Result<_, Error> {
            let outputs = s
                .run(ort::inputs![
                    "last_hidden_state" => hs_t.into_dyn(),
                    "schema_indices"    => idx_t.into_dyn(),
                ])
                .map_err(|e| Error::Tokenizer(format!("schema_gather run: {e}")))?;
            let mut iter = outputs.values();
            let pc_v = iter.next().ok_or_else(|| {
                Error::Tokenizer("schema_gather: missing pc_emb".into())
            })?;
            let fields_v = iter.next().ok_or_else(|| {
                Error::Tokenizer("schema_gather: missing field_embs".into())
            })?;
            let (pc_shape, pc_cow) = pc_v
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Tokenizer(format!("schema_gather pc extract: {e}")))?;
            let (fields_shape, fields_cow) = fields_v
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Tokenizer(format!("schema_gather fields extract: {e}")))?;
            let pc_data: Vec<f32> = pc_cow.to_vec();
            let pc_shape_usize: Vec<usize> = pc_shape.iter().map(|&s| s as usize).collect();
            let pc_arr = ndarray::ArrayD::from_shape_vec(pc_shape_usize, pc_data)
                .map_err(|e| Error::Tokenizer(format!("schema_gather pc reshape: {e}")))?;
            let fields_data: Vec<f32> = fields_cow.to_vec();
            let fields_shape_usize: Vec<usize> = fields_shape.iter().map(|&s| s as usize).collect();
            let fields_arr = ndarray::ArrayD::from_shape_vec(fields_shape_usize, fields_data)
                .map_err(|e| Error::Tokenizer(format!("schema_gather fields reshape: {e}")))?;
            Ok((pc_arr, fields_arr))
        },
    )?;

    let pc_emb: Array2<f32> = pc
        .into_dimensionality::<ndarray::Ix2>()
        .map_err(|e| Error::Tokenizer(format!("schema_gather pc dim: {e}")))?;
    let field_embs: Array2<f32> = fields
        .into_dimensionality::<ndarray::Ix2>()
        .map_err(|e| Error::Tokenizer(format!("schema_gather fields dim: {e}")))?;
    Ok(SchemaGatherOutput { pc_emb, field_embs })
}

/// Run `count_pred_argmax`. Returns the predicted instance count
/// (already argmaxed in-graph; the i64 output is a scalar).
pub(crate) fn run_count_pred_argmax(
    sessions: &Sessions,
    sg_out: &SchemaGatherOutput,
) -> Result<usize, Error> {
    let pc_t = crate::backends::ort_compat::tensor_from_ndarray(sg_out.pc_emb.clone())
        .map_err(|e| Error::Tokenizer(format!("count_pred pc tensor: {e}")))?;

    let count: ndarray::ArrayD<i64> = sessions.count_pred_argmax.with_session(
        |s| -> Result<_, Error> {
            let outputs = s
                .run(ort::inputs![
                    "pc_emb" => pc_t.into_dyn(),
                ])
                .map_err(|e| Error::Tokenizer(format!("count_pred run: {e}")))?;
            let v = outputs.values().next().ok_or_else(|| {
                Error::Tokenizer("count_pred_argmax: no outputs".into())
            })?;
            let (shape, cow) = v
                .try_extract_tensor::<i64>()
                .map_err(|e| Error::Tokenizer(format!("count_pred extract: {e}")))?;
            let data: Vec<i64> = cow.to_vec();
            let shape_usize: Vec<usize> = shape.iter().map(|&s| s as usize).collect();
            Ok(ndarray::ArrayD::from_shape_vec(shape_usize, data)
                .map_err(|e| Error::Tokenizer(format!("count_pred reshape: {e}")))?)
        },
    )?;

    let val = count.iter().next().copied().unwrap_or(0);
    Ok(val.max(0) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_span_idx_basic_shape() {
        let arr = build_span_idx(3);
        assert_eq!(arr.shape(), &[1, 3 * MAX_WIDTH, 2]);
    }

    #[test]
    fn build_span_idx_zero_pads_overflow() {
        // 2 words, MAX_WIDTH=8.
        let arr = build_span_idx(2);
        // First row is start=0 width=0 → [0,0].
        assert_eq!(arr[[0, 0, 0]], 0);
        assert_eq!(arr[[0, 0, 1]], 0);
        // Second row is start=0 width=1 → [0,1].
        assert_eq!(arr[[0, 1, 0]], 0);
        assert_eq!(arr[[0, 1, 1]], 1);
        // 9th row (index MAX_WIDTH = 8) is start=1 width=0 → [1,1].
        assert_eq!(arr[[0, MAX_WIDTH, 0]], 1);
        assert_eq!(arr[[0, MAX_WIDTH, 1]], 1);
        // 10th row is start=1 width=1 → would be (1,2) but 2 >= num_words=2,
        // so padded to [0,0].
        assert_eq!(arr[[0, MAX_WIDTH + 1, 0]], 0);
        assert_eq!(arr[[0, MAX_WIDTH + 1, 1]], 0);
    }
}
