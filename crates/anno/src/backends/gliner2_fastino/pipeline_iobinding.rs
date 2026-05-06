//! Phase 3.5: IoBinding-mode 8-session inference pipeline.
//!
//! Adapted from SemplificaAI/gliner2-rs/rust_component/src/lib_v2.rs:285-660
//! (`Gliner2EngineV2::extract_iobinding`, Apache-2.0). See
//! `docs/dev-notes/gliner2-iobinding-port-notes.md` for the symbol map.
//!
//! Each `run_*_io` function takes the prerequisite `Value`s from the
//! prior session and runs its session via `IoBinding`. Inter-session
//! tensors stay device-resident (CPU on stock Phase 3.5; future GPU
//! providers via `OnnxSessionConfig` will keep them in EP-allocated
//! buffers). Only the final `scorer` output is read back to host as
//! `ndarray::Array4<f32>` — the cost is amortised over the 7 chained
//! device-resident outputs.
//!
//! `MemoryInfo` is created per-call (not cached on the engine) because
//! `ort::memory::MemoryInfo` is `!Send + !Sync` (contains a `NonNull`).
//! Caching it on the engine would break the engine's `Send + Sync`
//! requirement for the `Model` and `ZeroShotNER` traits. Per-call cost
//! is negligible — pure metadata, no buffer allocation.

#![allow(dead_code)] // Phase 3.5 in-progress: these are wired up by M11.

use crate::backends::gliner2_fastino::errors::Error;
use crate::backends::gliner2_fastino::processor::ProcessedRecord;
use crate::backends::gliner2_fastino::sessions::Sessions;
use ort::memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType};
use ort::value::DynValue;

/// Build the device `MemoryInfo` used for chained inter-session outputs.
///
/// Phase 3.5 ships CPU-only. CUDA / CoreML providers will widen this
/// in Phase 4 once an `OnnxSessionConfig::prefer_cuda`-aware variant
/// is wired in.
pub(crate) fn device_memory_info() -> ort::Result<MemoryInfo> {
    MemoryInfo::new(
        AllocationDevice::CPU,
        0,
        AllocatorType::Device,
        MemoryType::Default,
    )
}

/// Build the CPU-output `MemoryInfo` used for outputs that must be
/// read back to host (e.g. `count_pred_argmax`'s scalar i64 result).
pub(crate) fn cpu_output_memory_info() -> ort::Result<MemoryInfo> {
    MemoryInfo::new(
        AllocationDevice::CPU,
        0,
        AllocatorType::Device,
        MemoryType::CPUOutput,
    )
}

/// Output of the encoder step, kept as a device-resident `DynValue`.
/// The wrapped value owns its buffer and is safe to thread through
/// later `run_*_io` functions as input.
pub(crate) struct EncoderOutputIo {
    pub hidden_states: DynValue,
    /// Cached output tensor name (varies across exports —
    /// `last_hidden_state` / `hidden_states` / `output`). Kept so callers
    /// don't have to re-resolve it.
    pub output_name: String,
}

/// Run the encoder graph via IoBinding. Inputs (`input_ids`,
/// `attention_mask`) come from `record`; the output is bound to
/// `device_mem` and returned as a device-resident `DynValue`.
pub(crate) fn run_encoder_io(
    sessions: &Sessions,
    record: &ProcessedRecord,
    device_mem: &MemoryInfo,
) -> Result<EncoderOutputIo, Error> {
    use ndarray::Array2;

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

    sessions.encoder.with_session(|s| -> Result<EncoderOutputIo, Error> {
        // Resolve output name. Different fastino exports ship different
        // names — match the standard pipeline's priority order.
        let output_name = resolve_output_name(s, &["hidden_states", "last_hidden_state", "output"]);

        let mut binding = s
            .create_binding()
            .map_err(|e| Error::Tokenizer(format!("encoder create_binding: {e}")))?;
        binding
            .bind_input("input_ids", &input_ids_t)
            .map_err(|e| Error::Tokenizer(format!("encoder bind input_ids: {e}")))?;
        binding
            .bind_input("attention_mask", &attn_mask_t)
            .map_err(|e| Error::Tokenizer(format!("encoder bind attention_mask: {e}")))?;
        binding
            .bind_output_to_device(&output_name, device_mem)
            .map_err(|e| Error::Tokenizer(format!("encoder bind_output_to_device: {e}")))?;

        let outputs = s
            .run_binding(&binding)
            .map_err(|e| Error::Tokenizer(format!("encoder run_binding: {e}")))?;

        // Take the bound output by name. The returned `Value` owns its
        // buffer and is safe to return out of the with_session closure.
        let hidden_states = outputs
            .into_iter()
            .find_map(|(name, val)| {
                if &*name == output_name.as_str() {
                    Some(val)
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::Tokenizer(format!(
                "encoder: output '{output_name}' not present in run_binding result"
            )))?;

        Ok(EncoderOutputIo { hidden_states, output_name })
    })
}

/// Look up an output's name in priority order. Falls back to the
/// session's first output if none of the candidates match — matches
/// the standard pipeline's behavior so IoBinding works on any export
/// the standard pipeline does.
fn resolve_output_name(
    session: &ort::session::Session,
    candidates: &[&str],
) -> String {
    let session_outputs: Vec<String> =
        session.outputs().iter().map(|o| o.name().to_string()).collect();
    for &c in candidates {
        if session_outputs.iter().any(|n| n == c) {
            return c.to_string();
        }
    }
    session_outputs
        .into_iter()
        .next()
        .unwrap_or_else(|| candidates.first().copied().unwrap_or("output").to_string())
}

/// Output of token_gather: word-level embeddings.
/// Shape: `[1, num_words, H]`. Device-resident DynValue.
pub(crate) struct TokenGatherOutputIo {
    pub text_embs: DynValue,
    pub output_name: String,
}

/// Output of span_rep: span-level embeddings.
/// Shape: `[1, num_words, MAX_WIDTH, H]`. Device-resident DynValue.
pub(crate) struct SpanRepOutputIo {
    pub span_embs: DynValue,
    pub output_name: String,
}

/// Run token_gather via IoBinding. Inputs:
/// - `last_hidden_state` from [`EncoderOutputIo`] (device-resident).
/// - `word_indices` built from `record.word_to_token_maps[*].0`.
///
/// Output is bound to `device_mem` and returned as a device-resident
/// DynValue for chaining into [`run_span_rep_io`] (M7).
pub(crate) fn run_token_gather_io(
    sessions: &Sessions,
    encoder_out: &EncoderOutputIo,
    record: &ProcessedRecord,
    device_mem: &MemoryInfo,
) -> Result<TokenGatherOutputIo, Error> {
    use ndarray::Array1;

    let num_words = record.word_to_token_maps.len();
    if num_words == 0 {
        return Err(Error::Tokenizer("token_gather_io: 0 words in record".into()));
    }
    let word_starts: Vec<i64> = record
        .word_to_token_maps
        .iter()
        .map(|&(start, _)| start as i64)
        .collect();
    let word_idx_arr: Array1<i64> = Array1::from_vec(word_starts);
    let word_idx_t = crate::backends::ort_compat::tensor_from_ndarray(word_idx_arr)
        .map_err(|e| Error::Tokenizer(format!("token_gather_io word_idx tensor: {e}")))?;

    sessions.token_gather.with_session(|s| -> Result<TokenGatherOutputIo, Error> {
        let output_name = resolve_output_name(s, &["text_embs"]);
        let mut binding = s
            .create_binding()
            .map_err(|e| Error::Tokenizer(format!("token_gather_io create_binding: {e}")))?;
        binding
            .bind_input("last_hidden_state", &encoder_out.hidden_states)
            .map_err(|e| Error::Tokenizer(format!("token_gather_io bind last_hidden_state: {e}")))?;
        binding
            .bind_input("word_indices", &word_idx_t)
            .map_err(|e| Error::Tokenizer(format!("token_gather_io bind word_indices: {e}")))?;
        binding
            .bind_output_to_device(&output_name, device_mem)
            .map_err(|e| Error::Tokenizer(format!("token_gather_io bind_output: {e}")))?;
        let outputs = s
            .run_binding(&binding)
            .map_err(|e| Error::Tokenizer(format!("token_gather_io run_binding: {e}")))?;
        let text_embs = outputs
            .into_iter()
            .find_map(|(name, val)| if &*name == output_name.as_str() { Some(val) } else { None })
            .ok_or_else(|| Error::Tokenizer(format!(
                "token_gather_io: output '{output_name}' not present"
            )))?;
        Ok(TokenGatherOutputIo { text_embs, output_name })
    })
}

/// Run span_rep via IoBinding. Inputs:
/// - `hidden_states`: device-resident text_embs from [`TokenGatherOutputIo`].
/// - `span_idx`: i64 Array3 of shape `[1, num_words * MAX_WIDTH, 2]` built by
///   [`super::pipeline::build_span_idx`] (shared with the standard pipeline).
///
/// Output: span_embs DynValue of shape `[1, num_words, MAX_WIDTH, H]`,
/// device-resident, chained into M10's run_scorer_io.
pub(crate) fn run_span_rep_io(
    sessions: &Sessions,
    tg_out: &TokenGatherOutputIo,
    num_words: usize,
    device_mem: &MemoryInfo,
) -> Result<SpanRepOutputIo, Error> {
    let span_idx = super::pipeline::build_span_idx(num_words);
    let span_idx_t = crate::backends::ort_compat::tensor_from_ndarray(span_idx)
        .map_err(|e| Error::Tokenizer(format!("span_rep_io span_idx tensor: {e}")))?;

    sessions.span_rep.with_session(|s| -> Result<SpanRepOutputIo, Error> {
        let output_name = resolve_output_name(s, &["span_embeddings", "span_embs"]);
        let mut binding = s
            .create_binding()
            .map_err(|e| Error::Tokenizer(format!("span_rep_io create_binding: {e}")))?;
        binding
            .bind_input("hidden_states", &tg_out.text_embs)
            .map_err(|e| Error::Tokenizer(format!("span_rep_io bind hidden_states: {e}")))?;
        binding
            .bind_input("span_idx", &span_idx_t)
            .map_err(|e| Error::Tokenizer(format!("span_rep_io bind span_idx: {e}")))?;
        binding
            .bind_output_to_device(&output_name, device_mem)
            .map_err(|e| Error::Tokenizer(format!("span_rep_io bind_output: {e}")))?;
        let outputs = s
            .run_binding(&binding)
            .map_err(|e| Error::Tokenizer(format!("span_rep_io run_binding: {e}")))?;
        let span_embs = outputs
            .into_iter()
            .find_map(|(name, val)| if &*name == output_name.as_str() { Some(val) } else { None })
            .ok_or_else(|| Error::Tokenizer(format!(
                "span_rep_io: output '{output_name}' not present"
            )))?;
        Ok(SpanRepOutputIo { span_embs, output_name })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_memory_info_constructs_on_cpu() {
        // Phase 3.5 M5: smoke test — CPU-only MemoryInfo creation must
        // succeed on every host. Ensures the runtime API works in our
        // ort 2.0.0-rc.12 pin.
        let mi = device_memory_info().expect("CPU MemoryInfo creation should succeed");
        let _ = format!("{:?}", mi);
    }

    #[test]
    fn cpu_output_memory_info_constructs() {
        // Phase 3.5 M5: same for the CPUOutput variant used by
        // count_pred_argmax (M9).
        let mi = cpu_output_memory_info().expect("CPUOutput MemoryInfo creation");
        let _ = format!("{:?}", mi);
    }
}
