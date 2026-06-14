//! File-level model inventory for MCP model readiness.

use anno_rag::config::AnnoRagConfig;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Required E5 model files relative to the effective models directory.
///
/// # Deprecated
/// Prefer [`embedder_required_files`] — this const is retained for backward compatibility.
#[deprecated(since = "0.12.0", note = "use embedder_required_files(dir) instead")]
pub const E5_REQUIRED_FILES: &[&str] = &[
    "multilingual-e5-small/config.json",
    "multilingual-e5-small/model.safetensors",
    "multilingual-e5-small/tokenizer.json",
];

/// Generate required embedder file paths relative to the models directory.
///
/// The `dir` argument is the last segment of `embed_model`, i.e. `AnnoRagConfig::embedder_dir()`.
pub fn embedder_required_files(dir: &str) -> Vec<String> {
    vec![
        format!("{dir}/config.json"),
        format!("{dir}/model.safetensors"),
        format!("{dir}/tokenizer.json"),
    ]
}

/// Required GLiNER model files relative to the effective models directory.
#[deprecated(since = "0.12.0", note = "use gliner_onnx_required_files(dir) instead")]
pub const GLINER_REQUIRED_FILES: &[&str] = &[
    "gliner2-multi-v1-onnx/fp32_v2/classifier_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/count_lstm_fixed_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/count_pred_argmax_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/encoder_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/schema_gather_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/scorer_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/span_rep_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/token_gather_fp32.onnx",
    "gliner2-multi-v1-onnx/fp32_v2/tokenizer.json",
];

/// Required Candle GLiNER files relative to the effective models directory.
#[deprecated(
    since = "0.12.0",
    note = "use candle_gliner_required_files(dir) instead"
)]
pub const CANDLE_GLINER_REQUIRED_FILES: &[&str] = &[
    "gliner2-multi-v1-candle/tokenizer.json",
    "gliner2-multi-v1-candle/config.json",
    "gliner2-multi-v1-candle/encoder_config/config.json",
    "gliner2-multi-v1-candle/model.safetensors",
];

const GLINER_ONNX_BASES: &[&str] = &[
    "classifier",
    "count_lstm_fixed",
    "count_pred_argmax",
    "encoder",
    "schema_gather",
    "scorer",
    "span_rep",
    "token_gather",
];

/// Generate required ONNX GLiNER file paths (fp32_v2 layout) relative to the models directory.
pub fn gliner_onnx_required_files(ner_onnx_dir: &str) -> Vec<String> {
    let mut files: Vec<String> = GLINER_ONNX_BASES
        .iter()
        .map(|base| format!("{ner_onnx_dir}/fp32_v2/{base}_fp32.onnx"))
        .collect();
    files.push(format!("{ner_onnx_dir}/fp32_v2/tokenizer.json"));
    files
}

/// Generate required Candle GLiNER file paths relative to the models directory.
pub fn candle_gliner_required_files(candle_dir: &str) -> Vec<String> {
    vec![
        format!("{candle_dir}/tokenizer.json"),
        format!("{candle_dir}/config.json"),
        format!("{candle_dir}/encoder_config/config.json"),
        format!("{candle_dir}/model.safetensors"),
    ]
}

/// Effective models directory plus whether it came from `ANNO_MODELS_DIR`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveModelsDir {
    /// Effective model root used by local loaders.
    pub path: PathBuf,
    /// True when `path` came from `ANNO_MODELS_DIR`.
    pub from_env: bool,
}

/// Overall model inventory state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInventoryState {
    /// Effective model root is absent.
    Missing,
    /// A download lock is present at the effective model root.
    Downloading,
    /// The model root exists but at least one required file is missing.
    Partial,
    /// All required files are present.
    Ready,
}

impl ModelInventoryState {
    /// Return the snake_case wire value for this state.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Downloading => "downloading",
            Self::Partial => "partial",
            Self::Ready => "ready",
        }
    }
}

/// File readiness for one model family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelFamilyStatus {
    /// Human-readable model family name.
    pub name: String,
    /// Expected model family directory.
    pub path: String,
    /// Required files that were not found, relative to the model root.
    pub missing_files: Vec<String>,
    /// True when no required files are missing.
    pub ready: bool,
}

/// Full model inventory used by MCP status and readiness checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelInventory {
    /// Effective model root used by local loaders.
    pub path: String,
    /// True when `path` came from `ANNO_MODELS_DIR`.
    pub from_env: bool,
    /// Overall inventory state.
    pub state: ModelInventoryState,
    /// True when all model families are ready.
    pub ready: bool,
    /// True when `.download-lock` exists under `path`.
    pub downloading: bool,
    /// E5 embedder file readiness.
    pub e5: ModelFamilyStatus,
    /// Detector backend selected for GLiNER readiness.
    pub detector_backend: String,
    /// Active GLiNER detector file readiness.
    pub gliner: ModelFamilyStatus,
}

/// Inspects the effective model directory without initializing model loaders.
#[derive(Debug, Clone)]
pub struct ModelInventoryService {
    effective: EffectiveModelsDir,
    detector_kind: DetectorInventoryKind,
    embedder_dir: String,
    ner_onnx_dir: String,
    ner_candle_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectorInventoryKind {
    Onnx,
    Candle,
}

impl ModelInventoryService {
    /// Create a model inventory service for the effective loader path.
    #[must_use]
    pub fn new(cfg: &AnnoRagConfig) -> Self {
        Self {
            effective: effective_models_dir(cfg),
            detector_kind: detector_inventory_kind(cfg),
            embedder_dir: cfg.embedder_dir(),
            ner_onnx_dir: cfg.ner_onnx_dir(),
            ner_candle_dir: cfg.ner_candle_dir(),
        }
    }

    /// Inspect required model files.
    #[must_use]
    pub fn inspect(&self) -> ModelInventory {
        let path = self.effective.path.clone();
        let downloading = path.join(".download-lock").exists();
        let embedder_files = embedder_required_files(&self.embedder_dir);
        let embedder_file_refs: Vec<&str> = embedder_files.iter().map(String::as_str).collect();
        let e5 = inspect_family(&path, &self.embedder_dir, &embedder_file_refs);
        let gliner = match self.detector_kind {
            DetectorInventoryKind::Onnx => inspect_onnx_gliner_family(&path, &self.ner_onnx_dir),
            DetectorInventoryKind::Candle => {
                let required = candle_gliner_required_files(&self.ner_candle_dir);
                let refs: Vec<&str> = required.iter().map(String::as_str).collect();
                inspect_family(&path, &self.ner_candle_dir, &refs)
            }
        };
        let ready = e5.ready && gliner.ready;
        let state = if ready {
            ModelInventoryState::Ready
        } else if downloading {
            ModelInventoryState::Downloading
        } else if path.exists() {
            ModelInventoryState::Partial
        } else {
            ModelInventoryState::Missing
        };

        ModelInventory {
            path: path.display().to_string(),
            from_env: self.effective.from_env,
            state,
            ready,
            downloading,
            e5,
            detector_backend: match self.detector_kind {
                DetectorInventoryKind::Onnx => "onnx".to_string(),
                DetectorInventoryKind::Candle => "candle-metal".to_string(),
            },
            gliner,
        }
    }

    /// Return true when all required model files are present.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.inspect().ready
    }
}

fn detector_inventory_kind(cfg: &AnnoRagConfig) -> DetectorInventoryKind {
    let requested = anno_rag::accelerator::AcceleratorPreference::from_env_or(cfg.accelerator)
        .unwrap_or(cfg.accelerator);
    match requested {
        anno_rag::accelerator::AcceleratorPreference::Metal => DetectorInventoryKind::Candle,
        anno_rag::accelerator::AcceleratorPreference::Auto
            if anno_rag::accelerator::compiled_accelerators().metal =>
        {
            DetectorInventoryKind::Candle
        }
        _ => DetectorInventoryKind::Onnx,
    }
}

/// Return the model root used by loaders: `ANNO_MODELS_DIR` or `cfg.models_cache()`.
#[must_use]
pub fn effective_models_dir(cfg: &AnnoRagConfig) -> EffectiveModelsDir {
    match std::env::var_os("ANNO_MODELS_DIR") {
        Some(value) if !value.is_empty() => EffectiveModelsDir {
            path: PathBuf::from(value),
            from_env: true,
        },
        _ => EffectiveModelsDir {
            path: cfg.models_cache(),
            from_env: false,
        },
    }
}

fn inspect_family(root: &Path, name: &str, required_files: &[&str]) -> ModelFamilyStatus {
    let missing_files = required_files
        .iter()
        .filter(|rel| !root.join(rel).is_file())
        .map(|rel| (*rel).to_string())
        .collect::<Vec<_>>();

    ModelFamilyStatus {
        name: name.to_string(),
        path: root.join(name).display().to_string(),
        ready: missing_files.is_empty(),
        missing_files,
    }
}

fn inspect_onnx_gliner_family(root: &Path, ner_onnx_dir: &str) -> ModelFamilyStatus {
    for (variant_dir, suffix) in [("fp32_v2", "fp32"), ("fp16_v2", "fp16")] {
        let graph_files = GLINER_ONNX_BASES
            .iter()
            .map(|base| format!("{ner_onnx_dir}/{variant_dir}/{base}_{suffix}.onnx"))
            .collect::<Vec<_>>();
        let tokenizer_candidates = [
            format!("{ner_onnx_dir}/{variant_dir}/tokenizer.json"),
            format!("{ner_onnx_dir}/tokenizer.json"),
        ];
        let graphs_ready = graph_files
            .iter()
            .all(|relative| root.join(relative).is_file());
        let tokenizer_ready = tokenizer_candidates
            .iter()
            .any(|relative| root.join(relative).is_file());
        if graphs_ready && tokenizer_ready {
            return ModelFamilyStatus {
                name: ner_onnx_dir.to_string(),
                path: root.join(ner_onnx_dir).display().to_string(),
                missing_files: Vec::new(),
                ready: true,
            };
        }
    }
    let required = gliner_onnx_required_files(ner_onnx_dir);
    let refs: Vec<&str> = required.iter().map(String::as_str).collect();
    inspect_family(root, ner_onnx_dir, &refs)
}

#[cfg(test)]
pub(crate) mod test_env {
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard};

    static ANNO_MODELS_DIR_LOCK: Mutex<()> = Mutex::new(());

    /// Scoped `ANNO_MODELS_DIR` mutation for tests.
    pub(crate) struct ScopedAnnoModelsDir {
        previous: Option<OsString>,
        _guard: MutexGuard<'static, ()>,
    }

    impl ScopedAnnoModelsDir {
        /// Set `ANNO_MODELS_DIR` to a path while holding the shared lock.
        pub(crate) fn set(path: &Path) -> Self {
            Self::set_raw(path.as_os_str())
        }

        /// Set `ANNO_MODELS_DIR` to a raw OS value while holding the shared lock.
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

        /// Unset `ANNO_MODELS_DIR` while holding the shared lock.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_rag::accelerator::AcceleratorPreference;
    use anno_rag::config::AnnoRagConfig;
    use std::path::Path;

    fn create_required_model_files(models_dir: &Path, embedder_dir: &str) {
        let mut rels: Vec<String> = embedder_required_files(embedder_dir);
        rels.extend([
            "gliner2-multi-v1-onnx/fp32_v2/classifier_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/count_lstm_fixed_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/count_pred_argmax_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/encoder_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/schema_gather_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/scorer_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/span_rep_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/token_gather_fp32.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp32_v2/tokenizer.json".to_string(),
        ]);
        for rel in &rels {
            let path = models_dir.join(rel);
            std::fs::create_dir_all(path.parent().expect("required file parent")).unwrap();
            std::fs::write(path, b"test model file").unwrap();
        }
    }

    #[test]
    fn empty_model_directories_are_not_ready() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let models_dir = tmp.path().join("models");
        let cfg = AnnoRagConfig {
            data_dir: tmp.path().to_path_buf(),
            accelerator: AcceleratorPreference::Cpu,
            ..Default::default()
        };
        std::fs::create_dir_all(models_dir.join(cfg.embedder_dir())).unwrap();
        std::fs::create_dir_all(models_dir.join("gliner2-multi-v1-onnx")).unwrap();
        let _models_env = test_env::ScopedAnnoModelsDir::unset();

        let inventory = ModelInventoryService::new(&cfg).inspect();

        assert!(!inventory.ready);
        assert_ne!(inventory.state, ModelInventoryState::Ready);
        assert!(!inventory.e5.ready);
        assert!(!inventory.gliner.ready);
        assert!(!inventory.e5.missing_files.is_empty());
        assert!(!inventory.gliner.missing_files.is_empty());
    }

    #[test]
    fn full_required_files_return_ready() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: tmp.path().to_path_buf(),
            accelerator: AcceleratorPreference::Cpu,
            ..Default::default()
        };
        create_required_model_files(&tmp.path().join("models"), &cfg.embedder_dir());
        let _models_env = test_env::ScopedAnnoModelsDir::unset();

        let inventory = ModelInventoryService::new(&cfg).inspect();

        assert!(inventory.ready);
        assert_eq!(inventory.state, ModelInventoryState::Ready);
        assert!(inventory.e5.ready);
        assert_eq!(inventory.detector_backend, "onnx");
        assert!(inventory.gliner.ready);
        assert!(inventory.e5.missing_files.is_empty());
        assert!(inventory.gliner.missing_files.is_empty());
    }

    #[test]
    fn fp16_onnx_required_files_return_ready() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let models_dir = tmp.path().join("models");
        let cfg = AnnoRagConfig {
            data_dir: tmp.path().to_path_buf(),
            accelerator: AcceleratorPreference::Cpu,
            ..Default::default()
        };
        let embedder_dir = cfg.embedder_dir();
        let mut rels: Vec<String> = embedder_required_files(&embedder_dir);
        rels.extend([
            "gliner2-multi-v1-onnx/fp16_v2/classifier_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/count_lstm_fixed_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/count_pred_argmax_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/encoder_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/schema_gather_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/scorer_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/span_rep_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/token_gather_fp16.onnx".to_string(),
            "gliner2-multi-v1-onnx/fp16_v2/tokenizer.json".to_string(),
        ]);
        for rel in &rels {
            let path = models_dir.join(rel);
            std::fs::create_dir_all(path.parent().expect("required file parent")).unwrap();
            std::fs::write(path, b"test model file").unwrap();
        }
        let _models_env = test_env::ScopedAnnoModelsDir::unset();

        let inventory = ModelInventoryService::new(&cfg).inspect();

        assert!(inventory.ready);
        assert_eq!(inventory.detector_backend, "onnx");
        assert!(inventory.gliner.ready);
    }

    #[test]
    fn effective_models_dir_prefers_anno_models_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let env_dir = tmp.path().join("env-models");
        let cfg = AnnoRagConfig {
            data_dir: tmp.path().join("data"),
            accelerator: AcceleratorPreference::Cpu,
            ..Default::default()
        };
        let _models_env = test_env::ScopedAnnoModelsDir::set(&env_dir);

        let effective = effective_models_dir(&cfg);

        assert_eq!(effective.path, env_dir);
        assert!(effective.from_env);
    }

    #[test]
    fn effective_models_dir_ignores_empty_anno_models_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: tmp.path().join("data"),
            accelerator: AcceleratorPreference::Cpu,
            ..Default::default()
        };
        let _models_env = test_env::ScopedAnnoModelsDir::set_raw("");

        let effective = effective_models_dir(&cfg);

        assert_eq!(effective.path, cfg.models_cache());
        assert!(!effective.from_env);
    }
}
