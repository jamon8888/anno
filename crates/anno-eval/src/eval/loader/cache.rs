//! Cache-content helpers: download size limits and checksums.
use anno::Result;

// Default download cap when `ANNO_MAX_DOWNLOAD_BYTES` is unset.
//
// Rationale: unset should be *usable* but *safe* by default. This cap is meant to prevent
// accidental multi-GB downloads while still allowing many evaluation datasets.
#[cfg(feature = "eval")]
const DEFAULT_MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024; // 50 MiB

#[cfg(feature = "eval")]
pub(crate) fn max_download_bytes() -> Option<u64> {
    match std::env::var("ANNO_MAX_DOWNLOAD_BYTES").ok() {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() {
                return Some(DEFAULT_MAX_DOWNLOAD_BYTES);
            }
            let Ok(v) = s.parse::<u64>() else {
                return Some(DEFAULT_MAX_DOWNLOAD_BYTES);
            };
            if v == 0 {
                None // explicit opt-out
            } else {
                Some(v)
            }
        }
        None => Some(DEFAULT_MAX_DOWNLOAD_BYTES),
    }
}

#[cfg(feature = "eval")]
pub(crate) fn enforce_max_download_bytes(content_len: usize, source: &str) -> Result<()> {
    let Some(limit) = max_download_bytes() else {
        return Ok(());
    };
    let len = content_len as u64;
    if len > limit {
        return Err(anno::Error::InvalidInput(format!(
            "Download rejected ({} bytes > ANNO_MAX_DOWNLOAD_BYTES={} bytes) from {}",
            len, limit, source
        )));
    }
    Ok(())
}

/// Compute SHA256 checksum of content.
#[cfg(feature = "eval")]
pub(crate) fn compute_sha256(content: &str) -> String {
    #[cfg(feature = "eval")]
    {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    #[cfg(not(feature = "eval"))]
    {
        // Fallback if sha2 not available
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}
