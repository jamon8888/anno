//! Plain HTTP download helpers shared by dataset acquisition paths.
use anno::{Error, Result};

/// Single download attempt with retry logic.
///
/// Retries up to 3 times with exponential backoff for transient failures (timeouts, 5xx errors).
#[cfg(feature = "eval")]
pub(crate) fn download_attempt(url: &str) -> Result<String> {
    const MAX_RETRIES: usize = 3;
    const INITIAL_TIMEOUT_SECS: u64 = 30;
    const MAX_TIMEOUT_SECS: u64 = 120;

    let mut last_error = None;

    for attempt in 0..=MAX_RETRIES {
        let timeout_secs = (INITIAL_TIMEOUT_SECS * (1 << attempt.min(2))).min(MAX_TIMEOUT_SECS);

        match ureq::get(url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .call()
        {
            Ok(response) => {
                if response.status() == 200 {
                    // Success - read content
                    let content = response.into_string().map_err(|e| {
                        Error::InvalidInput(format!(
                            "Failed to read response from {}: {}. \
                             Response may be too large or corrupted.",
                            url, e
                        ))
                    })?;

                    // Heuristic guardrail: many registry URLs are dataset homepages. If we fetch HTML,
                    // treat it as a download failure so we don't silently cache garbage and then parse
                    // "0 sentences".
                    let lower = content
                        .chars()
                        .take(2048)
                        .collect::<String>()
                        .to_lowercase();
                    if lower.contains("<html") || lower.contains("<!doctype html") {
                        return Err(Error::InvalidInput(format!(
                            "Downloaded HTML from {}. This URL looks like a webpage, not a raw dataset file.",
                            url
                        )));
                    }

                    return Ok(content);
                }

                let status = response.status();
                let body = response.into_string().unwrap_or_default();
                let body = body.trim();
                let body_preview = if body.len() > 800 {
                    format!("{}…", &body[..800])
                } else {
                    body.to_string()
                };

                // Retry on 5xx server errors (transient)
                if (500..600).contains(&status) && attempt < MAX_RETRIES {
                    let wait_ms = 1000 * (1 << attempt); // Exponential backoff: 1s, 2s, 4s
                    log::debug!(
                        "Server error {} downloading {} (attempt {}/{}), retrying in {}ms...",
                        status,
                        url,
                        attempt + 1,
                        MAX_RETRIES + 1,
                        wait_ms
                    );
                    std::thread::sleep(std::time::Duration::from_millis(wait_ms));
                    last_error = Some(format!(
                        "HTTP {} downloading {}. Server returned error status. {}{}",
                        status,
                        url,
                        if body_preview.is_empty() {
                            ""
                        } else {
                            "Response body: "
                        },
                        body_preview
                    ));
                    continue;
                }

                // Non-retryable error (4xx client errors)
                return Err(Error::InvalidInput(format!(
                    "HTTP {} downloading {}. \
                     Server returned error status. \
                     Dataset may be temporarily unavailable or URL changed. {}{}",
                    status,
                    url,
                    if body_preview.is_empty() {
                        ""
                    } else {
                        "Response body: "
                    },
                    body_preview
                )));
            }
            Err(ureq::Error::Transport(e)) => {
                // Network errors (timeouts, connection failures) - retry
                let error_msg = format!("{}", e);
                let is_timeout = error_msg.contains("timeout") || error_msg.contains("timed out");

                if is_timeout && attempt < MAX_RETRIES {
                    let wait_ms = 1000 * (1 << attempt); // Exponential backoff
                    log::debug!(
                        "Timeout downloading {} (attempt {}/{}), retrying in {}ms...",
                        url,
                        attempt + 1,
                        MAX_RETRIES + 1,
                        wait_ms
                    );
                    std::thread::sleep(std::time::Duration::from_millis(wait_ms));
                    last_error = Some(error_msg);
                    continue;
                }

                // Final attempt failed or non-timeout error
                return Err(Error::InvalidInput(format!(
                    "Network error downloading {}: {}. \
                     Check your internet connection and try again. \
                     {}",
                    url,
                    error_msg,
                    if attempt > 0 {
                        format!("(Failed after {} retries)", attempt)
                    } else {
                        String::new()
                    }
                )));
            }
            Err(e) => {
                // Other errors (non-retryable)
                let error_msg = format!("{}", e);
                return Err(Error::InvalidInput(format!(
                    "Error downloading {}: {}. \
                     Check your internet connection and try again.",
                    url, error_msg
                )));
            }
        }
    }

    // All retries exhausted
    Err(Error::InvalidInput(format!(
        "Failed to download {} after {} attempts. Last error: {}",
        url,
        MAX_RETRIES + 1,
        last_error.unwrap_or_else(|| "unknown error".to_string())
    )))
}

/// Single download attempt returning raw bytes (for binary formats like ZIP).
///
/// Same retry/backoff logic as `download_attempt` but reads response as bytes.
#[cfg(feature = "eval")]
pub(crate) fn download_attempt_bytes(url: &str) -> Result<Vec<u8>> {
    const MAX_RETRIES: usize = 3;
    const INITIAL_TIMEOUT_SECS: u64 = 30;
    const MAX_TIMEOUT_SECS: u64 = 120;
    const MAX_BYTES: usize = 50 * 1024 * 1024; // 50 MB limit

    let mut last_error = None;

    for attempt in 0..=MAX_RETRIES {
        let timeout_secs = (INITIAL_TIMEOUT_SECS * (1 << attempt.min(2))).min(MAX_TIMEOUT_SECS);

        match ureq::get(url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .call()
        {
            Ok(response) => {
                if response.status() == 200 {
                    use std::io::Read as _;
                    let mut bytes = Vec::new();
                    response
                        .into_reader()
                        .take(MAX_BYTES as u64)
                        .read_to_end(&mut bytes)
                        .map_err(|e| {
                            Error::InvalidInput(format!("Failed to read bytes from {}: {}", url, e))
                        })?;
                    return Ok(bytes);
                }

                let status = response.status();
                if (500..600).contains(&status) && attempt < MAX_RETRIES {
                    let wait_ms = 1000 * (1 << attempt);
                    std::thread::sleep(std::time::Duration::from_millis(wait_ms));
                    last_error = Some(format!("HTTP {} from {}", status, url));
                    continue;
                }

                return Err(Error::InvalidInput(format!(
                    "HTTP {} downloading binary from {}",
                    status, url
                )));
            }
            Err(ureq::Error::Transport(e)) => {
                let msg = format!("{}", e);
                if (msg.contains("timeout") || msg.contains("timed out")) && attempt < MAX_RETRIES {
                    let wait_ms = 1000 * (1 << attempt);
                    std::thread::sleep(std::time::Duration::from_millis(wait_ms));
                    last_error = Some(msg);
                    continue;
                }
                return Err(Error::InvalidInput(format!(
                    "Network error downloading {}: {}",
                    url, msg
                )));
            }
            Err(e) => {
                return Err(Error::InvalidInput(format!(
                    "Error downloading {}: {}",
                    url, e
                )));
            }
        }
    }

    Err(Error::InvalidInput(format!(
        "Failed to download {} after {} attempts. Last error: {}",
        url,
        MAX_RETRIES + 1,
        last_error.unwrap_or_else(|| "unknown".to_string())
    )))
}
