//! Dataset acquisition helpers, grouped by source (HTTP, HuggingFace Hub, S3).
pub(crate) mod hf_hub;
pub(crate) mod http;
pub(crate) mod s3;

use crate::eval::dataset_registry::DatasetAccessibility;
use crate::eval::loader::DatasetId;
use anno::{Error, Result};

/// Extract `<org>/<dataset>` from `https://huggingface.co/datasets/<org>/<dataset>(/...)`.
#[cfg(feature = "eval")]
fn extract_hf_dataset_name(url: &str) -> Option<String> {
    let marker = "huggingface.co/datasets/";
    let idx = url.find(marker)? + marker.len();
    let rest = &url[idx..];
    let rest = rest.split('?').next().unwrap_or(rest);
    let rest = rest.trim_matches('/');
    let mut parts = rest.split('/');
    let org = parts.next()?;
    let name = parts.next()?;
    Some(format!("{}/{}", org, name))
}

#[cfg(feature = "eval")]
fn url_encode_component(s: &str) -> String {
    // Minimal encoding for HF datasets-server query parameters.
    // In practice we mainly need `/` -> `%2F`.
    s.replace('/', "%2F").replace(' ', "%20")
}

#[cfg(feature = "eval")]
fn hf_rows_url(dataset: &str, config: &str, split: &str) -> String {
    format!(
        "https://datasets-server.huggingface.co/rows?dataset={}&config={}&split={}&offset=0&length=100",
        url_encode_component(dataset),
        url_encode_component(config),
        url_encode_component(split),
    )
}

#[cfg(feature = "eval")]
fn hf_dataset_from_rows_url(url: &str) -> Option<String> {
    // Example:
    // https://datasets-server.huggingface.co/rows?dataset=masakhane%2Fmasakhaner2&config=bam&split=test...
    let (_, query) = url.split_once('?')?;
    for part in query.split('&') {
        let (k, v) = part.split_once('=')?;
        if k == "dataset" {
            // Minimal percent-decoding sufficient for HF dataset ids.
            let decoded = v.replace("%2F", "/").replace("%2f", "/");
            return Some(decoded);
        }
    }
    None
}

/// Resolve a usable (config, split) pair for a HF dataset via datasets-server, with an optional
/// preferred config.
#[cfg(feature = "eval")]
fn resolve_hf_config_split_prefer(
    dataset: &str,
    preferred_config: Option<&str>,
) -> Result<(String, String)> {
    let url = format!(
        "https://datasets-server.huggingface.co/splits?dataset={}",
        url_encode_component(dataset)
    );
    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| Error::InvalidInput(format!("Failed to query HF splits: {}", e)))?;

    if response.status() != 200 {
        return Err(Error::InvalidInput(format!(
            "HF splits query returned HTTP {} for dataset {}",
            response.status(),
            dataset
        )));
    }

    let body = response
        .into_string()
        .map_err(|e| Error::InvalidInput(format!("Failed to read HF splits response: {}", e)))?;

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| Error::InvalidInput(format!("Invalid JSON from HF splits endpoint: {}", e)))?;

    let splits = json
        .get("splits")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::InvalidInput("HF splits response missing `splits`".to_string()))?;

    // Prefer smaller/eval splits if present, otherwise pick the first split.
    // If `preferred_config` is provided, try to pick a split within that config first.
    let mut chosen = None;
    let prefer = preferred_config.map(|s| s.trim()).filter(|s| !s.is_empty());

    for pass in 0..2 {
        for s in splits {
            let config = s.get("config").and_then(|v| v.as_str());
            let split = s.get("split").and_then(|v| v.as_str());
            if let (Some(config), Some(split)) = (config, split) {
                // Pass 0: only consider preferred config. Pass 1: consider any config.
                if pass == 0 {
                    if let Some(p) = prefer {
                        if config != p {
                            continue;
                        }
                    } else {
                        // No preference: skip pass 0.
                        break;
                    }
                }

                if split == "test" {
                    chosen = Some((config.to_string(), split.to_string()));
                    break;
                }
                if chosen.is_none() {
                    chosen = Some((config.to_string(), split.to_string()));
                }
            }
        }
        if chosen.is_some() {
            break;
        }
    }

    chosen.ok_or_else(|| {
        Error::InvalidInput(format!(
            "HF splits endpoint returned no usable (config, split) for dataset {}",
            dataset
        ))
    })
}

/// Download dataset from source with retry logic and pagination support.
///
/// Implements exponential backoff retry strategy:
/// - 3 retries maximum
/// - Initial delay: 1 second
/// - Exponential backoff: 2^attempt seconds
/// - Max delay: 10 seconds
///
/// For HuggingFace datasets-server API, automatically paginates to download full dataset.
#[cfg(feature = "eval")]
pub(crate) fn download_with_resolved_url(id: DatasetId) -> Result<(String, String)> {
    let url = id.download_url().to_string();

    fn env_single_csv(keys: &[&str]) -> Option<String> {
        for &k in keys {
            let Ok(raw) = std::env::var(k) else {
                continue;
            };
            let mut parts = raw
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            parts.sort();
            parts.dedup();
            if parts.len() == 1 {
                return Some(parts[0].clone());
            }
        }
        None
    }

    // Check if URL is empty (dataset not available for download)
    if url.is_empty() {
        let access_status = id.access_status();
        let notes = id.notes();
        let mirror_url = id.mirror_url();

        let mut error_msg = format!(
            "Dataset '{}' ({:?}) has no download URL available.",
            id.name(),
            id
        );

        // Add access status information
        match access_status {
            DatasetAccessibility::Public => {
                error_msg.push_str("\n\nStatus: Public (but URL not configured)");
            }
            DatasetAccessibility::HuggingFace => {
                if let Some(hf_id) = id.hf_id() {
                    error_msg.push_str(&format!(
                        "\n\nStatus: Available on HuggingFace\nDataset ID: {}",
                        hf_id
                    ));
                } else {
                    error_msg.push_str("\n\nStatus: Available on HuggingFace (ID not configured)");
                }
            }
            DatasetAccessibility::Local => {
                error_msg.push_str("\n\nStatus: Available locally (check testdata/ or cache)");
            }
            DatasetAccessibility::Registration => {
                error_msg.push_str("\n\nStatus: Requires registration (e.g., LDC for academics)");
            }
            DatasetAccessibility::ContactAuthors => {
                error_msg.push_str("\n\nStatus: Contact dataset authors for access");
            }
            DatasetAccessibility::NotYetReleased => {
                error_msg.push_str("\n\nStatus: Not yet publicly released");
            }
            DatasetAccessibility::DependsOnOther => {
                if let Some(dep) = id.depends_on() {
                    error_msg.push_str(&format!("\n\nStatus: Depends on another dataset: {}", dep));
                } else {
                    error_msg.push_str("\n\nStatus: Depends on another dataset");
                }
            }
            DatasetAccessibility::Deprecated => {
                error_msg.push_str("\n\nStatus: Deprecated or no longer available");
            }
        }

        if let Some(note) = notes {
            error_msg.push_str(&format!("\n\nNotes: {}", note));
        }

        if let Some(mirror) = mirror_url {
            error_msg.push_str(&format!("\n\nMirror URL: {}", mirror));
        }

        return Err(Error::InvalidInput(error_msg));
    }

    // For multilingual datasets hosted on HuggingFace with per-language configs (e.g. WikiANN),
    // let callers bias the HF "config" selection via `ANNO_MUXER_PIN_LANG`.
    //
    // This keeps muxer facet pins honest: a `.lang=de` run should ideally download a German
    // config, not a random/first config.
    let preferred_hf_config: Option<String> = if id.language().eq_ignore_ascii_case("mul") {
        // Explicit override wins if set.
        env_single_csv(&["ANNO_HF_DATASET_CONFIG"])
            .or_else(|| env_single_csv(&["ANNO_MUXER_PIN_LANG", "ANNO_MUXER_FILTER_LANG"]))
    } else {
        env_single_csv(&["ANNO_HF_DATASET_CONFIG"])
    };

    // If the registry marks this dataset as HuggingFace-accessible and provides an HF id,
    // prefer fetching via datasets-server (even if the registry `url` is a paper/GitHub homepage).
    if id.access_status() == DatasetAccessibility::HuggingFace {
        if let Some(hf_ds) = id.hf_id().map(|s| s.to_string()) {
            if let Ok((config, split)) =
                resolve_hf_config_split_prefer(&hf_ds, preferred_hf_config.as_deref())
            {
                let base = hf_rows_url(&hf_ds, &config, &split);
                if std::env::var("ANNO_MUXER_VERBOSE")
                    .ok()
                    .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                {
                    eprintln!(
                        "dataset-loader: hf dataset={} config={} split={} (preferred={:?})",
                        hf_ds, config, split, preferred_hf_config
                    );
                }
                match hf_hub::download_hf_dataset_paginated(id, &base) {
                    Ok(content) => return Ok((content, base)),
                    Err(_e) => {
                        // Fall back to downloading a raw file from the dataset repo.
                        if let Ok((content, resolved)) =
                            hf_hub::download_hf_dataset_file_from_hub(&hf_ds)
                        {
                            return Ok((content, resolved));
                        }
                    }
                }
            }
        }
    }

    // HuggingFace dataset pages are common in the registry. Convert them into a
    // datasets-server API download so we can actually fetch data (and avoid caching HTML).
    //
    // Example:
    // - page:  https://huggingface.co/datasets/KevinSpaghetti/cadec
    // - API:   https://datasets-server.huggingface.co/rows?dataset=KevinSpaghetti%2Fcadec&...
    if url.contains("huggingface.co/datasets/") {
        // Prefer the registry's `hf_id` when available (more reliable than parsing the URL).
        let hf_ds = id
            .hf_id()
            .map(|s| s.to_string())
            .or_else(|| extract_hf_dataset_name(&url));

        if let Some(hf_ds) = hf_ds {
            match resolve_hf_config_split_prefer(&hf_ds, preferred_hf_config.as_deref()) {
                Ok((config, split)) => {
                    let base = hf_rows_url(&hf_ds, &config, &split);
                    if std::env::var("ANNO_MUXER_VERBOSE")
                        .ok()
                        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    {
                        eprintln!(
                            "dataset-loader: hf dataset={} config={} split={} (preferred={:?})",
                            hf_ds, config, split, preferred_hf_config
                        );
                    }
                    match hf_hub::download_hf_dataset_paginated(id, &base) {
                        Ok(content) => return Ok((content, base)),
                        Err(e) => {
                            // Some HF datasets don't support datasets-server row export.
                            // Fall back to downloading a raw file from the dataset repo.
                            if let Ok((content, resolved)) =
                                hf_hub::download_hf_dataset_file_from_hub(&hf_ds)
                            {
                                return Ok((content, resolved));
                            }
                            return Err(e);
                        }
                    }
                }
                Err(_e) => {
                    // If the datasets-server endpoint is unavailable (rate limits, transient issues),
                    // prefer falling back to the hub-file path rather than downloading HTML.
                    if let Ok((content, resolved)) =
                        hf_hub::download_hf_dataset_file_from_hub(&hf_ds)
                    {
                        return Ok((content, resolved));
                    }
                }
            }
        }
    }

    // Check if this is a HuggingFace datasets-server API URL
    if url.contains("datasets-server.huggingface.co/rows") {
        match hf_hub::download_hf_dataset_paginated(id, &url) {
            Ok(content) => return Ok((content, url)),
            Err(e) => {
                // Some datasets fail row export (422). If we can infer the HF dataset id,
                // fall back to the hub file download path.
                if let Some(hf_ds) = hf_dataset_from_rows_url(&url) {
                    if let Ok((content, resolved)) =
                        hf_hub::download_hf_dataset_file_from_hub(&hf_ds)
                    {
                        return Ok((content, resolved));
                    }
                }
                return Err(e);
            }
        }
    }

    // Regular download with retry logic
    const MAX_RETRIES: u32 = 3;
    const INITIAL_DELAY_SECS: u64 = 1;

    let mut last_error = None;

    for attempt in 0..=MAX_RETRIES {
        match http::download_attempt(&url) {
            Ok(content) => {
                // Checksum validation removed - registry doesn't provide expected_checksum
                // Future: Add checksum validation if registry provides it

                return Ok((content, url.clone()));
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < MAX_RETRIES {
                    let delay_secs = (INITIAL_DELAY_SECS * (1 << attempt)).min(10);
                    log::warn!(
                        "Download attempt {} failed for {}, retrying in {}s...",
                        attempt + 1,
                        &url,
                        delay_secs
                    );
                    std::thread::sleep(std::time::Duration::from_secs(delay_secs));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        Error::InvalidInput(format!(
            "Failed to download {} after {} retries. \
             Check network connection and try again. \
             URL: {}",
            id.name(),
            MAX_RETRIES + 1,
            &url
        ))
    }))
}
