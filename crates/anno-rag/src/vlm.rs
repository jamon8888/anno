//! Vision-OCR trait and value types — shared across crates.
//!
//! Trait definition and domain types live here so that `anno-rag::ingest` can
//! reference them without creating a circular dependency (anno-rag-tabular
//! already depends on anno-rag). Backends (`VllmServerClient`, `LocalVlmClient`,
//! `RoutingVlmClient`) remain in `anno-rag-tabular::llm::vlm`.

#![cfg(feature = "vlm-ocr")]

use async_trait::async_trait;

/// Kreuzberg 4.8.0's exact VLM_OCR_TEMPLATE wording (MIT reference), with
/// hardcoded French language hint. No minijinja dependency.
pub const VLM_OCR_PROMPT_FR: &str = "\
Extract all visible text from this image. \
Reproduce the text exactly as it appears, preserving the original structure, \
paragraph breaks, and reading order. \
Do not add any commentary, explanation, or formatting beyond what is present in the image. \
If the image contains no text, respond with an empty string.\n\n\
The document is in language: fra";

/// Decoded page image + provenance for audit attribution.
#[derive(Debug, Clone)]
pub struct PageImage {
    /// Raw image bytes (PNG or JPEG — caller encodes from the source doc).
    pub bytes: Vec<u8>,
    /// MIME type: `"image/png"` or `"image/jpeg"`.
    pub mime: &'static str,
    /// Source document id.
    pub doc_id: String,
    /// Zero-based page index within the source document.
    pub page: usize,
}

/// Result of transcribing one page image.
#[derive(Debug, Clone)]
pub struct Transcription {
    /// Layout-aware transcribed text.
    pub text: String,
    /// Confidence in [0.0, 1.0]; drives the Tesseract fallback (Task 6).
    pub confidence: f32,
}

/// Vision-OCR call. `Send + Sync` so ingest can fan pages across tokio tasks.
#[async_trait]
pub trait VlmOcrClient: Send + Sync {
    /// Transcribe text from a page image. `hint` carries layout/language
    /// guidance, e.g. "French legal contract; preserve table structure".
    async fn transcribe(
        &self,
        image: &PageImage,
        hint: &str,
    ) -> crate::error::Result<Transcription>;
    /// Stable model identifier for audit logs.
    fn model_id(&self) -> &str;
}

/// Quality gate replacing a bare length check. Mirrors kreuzberg's NativeTextStats heuristics:
/// char count + fragmentation ratio. Returns a score in [0.0, 1.0].
pub fn vlm_quality_score(text: &str) -> f32 {
    let non_ws: usize = text.chars().filter(|c| !c.is_whitespace()).count();
    if non_ws < 10 {
        return 0.0;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return 0.0;
    }
    let short = words.iter().filter(|w| w.chars().count() <= 2).count();
    if short as f32 / words.len() as f32 > 0.80 {
        return 0.2;
    }
    0.85
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_scores_zero() {
        assert_eq!(vlm_quality_score(""), 0.0);
    }

    #[test]
    fn very_short_text_scores_zero() {
        assert_eq!(vlm_quality_score("abc"), 0.0);
    }

    #[test]
    fn garbage_short_words_scores_low() {
        // All 1-2 char "words" → fragmentation penalty
        let garbage = "a b c d e f g h i j k";
        assert!(vlm_quality_score(garbage) < 0.5);
    }

    #[test]
    fn normal_french_text_scores_high() {
        let text = "Le présent contrat est conclu entre les parties soussignées pour une durée indéterminée.";
        assert!(vlm_quality_score(text) > 0.8);
    }
}
