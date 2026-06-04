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
        Self::set_raw(path.as_os_str())
    }

    /// Set `ANNO_MODELS_DIR` to a raw OS value while holding the shared test lock.
    pub(crate) fn set_raw(value: impl AsRef<std::ffi::OsStr>) -> Self {
        let guard = ANNO_MODELS_DIR_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("ANNO_MODELS_DIR");
        unsafe { std::env::set_var("ANNO_MODELS_DIR", value.as_ref()) };
        Self {
            previous,
            _guard: guard,
        }
    }

    /// Unset `ANNO_MODELS_DIR` while holding the shared test lock.
    pub(crate) fn unset() -> Self {
        let guard = ANNO_MODELS_DIR_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("ANNO_MODELS_DIR");
        unsafe { std::env::remove_var("ANNO_MODELS_DIR") };
        Self {
            previous,
            _guard: guard,
        }
    }
}

impl Drop for ScopedAnnoModelsDir {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            unsafe { std::env::set_var("ANNO_MODELS_DIR", value) };
        } else {
            unsafe { std::env::remove_var("ANNO_MODELS_DIR") };
        }
    }
}
