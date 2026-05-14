//! Embed text with `intfloat/multilingual-e5-small` via candle.
//!
//! 118M params, 384-dim, 100+ languages. Weights are fetched from HuggingFace
//! Hub on first use and cached under `cfg.models_cache()` (via hf-hub's own
//! cache resolution). CPU-only in v0.1; GPU support is a v0.2 opt-in feature.
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
        let device = Device::Cpu;
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

        let dtype = match cfg.embedder_dtype.as_deref() {
            Some("f32") => DType::F32,
            _ => DType::F16,
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

    /// Embed a batch of texts.
    ///
    /// Returns a `Vec<Vec<f32>>` of shape `(texts.len(), dim)`, L2-normalized
    /// and mean-pooled with attention-mask weighting. Each text is prefixed
    /// with `"passage: "` per the e5 convention.
    ///
    /// # Errors
    /// Returns [`Error::Embed`] if tokenization, tensor construction, or the
    /// BERT forward pass fails.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefixed: Vec<String> = texts.iter().map(|t| format!("passage: {t}")).collect();
        let encs = self
            .tokenizer
            .encode_batch(prefixed, true)
            .map_err(|e| Error::Embed(format!("tokenize: {e}")))?;

        let max_len = encs.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);
        let n = texts.len();

        let mut ids: Vec<i64> = Vec::with_capacity(n * max_len);
        let mut mask: Vec<i64> = Vec::with_capacity(n * max_len);
        for e in &encs {
            let len = e.get_ids().len();
            let pad = max_len - len;
            ids.extend(e.get_ids().iter().map(|&x| i64::from(x)));
            ids.extend(std::iter::repeat(0i64).take(pad));
            mask.extend(e.get_attention_mask().iter().map(|&x| i64::from(x)));
            mask.extend(std::iter::repeat(0i64).take(pad));
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
    async fn loads_with_f16_default() {
        let cfg = AnnoRagConfig::default();
        let e = Embedder::load(&cfg).await.expect("f16 load");
        let v = e.embed_batch(&["Bonjour".into()]).expect("embed");
        assert_eq!(v[0].len(), 384);
    }

    #[tokio::test]
    #[ignore = "requires HF cache populated"]
    async fn loads_with_f32_override() {
        let mut cfg = AnnoRagConfig::default();
        cfg.embedder_dtype = Some("f32".into());
        let e = Embedder::load(&cfg).await.expect("f32 load");
        let v = e.embed_batch(&["Bonjour".into()]).expect("embed");
        assert_eq!(v[0].len(), 384);
    }
}
