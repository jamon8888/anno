//! ONNX session wrapper for `gliner2_fastino`. Phase 1 = CPU only.
//! GPU EP wiring (CUDA/CoreML) lands in Phase 3.
//!
//! Thin wrapper around `crate::backends::hf_loader::create_onnx_session` —
//! mostly here so future phases can layer IOBinding and execution-provider
//! selection without touching the call sites.

use crate::backends::gliner2_fastino::errors::Error;
use crate::backends::hf_loader;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Session {
    inner: Arc<Mutex<ort::session::Session>>,
}

impl Session {
    pub fn from_path(model_path: &Path) -> Result<Self, Error> {
        let cfg = hf_loader::OnnxSessionConfig::default();
        let session = hf_loader::create_onnx_session(model_path, cfg)
            .map_err(|e| Error::Tokenizer(format!("session: {e}")))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
        })
    }

    /// Run inference via a closure that receives a mutable session reference.
    ///
    /// The closure is responsible for calling `session.run(...)` and returning
    /// the result. Using a closure avoids lifetime entanglement between the
    /// `MutexGuard` and the returned `SessionOutputs`.
    pub fn with_session<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ort::session::Session) -> R,
    {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
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
