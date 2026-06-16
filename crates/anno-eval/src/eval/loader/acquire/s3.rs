//! S3-backed dataset cache download/upload helpers (via the `aws` CLI).
use crate::eval::loader::types::CacheManifestEntry;
use crate::eval::loader::DatasetId;
use anno::{Error, Result};

/// Download dataset from S3 cache bucket.
///
/// Uses AWS CLI under the hood (requires `aws` command in PATH and valid credentials).
#[cfg(feature = "eval")]
pub(crate) fn download_from_s3(
    bucket: &str,
    id: DatasetId,
) -> Result<(String, Option<CacheManifestEntry>)> {
    use std::process::Command;

    // Prefer a content-addressed snapshot if the pointer exists; fall back to the legacy key.
    let pointer_key = format!("datasets/{}.latest.json", id.cache_filename());
    let pointer_uri = format!("s3://{}/{}", bucket, pointer_key);

    let mut content: Option<String> = None;

    // Attempt: pointer -> by-sha256 snapshot
    let pointer = Command::new("aws")
        .args(["s3", "cp", &pointer_uri, "-"])
        .output()
        .ok()
        .and_then(|o| {
            if !o.status.success() {
                // Pointer missing or unreadable; fall back to legacy key.
                return None;
            }
            String::from_utf8(o.stdout).ok()
        })
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok());

    if let Some(pointer) = pointer {
        if let Some(sha) = pointer.get("sha256").and_then(|v| v.as_str()) {
            let by_sha_key = format!("datasets/by-sha256/{}/{}", sha, id.cache_filename());
            let by_sha_uri = format!("s3://{}/{}", bucket, by_sha_key);
            let output = Command::new("aws")
                .args(["s3", "cp", &by_sha_uri, "-"])
                .output()
                .ok();

            if let Some(output) = output {
                if output.status.success() {
                    content = String::from_utf8(output.stdout).ok();
                }
            }
        }
    }

    // Fall back: legacy key (mutable)
    if content.is_none() {
        let s3_key = format!("datasets/{}", id.cache_filename());
        let s3_uri = format!("s3://{}/{}", bucket, s3_key);

        // Use aws s3 cp to download to stdout
        let output = Command::new("aws")
            .args(["s3", "cp", &s3_uri, "-"])
            .output()
            .map_err(|e| Error::InvalidInput(format!("Failed to run aws s3 cp: {}", e)))?;

        if output.status.success() {
            content = Some(String::from_utf8(output.stdout).map_err(|e| {
                Error::InvalidInput(format!("S3 content not valid UTF-8: {}", e))
            })?);
        } else {
            return Err(Error::InvalidInput(format!(
                "S3 download failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
    }

    let Some(content) = content else {
        return Err(Error::InvalidInput("S3 download failed".to_string()));
    };

    // Best-effort: attempt to fetch per-dataset manifest entry.
    let manifest = download_manifest_entry_from_s3(bucket, id).ok();

    Ok((content, manifest))
}

/// Upload dataset to S3 cache bucket.
///
/// Best effort - failures are logged but don't stop execution.
#[cfg(feature = "eval")]
pub(crate) fn upload_to_s3(
    bucket: &str,
    id: DatasetId,
    content: &str,
    manifest_entry: &CacheManifestEntry,
) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let s3_key = format!("datasets/{}", id.cache_filename());
    let s3_uri = format!("s3://{}/{}", bucket, s3_key);

    // Use aws s3 cp to upload from stdin
    let mut child = Command::new("aws")
        .args(["s3", "cp", "-", &s3_uri])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::InvalidInput(format!("Failed to spawn aws s3 cp: {}", e)))?;

    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(content.as_bytes());
    }

    let status = child
        .wait()
        .map_err(|e| Error::InvalidInput(format!("Failed to wait for aws s3 cp: {}", e)))?;

    if !status.success() {
        return Err(Error::InvalidInput("S3 upload failed".to_string()));
    }

    // Also upload a content-addressed snapshot. This supports the "durable mirror" use case:
    // once uploaded, snapshots are stable even if upstream changes.
    let by_sha_key = format!(
        "datasets/by-sha256/{}/{}",
        manifest_entry.sha256,
        id.cache_filename()
    );
    let by_sha_uri = format!("s3://{}/{}", bucket, by_sha_key);

    let mut child = Command::new("aws")
        .args(["s3", "cp", "-", &by_sha_uri])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::InvalidInput(format!("Failed to spawn aws s3 cp: {}", e)))?;

    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(content.as_bytes());
    }

    let _ = child.wait();

    // Update a pointer file for the latest snapshot.
    let pointer_key = format!("datasets/{}.latest.json", id.cache_filename());
    let pointer_uri = format!("s3://{}/{}", bucket, pointer_key);
    let pointer_json = serde_json::json!({
        "dataset": id.cache_filename(),
        "sha256": manifest_entry.sha256,
        "updated_at": chrono::Utc::now().to_rfc3339(),
        "source_url": manifest_entry.source_url,
        "resolved_url": manifest_entry.resolved_url,
        "file_size": manifest_entry.file_size,
    })
    .to_string();

    let mut child = Command::new("aws")
        .args(["s3", "cp", "-", &pointer_uri])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::InvalidInput(format!("Failed to spawn aws s3 cp: {}", e)))?;

    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(pointer_json.as_bytes());
    }

    let _ = child.wait();

    // Upload a sidecar manifest entry (JSON). This makes the S3 cache self-describing.
    let manifest_key = format!("datasets/{}.manifest.json", id.cache_filename());
    let manifest_uri = format!("s3://{}/{}", bucket, manifest_key);
    let json = serde_json::to_string_pretty(manifest_entry)
        .map_err(|e| Error::InvalidInput(format!("Failed to serialize S3 manifest: {}", e)))?;

    let mut child = Command::new("aws")
        .args(["s3", "cp", "-", &manifest_uri])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::InvalidInput(format!("Failed to spawn aws s3 cp: {}", e)))?;

    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(json.as_bytes());
    }

    let status = child
        .wait()
        .map_err(|e| Error::InvalidInput(format!("Failed to wait for aws s3 cp: {}", e)))?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::InvalidInput("S3 manifest upload failed".to_string()))
    }
}

/// Best-effort: download the sidecar manifest entry for a dataset from S3.
#[cfg(feature = "eval")]
pub(crate) fn download_manifest_entry_from_s3(
    bucket: &str,
    id: DatasetId,
) -> Result<CacheManifestEntry> {
    use std::process::Command;

    let manifest_key = format!("datasets/{}.manifest.json", id.cache_filename());
    let manifest_uri = format!("s3://{}/{}", bucket, manifest_key);

    let output = Command::new("aws")
        .args(["s3", "cp", &manifest_uri, "-"])
        .output()
        .map_err(|e| Error::InvalidInput(format!("Failed to run aws s3 cp: {}", e)))?;

    if !output.status.success() {
        return Err(Error::InvalidInput(
            "S3 manifest download failed".to_string(),
        ));
    }

    let json = String::from_utf8(output.stdout)
        .map_err(|e| Error::InvalidInput(format!("S3 manifest not valid UTF-8: {}", e)))?;

    serde_json::from_str(&json)
        .map_err(|e| Error::InvalidInput(format!("Failed to parse S3 manifest JSON: {}", e)))
}
