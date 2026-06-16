//! HuggingFace Hub download helpers: datasets-server pagination and direct
//! `hf-hub` crate downloads.
use crate::eval::loader::DatasetId;
use crate::eval::loader::DatasetLoader;
use anno::{Error, Result};

/// Best-effort fallback: download a raw dataset file from the HuggingFace Hub.
///
/// This is a fallback for datasets that do not support the datasets-server row export.
#[cfg(feature = "eval")]
pub(crate) fn download_hf_dataset_file_from_hub(dataset: &str) -> Result<(String, String)> {
    // Example API: https://huggingface.co/api/datasets/coref-data/preco_raw
    let api_url = format!("https://huggingface.co/api/datasets/{}", dataset);
    let response = ureq::get(&api_url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| {
            Error::InvalidInput(format!(
                "Failed to query HuggingFace dataset metadata for {}: {}",
                dataset, e
            ))
        })?;

    if response.status() != 200 {
        return Err(Error::InvalidInput(format!(
            "HuggingFace dataset metadata request returned HTTP {} for {}",
            response.status(),
            dataset
        )));
    }

    let body = response
        .into_string()
        .map_err(|e| Error::InvalidInput(format!("Failed to read HuggingFace dataset metadata: {}", e)))?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        Error::InvalidInput(format!(
            "Invalid JSON from HuggingFace dataset metadata: {}",
            e
        ))
    })?;

    let siblings = json
        .get("siblings")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::InvalidInput("HF dataset metadata missing `siblings`".to_string()))?;

    // Prefer smaller / eval-friendly splits, but fall back to whatever exists.
    //
    // Many HF dataset repos store raw source files (CoNLL/TSV/etc.) instead of JSONL,
    // often under subdirectories. We handle both.
    let preferred = [
        // JSONL (common)
        "dev.jsonl",
        "validation.jsonl",
        "test.jsonl",
        "train.jsonl",
    ];
    let mut chosen: Option<String> = None;

    for want in preferred {
        if siblings.iter().any(|s| {
            s.get("rfilename")
                .and_then(|v| v.as_str())
                .is_some_and(|f| f == want)
        }) {
            chosen = Some(want.to_string());
            break;
        }
    }

    if chosen.is_none() {
        // Otherwise, pick a file we can plausibly parse (allow subdirectories).
        //
        // Priority:
        // - contains split hint: test > dev/valid > train
        // - extension: jsonl > conll/bio/iob/tsv/txt
        //
        // This is still best-effort; the parser selection is handled elsewhere via the dataset id.
        fn split_rank(name: &str) -> u8 {
            let n = name.to_lowercase();
            if n.contains("test") {
                0
            } else if n.contains("dev") || n.contains("valid") || n.contains("validation") {
                1
            } else if n.contains("train") {
                2
            } else {
                3
            }
        }
        fn ext_rank(name: &str) -> u8 {
            let n = name.to_lowercase();
            if n.ends_with(".jsonl") {
                0
            } else if n.ends_with(".conll")
                || n.ends_with(".bio")
                || n.ends_with(".iob")
                || n.ends_with(".iob2")
            {
                1
            } else if n.ends_with(".tsv") {
                2
            } else if n.ends_with(".txt") {
                3
            } else {
                9
            }
        }

        let mut candidates: Vec<String> = siblings
            .iter()
            .filter_map(|s| {
                s.get("rfilename")
                    .and_then(|v| v.as_str())
                    .map(|f| f.to_string())
            })
            .filter(|f| ext_rank(f) < 9)
            .collect();

        candidates.sort_by(|a, b| {
            (split_rank(a), ext_rank(a), a.len()).cmp(&(split_rank(b), ext_rank(b), b.len()))
        });

        chosen = candidates.first().cloned();
    }

    let Some(filename) = chosen else {
        return Err(Error::InvalidInput(format!(
            "No downloadable .jsonl file discovered for HF dataset {}",
            dataset
        )));
    };

    // Download file via resolve (use main; can be made content-addressed later via `sha`).
    let file_url = format!(
        "https://huggingface.co/datasets/{}/resolve/main/{}",
        dataset, filename
    );

    // Best-effort: avoid downloading huge files if a max byte limit is configured.
    if let Some(limit) = DatasetLoader::max_download_bytes() {
        if let Ok(resp) = ureq::head(&file_url)
            .timeout(std::time::Duration::from_secs(30))
            .call()
        {
            if resp.status() == 200 {
                if let Some(len) = resp
                    .header("Content-Length")
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    if len > limit {
                        return Err(Error::InvalidInput(format!(
                            "Download rejected ({} bytes > ANNO_MAX_DOWNLOAD_BYTES={} bytes) from {}",
                            len, limit, file_url
                        )));
                    }
                }
            }
        }
    }

    let content = super::http::download_attempt(&file_url)?;
    Ok((content, file_url))
}

/// Placeholder when `eval` is disabled.
#[cfg(not(feature = "eval"))]
#[allow(dead_code)]
pub(crate) fn download_hf_dataset_file_from_hub(_dataset: &str) -> Result<(String, String)> {
    Err(Error::InvalidInput(
        "HuggingFace file fallback requires feature `eval`".to_string(),
    ))
}

/// Download HuggingFace dataset with pagination support.
///
/// HF datasets-server API limits responses to 100 rows by default.
/// This function automatically paginates to download the full dataset.
///
/// For datasets available on HuggingFace Hub, can also use hf-hub crate
/// for direct file downloads (faster, no pagination needed).
#[cfg(feature = "eval")]
pub(crate) fn download_hf_dataset_paginated(id: DatasetId, base_url: &str) -> Result<String> {
    // Try hf-hub direct download first (if available and dataset is on HF)
    #[cfg(feature = "onnx")] // hf-hub is available with onnx feature
    {
        if let Ok(content) = try_hf_hub_download(id) {
            return Ok(content);
        }
    }

    // Fall back to paginated API download
    // HuggingFace datasets-server API limits to 100 rows per request
    const PAGE_SIZE: usize = 100;
    let mut all_rows = Vec::new();
    let mut features = None;
    let mut offset: usize = 0;
    let mut total_rows = None;

    log::info!(
        "Downloading {} with pagination (page size: {})",
        id.name(),
        PAGE_SIZE
    );

    loop {
        // Build paginated URL
        let url = if base_url.contains("offset=") {
            // Replace existing offset parameter
            let prev_offset = offset.saturating_sub(PAGE_SIZE);
            base_url
                .replace(
                    &format!("offset={}", prev_offset),
                    &format!("offset={}", offset),
                )
                .replace("length=100", &format!("length={}", PAGE_SIZE))
        } else {
            // Add pagination parameters
            let separator = if base_url.contains('?') { "&" } else { "?" };
            format!(
                "{}{}offset={}&length={}",
                base_url, separator, offset, PAGE_SIZE
            )
        };

        match super::http::download_attempt(&url) {
            Ok(content) => {
                let parsed: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| Error::InvalidInput(format!("Invalid JSON response: {}", e)))?;

                // Extract features (only from first page)
                if features.is_none() {
                    features = parsed.get("features").cloned();
                }

                // Extract total number of rows (if available)
                if total_rows.is_none() {
                    total_rows = parsed
                        .get("num_rows_total")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as usize);
                }

                // Extract rows from this page
                if let Some(rows) = parsed.get("rows").and_then(|v| v.as_array()) {
                    if rows.is_empty() {
                        break; // No more rows
                    }
                    all_rows.extend_from_slice(rows);
                    log::debug!(
                        "Downloaded {} rows (total so far: {})",
                        rows.len(),
                        all_rows.len()
                    );

                    // Check if we've got all rows
                    if let Some(total) = total_rows {
                        if all_rows.len() >= total {
                            break;
                        }
                    } else if rows.len() < PAGE_SIZE {
                        // No total available, but got fewer rows than requested = last page
                        break;
                    }

                    offset += PAGE_SIZE;
                } else {
                    // No rows in response, might be error or empty dataset
                    break;
                }
            }
            Err(e) => {
                // If we got some rows, return partial dataset with warning
                if !all_rows.is_empty() {
                    log::warn!(
                        "Failed to download full {} dataset (got {} rows before error: {}). \
                         Returning partial dataset.",
                        id.name(),
                        all_rows.len(),
                        e
                    );
                    break;
                } else {
                    return Err(e);
                }
            }
        }

        // Safety limit: prevent infinite loops
        if offset > 1_000_000 {
            log::warn!(
                "Reached safety limit (1M rows) for {}. Returning partial dataset ({} rows).",
                id.name(),
                all_rows.len()
            );
            break;
        }
    }

    // Reconstruct full API response format
    let mut response: serde_json::Value = serde_json::json!({
        "rows": all_rows,
    });

    if let Some(features_val) = features {
        response["features"] = features_val;
    }

    if let Some(total) = total_rows {
        response["num_rows_total"] = serde_json::json!(total);
    }

    serde_json::to_string(&response)
        .map_err(|e| Error::InvalidInput(format!("Failed to serialize paginated response: {}", e)))
}

/// Try downloading dataset directly from HuggingFace Hub using hf-hub crate.
///
/// This is faster than paginated API calls for datasets available on HF Hub.
/// Returns Ok(content) if successful, Err if not available or hf-hub not enabled.
///
/// Uses `HF_TOKEN` environment variable if set for accessing gated datasets.
#[cfg(all(feature = "eval", feature = "onnx"))]
pub(crate) fn try_hf_hub_download(id: DatasetId) -> Result<String> {
    use hf_hub::api::sync::{Api, ApiBuilder};

    // Map dataset IDs to HuggingFace dataset names and file paths
    let (dataset_name, file_path) = match id {
        DatasetId::MultiNERD => ("Babelscape/multinerd", "test/test_en.jsonl"),
        DatasetId::TweetNER7 => ("tner/tweetner7", "dataset/2020.dev.json"),
        DatasetId::BroadTwitterCorpus => ("GateNLP/broad_twitter_corpus", "test/a.conll"),
        DatasetId::CADEC => ("KevinSpaghetti/cadec", "data/test.jsonl"),
        DatasetId::PreCo => ("coref-data/preco", "data/test.jsonl"),
        // Gated datasets that require HF_TOKEN
        DatasetId::MultiCoNER => ("MultiCoNER/multiconer_v1", "en/test.conll"),
        DatasetId::MultiCoNERv2 => ("MultiCoNER/multiconer_v2", "en/test.conll"),
        _ => {
            return Err(Error::InvalidInput(
                "Dataset not available via hf-hub".to_string(),
            ))
        }
    };

    // Load .env for HF_TOKEN if not already set
    anno::env::load_dotenv();

    // Use HF_TOKEN from environment if available (for gated datasets)
    let api = if let Some(token) = anno::env::hf_token() {
        ApiBuilder::new()
            .with_token(Some(token))
            .build()
            .map_err(|e| {
                Error::InvalidInput(format!(
                    "Failed to initialize HuggingFace API with token: {}",
                    e
                ))
            })?
    } else {
        Api::new().map_err(|e| {
            Error::InvalidInput(format!("Failed to initialize HuggingFace API: {}", e))
        })?
    };

    let repo = api.dataset(dataset_name.to_string());
    let file_path_buf = repo.get(file_path).map_err(|e| {
        Error::InvalidInput(format!(
            "Failed to download {} from HuggingFace Hub: {}. \
             Falling back to HTTP download.",
            file_path, e
        ))
    })?;

    std::fs::read_to_string(&file_path_buf)
        .map_err(|e| Error::InvalidInput(format!("Failed to read downloaded file: {}", e)))
}

/// Placeholder for when hf-hub is not available.
#[cfg(not(all(feature = "eval", feature = "onnx")))]
#[allow(dead_code)] // Part of trait interface, may be unused in some feature combinations
pub(crate) fn try_hf_hub_download(_id: DatasetId) -> Result<String> {
    Err(Error::InvalidInput("hf-hub not available".to_string()))
}
