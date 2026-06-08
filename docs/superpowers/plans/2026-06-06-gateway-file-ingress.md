# Gateway File Ingress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe `/v1/files` and `document` block support to `anno-privacy-gateway`, so Cowork/Claude Desktop can send files while Anno enforces pseudonymized or DPA-cleartext routing by selected model privacy mode.

**Architecture:** Build this after the sovereign provider gateway plan, reusing `PrivacyMode`, provider model resolution, and DPA gates. File uploads are extracted locally, assigned `anno_file_*` ids, and stored as content-free metadata plus local text derivatives; upstream providers receive either pseudonymized document text or cleartext document text according to the resolved model privacy mode. URL documents and image documents remain fail-closed in this phase.

**Tech Stack:** Rust 2021, Axum 0.8 multipart, Reqwest, Tokio, Serde JSON, `uuid`, `sha2`, `kreuzberg`, existing `PrivacyEngine`, existing provider router, existing audit sink, GitNexus CLI, `scripts/dev-fast.ps1`.

**Spec:** [`docs/superpowers/specs/2026-06-06-cowork-3p-sovereign-gateway-design.md`](../specs/2026-06-06-cowork-3p-sovereign-gateway-design.md)

---

## Code Review Findings

- `crates/anno-privacy-gateway/src/server.rs` currently registers `/v1/files`, `/v1/files/{id}`, and `/v1/files/{id}/content`, but every route calls `files_unsupported()`.
- `crates/anno-privacy-gateway/src/privacy.rs` rejects `document` blocks in `reject_blocks()` before request text is transformed.
- `crates/anno-privacy-gateway/src/privacy.rs` already walks text in nested tool input values, so extracted document text can reuse the same privacy engine once it is represented as text blocks.
- `crates/anno-rag/src/ingest.rs` already uses `kreuzberg::extract_file` for local document extraction, so the gateway should use `kreuzberg` directly instead of adding a full `anno-rag` crate dependency.
- The upload API has no provider/model selection at upload time. The gateway therefore must store local derivatives and defer the final pseudonymized-or-cleartext choice until a `/v1/messages` request references the uploaded file.
- Phase 2 provider routing introduces `PrivacyMode`, `ProviderCatalog`, and `PrivacyEngine::transform_request_for_mode`. This plan relies on those interfaces and should start only after Phase 2 is merged.

## Scope Check

This plan implements Phase 3 only:

- implement local `/v1/files` upload, metadata, delete, and sanitized content read;
- expand `document` blocks with `source.type = "file"` and `source.type = "base64"`;
- reject `source.type = "url"` and image content in this release;
- keep provider-native remote file upload out of scope;
- keep OCR tuning and binary image redaction out of scope;
- never log prompt text, file text, or raw file bytes.

## File Map

Create:

- `crates/anno-privacy-gateway/src/file_registry.rs` - generate `anno_file_*` ids, store metadata JSON, persist pseudonymized and optional cleartext extracted text, and delete all local derivatives for a file.
- `crates/anno-privacy-gateway/src/document_extract.rs` - extract text from uploaded bytes with text/plain fast path and `kreuzberg` file extraction for supported document formats.
- `crates/anno-privacy-gateway/src/document_blocks.rs` - expand Anthropic `document` blocks into text blocks before provider request conversion.

Modify:

- `crates/anno-privacy-gateway/Cargo.toml` - enable Axum multipart and add `uuid`, `kreuzberg`, and `base64` dependencies.
- `crates/anno-privacy-gateway/src/lib.rs` - expose new modules.
- `crates/anno-privacy-gateway/src/config.rs` - add file store settings and env parsing.
- `crates/anno-privacy-gateway/src/privacy.rs` - expose a plain-text pseudonymization helper for file derivatives and document expansion.
- `crates/anno-privacy-gateway/src/audit.rs` - add content-free file event metadata.
- `crates/anno-privacy-gateway/src/server.rs` - wire `FileRegistry` into `AppState`, replace fail-closed file routes, and expand document blocks before privacy transform.
- `docs/developers/gateway-api.md` - document file routes and document block behavior.
- `docs/user-guide/privacy-gateway.md` - document file privacy modes, cleartext retention, and DPA behavior.

Do not modify:

- `crates/anno-rag-mcp/*`
- `crates/anno-rag/*`
- provider API keys or secret storage
- Cowork local-first root allowlist code from Phase 1

## Build And Test Commands

Run before edits:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
npx gitnexus status
```

Targeted checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
```

Targeted tests:

```powershell
cargo test -p anno-privacy-gateway file_registry -- --nocapture
cargo test -p anno-privacy-gateway document_extract -- --nocapture
cargo test -p anno-privacy-gateway document_blocks -- --nocapture
cargo test -p anno-privacy-gateway files_api -- --nocapture
cargo test -p anno-privacy-gateway provider_router_file_document -- --nocapture
```

Final package test:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

---

### Task 0: Pre-Flight And Impact Checks

**Files:** none.

- [ ] **Step 1: Confirm branch, worktree, and index**

Run:

```powershell
git status --short --branch
npx gitnexus status
```

Expected: known worktree state and up-to-date GitNexus index. If stale, run:

```powershell
npx gitnexus analyze
```

- [ ] **Step 2: Run impact checks**

Run:

```powershell
npx gitnexus impact --repo anno GatewayConfig --direction upstream
npx gitnexus impact --repo anno AppState --direction upstream
npx gitnexus impact --repo anno messages --direction upstream
npx gitnexus impact --repo anno stream_messages --direction upstream
npx gitnexus impact --repo anno PrivacyEngine --direction upstream
npx gitnexus impact --repo anno AuditEvent --direction upstream
```

Expected: record blast radius. Stop and report if HIGH or CRITICAL appears.

---

### Task 1: Dependencies And File Config

**Files:**
- Modify: `crates/anno-privacy-gateway/Cargo.toml`
- Modify: `crates/anno-privacy-gateway/src/config.rs`

- [ ] **Step 1: Add dependencies**

In `crates/anno-privacy-gateway/Cargo.toml`, change the Axum dependency and add file-ingress dependencies:

```toml
axum = { workspace = true, features = ["multipart"] }
base64 = "0.22"
kreuzberg = { workspace = true }
tempfile = "3"
uuid = { workspace = true }
```

Move the existing `tempfile = "3"` line from `[dev-dependencies]` into `[dependencies]`, because `document_extract.rs` uses temporary files at runtime.

- [ ] **Step 2: Write failing config tests**

In `crates/anno-privacy-gateway/src/config.rs`, add:

```rust
#[test]
fn file_config_default_is_local_and_bounded() {
    let cfg = GatewayConfig::default();
    assert_eq!(cfg.file_max_bytes, 25 * 1024 * 1024);
    assert!(!cfg.file_retain_raw);
    assert!(cfg.file_retain_cleartext);
    assert!(cfg.file_store_dir.ends_with("files"));
}

#[test]
fn file_config_env_parses_values() {
    std::env::set_var("ANNO_GATEWAY_FILE_STORE_DIR", "target/test-file-store");
    std::env::set_var("ANNO_GATEWAY_FILE_MAX_BYTES", "4096");
    std::env::set_var("ANNO_GATEWAY_FILE_RETAIN_RAW", "true");
    std::env::set_var("ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT", "false");

    let cfg = GatewayConfig::from_env();

    std::env::remove_var("ANNO_GATEWAY_FILE_STORE_DIR");
    std::env::remove_var("ANNO_GATEWAY_FILE_MAX_BYTES");
    std::env::remove_var("ANNO_GATEWAY_FILE_RETAIN_RAW");
    std::env::remove_var("ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT");

    assert_eq!(cfg.file_store_dir, std::path::PathBuf::from("target/test-file-store"));
    assert_eq!(cfg.file_max_bytes, 4096);
    assert!(cfg.file_retain_raw);
    assert!(!cfg.file_retain_cleartext);
}
```

- [ ] **Step 3: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway file_config -- --nocapture
```

Expected: FAIL because file config fields do not exist.

- [ ] **Step 4: Add config fields**

In `GatewayConfig`, add:

```rust
pub file_store_dir: std::path::PathBuf,
pub file_max_bytes: usize,
pub file_retain_raw: bool,
pub file_retain_cleartext: bool,
```

In `Default for GatewayConfig`, add:

```rust
file_store_dir: std::path::PathBuf::from(".anno/privacy-gateway/files"),
file_max_bytes: 25 * 1024 * 1024,
file_retain_raw: false,
file_retain_cleartext: true,
```

In `GatewayConfig::from_env()`, add:

```rust
if let Ok(path) = std::env::var("ANNO_GATEWAY_FILE_STORE_DIR") {
    cfg.file_store_dir = std::path::PathBuf::from(path);
}
if let Ok(value) = std::env::var("ANNO_GATEWAY_FILE_MAX_BYTES") {
    if let Ok(bytes) = value.parse::<usize>() {
        cfg.file_max_bytes = bytes;
    }
}
if let Ok(value) = std::env::var("ANNO_GATEWAY_FILE_RETAIN_RAW") {
    cfg.file_retain_raw = matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES");
}
if let Ok(value) = std::env::var("ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT") {
    cfg.file_retain_cleartext =
        !matches!(value.as_str(), "0" | "false" | "FALSE" | "no" | "NO");
}
```

- [ ] **Step 5: Run config tests**

Run:

```powershell
cargo test -p anno-privacy-gateway file_config -- --nocapture
```

Expected: PASS.

---

### Task 2: File Registry

**Files:**
- Create: `crates/anno-privacy-gateway/src/file_registry.rs`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`

- [ ] **Step 1: Write failing registry tests**

Create `crates/anno-privacy-gateway/src/file_registry.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_has_anno_prefix() {
        let id = StoredFileId::new();
        assert!(id.as_str().starts_with("anno_file_"));
        assert!(StoredFileId::parse(id.as_str()).is_ok());
        assert!(StoredFileId::parse("file_provider_123").is_err());
    }

    #[tokio::test]
    async fn registry_persists_metadata_and_derivatives() {
        let tmp = tempfile::TempDir::new().unwrap();
        let registry = FileRegistry::new(FileRegistryConfig {
            root: tmp.path().to_path_buf(),
            retain_raw: false,
            retain_cleartext: true,
        });

        let stored = registry
            .put_text_derivatives(
                "notes.txt",
                "text/plain",
                b"Bonjour Marie Dupont",
                "Bonjour Marie Dupont",
                "Bonjour PERSON_1",
            )
            .await
            .expect("store");

        let loaded = registry.get(stored.id.as_str()).await.expect("load");
        assert_eq!(loaded.filename, "notes.txt");
        assert_eq!(loaded.content_type, "text/plain");
        assert!(registry.read_pseudonymized_text(stored.id.as_str()).await.unwrap().contains("PERSON_1"));
        assert!(registry.read_cleartext_text(stored.id.as_str()).await.unwrap().unwrap().contains("Marie Dupont"));
        assert!(loaded.raw_path.is_none());
    }
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway file_registry -- --nocapture
```

Expected: FAIL because registry types do not exist.

- [ ] **Step 3: Implement registry types**

Replace `file_registry.rs` with:

```rust
//! Local file registry for gateway-managed uploaded documents.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use time::OffsetDateTime;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredFileId(String);

impl StoredFileId {
    #[must_use]
    pub fn new() -> Self {
        let id = uuid::Uuid::now_v7().simple().to_string();
        Self(format!("anno_file_{id}"))
    }

    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        if !value.starts_with("anno_file_") {
            return Err("file id must start with anno_file_".to_string());
        }
        let suffix = value.trim_start_matches("anno_file_");
        if suffix.len() != 32 || !suffix.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err("file id suffix must be a 32-character hex UUID".to_string());
        }
        Ok(Self(value.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct FileRegistryConfig {
    pub root: PathBuf,
    pub retain_raw: bool,
    pub retain_cleartext: bool,
}

#[derive(Debug, Clone)]
pub struct FileRegistry {
    config: FileRegistryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFile {
    pub id: StoredFileId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: usize,
    pub sha256_hex: String,
    pub created_at_unix: i64,
    pub metadata_path: PathBuf,
    pub pseudonymized_text_path: PathBuf,
    pub cleartext_text_path: Option<PathBuf>,
    pub raw_path: Option<PathBuf>,
}

impl FileRegistry {
    #[must_use]
    pub fn new(config: FileRegistryConfig) -> Self {
        Self { config }
    }

    pub async fn put_text_derivatives(
        &self,
        filename: &str,
        content_type: &str,
        raw_bytes: &[u8],
        cleartext_text: &str,
        pseudonymized_text: &str,
    ) -> Result<StoredFile> {
        let id = StoredFileId::new();
        let dir = self.file_dir(id.as_str());
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| Error::Privacy(format!("create file registry dir: {e}")))?;

        let metadata_path = dir.join("metadata.json");
        let pseudonymized_text_path = dir.join("pseudonymized.txt");
        let cleartext_text_path = self.config.retain_cleartext.then(|| dir.join("cleartext.txt"));
        let raw_path = self.config.retain_raw.then(|| dir.join("raw.bin"));

        tokio::fs::write(&pseudonymized_text_path, pseudonymized_text)
            .await
            .map_err(|e| Error::Privacy(format!("write pseudonymized file text: {e}")))?;
        if let Some(path) = &cleartext_text_path {
            tokio::fs::write(path, cleartext_text)
                .await
                .map_err(|e| Error::Privacy(format!("write cleartext file text: {e}")))?;
        }
        if let Some(path) = &raw_path {
            tokio::fs::write(path, raw_bytes)
                .await
                .map_err(|e| Error::Privacy(format!("write raw uploaded file: {e}")))?;
        }

        let sha256_hex = hex::encode(Sha256::digest(raw_bytes));
        let metadata = StoredFile {
            id,
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            size_bytes: raw_bytes.len(),
            sha256_hex,
            created_at_unix: OffsetDateTime::now_utc().unix_timestamp(),
            metadata_path: metadata_path.clone(),
            pseudonymized_text_path,
            cleartext_text_path,
            raw_path,
        };
        let json = serde_json::to_vec_pretty(&metadata)
            .map_err(|e| Error::Privacy(format!("serialize file metadata: {e}")))?;
        tokio::fs::write(&metadata_path, json)
            .await
            .map_err(|e| Error::Privacy(format!("write file metadata: {e}")))?;
        Ok(metadata)
    }

    pub async fn get(&self, id: &str) -> Result<StoredFile> {
        let parsed = StoredFileId::parse(id).map_err(Error::Privacy)?;
        let path = self.file_dir(parsed.as_str()).join("metadata.json");
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| Error::Privacy(format!("read file metadata: {e}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Privacy(format!("parse file metadata: {e}")))
    }

    pub async fn read_pseudonymized_text(&self, id: &str) -> Result<String> {
        let stored = self.get(id).await?;
        tokio::fs::read_to_string(&stored.pseudonymized_text_path)
            .await
            .map_err(|e| Error::Privacy(format!("read pseudonymized file text: {e}")))
    }

    pub async fn read_cleartext_text(&self, id: &str) -> Result<Option<String>> {
        let stored = self.get(id).await?;
        let Some(path) = stored.cleartext_text_path else {
            return Ok(None);
        };
        let text = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::Privacy(format!("read cleartext file text: {e}")))?;
        Ok(Some(text))
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let parsed = StoredFileId::parse(id).map_err(Error::Privacy)?;
        let dir = self.file_dir(parsed.as_str());
        if !dir.exists() {
            return Ok(false);
        }
        tokio::fs::remove_dir_all(dir)
            .await
            .map_err(|e| Error::Privacy(format!("delete stored file: {e}")))?;
        Ok(true)
    }

    fn file_dir(&self, id: &str) -> PathBuf {
        self.config.root.join(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_has_anno_prefix() {
        let id = StoredFileId::new();
        assert!(id.as_str().starts_with("anno_file_"));
        assert!(StoredFileId::parse(id.as_str()).is_ok());
        assert!(StoredFileId::parse("file_provider_123").is_err());
    }

    #[tokio::test]
    async fn registry_persists_metadata_and_derivatives() {
        let tmp = tempfile::TempDir::new().unwrap();
        let registry = FileRegistry::new(FileRegistryConfig {
            root: tmp.path().to_path_buf(),
            retain_raw: false,
            retain_cleartext: true,
        });

        let stored = registry
            .put_text_derivatives(
                "notes.txt",
                "text/plain",
                b"Bonjour Marie Dupont",
                "Bonjour Marie Dupont",
                "Bonjour PERSON_1",
            )
            .await
            .expect("store");

        let loaded = registry.get(stored.id.as_str()).await.expect("load");
        assert_eq!(loaded.filename, "notes.txt");
        assert_eq!(loaded.content_type, "text/plain");
        assert!(registry.read_pseudonymized_text(stored.id.as_str()).await.unwrap().contains("PERSON_1"));
        assert!(registry.read_cleartext_text(stored.id.as_str()).await.unwrap().unwrap().contains("Marie Dupont"));
        assert!(loaded.raw_path.is_none());
    }
}
```

- [ ] **Step 4: Expose the module**

In `crates/anno-privacy-gateway/src/lib.rs`, add:

```rust
pub mod file_registry;
```

- [ ] **Step 5: Run registry tests**

Run:

```powershell
cargo test -p anno-privacy-gateway file_registry -- --nocapture
```

Expected: PASS.

---

### Task 3: Document Extraction And Text Pseudonymization Helper

**Files:**
- Create: `crates/anno-privacy-gateway/src/document_extract.rs`
- Modify: `crates/anno-privacy-gateway/src/privacy.rs`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`

- [ ] **Step 1: Write failing extraction tests**

Create `crates/anno-privacy-gateway/src/document_extract.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extracts_utf8_text_plain() {
        let doc = extract_uploaded_document(
            "notes.txt",
            "text/plain",
            b"Bonjour Marie Dupont".to_vec(),
        )
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
}
```

- [ ] **Step 2: Add failing privacy helper test**

In `crates/anno-privacy-gateway/src/privacy.rs`, add:

```rust
#[test]
fn pseudonymizes_plain_text_for_file_derivative() {
    let mut engine = PrivacyEngine::from_config(&GatewayConfig::default()).unwrap();
    let report = engine
        .pseudonymize_plain_text("Bonjour Marie Dupont")
        .expect("plain text");
    assert!(report.text.contains("PERSON_"));
    assert_eq!(report.entities, 1);
}
```

- [ ] **Step 3: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway document_extract -- --nocapture
cargo test -p anno-privacy-gateway pseudonymizes_plain_text_for_file_derivative -- --nocapture
```

Expected: FAIL because extraction and plain-text helper do not exist.

- [ ] **Step 4: Implement document extraction**

Replace `document_extract.rs` with:

```rust
//! Local document extraction for uploaded files and base64 document blocks.

use std::path::PathBuf;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocument {
    pub filename: String,
    pub detected_content_type: String,
    pub text: String,
}

pub async fn extract_uploaded_document(
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> Result<ExtractedDocument> {
    let text = if content_type.starts_with("text/")
        || filename.ends_with(".txt")
        || filename.ends_with(".md")
        || filename.ends_with(".csv")
        || filename.ends_with(".json")
    {
        String::from_utf8(bytes.clone())
            .map_err(|e| Error::Privacy(format!("uploaded text is not valid UTF-8: {e}")))?
    } else {
        extract_with_kreuzberg(filename, bytes).await?
    };
    let normalized = text.trim().to_string();
    if normalized.is_empty() {
        return Err(Error::UnsupportedFeature(
            "uploaded document extracted no text".to_string(),
        ));
    }
    Ok(ExtractedDocument {
        filename: filename.to_string(),
        detected_content_type: content_type.to_string(),
        text: normalized,
    })
}

async fn extract_with_kreuzberg(filename: &str, bytes: Vec<u8>) -> Result<String> {
    let tmp_dir = tempfile::tempdir()
        .map_err(|e| Error::Privacy(format!("create temp extraction dir: {e}")))?;
    let path: PathBuf = tmp_dir.path().join(safe_temp_filename(filename));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| Error::Privacy(format!("write temp uploaded document: {e}")))?;
    let extracted = tokio::task::spawn_blocking(move || kreuzberg::extract_file(&path))
        .await
        .map_err(|e| Error::Privacy(format!("join document extraction: {e}")))?
        .map_err(|e| Error::Privacy(format!("extract uploaded document: {e}")))?;
    Ok(extracted.content)
}

fn safe_temp_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extracts_utf8_text_plain() {
        let doc = extract_uploaded_document(
            "notes.txt",
            "text/plain",
            b"Bonjour Marie Dupont".to_vec(),
        )
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
}
```

- [ ] **Step 5: Implement plain-text privacy helper**

In `privacy.rs`, add the report type:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainTextPrivacyReport {
    pub text: String,
    pub entities: usize,
}
```

Add this method to `impl PrivacyEngine`:

```rust
pub fn pseudonymize_plain_text(&mut self, text: &str) -> Result<PlainTextPrivacyReport> {
    let mut value = serde_json::Value::String(text.to_string());
    let report = self.pseudonymize_value(&mut value)?;
    let text = value
        .as_str()
        .ok_or_else(|| Error::Privacy("plain text transform returned non-string".to_string()))?
        .to_string();
    Ok(PlainTextPrivacyReport {
        text,
        entities: report.entities,
    })
}
```

If `pseudonymize_value` is private to another block, keep it private and add `pseudonymize_plain_text` next to the existing request transform methods so it can reuse the same mapper and vault.

- [ ] **Step 6: Expose the extraction module**

In `crates/anno-privacy-gateway/src/lib.rs`, add:

```rust
pub mod document_extract;
```

- [ ] **Step 7: Run extraction tests**

Run:

```powershell
cargo test -p anno-privacy-gateway document_extract -- --nocapture
cargo test -p anno-privacy-gateway pseudonymizes_plain_text_for_file_derivative -- --nocapture
```

Expected: PASS.

---

### Task 4: File API Routes

**Files:**
- Modify: `crates/anno-privacy-gateway/src/server.rs`
- Modify: `crates/anno-privacy-gateway/src/audit.rs`

- [ ] **Step 1: Write failing file API tests**

In `server.rs` tests, replace the `files_api_fails_closed` assertion with these tests:

```rust
#[tokio::test]
async fn files_api_uploads_text_and_returns_metadata_without_content() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = GatewayConfig {
        file_store_dir: tmp.path().join("files"),
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes("Bonjour Marie Dupont".as_bytes().to_vec())
            .file_name("notes.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let response: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/files"))
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(response["id"].as_str().unwrap().starts_with("anno_file_"));
    assert_eq!(response["object"], "file");
    assert_eq!(response["filename"], "notes.txt");
    assert!(response.get("text").is_none());
}

#[tokio::test]
async fn files_api_content_returns_pseudonymized_text_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = GatewayConfig {
        file_store_dir: tmp.path().join("files"),
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes("Bonjour Marie Dupont".as_bytes().to_vec())
            .file_name("notes.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let uploaded: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/files"))
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = uploaded["id"].as_str().unwrap();

    let content = reqwest::Client::new()
        .get(format!("http://{gateway_addr}/v1/files/{id}/content"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(content.contains("PERSON_"));
    assert!(!content.contains("Marie Dupont"));
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway files_api -- --nocapture
```

Expected: FAIL because `/v1/files` still fails closed.

- [ ] **Step 3: Extend `AppState`**

In `server.rs`, add a registry field:

```rust
file_registry: std::sync::Arc<crate::file_registry::FileRegistry>,
```

In `AppState::try_new`, construct it:

```rust
let file_registry = std::sync::Arc::new(crate::file_registry::FileRegistry::new(
    crate::file_registry::FileRegistryConfig {
        root: config.file_store_dir.clone(),
        retain_raw: config.file_retain_raw,
        retain_cleartext: config.file_retain_cleartext,
    },
));
```

In the returned `AppState`, include `file_registry`.

- [ ] **Step 4: Replace file routes**

In `router`, replace the existing file route registrations with:

```rust
.route("/v1/files", post(upload_file).get(list_files_unsupported))
.route(
    "/v1/files/{id}",
    get(get_file_metadata).delete(delete_file),
)
.route("/v1/files/{id}/content", get(get_file_content))
```

Keep `list_files_unsupported` fail-closed because the local registry does not need a global list for Cowork file references:

```rust
async fn list_files_unsupported() -> Error {
    Error::UnsupportedFeature("file listing is not exposed by anno gateway".to_string())
}
```

- [ ] **Step 5: Implement upload and metadata handlers**

Add these handlers in `server.rs`:

```rust
async fn upload_file(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<Value>> {
    let mut file_name = None;
    let mut content_type = "application/octet-stream".to_string();
    let mut bytes = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| Error::Privacy(format!("read multipart field: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        file_name = field.file_name().map(ToString::to_string);
        content_type = field
            .content_type()
            .map(ToString::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        bytes = field
            .bytes()
            .await
            .map_err(|e| Error::Privacy(format!("read uploaded file bytes: {e}")))?
            .to_vec();
        break;
    }

    if bytes.is_empty() {
        return Err(Error::UnsupportedFeature("multipart upload must include one file field".to_string()));
    }
    if bytes.len() > state.config.file_max_bytes {
        return Err(Error::UnsupportedFeature(format!(
            "uploaded file exceeds {} bytes",
            state.config.file_max_bytes
        )));
    }

    let filename = file_name.unwrap_or_else(|| "uploaded-document".to_string());
    let extracted =
        crate::document_extract::extract_uploaded_document(&filename, &content_type, bytes.clone())
            .await?;
    let pseudonymized = {
        let mut privacy = state.privacy.lock().await;
        privacy.pseudonymize_plain_text(&extracted.text)?
    };
    let stored = state
        .file_registry
        .put_text_derivatives(
            &extracted.filename,
            &extracted.detected_content_type,
            &bytes,
            &extracted.text,
            &pseudonymized.text,
        )
        .await?;

    state.audit.record(crate::audit::AuditEvent {
        request_id: "file-upload".to_string(),
        provider_profile: state.config.provider_profile.clone(),
        provider_id: "local-file-registry".to_string(),
        model_id: "none".to_string(),
        upstream_model: "none".to_string(),
        privacy_mode: "file-upload".to_string(),
        entity_count: pseudonymized.entities,
        fresh_pii_redacted: 0,
    });

    Ok(Json(file_metadata_json(&stored)))
}

async fn get_file_metadata(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>> {
    let stored = state.file_registry.get(&id).await?;
    Ok(Json(file_metadata_json(&stored)))
}

async fn delete_file(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>> {
    let deleted = state.file_registry.delete(&id).await?;
    Ok(Json(json!({
        "id": id,
        "object": "file.deleted",
        "deleted": deleted
    })))
}

async fn get_file_content(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse> {
    let text = state.file_registry.read_pseudonymized_text(&id).await?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        text,
    ))
}

fn file_metadata_json(stored: &crate::file_registry::StoredFile) -> Value {
    json!({
        "id": stored.id.as_str(),
        "object": "file",
        "filename": stored.filename,
        "bytes": stored.size_bytes,
        "created_at": stored.created_at_unix,
        "purpose": "assistants",
        "content_type": stored.content_type,
        "sha256": stored.sha256_hex
    })
}
```

- [ ] **Step 6: Run file route tests**

Run:

```powershell
cargo test -p anno-privacy-gateway files_api -- --nocapture
```

Expected: PASS.

---

### Task 5: Document Block Expansion

**Files:**
- Create: `crates/anno-privacy-gateway/src/document_blocks.rs`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`

- [ ] **Step 1: Write failing document block tests**

Create `crates/anno-privacy-gateway/src/document_blocks.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_mode::PrivacyMode;
    use base64::Engine;
    use serde_json::json;

    #[tokio::test]
    async fn rejects_url_document_sources() {
        let registry = test_registry().await;
        let mut body = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "url", "url": "https://example.com/a.pdf"}
                }]
            }]
        });
        let err = expand_document_blocks(
            &mut body,
            &registry,
            PrivacyMode::Pseudonymized,
            &mut crate::PrivacyEngine::from_config(&crate::GatewayConfig::default()).unwrap(),
        )
        .await
        .expect_err("url rejected");
        assert!(err.to_string().contains("url document sources"));
    }

    #[tokio::test]
    async fn expands_base64_document_to_text_block() {
        let registry = test_registry().await;
        let data = base64::engine::general_purpose::STANDARD.encode("Bonjour Marie Dupont");
        let mut body = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "base64", "media_type": "text/plain", "data": data},
                    "title": "notes.txt"
                }]
            }]
        });
        expand_document_blocks(
            &mut body,
            &registry,
            PrivacyMode::Pseudonymized,
            &mut crate::PrivacyEngine::from_config(&crate::GatewayConfig::default()).unwrap(),
        )
        .await
        .expect("expand");
        let text = body["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("PERSON_"));
        assert!(!text.contains("Marie Dupont"));
    }

    async fn test_registry() -> crate::file_registry::FileRegistry {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.into_path();
        crate::file_registry::FileRegistry::new(crate::file_registry::FileRegistryConfig {
            root,
            retain_raw: false,
            retain_cleartext: true,
        })
    }
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway document_blocks -- --nocapture
```

Expected: FAIL because document block expansion does not exist.

- [ ] **Step 3: Implement document block expansion**

Replace `document_blocks.rs` with:

```rust
//! Expand Anthropic document blocks into gateway-safe text blocks.

use base64::Engine;
use serde_json::{json, Value};

use crate::privacy_mode::PrivacyMode;
use crate::{Error, PrivacyEngine, Result};

pub async fn expand_document_blocks(
    body: &mut Value,
    registry: &crate::file_registry::FileRegistry,
    privacy_mode: PrivacyMode,
    privacy: &mut PrivacyEngine,
) -> Result<DocumentExpansionReport> {
    let mut report = DocumentExpansionReport::default();
    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            expand_message_content(message, registry, privacy_mode, privacy, &mut report).await?;
        }
    }
    Ok(report)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentExpansionReport {
    pub document_count: usize,
    pub entity_count: usize,
}

async fn expand_message_content(
    message: &mut Value,
    registry: &crate::file_registry::FileRegistry,
    privacy_mode: PrivacyMode,
    privacy: &mut PrivacyEngine,
    report: &mut DocumentExpansionReport,
) -> Result<()> {
    let Some(content) = message.get_mut("content") else {
        return Ok(());
    };
    if let Some(text) = content.as_str() {
        *content = Value::Array(vec![json!({"type": "text", "text": text})]);
    }
    let Some(blocks) = content.as_array_mut() else {
        return Ok(());
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("document") {
            continue;
        }
        let title = block
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("document")
            .to_string();
        let source = block
            .get("source")
            .ok_or_else(|| Error::UnsupportedFeature("document block missing source".to_string()))?;
        let text = document_text_from_source(source, &title, registry, privacy_mode, privacy).await?;
        *block = json!({
            "type": "text",
            "text": format!("[Document: {title}]\n{text}")
        });
        report.document_count += 1;
    }
    Ok(())
}

async fn document_text_from_source(
    source: &Value,
    title: &str,
    registry: &crate::file_registry::FileRegistry,
    privacy_mode: PrivacyMode,
    privacy: &mut PrivacyEngine,
) -> Result<String> {
    match source.get("type").and_then(Value::as_str) {
        Some("file") => {
            let id = source
                .get("file_id")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::UnsupportedFeature("file document source missing file_id".to_string()))?;
            match privacy_mode {
                PrivacyMode::Pseudonymized => registry.read_pseudonymized_text(id).await,
                PrivacyMode::CleartextDpa | PrivacyMode::CleartextLocal => registry
                    .read_cleartext_text(id)
                    .await?
                    .ok_or_else(|| Error::UnsupportedFeature("cleartext file derivative is not retained".to_string())),
            }
        }
        Some("base64") => {
            let media_type = source
                .get("media_type")
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream");
            let encoded = source
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::UnsupportedFeature("base64 document source missing data".to_string()))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| Error::Privacy(format!("decode base64 document: {e}")))?;
            let extracted =
                crate::document_extract::extract_uploaded_document(title, media_type, bytes).await?;
            if privacy_mode == PrivacyMode::Pseudonymized {
                let pseudonymized = privacy.pseudonymize_plain_text(&extracted.text)?;
                Ok(pseudonymized.text)
            } else {
                Ok(extracted.text)
            }
        }
        Some("url") => Err(Error::UnsupportedFeature(
            "url document sources are not fetched by anno privacy gateway".to_string(),
        )),
        Some(other) => Err(Error::UnsupportedFeature(format!(
            "unsupported document source type: {other}"
        ))),
        None => Err(Error::UnsupportedFeature(
            "document source must include type".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_mode::PrivacyMode;
    use base64::Engine;
    use serde_json::json;

    #[tokio::test]
    async fn rejects_url_document_sources() {
        let registry = test_registry().await;
        let mut body = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "url", "url": "https://example.com/a.pdf"}
                }]
            }]
        });
        let err = expand_document_blocks(
            &mut body,
            &registry,
            PrivacyMode::Pseudonymized,
            &mut crate::PrivacyEngine::from_config(&crate::GatewayConfig::default()).unwrap(),
        )
        .await
        .expect_err("url rejected");
        assert!(err.to_string().contains("url document sources"));
    }

    #[tokio::test]
    async fn expands_base64_document_to_text_block() {
        let registry = test_registry().await;
        let data = base64::engine::general_purpose::STANDARD.encode("Bonjour Marie Dupont");
        let mut body = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "base64", "media_type": "text/plain", "data": data},
                    "title": "notes.txt"
                }]
            }]
        });
        expand_document_blocks(
            &mut body,
            &registry,
            PrivacyMode::Pseudonymized,
            &mut crate::PrivacyEngine::from_config(&crate::GatewayConfig::default()).unwrap(),
        )
        .await
        .expect("expand");
        let text = body["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("PERSON_"));
        assert!(!text.contains("Marie Dupont"));
    }

    async fn test_registry() -> crate::file_registry::FileRegistry {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.into_path();
        crate::file_registry::FileRegistry::new(crate::file_registry::FileRegistryConfig {
            root,
            retain_raw: false,
            retain_cleartext: true,
        })
    }
}
```

- [ ] **Step 4: Expose the module**

In `crates/anno-privacy-gateway/src/lib.rs`, add:

```rust
pub mod document_blocks;
```

- [ ] **Step 5: Run document block tests**

Run:

```powershell
cargo test -p anno-privacy-gateway document_blocks -- --nocapture
```

Expected: PASS.

---

### Task 6: Provider Router Integration For File Documents

**Files:**
- Modify: `crates/anno-privacy-gateway/src/server.rs`
- Modify: `crates/anno-privacy-gateway/src/privacy.rs`

- [ ] **Step 1: Add provider router file tests**

In `server.rs` tests, add:

```rust
#[tokio::test]
async fn provider_router_file_document_pseudonymized_sends_no_cleartext() {
    let tmp = tempfile::TempDir::new().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/chat/completions", post(mock_openai_chat))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;
    let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);
    let config = GatewayConfig {
        provider_catalog_path: Some(catalog_path),
        file_store_dir: tmp.path().join("files"),
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let file_id = upload_text_file(gateway_addr, "notes.txt", "Bonjour Marie Dupont").await;

    let response: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "anno/mistral/mistral-large-latest:pseudonymized",
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "file", "file_id": file_id},
                    "title": "notes.txt"
                }]
            }]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let upstream_body = captured.lock().await.clone().expect("upstream called");
    let upstream_text = serde_json::to_string(&upstream_body).unwrap();
    assert!(!upstream_text.contains("Marie Dupont"));
    assert!(upstream_text.contains("PERSON_"));
    assert_eq!(response["content"][0]["text"], "Bonjour Marie Dupont");
}

#[tokio::test]
async fn provider_router_file_document_cleartext_dpa_sends_cleartext_to_verified_provider() {
    let tmp = tempfile::TempDir::new().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/chat/completions", post(mock_openai_chat))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;
    let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);
    let config = GatewayConfig {
        provider_catalog_path: Some(catalog_path),
        file_store_dir: tmp.path().join("files"),
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let file_id = upload_text_file(gateway_addr, "notes.txt", "Bonjour Marie Dupont").await;

    let status = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "anno/mistral/mistral-large-latest:cleartext-dpa",
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "file", "file_id": file_id},
                    "title": "notes.txt"
                }]
            }]
        }))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status, reqwest::StatusCode::OK);
    let upstream_body = captured.lock().await.clone().expect("upstream called");
    let upstream_text = serde_json::to_string(&upstream_body).unwrap();
    assert!(upstream_text.contains("Marie Dupont"));
}

async fn upload_text_file(addr: std::net::SocketAddr, filename: &str, text: &str) -> String {
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(text.as_bytes().to_vec())
            .file_name(filename.to_string())
            .mime_str("text/plain")
            .unwrap(),
    );
    let uploaded: serde_json::Value = reqwest::Client::new()
        .post(format!("http://{addr}/v1/files"))
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    uploaded["id"].as_str().unwrap().to_string()
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway provider_router_file_document -- --nocapture
```

Expected: FAIL because provider messages still reject document blocks or do not expand them.

- [ ] **Step 3: Expand documents before mode-aware privacy**

In Phase 2 `provider_messages`, before `transform_request_for_mode`, add:

```rust
let document_report = {
    let mut privacy = state.privacy.lock().await;
    crate::document_blocks::expand_document_blocks(
        &mut body,
        &state.file_registry,
        resolved.privacy_mode,
        &mut privacy,
    )
    .await?
};
```

Then pass the same `body` into `transform_request_for_mode`. Add document entities to the audit count:

```rust
entity_count: privacy_report.entities + document_report.entity_count,
```

In Phase 2 `provider_stream_messages`, add the same expansion before `transform_request_for_mode`.

- [ ] **Step 4: Keep legacy upstream fail-closed for documents**

Do not expand document blocks in the legacy branch that uses `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE`. The existing `reject_blocks()` behavior must still reject documents when the provider catalog is not enabled.

Run:

```powershell
cargo test -p anno-privacy-gateway rejects_document_blocks -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Run provider file document tests**

Run:

```powershell
cargo test -p anno-privacy-gateway provider_router_file_document -- --nocapture
```

Expected: PASS.

---

### Task 7: Docs, Security Checks, And Final Verification

**Files:**
- Modify: `docs/developers/gateway-api.md`
- Modify: `docs/user-guide/privacy-gateway.md`

- [ ] **Step 1: Document file API**

In `docs/developers/gateway-api.md`, add:

````markdown
## File Ingress

`POST /v1/files` accepts a multipart `file` field. The gateway extracts text
locally, stores an `anno_file_*` reference, and returns metadata only:

```json
{
  "id": "anno_file_018f4fd0f70a7e26b6b0c4d4ec0a09b0",
  "object": "file",
  "filename": "contract.pdf",
  "bytes": 123456,
  "created_at": 1780747200,
  "purpose": "assistants",
  "content_type": "application/pdf",
  "sha256": "hex"
}
```

`GET /v1/files/{id}/content` returns the pseudonymized text derivative only.
It never returns the cleartext derivative.

`DELETE /v1/files/{id}` deletes metadata, pseudonymized text, optional cleartext
text, and optional raw bytes for that gateway-managed file id.
````

- [ ] **Step 2: Document document block support**

In `docs/user-guide/privacy-gateway.md`, add:

````markdown
## Files And Document Blocks

The gateway supports Claude/Cowork document blocks for local uploaded files and
inline base64 documents:

- `source.type = "file"` is accepted only for gateway ids that start with
  `anno_file_`.
- `source.type = "base64"` is extracted locally before provider routing.
- `source.type = "url"` is rejected. Fetch remote URLs yourself, upload the file
  to the gateway, then reference the returned `anno_file_*` id.

For `:pseudonymized` models, document text sent upstream uses the pseudonymized
derivative. For `:cleartext-dpa` models, document text is sent in cleartext only
when the selected provider has `dpa_verified=true` and the deployment enables
`allow_cleartext_dpa=true`.

Set `ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT=false` to prevent local cleartext text
retention after upload. In that mode, `:cleartext-dpa` requests that reference
stored files are rejected because no cleartext derivative is available.
````

- [ ] **Step 3: Run targeted tests**

Run:

```powershell
cargo test -p anno-privacy-gateway file_registry -- --nocapture
cargo test -p anno-privacy-gateway document_extract -- --nocapture
cargo test -p anno-privacy-gateway document_blocks -- --nocapture
cargo test -p anno-privacy-gateway files_api -- --nocapture
cargo test -p anno-privacy-gateway provider_router_file_document -- --nocapture
```

Expected: PASS.

- [ ] **Step 4: Run package verification**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

Expected: PASS.

- [ ] **Step 5: Verify secret and content logging safety**

Run:

```powershell
rg -n "MISTRAL_API_KEY\\s*=|SCALEWAY_API_KEY\\s*=|OVH_AI_ENDPOINTS_ACCESS_TOKEN\\s*=|Bearer [A-Za-z0-9]|Marie Dupont|raw_bytes|cleartext_text" crates\anno-privacy-gateway docs\developers\gateway-api.md docs\user-guide\privacy-gateway.md
```

Expected: no real secrets. Test fixture names such as `Marie Dupont` may appear only inside `#[cfg(test)]` blocks. `raw_bytes` and `cleartext_text` may appear in registry code and tests but not in audit serialization or docs examples.

- [ ] **Step 6: Run GitNexus change detection**

Run:

```powershell
npx gitnexus detect-changes --scope all
```

Expected: changed symbols match `GatewayConfig`, `AppState`, file routes, document expansion, privacy helper, and audit metadata.

- [ ] **Step 7: Commit**

Run:

```powershell
git add crates\anno-privacy-gateway docs\developers\gateway-api.md docs\user-guide\privacy-gateway.md
git commit -m "feat: add gateway file ingress privacy routing"
npx gitnexus analyze
npx gitnexus status
```

Expected: commit succeeds and GitNexus is up to date.

## Acceptance Criteria

- `/v1/files` accepts multipart uploads and returns metadata without file text.
- File IDs use the `anno_file_*` namespace and reject provider-native file IDs.
- `/v1/files/{id}/content` returns pseudonymized extracted text only.
- `/v1/files/{id}` metadata and delete work for gateway-managed files.
- `document` blocks with `source.type = "file"` expand only for `anno_file_*` ids.
- `document` blocks with `source.type = "base64"` are decoded and extracted locally.
- `document` blocks with `source.type = "url"` remain fail-closed.
- Pseudonymized provider-router requests do not send detected raw PII from file text upstream.
- `cleartext_dpa` provider-router requests can send file text cleartext only through Phase 2 DPA gates.
- If local cleartext text retention is disabled, cleartext DPA file references are rejected.
- Audit events contain provider/model/privacy/file counts but no prompt text, file text, or raw bytes.
- Legacy upstream mode still rejects native document blocks.
