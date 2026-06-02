//! Narrow privacy entrypoint for the knowledge indexer.
//!
//! Reuses the existing detector + vault to pseudonymize knowledge-object text,
//! title, and metadata into chunks. No embedding, no legal enrichment, no
//! LanceDB. IDs are passed through as opaque strings so this module does not
//! depend on the knowledge ID newtypes.

use crate::pipeline::Pipeline;
use crate::Result;
use std::collections::HashMap;

/// One extracted (pre-pseudonymization) chunk.
#[derive(Debug, Clone)]
pub struct ExtractedChunkInput {
    /// Chunk index.
    pub idx: u32,
    /// Raw chunk text.
    pub text: String,
    /// Char start offset.
    pub char_start: u32,
    /// Char end offset.
    pub char_end: u32,
}

/// Input to pseudonymize one knowledge object.
#[derive(Debug, Clone)]
pub struct PrivacyIndexInput {
    /// Opaque object id string.
    pub object_id: String,
    /// Opaque revision id string.
    pub revision_id: String,
    /// Opaque part id string.
    pub part_id: String,
    /// Raw title (file name).
    pub title_raw: Option<String>,
    /// Raw metadata JSON.
    pub metadata_raw: serde_json::Value,
    /// Extracted chunks.
    pub chunks: Vec<ExtractedChunkInput>,
}

/// One pseudonymized chunk produced by the privacy API.
#[derive(Debug, Clone)]
pub struct PseudonymizedChunk {
    /// Chunk index.
    pub chunk_idx: u32,
    /// Pseudonymized title (same for all chunks of the object).
    pub title_pseudo: Option<String>,
    /// Pseudonymized chunk body.
    pub text_pseudo: String,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: String,
    /// Char start offset.
    pub char_start: u32,
    /// Char end offset.
    pub char_end: u32,
}

impl Pipeline {
    /// Pseudonymize a knowledge object's chunks, title, and metadata.
    ///
    /// Uses the PII subset only (legal labels are not requested). Loads the NER
    /// detector on demand. Does not embed.
    ///
    /// # Errors
    /// Returns detector or vault errors.
    pub async fn pseudonymize_knowledge_object(
        &self,
        input: PrivacyIndexInput,
    ) -> Result<Vec<PseudonymizedChunk>> {
        let detector = self.detector_get_or_init()?;
        let no_legal: Vec<crate::legal::LegalLabel> = Vec::new();
        let no_thresholds: HashMap<&'static str, f32> = HashMap::new();

        // Title: pseudonymize the file name.
        let title_pseudo = match &input.title_raw {
            Some(t) if !t.is_empty() => {
                let bundle = detector.detect_for_ingest(t, &no_legal, &no_thresholds)?;
                let (p, _map) = self.vault.pseudonymize_with_map(t, &bundle.pii).await?;
                Some(p)
            }
            _ => None,
        };

        // Metadata: pseudonymize the serialized JSON string.
        let metadata_raw_str = serde_json::to_string(&input.metadata_raw)
            .unwrap_or_else(|_| "{}".to_string());
        let metadata_pseudo_json = {
            let bundle =
                detector.detect_for_ingest(&metadata_raw_str, &no_legal, &no_thresholds)?;
            let (p, _map) = self
                .vault
                .pseudonymize_with_map(&metadata_raw_str, &bundle.pii)
                .await?;
            p
        };

        let mut out = Vec::with_capacity(input.chunks.len());
        for chunk in &input.chunks {
            let bundle =
                detector.detect_for_ingest(&chunk.text, &no_legal, &no_thresholds)?;
            let (text_pseudo, _map) =
                self.vault.pseudonymize_with_map(&chunk.text, &bundle.pii).await?;
            out.push(PseudonymizedChunk {
                chunk_idx: chunk.idx,
                title_pseudo: title_pseudo.clone(),
                text_pseudo,
                metadata_pseudo_json: metadata_pseudo_json.clone(),
                char_start: chunk.char_start,
                char_end: chunk.char_end,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AnnoRagConfig;

    fn models_present(cfg: &AnnoRagConfig) -> bool {
        cfg.models_cache().exists()
    }

    #[tokio::test]
    async fn pseudonymizes_chunks_title_and_metadata() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        if !models_present(&cfg) {
            eprintln!(
                "skipping: no models dir at {}",
                cfg.models_cache().display()
            );
            return;
        }
        let pipeline = match Pipeline::new(cfg, [0u8; 32]).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping: pipeline unavailable: {e}");
                return;
            }
        };

        let input = PrivacyIndexInput {
            object_id: "obj-1".to_string(),
            revision_id: "rev-1".to_string(),
            part_id: "part-1".to_string(),
            title_raw: Some("Contrat Dupont.pdf".to_string()),
            metadata_raw: serde_json::json!({"path": "C:/clients/Dupont/contrat.pdf"}),
            chunks: vec![ExtractedChunkInput {
                idx: 0,
                text: "Le contrat de Jean Dupont prévoit 5000 euros.".to_string(),
                char_start: 0,
                char_end: 45,
            }],
        };

        let out = pipeline
            .pseudonymize_knowledge_object(input)
            .await
            .expect("pseudo");
        assert_eq!(out.len(), 1);
        assert!(!out[0].text_pseudo.contains("Dupont"));
        assert!(out[0]
            .title_pseudo
            .as_deref()
            .map(|t| !t.contains("Dupont"))
            .unwrap_or(true));
        assert!(!out[0].metadata_pseudo_json.contains("Dupont"));
    }
}
