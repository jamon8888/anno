//! Ingest documents via kreuzberg.
//!
//! v0.1: extract content + chunks (markdown-aware chunker, FR-language Tesseract OCR).

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use kreuzberg::core::config::{ChunkerType, ChunkingConfig, ExtractionConfig};
use std::path::Path;

/// One extracted document. Carries chunk-level provenance (page + char offsets).
#[derive(Debug, Clone)]
pub struct ExtractedDoc {
    /// Source path on disk.
    pub source_path: String,
    /// Full extracted text (for diagnostics; the index stores chunks, not this).
    pub content: String,
    /// Section-aware chunks produced by kreuzberg's markdown chunker.
    pub chunks: Vec<ExtractedChunk>,
}

/// One section-aware chunk with provenance back to the source.
#[derive(Debug, Clone)]
pub struct ExtractedChunk {
    /// Index of this chunk in the document (0-based, sequential).
    pub idx: u32,
    /// Pseudonymized-pending text content of the chunk.
    pub text: String,
    /// Character offset in `ExtractedDoc::content` where this chunk starts.
    ///
    /// Note: kreuzberg reports byte offsets at UTF-8 boundaries; we forward them
    /// as `char_start`/`char_end` (byte offsets, in practice — the v0.1 index
    /// does not depend on character semantics here).
    pub char_start: u32,
    /// Character offset where it ends (exclusive). See [`Self::char_start`].
    pub char_end: u32,
    /// PDF page number if known (1-indexed, first page the chunk spans).
    pub page: Option<u32>,
}

/// Extract a single document file into an [`ExtractedDoc`].
///
/// Uses kreuzberg's async `extract_file` with the markdown chunker, Tesseract
/// OCR set to French (`"fra"`), and chunk sizing from
/// [`AnnoRagConfig::chunk_max_chars`] / [`AnnoRagConfig::chunk_overlap`].
///
/// # Errors
///
/// Returns [`Error::Ingest`] wrapping the underlying kreuzberg error if
/// extraction fails (missing file, unsupported format, OCR error, etc.).
pub async fn extract(path: &Path, cfg: &AnnoRagConfig) -> Result<ExtractedDoc> {
    let chunking = ChunkingConfig {
        max_characters: cfg.chunk_max_chars,
        overlap: cfg.chunk_overlap,
        chunker_type: ChunkerType::Markdown,
        ..Default::default()
    };

    let extraction_config = ExtractionConfig {
        chunking: Some(chunking),
        ..Default::default()
    };

    let result = kreuzberg::extract_file(path, None, &extraction_config)
        .await
        .map_err(|e| Error::Ingest {
            path: path.display().to_string(),
            source: Box::new(e),
        })?;

    let mut content = result.content;
    let mut chunks: Vec<ExtractedChunk> = result
        .chunks
        .unwrap_or_default()
        .into_iter()
        .map(|c| ExtractedChunk {
            idx: c.metadata.chunk_index as u32,
            text: c.content,
            char_start: c.metadata.byte_start as u32,
            char_end: c.metadata.byte_end as u32,
            page: c.metadata.first_page.map(|p| p as u32),
        })
        .collect();

    // System-tesseract opt-in fallback for PDFs without a text layer.
    // Bundled OCR was dropped in v0.5 (~500 MB resident); user installs system
    // tesseract and passes --enable-ocr (or sets enable_ocr=true in config).
    let is_pdf = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false);

    if cfg.enable_ocr && is_pdf && content.trim().is_empty() {
        match crate::ocr::ocr_pdf(path, cfg.tesseract_path.as_ref()).await {
            Ok(text) => {
                // Synthesize a single chunk covering the OCR'd text. This keeps
                // the indexing path simple at v0.5 — for larger scanned docs we
                // re-chunk in a later iteration.
                let end = text.len() as u32;
                chunks = vec![ExtractedChunk {
                    idx: 0,
                    text: text.clone(),
                    char_start: 0,
                    char_end: end,
                    page: None,
                }];
                content = text;
            }
            Err(e) => tracing::warn!(
                error = %e,
                path = %path.display(),
                "OCR fork failed; document text empty"
            ),
        }
    } else if is_pdf && content.trim().is_empty() && !cfg.enable_ocr {
        tracing::warn!(
            path = %path.display(),
            "PDF has no text layer; enable --enable-ocr (requires system tesseract) to OCR it"
        );
    }

    Ok(ExtractedDoc {
        source_path: path.display().to_string(),
        content,
        chunks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn extracts_plain_text_file() {
        let mut f = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        writeln!(
            f,
            "Article 1.\n\nLe présent contrat est régi par le droit français.\n\nArticle 2.\n\nLa juridiction est Paris."
        )
        .unwrap();
        f.flush().unwrap();

        let cfg = AnnoRagConfig::default();
        let doc = extract(f.path(), &cfg).await.expect("extract ok");

        assert!(doc.content.contains("Article 1"));
        assert!(doc.content.contains("droit français"));
        // Short content may produce 0 chunks (single chunk under max_characters);
        // the contract is: content is populated, and chunks list is valid (possibly empty).
        for c in &doc.chunks {
            assert!(c.char_end >= c.char_start);
        }
    }

    #[tokio::test]
    async fn extracts_markdown_file() {
        let mut f = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
        writeln!(
            f,
            "# Contrat de prestation\n\n## Article 1\n\nObjet du contrat.\n\n## Article 2\n\nDurée: 12 mois."
        )
        .unwrap();
        f.flush().unwrap();

        let cfg = AnnoRagConfig::default();
        let doc = extract(f.path(), &cfg).await.expect("extract ok");

        assert!(doc.content.contains("Contrat de prestation"));
        // Indices are sequential when chunks exist.
        for (i, c) in doc.chunks.iter().enumerate() {
            assert_eq!(c.idx as usize, i);
        }
    }

    // Sanity: NamedTempFile reference to ensure import remains used if tests above
    // change to use it directly.
    #[allow(dead_code)]
    fn _typecheck() {
        let _: Option<NamedTempFile> = None;
    }
}
