//! System tesseract fork for PDFs without a text layer.
//!
//! Off by default. Enable via `--enable-ocr` (or `enable_ocr = true` in config).
//! Tesseract must be installed:
//!   - Linux:   `sudo apt install tesseract-ocr tesseract-ocr-fra`
//!   - macOS:   `brew install tesseract tesseract-lang`
//!   - Windows: `winget install --id UB-Mannheim.TesseractOCR`

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Run system tesseract on `pdf_path`, returning the extracted plain text.
/// `tesseract_path` is the binary path (defaults to "tesseract" on PATH).
/// Timeout: 60s.
///
/// # Errors
/// Returns [`Error::Detect`] if the binary is missing, the fork fails, the
/// process exits non-zero, or the timeout fires.
pub async fn ocr_pdf(pdf_path: &Path, tesseract_path: Option<&PathBuf>) -> Result<String> {
    let bin = tesseract_path
        .map(|p| p.as_os_str().to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("tesseract"));

    let out = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        Command::new(&bin)
            .arg(pdf_path)
            .arg("-") // stdout
            .arg("-l")
            .arg("fra+eng")
            .output(),
    )
    .await
    .map_err(|_| Error::Detect("tesseract: 60s timeout".into()))?
    .map_err(|e| Error::Detect(format!("tesseract fork: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Detect(format!(
            "tesseract exited {}: {}",
            out.status, stderr
        )));
    }
    String::from_utf8(out.stdout).map_err(|e| Error::Detect(format!("tesseract output utf8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_binary_returns_detect_error() {
        let fake = PathBuf::from("/nonexistent/tesseract-xyz-does-not-exist");
        let res = ocr_pdf(Path::new("/tmp/whatever.pdf"), Some(&fake)).await;
        assert!(matches!(res, Err(Error::Detect(_))));
    }
}
