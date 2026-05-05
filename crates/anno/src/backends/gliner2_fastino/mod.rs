//! gliner2_fastino — fastino-ai GLiNER2 backend (issue #18).
//!
//! **Status:** experimental / WIP. No API stability guarantees in Phase 1.
//!
//! Loads `fastino/gliner2-*` ONNX models (Zaratiana et al. 2025,
//! arXiv:2507.18546). Distinct from `gliner_multitask` (which loads GLiNER v1
//! multi-task models with hardcoded `<<ENT>>=128002` IDs and rejects any
//! `fastino/*` model id at the discovery layer).
//!
//! # Architecture deltas vs `gliner_multitask`
//!
//! - Special-token vocabulary: `[P]`, `[E]`, `[C]`, `[L]`, `[R]`,
//!   `[SEP_STRUCT]`, `[SEP_TEXT]`. IDs read from `tokenizer.json` at load
//!   time; never hardcoded.
//! - Prompt format: `( [P] task_name ( [E] label1 [E] label2 ) ) [SEP_TEXT] tokens...`
//! - Span scoring: dot-product similarity (Eq. 1 of arXiv:2507.18546).
//!
//! # LoRA
//!
//! Phase 1 does **not** support runtime LoRA adapter loading. To use a
//! LoRA-fine-tuned model, merge the adapter into the base weights and
//! re-export to ONNX:
//!
//! ```bash
//! python scripts/gliner2_export_onnx.py \
//!     --base fastino/gliner2-multi-v1 \
//!     --lora-adapter ./my_adapter \
//!     --output ./my_merged.onnx
//! ```
//!
//! Pointing `from_local` at a directory containing `adapter_config.json`
//! returns [`errors::Error::LoraAdapterNotSupported`].
//!
//! # Source attribution
//!
//! `processor.rs` is adapted from SemplificaAI/gliner2-rs (Apache-2.0):
//! <https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/processor.rs>

#![cfg(feature = "gliner2-fastino")]

pub mod errors;
pub(crate) mod config;
pub(crate) mod decoder;
pub(crate) mod processor;
pub(crate) mod sessions;

/// fastino-ai GLiNER2 model.
///
/// **Experimental.** API may change without semver bump.
pub struct GLiNER2Fastino {
    pub(crate) tokenizer: tokenizers::Tokenizer,
    pub(crate) special: processor::SpecialTokenIds,
    pub(crate) transformer: processor::SchemaTransformer,
    pub(crate) config: config::FastinoConfig,
    pub(crate) sessions: sessions::Sessions,
    pub(crate) model_id: String,
}

impl std::fmt::Debug for GLiNER2Fastino {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GLiNER2Fastino")
            .field("model_id", &self.model_id)
            .field("hidden_size", &self.config.hidden_size)
            .finish()
    }
}

use std::path::Path;

impl GLiNER2Fastino {
    /// Load a fastino GLiNER2 model from a local directory.
    pub fn from_local(model_dir: &Path) -> crate::Result<Self> {
        if model_dir.join("adapter_config.json").exists() {
            return Err(errors::Error::LoraAdapterNotSupported {
                path: model_dir.to_path_buf(),
            }
            .into());
        }

        let tokenizer_path = model_dir.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(errors::Error::TokenizerMissing(tokenizer_path).into());
        }
        let tokenizer = crate::backends::hf_loader::load_tokenizer(&tokenizer_path)
            .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: tokenizer: {e}")))?;

        let special = processor::SpecialTokenIds::resolve(&tokenizer)?;
        let transformer = processor::SchemaTransformer::new(tokenizer.clone())?;
        let config = config::FastinoConfig::from_path(&model_dir.join("config.json"))?;

        let sessions = sessions::Sessions::from_dir(model_dir)?;

        Ok(Self {
            tokenizer,
            special,
            transformer,
            config,
            sessions,
            model_id: model_dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "gliner2_fastino_local".to_string()),
        })
    }

    /// Extract entities for the given labels at the given threshold.
    ///
    /// **Phase 3 stub.** Real pipeline implementation pending M5–M11.
    pub(crate) fn extract_ner(
        &self,
        text: &str,
        types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<crate::Entity>> {
        if types.is_empty() {
            return Ok(vec![]);
        }
        let _ = (text, types, threshold, &self.sessions);
        Err(crate::Error::Backend(
            "gliner2_fastino: extract_ner is being rewritten in Phase 3 \
             (multi-session pipeline) — not yet wired".into(),
        ))
    }

    /// Load a fastino GLiNER2 model by Hugging Face model id.
    ///
    /// Downloads `tokenizer.json`, `config.json`, and the ONNX model file
    /// (trying `onnx/model.onnx` then `model.onnx`) into the standard HF
    /// cache, then defers to `from_local` on the cache snapshot directory.
    ///
    /// **Phase 1 / experimental.** No retry/backoff on transient HF Hub
    /// failures beyond what `hf-hub` itself provides.
    pub fn from_pretrained(model_id: &str) -> crate::Result<Self> {
        let api = crate::backends::hf_loader::hf_api()
            .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: hf_api: {e}")))?;
        let repo = api.model(model_id.to_string());

        let _model_path = crate::backends::hf_loader::download_model_file(
            &repo,
            &["onnx/model.onnx", "model.onnx"],
        )
        .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: download model: {e}")))?;
        let tokenizer_path =
            crate::backends::hf_loader::download_model_file(&repo, &["tokenizer.json"])
                .map_err(|e| {
                    crate::Error::Backend(format!("gliner2_fastino: download tokenizer: {e}"))
                })?;
        let _config_path =
            crate::backends::hf_loader::download_model_file(&repo, &["config.json"])
                .map_err(|e| {
                    crate::Error::Backend(format!("gliner2_fastino: download config: {e}"))
                })?;

        // hf_loader::download_model_file returns paths in the HF cache. Their
        // common parent is the snapshot dir.
        let snapshot_dir = tokenizer_path.parent().ok_or_else(|| {
            crate::Error::Backend("gliner2_fastino: tokenizer parent missing".into())
        })?;
        let mut model = Self::from_local(snapshot_dir)?;
        model.model_id = model_id.to_string();
        Ok(model)
    }
}

use crate::backends::inference::ZeroShotNER;
use crate::{EntityCategory, EntityType, Language};

impl crate::Model for GLiNER2Fastino {
    fn extract_entities(
        &self,
        text: &str,
        _language: Option<Language>,
    ) -> crate::Result<Vec<crate::Entity>> {
        self.extract_ner(text, &["person", "organization", "location", "date"], 0.5)
    }

    fn supported_types(&self) -> Vec<EntityType> {
        vec![
            EntityType::Person,
            EntityType::Organization,
            EntityType::Location,
            EntityType::Date,
            EntityType::custom("misc", EntityCategory::Misc),
        ]
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "GLiNER2Fastino"
    }

    fn description(&self) -> &'static str {
        "fastino-ai GLiNER2 (NER + classification, ONNX, experimental)"
    }

    fn capabilities(&self) -> crate::ModelCapabilities {
        crate::ModelCapabilities {
            zero_shot: true,
            ..Default::default()
        }
    }

    fn as_zero_shot(&self) -> Option<&dyn ZeroShotNER> {
        Some(self)
    }
}

impl ZeroShotNER for GLiNER2Fastino {
    fn default_types(&self) -> &[&'static str] {
        &["person", "organization", "location", "date", "event"]
    }

    fn extract_with_types(
        &self,
        text: &str,
        types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<crate::Entity>> {
        self.extract_ner(text, types, threshold)
    }

    fn extract_with_descriptions(
        &self,
        text: &str,
        descriptions: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<crate::Entity>> {
        // Phase 1: descriptions are treated as simple type labels
        self.extract_ner(text, descriptions, threshold)
    }
}

impl GLiNER2Fastino {
    /// Internal classification.
    ///
    /// **Phase 1 caveat:** this implementation reuses the NER head over the
    /// classification labels and collapses span-level scores to label-level
    /// (max over spans). The fastino architecture's dedicated `[L]`-head MLP
    /// is not yet wired (tracked as a Phase 1.5 follow-up). For coarse-grained
    /// classification tasks the approximation is adequate; for fine-grained
    /// or multi-label tasks expect lower fidelity than the Python reference.
    ///
    /// Not behind a public trait — see spec §3.
    pub fn classify(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<(String, f32)>> {
        if labels.is_empty() {
            return Ok(vec![]);
        }
        let entities = self.extract_ner(text, labels, threshold)?;
        let mut by_label: std::collections::HashMap<String, f32> = Default::default();
        for e in entities {
            // entity_type is a public field; format!("{:?}", ...) gives the
            // variant name. Lowercase for label-string matching.
            let label = format!("{:?}", e.entity_type).to_lowercase();
            let prev = by_label.get(&label).copied().unwrap_or(0.0);
            // Confidence has From<Confidence> for f32 impl
            let score: f32 = f32::from(e.confidence);
            by_label.insert(label, prev.max(score));
        }
        let mut out: Vec<(String, f32)> = labels
            .iter()
            .map(|&l| (l.to_string(), by_label.get(l).copied().unwrap_or(0.0)))
            .collect();
        out.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(out)
    }
}

#[cfg(test)]
mod from_local_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn from_local_rejects_lora_adapter_dir() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("adapter_config.json"), "{}").unwrap();

        let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("scripts/gliner2_export_onnx.py"), "missing script path: {msg}");
        assert!(msg.contains("--lora-adapter"), "missing flag: {msg}");
    }

    #[test]
    fn from_local_missing_tokenizer_returns_typed_error() {
        let dir = tempdir().unwrap();
        // Empty directory — no tokenizer.json, no adapter_config.json.
        let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
        assert!(err.to_string().contains("tokenizer"), "got {err}");
    }
}
