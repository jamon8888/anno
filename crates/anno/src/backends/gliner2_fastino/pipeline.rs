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
                    let (_shape, cow) = v
                        .try_extract_tensor::<f32>()
                        .map_err(|e| Error::Tokenizer(format!("encoder extract: {e}")))?;
                    return Ok(cow.into_owned());
                }
            }
            // Fallback: take the first output.
            let first = outputs.values().next().ok_or_else(|| {
                Error::Tokenizer("encoder: no outputs".into())
            })?;
            let (_shape, cow) = first
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Tokenizer(format!("encoder extract first: {e}")))?;
            Ok(cow.into_owned())
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
