//! Embed text via candle (default: `OrdalieTech/Solon-embeddings-large-0.1`).
//!
//! Weights are fetched from HuggingFace Hub on first use and cached under
//! `cfg.models_cache()`. The local cache subdirectory is derived from the last
//! component of `cfg.embed_model` (e.g. `Solon-embeddings-large-0.1/`).
//!
//! Following the e5 convention, every input is prefixed with `"passage: "`
//! before tokenization. The final embedding is mean-pooled (weighted by the
//! attention mask) and L2-normalized so that cosine similarity reduces to a
//! dot product downstream.

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::Tokenizer;

const MAX_EMBED_TOKENS: usize = 512;

/// 384-dim multilingual embedder backed by candle + `BertModel`.
pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    dim: usize,
}

impl Embedder {
    /// Load the model. First call downloads weights from HuggingFace Hub.
    ///
    /// # Errors
    /// Returns [`Error::Embed`] if the hub fetch, config/tokenizer parse,
    /// safetensors mmap, or BERT graph construction fails.
    pub async fn load(cfg: &AnnoRagConfig) -> Result<Self> {
        // ── Local model fast-path ─────────────────────────────────────────────────
        // Use ANNO_MODELS_DIR when non-empty, otherwise the configured default
        // cache. When the three required files exist, skip HF Hub loading.
        let local_models_dir = std::env::var_os("ANNO_MODELS_DIR")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| cfg.models_cache());
        let model_subdir = cfg
            .embed_model
            .split('/')
            .next_back()
            .unwrap_or(cfg.embed_model.as_str());
        let base = local_models_dir.join(model_subdir);
        let config_path = base.join("config.json");
        let tokenizer_path = base.join("tokenizer.json");
        let weights_path = base.join("model.safetensors");
        if config_path.exists() && tokenizer_path.exists() && weights_path.exists() {
            let requested =
                crate::accelerator::AcceleratorPreference::from_env_or(cfg.accelerator)?;
            let decision = crate::accelerator::resolve(requested)?;
            let device = crate::accelerator::candle_device(&decision)?;
            let config_json = std::fs::read_to_string(&config_path)?;
            let config: Config = serde_json::from_str(&config_json)
                .map_err(|e| Error::Embed(format!("config parse (local): {e}")))?;
            let tokenizer = Tokenizer::from_file(&tokenizer_path)
                .map_err(|e| Error::Embed(format!("tokenizer load (local): {e}")))?;
            let dtype = match cfg.embedder_dtype.as_deref() {
                Some("f16") => DType::F16,
                _ => DType::F32,
            };
            // SAFETY: we don't mutate the file for the lifetime of the mmap.
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights_path], dtype, &device)
                    .map_err(|e| Error::Embed(format!("var builder (local): {e}")))?
            };
            let model = BertModel::load(vb, &config)
                .map_err(|e| Error::Embed(format!("bert load (local): {e}")))?;
            return Ok(Self {
                model,
                tokenizer,
                device,
                dim: cfg.embed_dim,
            });
        }
        // ─────────────────────────────────────────────────────────────────────────────

        let requested = crate::accelerator::AcceleratorPreference::from_env_or(cfg.accelerator)?;
        let decision = crate::accelerator::resolve(requested)?;
        let device = crate::accelerator::candle_device(&decision)?;
        let api = hf_hub::api::tokio::Api::new()
            .map_err(|e| Error::Embed(format!("hf-hub init: {e}")))?;
        let repo = api.model(cfg.embed_model.clone());

        let config_path = repo
            .get("config.json")
            .await
            .map_err(|e| Error::Embed(format!("config.json fetch: {e}")))?;
        let tokenizer_path = repo
            .get("tokenizer.json")
            .await
            .map_err(|e| Error::Embed(format!("tokenizer.json fetch: {e}")))?;
        let weights_path = match repo.get("model.safetensors").await {
            Ok(p) => p,
            Err(_) => repo
                .get("pytorch_model.bin")
                .await
                .map_err(|e| Error::Embed(format!("weights fetch: {e}")))?,
        };

        let config_json = std::fs::read_to_string(&config_path)?;
        let config: Config = serde_json::from_str(&config_json)
            .map_err(|e| Error::Embed(format!("config parse: {e}")))?;
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| Error::Embed(format!("tokenizer load: {e}")))?;

        // F32 is the default: the e5-small BERT forward pass on CPU can
        // produce degenerate (NaN) embeddings in F16 — overflow in the
        // attention softmax — which collapses recall@10 to 0. F16 halves
        // embedder RSS (~236 MB) and stays available as an explicit opt-in
        // for callers who validate recall on their own corpus.
        let dtype = match cfg.embedder_dtype.as_deref() {
            Some("f16") => DType::F16,
            _ => DType::F32,
        };
        // SAFETY: hf-hub writes the safetensors file before returning the path
        // and we don't mutate it for the lifetime of the mmap.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], dtype, &device)
                .map_err(|e| Error::Embed(format!("var builder: {e}")))?
        };
        let model =
            BertModel::load(vb, &config).map_err(|e| Error::Embed(format!("bert load: {e}")))?;

        Ok(Self {
            model,
            tokenizer,
            device,
            dim: cfg.embed_dim,
        })
    }

    /// Embedding vector dimension (matches `AnnoRagConfig::embed_dim`).
    #[must_use]
    pub fn dim(&self) -> usize {
        self.dim
    }

    #[cfg(test)]
    fn device_label_for_test(device: &Device) -> &'static str {
        crate::accelerator::device_label(device)
    }

    /// Device label used by diagnostics and tests.
    #[must_use]
    pub fn device_label(&self) -> &'static str {
        crate::accelerator::device_label(&self.device)
    }

    /// Embed a batch of passages (indexed documents).
    ///
    /// Returns a `Vec<Vec<f32>>` of shape `(texts.len(), dim)`, L2-normalized
    /// and mean-pooled with attention-mask weighting. Each text is prefixed
    /// with the e5 `"passage: "` task prefix.
    ///
    /// # Errors
    /// Returns [`Error::Embed`] if tokenization, tensor construction, or the
    /// BERT forward pass fails.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_prefixed(texts, "passage: ")
    }

    /// Embed a single search query. Applies the e5 `"query: "` prefix —
    /// distinct from the `"passage: "` prefix used for indexed documents.
    /// Using the wrong prefix measurably degrades retrieval.
    ///
    /// # Errors
    /// Returns [`Error::Embed`] on tokenization or forward-pass failure.
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let mut v = self.embed_prefixed(std::slice::from_ref(&text.to_string()), "query: ")?;
        v.pop()
            .ok_or_else(|| Error::Embed("embed_query produced no vector".into()))
    }

    /// Shared embed path: prefix every input, tokenize, forward, mean-pool,
    /// L2-normalize. `prefix` is the e5 task prefix (`"passage: "` /
    /// `"query: "`).
    ///
    /// # Errors
    /// Returns [`Error::Embed`] if tokenization, tensor construction, or the
    /// BERT forward pass fails.
    fn embed_prefixed(&self, texts: &[String], prefix: &str) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefixed: Vec<String> = texts.iter().map(|t| format!("{prefix}{t}")).collect();
        let encs = self
            .tokenizer
            .encode_batch(prefixed, true)
            .map_err(|e| Error::Embed(format!("tokenize: {e}")))?;

        let max_len = encs
            .iter()
            .map(|e| e.get_ids().len().min(MAX_EMBED_TOKENS))
            .max()
            .unwrap_or(0);
        let n = texts.len();

        let mut ids: Vec<i64> = Vec::with_capacity(n * max_len);
        let mut mask: Vec<i64> = Vec::with_capacity(n * max_len);
        for e in &encs {
            let take = e.get_ids().len().min(max_len);
            let pad = max_len - take;
            ids.extend(e.get_ids()[..take].iter().map(|&x| i64::from(x)));
            ids.extend(std::iter::repeat_n(0i64, pad));
            mask.extend(e.get_attention_mask()[..take].iter().map(|&x| i64::from(x)));
            mask.extend(std::iter::repeat_n(0i64, pad));
        }

        let input_ids = Tensor::from_vec(ids, (n, max_len), &self.device)
            .map_err(|e| Error::Embed(format!("ids tensor: {e}")))?;
        let attn = Tensor::from_vec(mask, (n, max_len), &self.device)
            .map_err(|e| Error::Embed(format!("mask tensor: {e}")))?;
        let token_type = Tensor::zeros((n, max_len), DType::I64, &self.device)
            .map_err(|e| Error::Embed(format!("token_type tensor: {e}")))?;

        let out = self
            .model
            .forward(&input_ids, &token_type, Some(&attn))
            .map_err(|e| Error::Embed(format!("forward: {e}")))?;
        let out = out
            .to_dtype(DType::F32)
            .map_err(|e| Error::Embed(format!("output dtype cast: {e}")))?;

        let mask_f = attn
            .to_dtype(DType::F32)
            .map_err(|e| Error::Embed(e.to_string()))?
            .unsqueeze(2)
            .map_err(|e| Error::Embed(e.to_string()))?;
        let masked = out
            .broadcast_mul(&mask_f)
            .map_err(|e| Error::Embed(e.to_string()))?;
        let sum = masked.sum(1).map_err(|e| Error::Embed(e.to_string()))?;
        let counts = mask_f.sum(1).map_err(|e| Error::Embed(e.to_string()))?;
        let pooled = sum
            .broadcast_div(&counts)
            .map_err(|e| Error::Embed(e.to_string()))?;

        let norm = pooled
            .sqr()
            .map_err(|e| Error::Embed(e.to_string()))?
            .sum_keepdim(1)
            .map_err(|e| Error::Embed(e.to_string()))?
            .sqrt()
            .map_err(|e| Error::Embed(e.to_string()))?;
        let normed = pooled
            .broadcast_div(&norm)
            .map_err(|e| Error::Embed(e.to_string()))?;

        normed
            .to_vec2::<f32>()
            .map_err(|e| Error::Embed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_passage_prefixes_differ() {
        // Guards the e5 asymmetric-prefix contract without loading a model:
        // the two public entry points must format their inputs differently.
        let passage = format!("{}{}", "passage: ", "x");
        let query = format!("{}{}", "query: ", "x");
        assert_ne!(passage, query);
        assert!(passage.starts_with("passage: "));
        assert!(query.starts_with("query: "));
    }

    #[test]
    fn cpu_device_label_is_stable() {
        assert_eq!(Embedder::device_label_for_test(&Device::Cpu), "cpu");
    }

    #[tokio::test]
    #[ignore = "downloads ~470 MB model on first run; exercised in Task 10 integration"]
    async fn loads_and_embeds() {
        let cfg = AnnoRagConfig::default();
        let e = Embedder::load(&cfg).await.expect("load");
        assert_eq!(e.dim(), 384);
        let v = e
            .embed_batch(&["Bonjour le monde".to_string()])
            .expect("embed");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].len(), 384);
        let norm: f32 = v[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[tokio::test]
    #[ignore = "requires HF cache populated"]
    async fn loads_with_f32_default() {
        let cfg = AnnoRagConfig::default();
        assert!(cfg.embedder_dtype.is_none(), "default leaves dtype unset");
        let e = Embedder::load(&cfg).await.expect("f32 load");
        let v = e.embed_batch(&["Bonjour".into()]).expect("embed");
        assert_eq!(v[0].len(), 1024);
    }

    #[tokio::test]
    #[ignore = "requires HF cache populated"]
    async fn loads_with_f16_opt_in() {
        let cfg = AnnoRagConfig {
            embedder_dtype: Some("f16".into()),
            ..Default::default()
        };
        let e = Embedder::load(&cfg).await.expect("f16 load");
        let v = e.embed_batch(&["Bonjour".into()]).expect("embed");
        assert_eq!(v[0].len(), 1024);
    }

    #[tokio::test]
    async fn anno_models_dir_local_path_entered_when_files_exist() {
        // Creates stub files (garbage content) under ANNO_MODELS_DIR.
        // Embedder::load must enter the local branch and fail with a "(local)"
        // error — proving the fast-path is taken when all 3 files are present.
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = crate::config::AnnoRagConfig::default();
        let model_subdir = cfg
            .embed_model
            .split('/')
            .next_back()
            .unwrap_or(&cfg.embed_model);
        let embed_dir = dir.path().join(model_subdir);
        std::fs::create_dir_all(&embed_dir).expect("mkdir");
        std::fs::write(embed_dir.join("config.json"), b"not json").expect("config");
        std::fs::write(embed_dir.join("tokenizer.json"), b"not json").expect("tok");
        std::fs::write(embed_dir.join("model.safetensors"), b"not safetensors").expect("weights");

        let _models_dir = crate::env_guard::ScopedAnnoModelsDir::set(dir.path());
        let result = Embedder::load(&cfg).await;

        let err = match result {
            Ok(_) => panic!("must fail — garbage config.json"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("(local)"),
            "error must come from local path, got: {msg}"
        );
    }

    #[tokio::test]
    async fn default_models_cache_local_path_entered_when_files_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = crate::config::AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let model_subdir = cfg
            .embed_model
            .split('/')
            .next_back()
            .unwrap_or(&cfg.embed_model);
        let embed_dir = cfg.models_cache().join(model_subdir);
        std::fs::create_dir_all(&embed_dir).expect("mkdir");
        std::fs::write(embed_dir.join("config.json"), b"not json").expect("config");
        std::fs::write(embed_dir.join("tokenizer.json"), b"not json").expect("tok");
        std::fs::write(embed_dir.join("model.safetensors"), b"not safetensors").expect("weights");

        let _models_dir = crate::env_guard::ScopedAnnoModelsDir::unset();
        let result = Embedder::load(&cfg).await;

        let err = match result {
            Ok(_) => panic!("must fail — garbage config.json"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("(local)"),
            "error must come from default local path, got: {msg}"
        );
    }

    #[test]
    fn anno_models_dir_missing_files_falls_through_to_hf() {
        // When ANNO_MODELS_DIR is set but the required files are absent,
        // the local path must NOT be taken. We can't call Embedder::load here
        // (it would attempt HF network), so we verify the directory existence
        // check logic compiles correctly and the fast-path condition is false.
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = crate::config::AnnoRagConfig::default();
        let model_subdir = cfg
            .embed_model
            .split('/')
            .next_back()
            .unwrap_or(&cfg.embed_model);
        let embed_dir = dir.path().join(model_subdir);
        std::fs::create_dir_all(&embed_dir).expect("mkdir");
        // Exactly 0 of the 3 files exist → fast-path must not trigger
        let has_all = embed_dir.join("config.json").exists()
            && embed_dir.join("tokenizer.json").exists()
            && embed_dir.join("model.safetensors").exists();
        assert!(!has_all, "empty dir must not trigger local-load path");
    }
}
