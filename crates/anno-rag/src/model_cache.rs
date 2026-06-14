use crate::config::AnnoRagConfig;
use std::path::Path;

/// Renames legacy basename model dirs to the full `org/model` two-level layout.
///
/// Guarded by `models_dir/.cache-v2` — no-op if the marker already exists.
/// Errors during rename are logged as warnings and do not abort startup.
pub fn migrate_legacy_cache(models_dir: &Path, cfg: &AnnoRagConfig) {
    let marker = models_dir.join(".cache-v2");
    if marker.exists() {
        return;
    }

    let migrations = [
        (cfg.ner_onnx_dir(),   last_segment(&cfg.ner_model_id).to_string()),
        (cfg.ner_candle_dir(), format!("{}-candle", last_segment(&cfg.ner_candle_model_id))),
        (cfg.embedder_dir(),   last_segment(&cfg.embed_model).to_string()),
    ];

    for (canonical_rel, legacy_rel) in &migrations {
        if canonical_rel == legacy_rel {
            continue; // model ID has no '/'; nothing to rename
        }
        let canonical = models_dir.join(canonical_rel);
        let legacy = models_dir.join(legacy_rel);
        if !canonical.exists() && legacy.exists() {
            if let Some(parent) = canonical.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("migrate_legacy_cache: create_dir_all {}: {e}", parent.display());
                    continue;
                }
            }
            match std::fs::rename(&legacy, &canonical) {
                Ok(()) => tracing::info!(
                    "migrated model cache: {} → {}",
                    legacy.display(),
                    canonical.display()
                ),
                Err(e) => tracing::warn!(
                    "migrate_legacy_cache: rename {} → {}: {e}",
                    legacy.display(),
                    canonical.display()
                ),
            }
        }
    }

    if let Err(e) = std::fs::write(&marker, b"") {
        tracing::warn!("migrate_legacy_cache: write marker {}: {e}", marker.display());
    }
}

fn last_segment(model_id: &str) -> &str {
    model_id.split('/').next_back().unwrap_or(model_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn default_cfg() -> AnnoRagConfig {
        AnnoRagConfig::default()
    }

    #[test]
    fn migrate_renames_legacy_dirs() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Create legacy basename dirs
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();
        let candle_legacy = format!("{}-candle", last_segment(&cfg.ner_candle_model_id));
        std::fs::create_dir_all(models.join(&candle_legacy)).unwrap();
        std::fs::create_dir_all(models.join(last_segment(&cfg.embed_model))).unwrap();

        migrate_legacy_cache(models, &cfg);

        // New two-level paths exist
        assert!(models.join(&cfg.ner_onnx_dir()).exists(), "onnx canonical missing");
        assert!(models.join(&cfg.ner_candle_dir()).exists(), "candle canonical missing");
        assert!(models.join(&cfg.embedder_dir()).exists(), "embedder canonical missing");

        // Legacy dirs gone
        assert!(!models.join(last_segment(&cfg.ner_model_id)).exists(), "legacy onnx still present");
        assert!(!models.join(last_segment(&cfg.embed_model)).exists(), "legacy embedder still present");

        // Marker written
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Pre-write marker
        std::fs::write(models.join(".cache-v2"), b"").unwrap();
        // Create a legacy dir that should NOT be renamed
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();

        migrate_legacy_cache(models, &cfg);

        // Legacy dir still present — migration was skipped
        assert!(models.join(last_segment(&cfg.ner_model_id)).exists());
    }

    #[test]
    fn migrate_skips_absent_legacy() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // No legacy dirs at all
        migrate_legacy_cache(models, &cfg);

        // Marker still written
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_partial_only_onnx_present() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Only ONNX legacy dir
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();

        migrate_legacy_cache(models, &cfg);

        assert!(models.join(&cfg.ner_onnx_dir()).exists(), "onnx canonical missing");
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_noop_when_canonical_already_exists() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Both canonical and legacy present — canonical wins, no clobber
        let canonical = models.join(&cfg.ner_onnx_dir());
        std::fs::create_dir_all(&canonical).unwrap();
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();

        migrate_legacy_cache(models, &cfg);

        assert!(canonical.exists());
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_noop_for_model_id_without_slash() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let mut cfg = default_cfg();
        cfg.ner_model_id = "local-model".to_string();

        // canonical == legacy for no-slash IDs — nothing to rename
        std::fs::create_dir_all(models.join("local-model")).unwrap();

        migrate_legacy_cache(models, &cfg);

        assert!(models.join("local-model").exists());
        assert!(models.join(".cache-v2").exists());
    }
}
