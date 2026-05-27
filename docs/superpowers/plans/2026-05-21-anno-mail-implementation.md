# anno-mail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Bichon-inspired email knowledge base (`anno-mail`) with full pseudonymization,
incremental IMAP sync, and search, integrated with anno's existing Detector/Vault/LanceDB pipeline.

**Architecture:** New workspace crate `crates/anno-mail/` reuses `anno-rag`'s `Detector`, `Vault`,
`Embedder`, and `kreuzberg` for email extraction. LanceDB 0.29 stores metadata, chunks, and
sync state. `async-imap` handles IMAP. `axum` serves the REST API. `rmcp` serves MCP tools.

**Tech Stack:** Rust (async tokio), `async-imap` (IMAP), `lancedb =0.29.0`, `kreuzberg` (email parsing),
`anno-rag` (Detector + Vault + Embedder), `axum 0.8` (REST), `rmcp` (MCP), `cloakpipe-core` (vault).

---

### File Map

```
crates/anno-mail/
├── Cargo.toml
├── src/
│   ├── lib.rs                   Public API re-exports
│   ├── record.rs                EmailRecord, ChunkRecord, SyncStateRecord types
│   ├── mime.rs                  kreuzberg-based email body extraction
│   ├── headers.rs               Message-ID, In-Reply-To, References parsing
│   ├── error.rs                 Error enum
│   ├── imap/
│   │   ├── mod.rs               AccountConfig, ConnectionPool
│   │   ├── sync.rs              Incremental UID sync, reconciliation
│   │   └── oauth2.rs            SASL XOAUTH2, PKCE, token refresh
│   ├── pseudonym/
│   │   └── mod.rs               Detect → pseudonymize → embed → store
│   ├── storage/
│   │   ├── mod.rs               Storage coordinator
│   │   ├── schema.rs            Arrow schema definitions (3 tables)
│   │   └── tables.rs            LanceDB table ops (create, upsert, search)
│   ├── thread.rs                Conversation tree reconstruction
│   ├── contacts.rs              Address book extraction + dedup
│   ├── search.rs                Hybrid search (vector + FTS + filters)
│   ├── api/
│   │   ├── mod.rs               axum router setup + AppState
│   │   ├── accounts.rs          CRUD for IMAP accounts
│   │   ├── search_routes.rs     Search endpoints (incl. threads)
│   │   ├── contacts_routes.rs   Contacts endpoint
│   │   └── mcp.rs               MCP tool handlers                   MCP tool handlers
└── tests/
    ├── fixtures/                .eml sample files
    └── imap_test.rs             GreenMail integration tests
```

---

### Task 0: Scaffold crate + workspace registration

**Files:**
- Create: `crates/anno-mail/Cargo.toml`
- Create: `crates/anno-mail/src/lib.rs`
- Create: `crates/anno-mail/src/error.rs`
- Modify: `Cargo.toml` (root workspace `members`)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name         = "anno-mail"
version      = "0.1.0"
edition.workspace      = true
rust-version.workspace = true
license      = "MIT OR Apache-2.0"
description  = "Bichon-inspired email knowledge base with full pseudonymization"

[dependencies]
async-imap = { version = "0.11", features = ["runtime-tokio"] }
imap-proto = "0.3"
oauth2 = { version = "5", default-features = false, features = ["rt-async-std"] }
reqwest = { workspace = true }
kreuzberg = { workspace = true }
anno-rag = { path = "../anno-rag" }
cloakpipe-core = { path = "../../vendor/cloakpipe/crates/cloakpipe-core" }
lancedb = { workspace = true }
lance-index = "=6.0.0"
arrow-array = { workspace = true }
arrow-schema = { workspace = true }
axum = { workspace = true }
tower = "0.5"
tower-http = { version = "0.6", features = ["auth", "cors"] }
rmcp = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
futures = { workspace = true }
blake3 = "1"
regex = { workspace = true }

[dev-dependencies]
tempfile = "3"
proptest = { workspace = true }

[lints]
workspace = true
```

- [ ] **Step 2: Register workspace member**

Add `"crates/anno-mail"` to `members = [...]` in root `Cargo.toml`.

- [ ] **Step 3: Write error.rs**

```rust
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IMAP error: {0}")] Imap(#[from] async_imap::Error),
    #[error("IMAP auth error: {0}")] ImapAuth(String),
    #[error("OAuth2 error: {0}")] Oauth2(oauth2::RequestTokenError<oauth2::reqwest::Error<reqwest::Error>, oauth2::StandardErrorResponse>),
    #[error("Extraction error: {0}")] Extraction(#[from] kreuzberg::Error),
    #[error("LanceDB error: {0}")] Lancedb(#[from] lancedb::Error),
    #[error("Arrow error: {0}")] Arrow(#[from] arrow_schema::ArrowError),
    #[error("Serde error: {0}")] Serde(#[from] serde_json::Error),
    #[error("Vault error: {0}")] Vault(String),
    #[error("Detector error: {0}")] Detector(String),
    #[error("Embedding error: {0}")] Embedding(String),
    #[error("IO error: {0}")] Io(#[from] std::io::Error),
}
```

- [ ] **Step 4: Write lib.rs**

```rust
pub mod error;
pub mod record;
pub mod mime;
pub mod headers;
pub mod pseudonym;
pub mod storage;
pub mod search;
pub mod thread;
pub mod contacts;
pub mod imap;
pub mod api;
pub use record::{EmailRecord, ChunkRecord, SyncStateRecord};
pub use error::{Error, Result};
```

- [ ] **Step 5: Verify crate compiles**

```bash
cd crates/anno-mail && cargo check
```

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/anno-mail/
git commit -m "feat(anno-mail): scaffold crate with workspace registration"
```

---

### Task 1: Core types — EmailRecord, ChunkRecord, SyncStateRecord

**Files:**
- Create: `crates/anno-mail/src/record.rs`

- [ ] **Step 1: Write record.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailRecord {
    pub email_id: Uuid,
    pub account_id: String,
    pub folder_name: String,
    pub uid: i64,
    pub uidvalidity: i64,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub sender_pseudo: String,
    pub recipients_pseudo: Vec<String>,
    pub subject_pseudo: String,
    pub date: DateTime<Utc>,
    pub has_attachment: bool,
    pub attachment_count: i32,
    pub is_deleted: bool,
    pub last_synced_at: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub valid_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ChunkRecord {
    pub chunk_id: Uuid,
    pub email_id: Uuid,
    pub chunk_idx: i32,
    pub text_pseudo: String,
    pub embedding: Vec<f32>,
    pub valid_from: DateTime<Utc>,
    pub valid_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStateRecord {
    pub account_id: String,
    pub folder_name: String,
    pub uidvalidity: i64,
    pub last_uid: i64,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub status: String,
    pub error_msg: Option<String>,
}
```

- [ ] **Step 2: Write test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn test_email_record_creation() {
        let rec = EmailRecord {
            email_id: Uuid::now_v7(),
            account_id: "test".into(),
            folder_name: "INBOX".into(),
            uid: 1,
            uidvalidity: 100,
            message_id: Some("<abc@def>".into()),
            in_reply_to: None,
            references: vec![],
            sender_pseudo: "PERSON_1 <m@ex.fr>".into(),
            recipients_pseudo: vec!["PERSON_2 <p@ex.fr>".into()],
            subject_pseudo: "Re: PERSON_3 report".into(),
            date: Utc.with_ymd_and_hms(2026, 5, 21, 10, 0, 0).unwrap(),
            has_attachment: false,
            attachment_count: 0,
            is_deleted: false,
            last_synced_at: Utc::now(),
            valid_from: Utc::now(),
            valid_to: None,
        };
        assert!(rec.valid_to.is_none());
    }
}
```

- [ ] **Step 3: Run test**

```bash
cd crates/anno-mail && cargo test -- test_email_record_creation
```
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): core EmailRecord, ChunkRecord, SyncStateRecord types"
```

---

### Task 2: MIME parsing + header extraction

**Files:**
- Create: `crates/anno-mail/src/mime.rs`
- Create: `crates/anno-mail/src/headers.rs`
- Create: `tests/fixtures/simple.eml`

- [ ] **Step 1: Create test fixture**

`crates/anno-mail/tests/fixtures/simple.eml`:
```
From: marie@example.fr
To: paul@example.fr
Subject: Meeting reminder
Date: Wed, 21 May 2026 09:00:00 +0200
Message-ID: <abc123@example.fr>

Hi Paul,

Don't forget our meeting at 2 PM.

Best,
Marie
```

- [ ] **Step 2: Write headers.rs**

```rust
use chrono::{DateTime, Utc};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct EmailHeaders {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from_name: Option<String>,
    pub from_addr: Option<String>,
    pub to_names: Vec<String>,
    pub to_addrs: Vec<String>,
    pub subject: String,
    pub date: Option<DateTime<Utc>>,
}

pub fn extract_headers(raw: &[u8]) -> EmailHeaders { ... }
pub fn clean_message_id(raw: &str) -> Option<String> { ... }
pub fn parse_references(raw: &str) -> Vec<String> { ... }
```

Key implementation: split headers from body on `\r\n\r\n`, parse each header
line with regex, handle folded headers (continuation with whitespace prefix).

- [ ] **Step 3: Write mime.rs**

```rust
use kreuzberg::extract_text;
use crate::error::Result;

pub fn extract_body(raw: &[u8]) -> Result<String> {
    let result = extract_text(raw)?;
    Ok(result.text)
}

pub struct AttachmentInfo {
    pub name: Option<String>,
    pub content_type: Option<String>,
    pub size: usize,
}

pub fn extract_attachments(raw: &[u8]) -> Result<Vec<AttachmentInfo>> { ... }
```

- [ ] **Step 4: Write tests**

```rust
#[test]
fn test_extract_headers_simple() {
    let raw = include_bytes!("../tests/fixtures/simple.eml");
    let headers = extract_headers(raw);
    assert_eq!(headers.subject, "Meeting reminder");
    assert_eq!(headers.from_addr.as_deref(), Some("marie@example.fr"));
}

#[test]
fn test_clean_message_id() {
    assert_eq!(clean_message_id("<abc@def>"), Some("abc@def".into()));
    assert_eq!(clean_message_id("abc@def"), Some("abc@def".into()));
}
```

- [ ] **Step 5: Run tests**

```bash
cd crates/anno-mail && cargo test -- test_extract_headers_simple test_clean_message_id
```
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): MIME parsing + header extraction"
```

---

### Task 3: LanceDB storage — schema definitions + table operations

**Files:**
- Create: `crates/anno-mail/src/storage/mod.rs`
- Create: `crates/anno-mail/src/storage/schema.rs`
- Create: `crates/anno-mail/src/storage/tables.rs`

- [ ] **Step 1: Write schema.rs**

Define `email_metadata_schema()`, `email_chunks_schema()`, `sync_state_schema()`
as Arrow `Schema` objects matching the spec (see LanceDB Schema section).

Example for `email_metadata_schema()`:
```rust
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

pub fn email_metadata_schema() -> Schema {
    Schema::new(vec![
        Field::new("email_id", DataType::FixedSizeBinary(16), false),
        Field::new("account_id", DataType::Utf8, false),
        Field::new("folder_name", DataType::Utf8, false),
        Field::new("uid", DataType::Int64, false),
        Field::new("uidvalidity", DataType::Int64, false),
        Field::new("message_id", DataType::Utf8, true),
        Field::new("in_reply_to", DataType::Utf8, true),
        Field::new("references", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("sender_pseudo", DataType::Utf8, false),
        Field::new("recipients_pseudo", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("subject_pseudo", DataType::Utf8, false),
        Field::new("date", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("has_attachment", DataType::Boolean, false),
        Field::new("attachment_count", DataType::Int32, false),
        Field::new("is_deleted", DataType::Boolean, false),
        Field::new("last_synced_at", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("valid_from", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("valid_to", DataType::Timestamp(TimeUnit::Microsecond, None), true),
    ])
}
```

- [ ] **Step 2: Write tables.rs**

```rust
use lancedb::{Connection, Table};
use crate::error::Result;

pub struct EmailTables {
    pub metadata: Table,
    pub chunks: Table,
    pub sync_state: Table,
}

impl EmailTables {
    pub async fn open(db: &Connection) -> Result<Self> { ... }
    pub async fn upsert_metadata(&self, records: &[EmailRecord]) -> Result<()> { ... }
    pub async fn upsert_chunks(&self, records: &[ChunkRecord]) -> Result<()> { ... }
    pub async fn upsert_sync_state(&self, record: &SyncStateRecord) -> Result<()> { ... }
    pub async fn get_sync_state(&self, account_id: &str, folder: &str) -> Result<Option<SyncStateRecord>> { ... }
}
```

Key: use `merge_insert(&["email_id"]).when_matched_update_all(None).when_not_matched_insert_all().execute(...)` for upsert.

- [ ] **Step 3: Write tests**

```rust
#[test]
fn test_metadata_schema_has_expected_fields() {
    let schema = schema::email_metadata_schema();
    assert!(schema.field_with_name("email_id").is_ok());
    assert!(schema.field_with_name("uid").is_ok());
}

#[tokio::test]
async fn test_upsert_metadata_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let db = lancedb::connect(tmp.path().to_str().unwrap()).execute().await.unwrap();
    let tables = EmailTables::open(&db).await.unwrap();
    // upsert a record, count_rows, assert 1
}
```

- [ ] **Step 4: Run tests**

```bash
cd crates/anno-mail && cargo test -- test_metadata_schema_has_expected_fields test_upsert_metadata_roundtrip
```
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): LanceDB storage — schema definitions + table operations"
```

---

### Task 4: Pseudonymization pipeline

**Files:**
- Create: `crates/anno-mail/src/pseudonym/mod.rs`

- [ ] **Step 1: Write pseudonymize_email**

```rust
use anno_rag::vault::Vault;
use anno_rag::Detector;
use cloakpipe_core::DetectedEntity;
use crate::error::{Error, Result};

pub struct EmailPseudonymized {
    pub body_pseudo: String,
    pub subject_pseudo: String,
    pub sender_pseudo: String,
    pub recipients_pseudo: Vec<String>,
    pub body_entity_count: usize,
}

pub async fn pseudonymize_email(
    vault: &Vault,
    detector: &Detector,
    body: &str,
    subject: &str,
    sender_name: &str,
    recipient_names: &[String],
) -> Result<EmailPseudonymized> {
    // Detect PII per field
    let body_entities = detector.detect(body).map_err(|e| Error::Detector(e.to_string()))?;
    let sender_entities = if sender_name.is_empty() { vec![] } else {
        detector.detect(sender_name).map_err(|e| Error::Detector(e.to_string()))?
    };
    let subject_entities = detector.detect(subject).map_err(|e| Error::Detector(e.to_string()))?;
    let mut all_recip_entities = Vec::new();
    for name in recipient_names {
        all_recip_entities.extend(detector.detect(name).map_err(|e| Error::Detector(e.to_string()))?);
    }

    // Pseudonymize per field
    let body_pseudo = vault.pseudonymize(body, &body_entities).await
        .map_err(|e| Error::Vault(e.to_string()))?;
    let sender_pseudo = vault.pseudonymize(sender_name, &sender_entities).await
        .map_err(|e| Error::Vault(e.to_string()))?;
    let subject_pseudo = vault.pseudonymize(subject, &subject_entities).await
        .map_err(|e| Error::Vault(e.to_string()))?;
    let mut recipients_pseudo = Vec::new();
    for (name, entities) in recipient_names.iter().zip(all_recip_entities.chunks(...)) {
        recipients_pseudo.push(vault.pseudonymize(name, entities).await
            .map_err(|e| Error::Vault(e.to_string()))?);
    }

    Ok(EmailPseudonymized {
        body_pseudo,
        subject_pseudo,
        sender_pseudo,
        recipients_pseudo,
        body_entity_count: body_entities.len(),
    })
}
```

- [ ] **Step 2: Write test**

```rust
#[tokio::test]
async fn test_pseudonymize_email_replaces_pii() {
    let vault = Vault::ephemeral_for_test();
    let detector = Detector::new().unwrap();
    let result = pseudonymize_email(
        &vault, &detector,
        "Contact marie@example.fr for details.",
        "Meeting with John Doe",
        "John Doe",
        &["Jane Smith".to_string()],
    ).await.unwrap();
    assert!(!result.body_pseudo.contains("marie@example.fr"));
    assert!(result.body_pseudo.contains("EMAIL_"));
}
```

- [ ] **Step 3: Run test**

```bash
cd crates/anno-mail && cargo test -- test_pseudonymize_email_replaces_pii --nocapture
```

- [ ] **Step 4: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): PII detection + vault pseudonymization pipeline"
```

---

### Task 5: IMAP sync engine

**Files:**
- Create: `crates/anno-mail/src/imap/mod.rs`
- Create: `crates/anno-mail/src/imap/sync.rs`
- Create: `crates/anno-mail/src/imap/oauth2.rs`

- [ ] **Step 1: Write imap/mod.rs**

```rust
use async_imap::Client;
use async_imap::error::Result as ImapResult;
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct AccountConfig {
    pub host: String, pub port: u16, pub tls: TlsMode,
    pub auth: AuthConfig,
    pub folders: Option<Vec<String>>,
    pub sync_interval_minutes: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TlsMode { Tls, StartTls, None }

#[derive(Debug, Clone)]
pub enum AuthConfig {
    Password { username: String, password: String },
    OAuth2 { access_token: String, refresh_token: Option<String>, client_id: String, client_secret: String, tenant: Option<String> },
}

pub async fn connect(cfg: &AccountConfig) -> Result<Session<TlsStream<TcpStream>>> {
    let client = match cfg.tls {
        TlsMode::Tls => Client::connect_tls(&cfg.host, cfg.port, ...).await?,
        TlsMode::StartTls => Client::connect(&cfg.host, cfg.port).await?.upgrade_tls().await?,
        TlsMode::None => Client::connect(&cfg.host, cfg.port).await?,
    };
    match &cfg.auth {
        AuthConfig::Password { username, password } => {
            client.login(username, password).await.map_err(|e| Error::ImapAuth(e.0))?
        }
        AuthConfig::OAuth2 { access_token, .. } => {
            let auth_str = format!("user={}\x01auth=Bearer {}\x01\x01", "email", access_token);
            client.authenticate("XOAUTH2", auth_str).await.map_err(|e| Error::ImapAuth(e.0))?
        }
    }
}
```

- [ ] **Step 2: Write oauth2.rs**

```rust
use oauth2::{basic::BasicClient, AuthUrl, TokenUrl, ClientId, ClientSecret, RefreshToken, TokenResponse};
use crate::error::Result;

pub async fn refresh_access_token(
    client_id: &str, client_secret: &str, refresh_token: &str,
) -> Result<String> {
    let client = BasicClient::new(ClientId::new(client_id.into()))
        .set_client_secret(ClientSecret::new(client_secret.into()))
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".into()).unwrap());
    let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build()?;
    let result = client
        .exchange_refresh_token(RefreshToken::new(refresh_token.into()))
        .request_async(&http)
        .await?;
    Ok(result.access_token().secret().clone())
}
```

- [ ] **Step 3: Write sync.rs**

```rust
use crate::imap::{AccountConfig, connect};
use crate::storage::EmailTables;
use crate::record::SyncStateRecord;
use tokio::sync::Semaphore;

pub struct SyncEngine {
    tables: EmailTables,
    semaphore: Arc<Semaphore>,
}

impl SyncEngine {
    /// Sync a single account with retry logic:
    /// - IMAP timeout: retry 3× with backoff (1s → 5s → 30s)
    /// - Auth failure: skip account, set status=error, log audit, no retry
    /// - Any other failure: retry once after 5s, then mark error
    /// On success: update sync_state last_uid and status=idle
    pub async fn sync_account(&self, config: &AccountConfig) -> Result<()> {
        let mut last_err = None;
        for delay_ms in [0, 1000, 5000, 30000] {
            if delay_ms > 0 { tokio::time::sleep(Duration::from_millis(delay_ms)).await; }
            match self.try_sync_account(config).await {
                Ok(()) => return Ok(()),
                Err(Error::ImapAuth(_)) => {
                    self.tables.set_status(&config, "error").await?;
                    return Err(e); // no retry on auth failure
                }
                Err(e) => { last_err = Some(e); }
            }
        }
        Err(last_err.unwrap())
    }

    async fn try_sync_account(&self, config: &AccountConfig) -> Result<()> { ... }

    /// Fetch new messages via UID delta. On fetch failure:
    /// - Partial fetch (some UIDs succeeded): process what we got, retry missing later
    /// - Total failure: return error for retry layer above
    async fn fetch_delta(session: &mut ..., folder: &str, last_uid: i64) -> Result<Vec<Vec<u8>>> {
        session.select(folder).await?;
        let uids = session.uid_search(format!("{}:*", last_uid + 1)).await?;
        let fetches = session.uid_fetch(uids).await?;
        Ok(fetches.into_iter().filter_map(|f| f.body().map(|b| b.to_vec())).collect())
    }

    async fn reconcile_folder(session: &mut ..., folder: &str, tables: &EmailTables) -> Result<()> {
        // Every N ticks: UID SEARCH ALL, diff against known UIDs, soft-delete missing
        let remote_uids: HashSet<i64> = session.uid_search("1:*").await?.into_iter().collect();
        let local_uids = get_all_uids(tables, account_id, folder).await?;
        for uid in local_uids.difference(&remote_uids) {
            tables.soft_delete(account_id, folder, *uid).await?;
        }
    }
}
```

- [ ] **Step 4: Write tests (lightweight — no IMAP server)**

```rust
#[test]
fn test_uid_range_delta() {
    let last_uid = 42i64;
    assert_eq!(format!("{}:*", last_uid + 1), "43:*");
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): IMAP sync engine — connection, OAuth2, UID delta fetch"
```

---

### Task 6: Full ingestion pipeline

**Files:**
- Modify: `crates/anno-mail/src/pseudonym/mod.rs` (add `ingest_email`)

- [ ] **Step 1: Write ingest_email**

```rust
use uuid::Uuid;

/// Simple text chunker: split text into ~512-char chunks at sentence boundaries.
/// Returns `Vec<Chunk>` with text and byte offset. Used by ingest_email.
pub struct Chunk { pub text: String, pub offset: usize }
pub fn chunk_text(text: &str, max_size: usize) -> Vec<Chunk> {
    text.split_inclusive(|c: char| c == '.' || c == '!' || c == '?')
        .scan(String::new(), move |buf, sent| {
            if buf.len() + sent.len() > max_size && !buf.is_empty() {
                let chunk = std::mem::take(buf);
                Some(Some(Chunk { text: chunk, offset: 0 }))
            } else {
                buf.push_str(sent);
                Some(None)
            }
        })
        .flatten()
        .collect()
}

/// End-to-end: raw bytes → parse → detect → pseudonymize → embed → store
pub async fn ingest_email(
    vault: &Vault,
    detector: &Detector,
    embedder: &Embedder,
    tables: &EmailTables,
    account_id: &str,
    folder_name: &str,
    uid: i64,
    uidvalidity: i64,
    raw_bytes: &[u8],
) -> Result<Uuid> {
    let headers = crate::headers::extract_headers(raw_bytes);
    let body = crate::mime::extract_body(raw_bytes)?;
    let attachments = crate::mime::extract_attachments(raw_bytes)?;
    let pseudo = pseudonymize_email(
        vault, detector, &body, &headers.subject,
        headers.from_name.as_deref().unwrap_or(""),
        &headers.to_names,
    ).await?;
    let email_id = Uuid::now_v7();
    let now = Utc::now();
    let record = EmailRecord {
        email_id, account_id: account_id.into(), folder_name: folder_name.into(),
        uid, uidvalidity,
        message_id: headers.message_id, in_reply_to: headers.in_reply_to,
        references: headers.references,
        sender_pseudo: pseudo.sender_pseudo,
        recipients_pseudo: pseudo.recipients_pseudo,
        subject_pseudo: pseudo.subject_pseudo,
        date: headers.date.unwrap_or(now),
        has_attachment: !attachments.is_empty(),
        attachment_count: attachments.len() as i32,
        is_deleted: false,
        last_synced_at: now, valid_from: now, valid_to: None,
    };

    // Chunk + embed
    let chunks = chunk_text(&pseudo.body_pseudo, 512);
    let vectors = embedder.embed_batch(&chunks.iter().map(|c| c.text.clone()).collect::<Vec<_>>())?;
    let chunk_records: Vec<ChunkRecord> = chunks.into_iter().zip(vectors).enumerate().map(
        |(i, (chunk, emb))| ChunkRecord {
            chunk_id: Uuid::now_v7(), email_id, chunk_idx: i as i32,
            text_pseudo: chunk.text, embedding: emb,
            valid_from: now, valid_to: None,
        }
    ).collect();
    tables.upsert_metadata(&[record]).await?;
    tables.upsert_chunks(&chunk_records).await?;
    Ok(email_id)
}
```

- [ ] **Step 2: Write E2E test with fixture .eml**

```rust
#[tokio::test]
async fn test_ingest_email_roundtrip() {
    let raw = include_bytes!("../tests/fixtures/simple.eml");
    let vault = Vault::ephemeral_for_test();
    let detector = Detector::new().unwrap();
    let embedder = Embedder::load(&AnnoRagConfig::default()).await.unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let db = lancedb::connect(tmp.path().to_str().unwrap()).execute().await.unwrap();
    let tables = EmailTables::open(&db).await.unwrap();
    let email_id = ingest_email(&vault, &detector, &embedder, &tables, "test", "INBOX", 1, 100, raw).await.unwrap();
    let results = tables.search_chunks(&[0.0; 384], &Default::default(), 10).await.unwrap();
    assert!(!results.is_empty());
}
```

- [ ] **Step 3: Run test**

```bash
cd crates/anno-mail && cargo test -- test_ingest_email_roundtrip --nocapture
```

- [ ] **Step 4: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): full ingestion pipeline — parse → pseudonymize → embed → store"
```

---

### Task 7: Thread reconstruction + contacts dedup

**Files:**
- Create: `crates/anno-mail/src/thread.rs`
- Create: `crates/anno-mail/src/contacts.rs`

- [ ] **Step 1: Write thread.rs**

```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ThreadNode {
    pub email_id: uuid::Uuid,
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub subject: String, pub sender: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub children: Vec<ThreadNode>,
}

pub fn build_thread(emails: &[ThreadNodeInput]) -> Vec<ThreadNode> {
    let mut map: HashMap<String, ThreadNode> = HashMap::new();
    // 1. Build all nodes
    // 2. Assign children by in_reply_to
    // 3. Collect roots (no parent found)
}
```

- [ ] **Step 2: Write contacts.rs**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Contact {
    pub display_name_pseudo: String,
    pub raw_email: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

pub fn extract_contacts(emails: &[ContactInput]) -> Vec<Contact> {
    // Dedup by raw_email, prefer most recent display_name
}
```

- [ ] **Step 3: Write tests**

```rust
#[test]
fn test_build_thread_parent_child() {
    let input = vec![
        node("1", None),
        node("2", Some("1")),
    ];
    let tree = build_thread(&input);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].children.len(), 1);
}

#[test]
fn test_extract_contacts_dedup() {
    let input = vec![
        contact("m@ex.fr", "PERSON_1"),
        contact("m@ex.fr", "PERSON_1"),
        contact("p@ex.fr", "PERSON_2"),
    ];
    assert_eq!(extract_contacts(&input).len(), 2);
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): thread reconstruction + contacts dedup"
```

---

### Task 8: Hybrid search

**Files:**
- Create: `crates/anno-mail/src/search.rs`

- [ ] **Step 1: Write search.rs**

```rust
use crate::storage::EmailTables;

#[derive(Debug, Default)]
pub struct SearchFilters {
    pub account_id: Option<String>,
    pub date_from: Option<chrono::DateTime<chrono::Utc>>,
    pub date_to: Option<chrono::DateTime<chrono::Utc>>,
    pub sender: Option<String>,
    pub folder_name: Option<String>,
    pub has_attachment: Option<bool>,
    pub query_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk_id: uuid::Uuid,
    pub email_id: uuid::Uuid,
    pub text_pseudo: String,
    pub score: f64,
}

pub async fn hybrid_search(
    tables: &EmailTables,
    query_embedding: &[f32],
    filters: &SearchFilters,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let mut query = tables.chunks.vector_search(query_embedding)?.limit(limit as u32);
    if let Some(account) = &filters.account_id {
        query = query.column("account_id").eq(account)?;
    }
    // ... apply other filters ...
    let results = query.execute().await?;
    Ok(results.map(|r| SearchResult { ... }).collect())
}
```

- [ ] **Step 2: Write test**

```rust
#[tokio::test]
async fn test_hybrid_search_returns_results() {
    // Seed known chunk, search for it
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): hybrid search (vector + FTS + filters)"
```

---

### Task 9: Integration tests with GreenMail

**Files:**
- Create: `crates/anno-mail/tests/imap_test.rs`

- [ ] **Step 1: Write E2E test**

```rust
/// Requires Docker: docker run -d --name greenmail -p 3143:3143 -p 3025:3025
///   -e GREENMAIL_OPTS='-Dgreenmail.setup.test.all -Dgreenmail.hostname=0.0.0.0'
///   greenmail/standalone:1.5.9

#[tokio::test]
#[ignore = "requires Docker with GreenMail"]
async fn test_e2e_imap_sync_pseudonymize_search() {
    let cfg = AccountConfig {
        host: "127.0.0.1".into(), port: 3143, tls: TlsMode::None,
        auth: AuthConfig::Password { username: "test@localhost".into(), password: "test".into() },
        folders: None, sync_interval_minutes: 10,
    };
    // Inject email via GreenMail SMTP
    // Run sync
    // Search — assert found
}

#[tokio::test]
#[ignore = "requires Docker with GreenMail"]
async fn test_incremental_sync_only_new() {
    // Inject email A, sync, assert last_uid
    // Inject email B, sync, assert only 1 new processed
}
```

- [ ] **Step 2: Add more test scenarios**

```rust
#[tokio::test]
#[ignore = "requires Docker with GreenMail"]
async fn test_uidvalidity_change_triggers_rebuild() {
    // 1. Sync account, verify metadata stored
    // 2. Restart GreenMail (new UIDVALIDITY)
    // 3. Sync again — verify full rebuild (all UIDs refetched, re-pseudonymized)
    // 4. Verify search still returns previous emails
}

#[tokio::test]
#[ignore = "requires Docker with GreenMail"]
async fn test_thread_reconstruction() {
    // 1. Inject parent email + 3 replies (via SMTP)
    // 2. Sync
    // 3. Call build_thread — verify tree has 1 root + 3 children
}

#[tokio::test]
#[ignore = "requires Docker with GreenMail"]
async fn test_contacts_extracted() {
    // 1. Inject 2 emails from same sender
    // 2. Sync
    // 3. Call extract_contacts — verify 1 unique contact
}

#[tokio::test]
#[ignore = "requires Docker with GreenMail"]
async fn test_oauth2_token_refresh() {
    // Uses a mock OAuth2 token endpoint instead of real GreenMail
    // 1. Start local HTTP server returning valid OAuth2 tokens
    // 2. Configure account with OAuth2 + refresh_token
    // 3. Sync — verify access token is requested and IMAP login succeeds
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/anno-mail/tests/
git commit -m "test(anno-mail): GreenMail integration tests (E2E + incremental + UIDVALIDITY + threads + contacts)"
```

---

### Task 10: REST API — account management

**Files:**
- Create: `crates/anno-mail/src/api/mod.rs`
- Create: `crates/anno-mail/src/api/accounts.rs`
- Create: `crates/anno-mail/src/bin/server.rs`

- [ ] **Step 1: Write api/mod.rs (router + AppState)**

```rust
use axum::{Router, routing::{get, post, delete}};
use std::sync::Arc;

pub struct AppState {
    pub tables: EmailTables,
    pub vault: Arc<Vault>,
    pub detector: Arc<Detector>,
    pub embedder: Arc<Embedder>,
    pub api_token: String,
}

pub async fn vault_stats(State(state): State<AppState>) -> Json<VaultStats> { ... }

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/accounts", get(accounts::list).post(accounts::create))
        .route("/api/v1/accounts/:id", delete(accounts::delete))
        .route("/api/v1/accounts/:id/sync", post(accounts::sync)
            .get(accounts::sync_status).delete(accounts::cancel_sync))
        .route("/api/v1/search", get(search_routes::search))
        .route("/api/v1/search/threads", get(search_routes::threads))
        .route("/api/v1/contacts", get(contacts_routes::list))
        .route("/api/v1/vault/stats", get(vault_stats))
        .layer(RequireAuthorizationLayer::bearer(&state.api_token))
        .with_state(state)
}
```

- [ ] **Step 2: Write accounts.rs handlers**

```rust
pub async fn list(State(state): State<AppState>) -> Json<Vec<AccountConfig>> { ... }
pub async fn create(State(state): State<AppState>, Json(body): Json<AccountConfig>) -> impl IntoResponse { ... }
pub async fn delete(Path(id): Path<String>, State(state): State<AppState>) -> impl IntoResponse { ... }
pub async fn sync(Path(id): Path<String>, State(state): State<AppState>) -> impl IntoResponse { ... }
pub async fn sync_status(Path(id): Path<String>, State(state): State<AppState>) -> Json<SyncStatus> { ... }
pub async fn cancel_sync(Path(id): Path<String>, State(state): State<AppState>) -> impl IntoResponse { ... }
```

- [ ] **Step 3: Write bin/server.rs**

```rust
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cfg = load_config_from_env();
    let vault = Arc::new(Vault::open(&cfg.vault_path(), anno_rag::vault::derive_key()?)?);
    let tables = EmailTables::open(&lancedb::connect(&cfg.data_dir).execute().await?).await?;
    let api_token = std::env::var("ANNO_MAIL_API_KEY")
        .unwrap_or_else(|_| "dev-token".to_string());
    let app = router(AppState {
        tables, vault, api_token,
        detector: Arc::new(Detector::new()?),
        embedder: Arc::new(Embedder::load(&cfg)?),
    });
    let listener = tokio::net::TcpListener::bind("0.0.0.0:15631").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): REST API — account management CRUD + sync control"
```

---

### Task 11: REST API — search + contacts routes

**Files:**
- Create: `crates/anno-mail/src/api/search_routes.rs`
- Create: `crates/anno-mail/src/api/contacts_routes.rs`

- [ ] **Step 1: Write search_routes.rs**

```rust
#[derive(Deserialize)]
pub struct SearchParams {
    pub q: Option<String>, pub account_id: Option<String>,
    pub date_from: Option<String>, pub date_to: Option<String>,
    pub sender: Option<String>, pub folder: Option<String>,
    pub has_attachment: Option<bool>, pub limit: Option<usize>,
}

pub async fn search(State(state): State<AppState>, Query(params): Query<SearchParams>) -> Json<Vec<SearchResult>> { ... }
```

- [ ] **Step 2: Write contacts_routes.rs**

```rust
pub async fn list(State(state): State<AppState>) -> Json<Vec<Contact>> { ... }
```

- [ ] **Step 3: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): REST API — search + contacts endpoints"
```

---

### Task 12: MCP server (standalone binary)

**Files:**
- Modify: `crates/anno-mail/Cargo.toml` (add MCP binary)
- Create: `crates/anno-mail/src/api/mcp.rs`
- Create: `crates/anno-mail/src/bin/mcp.rs`

- [ ] **Step 1: Write mcp.rs tool handlers**

Use `rmcp` macros (follow `anno-rag-mcp/src/lib.rs` pattern):

```rust
use rmcp::service::Service;
use rmcp::tool_router;

#[tool_router]
pub struct AnnoMailServer { state: AppState }

// Tools: mail_search, mail_thread, mail_contacts, mail_accounts, mail_sync, mail_rehydrate
```

- [ ] **Step 2: Write bin/mcp.rs**

```rust
#[tokio::main]
async fn main() -> Result<()> { /* init state, run Service */ }
```

Add to `Cargo.toml`:
```toml
[[bin]]
name = "anno-mail-mcp"
path = "src/bin/mcp.rs"
```

- [ ] **Step 3: Verify compilation**

```bash
cd crates/anno-mail && cargo check
```

- [ ] **Step 4: Commit**

```bash
git add crates/anno-mail/
git commit -m "feat(anno-mail): standalone MCP server with email tools"
```

---

### Task 13: Property-based tests

**Files:**
- Create: `crates/anno-mail/tests/proptest.rs`

- [ ] **Step 1: Write proptests**

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn pseudonymize_roundtrip_identity(text: String) {
        prop_assume!(!text.is_empty());
        // detect → pseudonymize → rehydrate = original text
    }

    #[test]
    fn thread_tree_acyclic(input: Vec<(String, Option<String>)>) {
        // DFS, assert no cycles
    }

    #[test]
    fn merge_insert_idempotent(records: Vec<EmailRecord>) {
        // Insert batch A → insert batch A again → assert row count unchanged
        // (merge_insert with same keys is a no-op for existing rows)
    }

    #[test]
    fn uid_monotonicity(uids: Vec<u64>) {
        // After any sequence of syncs, UIDs are monotonically increasing
        let mut sorted = uids.clone();
        sorted.sort();
        // Simulate: verify no UID regresses
    }
}
```

- [ ] **Step 2: Run proptests**

```bash
cd crates/anno-mail && cargo test -- proptest --nocapture
```

- [ ] **Step 3: Commit**

```bash
git add crates/anno-mail/
git commit -m "test(anno-mail): property-based tests (roundtrip, monotonicity, acyclicity)"
```

---

### Dependency Order

```
Task 0 → Task 1 → Task 2 → Task 3
                              ↓
        Task 5 → Task 4 → Task 6
                              ↓
                      Task 7 + Task 8
                              ↓
                Task 9 (needs Docker)
                              ↓
              Task 10 + Task 11 → Task 12
                              ↓
                         Task 13
```

All Tasks 0-8 work without a real IMAP server (use local .eml fixtures).
Task 9 requires Docker + GreenMail for E2E integration.
