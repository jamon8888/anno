//! Ingest documents via kreuzberg.
//!
//! v0.1: extract content + chunks (markdown-aware chunker, FR-language Tesseract OCR).

use crate::config::{AnnoRagConfig, OcrMode};
use crate::error::{Error, Result};
#[cfg(feature = "embedded-ocr")]
use kreuzberg::core::config::OcrConfig;
use kreuzberg::core::config::{
    ChunkerType, ChunkingConfig, ContentFilterConfig, ExtractionConfig, HierarchyConfig,
    PageConfig, PdfConfig,
};
use kreuzberg::types::{Chunk, ExtractionResult, PageContent};
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
    /// Classification produced by the native no-OCR extraction pass.
    pub class: DocClass,
    /// Whether OCR was unnecessary, completed, or deferred.
    pub ocr_status: OcrStatus,
}

/// Native document classification used to gate OCR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocClass {
    /// Document has usable native text.
    TextLayer,
    /// PDF has no usable native text and needs full-document OCR.
    ScannedPdf,
    /// PDF has some usable native pages and some scanned/weak pages.
    MixedPdf {
        /// 1-indexed pages that should be OCR'd.
        ocr_pages: Vec<usize>,
    },
    /// Empty or unsupported non-PDF content.
    Empty,
}

impl DocClass {
    fn requires_ocr(&self) -> bool {
        matches!(self, Self::ScannedPdf | Self::MixedPdf { .. })
    }
}

/// OCR outcome for an extracted document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrStatus {
    /// OCR was not needed.
    NotRequired,
    /// OCR ran through the embedded Kreuzberg path.
    CompletedEmbedded,
    /// OCR is needed but was intentionally not run.
    Deferred(OcrDeferredReason),
}

impl OcrStatus {
    /// Returns true when the caller should skip indexing this document for now.
    #[must_use]
    pub fn is_deferred(&self) -> bool {
        matches!(self, Self::Deferred(_))
    }
}

/// Reason OCR work was deferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrDeferredReason {
    /// Runtime config disabled OCR.
    Disabled,
    /// This binary was not built with the `embedded-ocr` feature.
    Unavailable,
}

/// One heading from Kreuzberg's markdown chunk heading context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedHeading {
    /// Heading depth where 1 is the top-level heading.
    pub level: u8,
    /// Heading text.
    pub text: String,
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
    /// PDF page number if known (1-indexed, last page the chunk spans).
    pub page_end: Option<u32>,
    /// Markdown heading hierarchy enclosing this chunk.
    pub heading_context: Vec<ExtractedHeading>,
}

/// Extract a single document file into an [`ExtractedDoc`].
///
/// Uses kreuzberg's async `extract_file` with the markdown chunker and chunk sizing from
/// [`AnnoRagConfig::chunk_max_chars`] / [`AnnoRagConfig::chunk_overlap`].
/// OCR is gated: a native no-OCR pass runs first, and embedded OCR is only
/// attempted for scanned PDFs/pages when runtime config allows it.
///
/// # Errors
///
/// Returns [`Error::Ingest`] wrapping the underlying kreuzberg error if
/// extraction fails (missing file, unsupported format, OCR error, etc.).
pub async fn extract(path: &Path, cfg: &AnnoRagConfig) -> Result<ExtractedDoc> {
    let native_config = native_extraction_config(cfg);

    let result = kreuzberg::extract_file(path, None, &native_config)
        .await
        .map_err(|e| Error::Ingest {
            path: path.display().to_string(),
            source: Box::new(e),
        })?;

    let is_pdf = is_pdf(path);
    let class = classify_document(is_pdf, &result.content, result.pages.as_deref());

    if class.requires_ocr() {
        tracing::debug!(
            path = %path.display(),
            class = ?class,
            content_len = result.content.len(),
            chunks = result.chunks.as_ref().map(|c| c.len()).unwrap_or(0),
            "document needs OCR"
        );

        return match cfg.effective_ocr_mode() {
            OcrMode::Off => {
                tracing::info!(
                    path = %path.display(),
                    "deferring: OCR disabled at runtime"
                );
                Ok(extracted_doc(
                    path,
                    result,
                    class,
                    OcrStatus::Deferred(OcrDeferredReason::Disabled),
                ))
            }
            OcrMode::AutoEmbedded => match embedded_ocr_extract(path, cfg, &class).await? {
                Some(ocr_result) => {
                    let ocr_chunks_len = ocr_result.chunks.as_ref().map(|c| c.len()).unwrap_or(0);
                    let native_chunks_len = result.chunks.as_ref().map(|c| c.len()).unwrap_or(0);

                    // Use OCR result if it produced chunks, otherwise fall back to native
                    if ocr_chunks_len > 0 {
                        tracing::info!(
                            path = %path.display(),
                            ocr_chunks = ocr_chunks_len,
                            native_chunks = native_chunks_len,
                            "completed embedded OCR"
                        );
                        Ok(extracted_doc(
                            path,
                            ocr_result,
                            class,
                            OcrStatus::CompletedEmbedded,
                        ))
                    } else if native_chunks_len > 0 {
                        tracing::info!(
                            path = %path.display(),
                            "OCR produced no chunks, falling back to native extraction"
                        );
                        Ok(extracted_doc(
                            path,
                            result,
                            class,
                            OcrStatus::NotRequired, // OCR wasn't needed after all
                        ))
                    } else {
                        tracing::warn!(
                            path = %path.display(),
                            "neither OCR nor native extraction produced chunks"
                        );
                        Ok(extracted_doc(
                            path,
                            result,
                            class,
                            OcrStatus::Deferred(OcrDeferredReason::Unavailable),
                        ))
                    }
                }
                None => {
                    tracing::warn!(
                        path = %path.display(),
                        "deferring: embedded OCR unavailable (feature not compiled or OCR failed)"
                    );
                    Ok(extracted_doc(
                        path,
                        result,
                        class,
                        OcrStatus::Deferred(OcrDeferredReason::Unavailable),
                    ))
                }
            },
        };
    }

    Ok(extracted_doc(path, result, class, OcrStatus::NotRequired))
}

fn extracted_doc(
    path: &Path,
    result: ExtractionResult,
    class: DocClass,
    ocr_status: OcrStatus,
) -> ExtractedDoc {
    ExtractedDoc {
        source_path: path.display().to_string(),
        content: result.content,
        chunks: map_chunks(result.chunks.unwrap_or_default()),
        class,
        ocr_status,
    }
}

fn chunking_config(cfg: &AnnoRagConfig) -> ChunkingConfig {
    ChunkingConfig {
        max_characters: cfg.chunk_max_chars,
        overlap: cfg.chunk_overlap,
        chunker_type: ChunkerType::Markdown,
        ..Default::default()
    }
}

fn native_extraction_config(cfg: &AnnoRagConfig) -> ExtractionConfig {
    let mut extraction_config = ExtractionConfig {
        chunking: Some(chunking_config(cfg)),
        disable_ocr: true,
        ..Default::default()
    };

    if cfg.advanced_pdf_native.is_enabled() {
        extraction_config.output_format = kreuzberg::core::config::OutputFormat::Markdown;
        extraction_config.result_format = kreuzberg::types::OutputFormat::ElementBased;
        extraction_config.include_document_structure = true;
        extraction_config.pages = Some(PageConfig {
            extract_pages: true,
            insert_page_markers: false,
            ..Default::default()
        });
        extraction_config.pdf_options = Some(PdfConfig {
            extract_images: false,
            extract_metadata: true,
            hierarchy: Some(HierarchyConfig {
                enabled: true,
                k_clusters: cfg.effective_pdf_hierarchy_clusters(),
                include_bbox: true,
                ocr_coverage_threshold: None,
            }),
            extract_annotations: cfg.pdf_extract_annotations,
            allow_single_column_tables: cfg.pdf_allow_single_column_tables,
            ..Default::default()
        });
        extraction_config.content_filter = Some(ContentFilterConfig {
            include_headers: cfg.pdf_keep_headers,
            include_footers: cfg.pdf_keep_footers,
            strip_repeating_text: true,
            include_watermarks: false,
        });
    }

    extraction_config
}

fn map_chunks(chunks: Vec<Chunk>) -> Vec<ExtractedChunk> {
    chunks
        .into_iter()
        .map(|c| ExtractedChunk {
            idx: c.metadata.chunk_index as u32,
            text: c.content,
            char_start: c.metadata.byte_start as u32,
            char_end: c.metadata.byte_end as u32,
            page: c.metadata.first_page.map(|p| p as u32),
            page_end: c.metadata.last_page.map(|p| p as u32),
            heading_context: c
                .metadata
                .heading_context
                .map(|ctx| {
                    ctx.headings
                        .into_iter()
                        .map(|h| ExtractedHeading {
                            level: h.level,
                            text: h.text,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

fn classify_document(is_pdf: bool, content: &str, pages: Option<&[PageContent]>) -> DocClass {
    if !is_pdf {
        return if content.trim().is_empty() {
            DocClass::Empty
        } else {
            DocClass::TextLayer
        };
    }

    if let Some(pages) = pages.filter(|pages| !pages.is_empty()) {
        let ocr_pages: Vec<usize> = pages
            .iter()
            .filter(|page| page_needs_ocr(page))
            .map(|page| page.page_number)
            .collect();

        if ocr_pages.is_empty() {
            return DocClass::TextLayer;
        }
        if ocr_pages.len() == pages.len() || content.trim().is_empty() {
            return DocClass::ScannedPdf;
        }
        return DocClass::MixedPdf { ocr_pages };
    }

    if content.trim().is_empty() {
        DocClass::ScannedPdf
    } else {
        DocClass::TextLayer
    }
}

fn page_needs_ocr(page: &PageContent) -> bool {
    if page.is_blank.unwrap_or(false) {
        return true;
    }

    let text = page.content.trim();
    let non_whitespace = text.chars().filter(|c| !c.is_whitespace()).count();

    // Only trigger OCR for pages with very minimal text (< 5 chars)
    // Most PDFs with native text will have more than this
    if non_whitespace < 5 {
        return true;
    }

    let total = text.chars().count();
    if total == 0 {
        return true;
    }

    let alnum_or_ws = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count();
    let alnum_ws_ratio = alnum_or_ws as f64 / total as f64;
    // Only trigger OCR if ratio is very low (< 0.2) - high symbol/punctuation content
    alnum_ws_ratio < 0.2
}

#[cfg(feature = "embedded-ocr")]
async fn embedded_ocr_extract(
    path: &Path,
    cfg: &AnnoRagConfig,
    class: &DocClass,
) -> Result<Option<ExtractionResult>> {
    let mut extraction_config = ExtractionConfig {
        chunking: Some(chunking_config(cfg)),
        use_cache: cfg.ocr_cache_enabled,
        ocr: Some(OcrConfig {
            backend: "tesseract".to_string(),
            language: "fra+eng".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    match class {
        DocClass::ScannedPdf => {
            extraction_config.force_ocr = true;
        }
        DocClass::MixedPdf { ocr_pages } => {
            extraction_config.force_ocr_pages = Some(ocr_pages.clone());
        }
        DocClass::TextLayer | DocClass::Empty => return Ok(None),
    }

    tracing::debug!(
        path = %path.display(),
        cache_enabled = cfg.ocr_cache_enabled,
        "OCR extraction starting"
    );

    kreuzberg::extract_file(path, None, &extraction_config)
        .await
        .map(Some)
        .map_err(|e| Error::Ingest {
            path: path.display().to_string(),
            source: Box::new(e),
        })
}

#[cfg(not(feature = "embedded-ocr"))]
async fn embedded_ocr_extract(
    _path: &Path,
    _cfg: &AnnoRagConfig,
    _class: &DocClass,
) -> Result<Option<ExtractionResult>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AdvancedPdfNativeMode;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    #[test]
    fn native_extraction_config_keeps_current_defaults_when_advanced_pdf_off() {
        let cfg = AnnoRagConfig::default();

        let extraction_config = native_extraction_config(&cfg);

        assert!(extraction_config.disable_ocr);
        assert!(extraction_config.chunking.is_some());
        assert!(extraction_config.pages.is_none());
        assert!(extraction_config.pdf_options.is_none());
        assert!(extraction_config.content_filter.is_none());
        assert!(!extraction_config.include_document_structure);
        assert_eq!(
            extraction_config.output_format,
            kreuzberg::core::config::OutputFormat::Plain
        );
        assert_eq!(
            extraction_config.result_format,
            kreuzberg::types::OutputFormat::Unified
        );
    }

    #[test]
    fn native_extraction_config_enables_structured_pdf_profile() {
        let cfg = AnnoRagConfig {
            advanced_pdf_native: AdvancedPdfNativeMode::Structured,
            pdf_keep_headers: true,
            pdf_extract_annotations: true,
            pdf_allow_single_column_tables: true,
            pdf_hierarchy_clusters: 4,
            ..Default::default()
        };

        let extraction_config = native_extraction_config(&cfg);

        assert!(extraction_config.disable_ocr);
        assert!(extraction_config.include_document_structure);
        assert_eq!(
            extraction_config.output_format,
            kreuzberg::core::config::OutputFormat::Markdown
        );
        assert_eq!(
            extraction_config.result_format,
            kreuzberg::types::OutputFormat::ElementBased
        );

        let pages = extraction_config.pages.expect("page tracking enabled");
        assert!(pages.extract_pages);
        assert!(!pages.insert_page_markers);

        let pdf = extraction_config.pdf_options.expect("pdf options enabled");
        assert!(pdf.extract_metadata);
        assert!(pdf.extract_annotations);
        assert!(pdf.allow_single_column_tables);
        let hierarchy = pdf.hierarchy.expect("hierarchy enabled");
        assert!(hierarchy.enabled);
        assert_eq!(hierarchy.k_clusters, 4);
        assert!(hierarchy.include_bbox);
        assert_eq!(hierarchy.ocr_coverage_threshold, None);

        let content_filter = extraction_config
            .content_filter
            .expect("content filter enabled");
        assert!(content_filter.include_headers);
        assert!(!content_filter.include_footers);
        assert!(content_filter.strip_repeating_text);
        assert!(!content_filter.include_watermarks);
    }

    #[test]
    fn map_chunks_preserves_page_range_and_heading_context() {
        let chunks = vec![Chunk {
            content: "Clause de résiliation".to_string(),
            chunk_type: kreuzberg::types::ChunkType::Unknown,
            embedding: None,
            metadata: kreuzberg::types::ChunkMetadata {
                byte_start: 10,
                byte_end: 31,
                token_count: None,
                chunk_index: 0,
                total_chunks: 1,
                first_page: Some(2),
                last_page: Some(3),
                heading_context: Some(kreuzberg::types::HeadingContext {
                    headings: vec![
                        kreuzberg::types::HeadingLevel {
                            level: 1,
                            text: "Contrat".to_string(),
                        },
                        kreuzberg::types::HeadingLevel {
                            level: 2,
                            text: "Résiliation".to_string(),
                        },
                    ],
                }),
            },
        }];

        let mapped = map_chunks(chunks);

        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].page, Some(2));
        assert_eq!(mapped[0].page_end, Some(3));
        assert_eq!(
            mapped[0].heading_context,
            vec![
                ExtractedHeading {
                    level: 1,
                    text: "Contrat".to_string(),
                },
                ExtractedHeading {
                    level: 2,
                    text: "Résiliation".to_string(),
                },
            ]
        );
    }

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
        assert_eq!(doc.class, DocClass::TextLayer);
        assert_eq!(doc.ocr_status, OcrStatus::NotRequired);
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

    #[test]
    fn classifies_text_layer_pdf_when_pages_are_usable() {
        let pages = vec![page(
            1,
            "Contrat de prestation avec plusieurs clauses exploitables.",
            Some(false),
        )];

        let class = classify_document(true, &pages[0].content, Some(&pages));

        assert_eq!(class, DocClass::TextLayer);
    }

    #[test]
    fn classifies_scanned_pdf_when_all_pages_are_weak() {
        let pages = vec![page(1, "", Some(true)), page(2, "   ", Some(true))];

        let class = classify_document(true, "", Some(&pages));

        assert_eq!(class, DocClass::ScannedPdf);
    }

    #[test]
    fn classifies_mixed_pdf_with_only_weak_pages_marked_for_ocr() {
        let pages = vec![
            page(
                1,
                "Contrat de prestation avec une page native exploitable.",
                Some(false),
            ),
            page(2, "", Some(true)),
            page(
                3,
                "Clause finale native lisible et suffisante.",
                Some(false),
            ),
        ];
        let content = pages
            .iter()
            .map(|p| p.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let class = classify_document(true, &content, Some(&pages));

        assert_eq!(class, DocClass::MixedPdf { ocr_pages: vec![2] });
    }

    #[test]
    fn classifies_empty_non_pdf_as_empty() {
        let class = classify_document(false, "   ", None);

        assert_eq!(class, DocClass::Empty);
    }

    fn page(page_number: usize, content: &str, is_blank: Option<bool>) -> PageContent {
        PageContent {
            page_number,
            content: content.to_string(),
            tables: Vec::<Arc<kreuzberg::types::Table>>::new(),
            images: Vec::<Arc<kreuzberg::types::ExtractedImage>>::new(),
            hierarchy: None,
            is_blank,
            layout_regions: None,
        }
    }

    // Sanity: NamedTempFile reference to ensure import remains used if tests above
    // change to use it directly.
    #[allow(dead_code)]
    fn _typecheck() {
        let _: Option<NamedTempFile> = None;
    }
}
