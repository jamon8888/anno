//! Test-only helpers for isolating process-global environment state.

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

static ANNO_MODELS_DIR_LOCK: Mutex<()> = Mutex::new(());

/// Scoped `ANNO_MODELS_DIR` override that restores the previous value on drop.
pub(crate) struct ScopedAnnoModelsDir {
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

impl ScopedAnnoModelsDir {
    /// Set `ANNO_MODELS_DIR` while holding the shared test lock.
    pub(crate) fn set(path: &Path) -> Self {
        let guard = ANNO_MODELS_DIR_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("ANNO_MODELS_DIR");
        std::env::set_var("ANNO_MODELS_DIR", path);
        Self {
            previous,
            _guard: guard,
        }
    }
}

impl Drop for ScopedAnnoModelsDir {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var("ANNO_MODELS_DIR", value);
        } else {
            std::env::remove_var("ANNO_MODELS_DIR");
        }
    }
}
