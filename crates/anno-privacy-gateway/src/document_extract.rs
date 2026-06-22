//! Local document extraction for uploaded files and base64 document blocks.

use crate::{Error, Result};
use std::path::PathBuf;

/// 50 MB — upload guard (M1). Kreuzberg 4.7.4 has no internal file-size cap.
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

/// 64 MP pixel count — image decompression-bomb guard (M1).
/// Matches the cap added in kreuzberg 4.9.6 that we lost when downgrading to 4.7.4.
const MAX_IMAGE_PIXELS: u64 = 64_000_000;

/// 30 s — native extraction timeout (M3). Guards against Ghostscript-PDF hangs.
const EXTRACT_TIMEOUT_SECS: u64 = 30;

/// 120 s — OCR path timeout (M3). Longer budget for Tesseract on scanned pages.
const EXTRACT_OCR_TIMEOUT_SECS: u64 = 120;

/// Extracted document text and basic source metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocument {
    /// Original filename from the upload or document block.
    pub filename: String,
    /// Content type supplied by the caller.
    pub detected_content_type: String,
    /// Trimmed extracted text.
    pub text: String,
}

/// Extract local document text without sending file bytes to any provider.
pub async fn extract_uploaded_document(
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> Result<ExtractedDocument> {
    // M1: file-size cap — kreuzberg 4.7.4 has no internal limit.
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(Error::Privacy(format!(
            "uploaded document exceeds {} MB limit",
            MAX_UPLOAD_BYTES / 1_048_576
        )));
    }
    // M1: image pixel-count cap — header-only read, no full decode.
    if is_image_like(filename, content_type, &bytes) {
        guard_image_dimensions(&bytes)?;
    }

    let text = if is_text_like(filename, content_type) {
        String::from_utf8(bytes)
            .map_err(|e| Error::Privacy(format!("uploaded text is not valid UTF-8: {e}")))?
    } else {
        extract_with_kreuzberg(filename, bytes).await?
    };
    let normalized = text.trim().to_string();
    if normalized.is_empty() {
        return Err(Error::UnsupportedFeature(
            "uploaded document extracted empty text".to_string(),
        ));
    }
    Ok(ExtractedDocument {
        filename: filename.to_string(),
        detected_content_type: content_type.to_string(),
        text: normalized,
    })
}

fn is_text_like(filename: &str, content_type: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    content_type.starts_with("text/")
        || lower.ends_with(".txt")
        || lower.ends_with(".md")
        || lower.ends_with(".csv")
        || lower.ends_with(".json")
}

/// ORDERING INVARIANT: this function must never fully decode pixel data.
/// `image::guess_format` reads only magic bytes — it does NOT decompress.
/// `guard_image_dimensions` (the decompression-bomb check) runs AFTER this
/// function; if a future change here calls `image::open` or similar, the bomb
/// guard would be bypassed. Keep this read-only on the raw bytes.
fn is_image_like(filename: &str, content_type: &str, bytes: &[u8]) -> bool {
    let lower = filename.to_ascii_lowercase();
    content_type.starts_with("image/")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".tiff")
        || lower.ends_with(".tif")
        || lower.ends_with(".bmp")
        || image::guess_format(bytes).is_ok()
}

/// Header-only dimension check. Returns `Err` if the image exceeds `MAX_IMAGE_PIXELS`
/// or if the image header is unreadable (fail-closed decompression-bomb guard).
fn guard_image_dimensions(bytes: &[u8]) -> Result<()> {
    use std::io::Cursor;
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::Privacy(format!("inspect image header: {e}")))?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| Error::Privacy(format!("image dimensions unreadable: {e}")))?;
    let pixels = (w as u64).saturating_mul(h as u64);
    if pixels > MAX_IMAGE_PIXELS {
        return Err(Error::Privacy(format!(
            "image dimensions {}×{} exceed {} MP limit (decompression-bomb guard)",
            w,
            h,
            MAX_IMAGE_PIXELS / 1_000_000
        )));
    }
    Ok(())
}

async fn extract_with_kreuzberg(filename: &str, bytes: Vec<u8>) -> Result<String> {
    let tmp_dir = tempfile::tempdir()
        .map_err(|e| Error::Privacy(format!("create temp extraction dir: {e}")))?;
    let path: PathBuf = tmp_dir.path().join(safe_temp_filename(filename));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| Error::Privacy(format!("write temp uploaded document: {e}")))?;

    let config = kreuzberg::core::config::ExtractionConfig {
        disable_ocr: true,
        ..Default::default()
    };

    // M3: timeout — guards against Ghostscript-PDF hangs introduced between 4.7.4 and 4.9.0.
    // disable_ocr = true so we use the shorter native-extraction budget.
    let timeout = std::time::Duration::from_secs(EXTRACT_TIMEOUT_SECS);
    let extracted = tokio::time::timeout(timeout, kreuzberg::extract_file(&path, None, &config))
        .await
        .map_err(|_| {
            Error::Privacy(format!(
                "document extraction timed out after {} s",
                EXTRACT_TIMEOUT_SECS
            ))
        })?
        .map_err(|e| Error::Privacy(format!("extract uploaded document: {e}")))?;
    Ok(extracted.content)
}

fn safe_temp_filename(filename: &str) -> String {
    let sanitized: String = filename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "upload.bin".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extracts_utf8_text_plain() {
        let doc =
            extract_uploaded_document("notes.txt", "text/plain", b"Bonjour Marie Dupont".to_vec())
                .await
                .expect("extract");

        assert_eq!(doc.text, "Bonjour Marie Dupont");
        assert_eq!(doc.detected_content_type, "text/plain");
    }

    #[tokio::test]
    async fn rejects_empty_document_text() {
        let err = extract_uploaded_document("empty.txt", "text/plain", b"   ".to_vec())
            .await
            .expect_err("empty rejected");

        assert!(err.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn rejects_oversized_upload() {
        let big = vec![0u8; MAX_UPLOAD_BYTES + 1];
        let err = extract_uploaded_document("big.pdf", "application/pdf", big)
            .await
            .expect_err("oversized rejected");
        assert!(err.to_string().contains("MB limit"));
    }

    #[test]
    fn rejects_oversized_image_dimensions() {
        // 8001×8001 = 64_016_001 px > 64 MP limit.
        // GrayImage (1 byte/px) keeps the buffer at ~64 MB; PNG of solid black compresses tiny.
        let img = image::GrayImage::new(8001, 8001);
        let dyn_img = image::DynamicImage::from(img);
        let mut buf = std::io::Cursor::new(Vec::new());
        dyn_img
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("encode PNG");
        let err = guard_image_dimensions(&buf.into_inner()).expect_err("oversized rejected");
        assert!(err.to_string().contains("MP limit"));
    }

    #[test]
    fn accepts_normal_image_dimensions() {
        let img = image::GrayImage::new(1920, 1080);
        let dyn_img = image::DynamicImage::from(img);
        let mut buf = std::io::Cursor::new(Vec::new());
        dyn_img
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("encode PNG");
        guard_image_dimensions(&buf.into_inner()).expect("normal size accepted");
    }
}
