//! Multi-session ONNX container for `gliner2_fastino`. Phase 3.
//!
//! Adapted from SemplificaAI/gliner2-rs (Apache-2.0):
//! https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/lib_v2.rs
//! Original: Copyright 2026 Dario Finardi, Semplifica s.r.l.

use crate::backends::gliner2_fastino::errors::Error;
use crate::backends::hf_loader;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Eight ONNX sessions making up the GLiNER2 v2 inference pipeline.
///
/// Each `SessionSlot` is wrapped in `Arc<Mutex<>>` so the engine can hand
/// out `&self` while the closure-style `with_session` API mutates the
/// underlying ort `Session::run`. This mirrors Phase 1's single-Session
/// pattern, applied per role.
pub struct Sessions {
    pub encoder:           SessionSlot,
    pub token_gather:      SessionSlot,
    pub span_rep:          SessionSlot,
    pub schema_gather:     SessionSlot,
    pub count_pred_argmax: SessionSlot,
    pub count_lstm_fixed:  SessionSlot,
    pub scorer:            SessionSlot,
    pub classifier:        SessionSlot,
}

/// Single session wrapped in Arc<Mutex<>> with a `with_session` closure
/// API. Identical to Phase 1's `session::Session` but extracted as a
/// reusable type.
#[derive(Debug)]
pub struct SessionSlot {
    inner: Arc<Mutex<ort::session::Session>>,
}

impl SessionSlot {
    pub fn from_path(model_path: &Path) -> Result<Self, Error> {
        let cfg = hf_loader::OnnxSessionConfig::default();
        let session = hf_loader::create_onnx_session(model_path, cfg)
            .map_err(|e| Error::Tokenizer(format!("session {}: {e}", model_path.display())))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
        })
    }

    pub fn with_session<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ort::session::Session) -> R,
    {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }
}

impl std::fmt::Debug for Sessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sessions").field("count", &8usize).finish()
    }
}

impl Sessions {
    /// Load all 8 sessions from a directory. Tries the v2 8-graph layouts
    /// (`fp32_v2/`, `fp16_v2/`) first, then v1 5-graph fallbacks (which
    /// will fail the all_present check below — kept for future-proofing).
    ///
    /// Phase 3 standard mode does NOT use the `_iobinding.onnx` variants
    /// — those are reserved for Phase 3.5 IOBinding mode.
    pub fn from_dir(model_dir: &Path) -> Result<(Self, std::path::PathBuf), Error> {
        // The 8-session v2 layout lives in `_v2` subdirs only — `fp32/`
        // and `fp16/` are the legacy v1 layout (5 graphs, missing
        // token_gather/schema_gather/count_pred_argmax/count_lstm_fixed).
        // The `all_present` guard below filters incompatible layouts.
        for (subdir, suffix) in [
            ("fp32_v2", "_fp32.onnx"),
            ("fp16_v2", "_fp16.onnx"),
            ("fp32",    "_fp32.onnx"),  // v1 fallback — likely won't match
            ("fp16",    "_fp16.onnx"),  // v1 fallback
        ] {
            let try_dir = model_dir.join(subdir);
            if !try_dir.is_dir() {
                continue;
            }
            let candidate = |name: &str| try_dir.join(format!("{name}{suffix}"));
            let all_present = [
                "encoder", "token_gather", "span_rep", "schema_gather",
                "count_pred_argmax", "count_lstm_fixed", "scorer", "classifier",
            ].iter().all(|n| candidate(n).exists());
            if !all_present {
                continue;
            }
            return Ok((
                Self {
                    encoder:           SessionSlot::from_path(&candidate("encoder"))?,
                    token_gather:      SessionSlot::from_path(&candidate("token_gather"))?,
                    span_rep:          SessionSlot::from_path(&candidate("span_rep"))?,
                    schema_gather:     SessionSlot::from_path(&candidate("schema_gather"))?,
                    count_pred_argmax: SessionSlot::from_path(&candidate("count_pred_argmax"))?,
                    count_lstm_fixed:  SessionSlot::from_path(&candidate("count_lstm_fixed"))?,
                    scorer:            SessionSlot::from_path(&candidate("scorer"))?,
                    classifier:        SessionSlot::from_path(&candidate("classifier"))?,
                },
                try_dir,
            ));
        }
        Err(Error::Tokenizer(format!(
            "no complete v2 session set found under {} (looked in fp32_v2/, fp16_v2/, fp32/, fp16/)",
            model_dir.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn from_dir_fails_clearly_on_empty_dir() {
        let dir = tempdir().unwrap();
        let err = Sessions::from_dir(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no complete v2 session set"), "got: {msg}");
        assert!(msg.contains("fp32") || msg.contains("fp16"), "got: {msg}");
    }

    #[test]
    fn from_dir_fails_clearly_on_partial_layout() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("fp32_v2")).unwrap();
        // Only encoder present — should not be a "complete set".
        std::fs::write(dir.path().join("fp32_v2/encoder_fp32.onnx"), b"").unwrap();
        let err = Sessions::from_dir(dir.path()).unwrap_err();
        assert!(err.to_string().contains("no complete v2 session set"));
    }
}
