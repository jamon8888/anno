//! Persisted last-known warmup durations, used to estimate ETA on next start.
//! Stored as a tiny JSON beside the models dir. Durations are not sensitive.
//! Spec C §12 (D3).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct WarmupHistory {
    pub(crate) download_ms: Option<u64>,
    pub(crate) load_ms: Option<u64>,
}

fn history_path(models_dir: &Path) -> PathBuf {
    models_dir.join("warmup_history.json")
}

pub(crate) fn load(models_dir: &Path) -> WarmupHistory {
    std::fs::read_to_string(history_path(models_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save(models_dir: &Path, h: &WarmupHistory) {
    if let Ok(s) = serde_json::to_string(h) {
        let _ = std::fs::write(history_path(models_dir), s);
    }
}

/// Remaining seconds in the current phase, or `None` with no history.
pub(crate) fn eta_seconds(last_phase_ms: Option<u64>, elapsed_ms: u64) -> Option<u64> {
    last_phase_ms.map(|total| total.saturating_sub(elapsed_ms) / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_eta() {
        let tmp = tempfile::tempdir().unwrap(); // unique per-test, auto-cleaned
        let dir = tmp.path();
        let h = WarmupHistory {
            download_ms: Some(600_000),
            load_ms: Some(90_000),
        };
        save(dir, &h);
        assert_eq!(load(dir), h);
        assert_eq!(eta_seconds(Some(600_000), 60_000), Some(540));
        assert_eq!(eta_seconds(None, 1_000), None);
    }
}
