//! ONNX session wrapper for `gliner2_fastino`. Phase 1 = CPU only.
//! GPU EP wiring (CUDA/CoreML) lands in Phase 3.
//!
//! Thin wrapper around `crate::backends::hf_loader::create_onnx_session` —
//! mostly here so future phases can layer IOBinding and execution-provider
//! selection without touching the call sites.

use crate::backends::gliner2_fastino::errors::Error;
use crate::backends::hf_loader;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
pub struct Session {
    inner: Arc<ort::session::Session>,
}

impl Session {
    pub fn from_path(model_path: &Path) -> Result<Self, Error> {
        let cfg = hf_loader::OnnxSessionConfig::default();
        let session = hf_loader::create_onnx_session(model_path, cfg)
            .map_err(|e| Error::Tokenizer(format!("session: {e}")))?;
        Ok(Self {
            inner: Arc::new(session),
        })
    }

    pub fn inner(&self) -> &ort::session::Session {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_load_failure_returns_error_for_missing_file() {
        let p = Path::new("/nonexistent/gliner2_fastino_model.onnx");
        let err = Session::from_path(p).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("session") || msg.contains("not found") || msg.contains("nonexistent"),
            "expected loading error, got: {msg}"
        );
    }
}
