//! Test-only helpers for isolating process-global environment state.
//!
//! All helpers share `ENV_VAR_LOCK`.  Concurrent `setenv`/`getenv` from
//! different OS threads is UB on Linux (glibc's env table is not thread-safe),
//! so EVERY test that mutates any environment variable must hold this lock for
//! the lifetime of the test — even for variables that look unrelated.

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

static ENV_VAR_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the shared env-var test lock.
///
/// Hold the returned guard for the duration of the test.  Use this when a
/// test needs to modify several environment variables under one lock.
pub(crate) fn lock_env() -> MutexGuard<'static, ()> {
    ENV_VAR_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// Generic scoped env-var override
// ---------------------------------------------------------------------------

/// Scoped override for any environment variable.
///
/// Holds `ENV_VAR_LOCK` for the duration so no other thread can race on the
/// environment.  Restores the previous value (or removes the variable) on drop.
pub(crate) struct ScopedEnvVar {
    key: &'static str,
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

impl ScopedEnvVar {
    pub(crate) fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let guard = lock_env();
        let previous = std::env::var_os(key);
        unsafe { std::env::set_var(key, value.as_ref()) };
        Self {
            key,
            previous,
            _guard: guard,
        }
    }

    pub(crate) fn unset(key: &'static str) -> Self {
        let guard = lock_env();
        let previous = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        Self {
            key,
            previous,
            _guard: guard,
        }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            unsafe { std::env::set_var(self.key, value) };
        } else {
            unsafe { std::env::remove_var(self.key) };
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience wrapper: ANNO_MODELS_DIR
// ---------------------------------------------------------------------------

/// Scoped `ANNO_MODELS_DIR` override that restores the previous value on drop.
pub(crate) struct ScopedAnnoModelsDir(ScopedEnvVar);

impl ScopedAnnoModelsDir {
    /// Set `ANNO_MODELS_DIR` while holding the shared test lock.
    pub(crate) fn set(path: &Path) -> Self {
        Self(ScopedEnvVar::set("ANNO_MODELS_DIR", path.as_os_str()))
    }

    /// Set `ANNO_MODELS_DIR` to a raw OS value while holding the shared test lock.
    pub(crate) fn set_raw(value: impl AsRef<std::ffi::OsStr>) -> Self {
        Self(ScopedEnvVar::set("ANNO_MODELS_DIR", value.as_ref()))
    }

    /// Unset `ANNO_MODELS_DIR` while holding the shared test lock.
    pub(crate) fn unset() -> Self {
        Self(ScopedEnvVar::unset("ANNO_MODELS_DIR"))
    }
}
