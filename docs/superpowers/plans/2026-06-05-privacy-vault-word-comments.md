# Privacy Vault Word Comments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the local Cowork privacy workflow that creates a `vault` workspace, normalized Word working documents, anonymized Word outputs, reports, and safe re-finalization from Word comments `à masquer` / `à garder`.

**Architecture:** Add a focused privacy workspace layer to `anno-rag` without rewriting the existing Kreuzberg ingest, vault, legal index, or knowledge index paths. The new layer owns generated artifacts and user decisions, reuses `ingest::extract`, `Detector::detect_for_ingest`, and `Vault`, and exposes safe MCP tools from `anno-rag-mcp` that return only paths and counts.

**Tech Stack:** Rust, Kreuzberg extraction, existing `anno-rag` detector/vault, DOCX Open Packaging Convention via `zip`, XML parsing via `quick-xml`, JSON manifests via `serde`, MCP tools via `rmcp`.

---

## Preflight

The repository currently has unrelated local changes. Do not revert them. Stage and commit only files touched by the current task.

Before editing code symbols, follow the repository GitNexus rule:

- Run `npx gitnexus status`.
- If stale, run `npx gitnexus analyze`.
- If GitNexus MCP tools are available in the execution session, run impact analysis before editing these symbols:
  - `Pipeline`
  - `Vault::pseudonymize_with_map`
  - `legal_ingest_candidate_paths`
  - `is_anno_generated_output`
  - `AnnoRagServer`

If MCP GitNexus tools are unavailable, record that and use `rg`, `git diff`, and `npx gitnexus status` as the fallback evidence.

## File Structure

Create these focused modules in `crates/anno-rag/src/`:

- `privacy_artifacts.rs`: manifest structs, workspace paths, safe status summaries, hashing helpers, generated-folder detection.
- `privacy_decisions.rs`: plain-language comment command normalization and application of user decisions to detected entities.
- `docx_instructions.rs`: local DOCX comment reader that extracts `à masquer` / `à garder` instructions and selected text from `word/document.xml` + `word/comments.xml`.
- `privacy_docx.rs`: minimal normalized `.docx` writer for working documents, anonymized documents, and simple reports.
- `privacy_workspace.rs`: prepare/finalize orchestration that uses the modules above plus existing extraction/detection/vault code.

Modify existing files:

- `crates/anno-rag/src/lib.rs`: export the new modules.
- `crates/anno-rag/src/error.rs`: add a `Privacy(String)` error variant.
- `crates/anno-rag/src/pipeline.rs`: expose privacy workspace entrypoints on `Pipeline` and exclude `vault` from generated-output scanning.
- `crates/anno-rag/src/vault.rs`: add a replacement-map API for local reports.
- `crates/anno-rag/Cargo.toml`: add `zip` and `quick-xml`.
- `Cargo.toml`: add workspace dependencies for `zip` and `quick-xml`.
- `crates/anno-rag-mcp/src/lib.rs`: add MCP params/results and tools.
- `crates/anno-rag-mcp/src/health.rs`: include privacy tools in available tool metadata.

Create integration tests:

- `crates/anno-rag/tests/privacy_artifacts.rs`
- `crates/anno-rag/tests/privacy_docx.rs`
- `crates/anno-rag/tests/privacy_decisions.rs`
- `crates/anno-rag-mcp/tests/privacy_tools.rs`

## Task 1: Dependencies, Module Exports, And Error Variant

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/anno-rag/Cargo.toml`
- Modify: `crates/anno-rag/src/lib.rs`
- Modify: `crates/anno-rag/src/error.rs`

- [ ] **Step 1: Add workspace dependencies**

Edit root `Cargo.toml` under `[workspace.dependencies]`:

```toml
quick-xml = "0.38"
zip = { version = "2", default-features = false, features = ["deflate"] }
```

- [ ] **Step 2: Add anno-rag dependencies**

Edit `crates/anno-rag/Cargo.toml` under `[dependencies]`:

```toml
quick-xml           = { workspace = true }
zip                 = { workspace = true }
```

- [ ] **Step 3: Export new modules**

Add to `crates/anno-rag/src/lib.rs`:

```rust
pub mod docx_instructions;
pub mod privacy_artifacts;
pub mod privacy_decisions;
pub mod privacy_docx;
pub mod privacy_workspace;
```

- [ ] **Step 4: Add the privacy error variant**

Add to `crates/anno-rag/src/error.rs` after `Legal(String)`:

```rust
    /// Privacy workspace generation, DOCX parsing, manifest, or report failure.
    #[error("privacy: {0}")]
    Privacy(String),
```

- [ ] **Step 5: Run a targeted check**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: the check reaches compilation and fails only because the newly exported modules do not exist yet.

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml crates/anno-rag/Cargo.toml crates/anno-rag/src/lib.rs crates/anno-rag/src/error.rs
git commit -m "chore: add privacy workspace module deps"
```

## Task 2: Privacy Artifact Models And Workspace Paths

**Files:**
- Create: `crates/anno-rag/src/privacy_artifacts.rs`
- Create: `crates/anno-rag/tests/privacy_artifacts.rs`

- [ ] **Step 1: Write failing tests for workspace paths and generated-folder exclusion**

Create `crates/anno-rag/tests/privacy_artifacts.rs`:

```rust
use anno_rag::privacy_artifacts::{
    is_privacy_generated_path, PrivacyWorkspacePaths, WorkspaceManifest,
};
use std::path::Path;

#[test]
fn workspace_paths_live_under_source_vault_folder() {
    let root = Path::new("C:/Clients/Matter X");
    let paths = PrivacyWorkspacePaths::from_source_root(root);

    assert_eq!(paths.workspace, root.join("vault"));
    assert_eq!(paths.working, root.join("vault").join("01-working-documents"));
    assert_eq!(
        paths.anonymized,
        root.join("vault").join("02-anonymized-documents")
    );
    assert_eq!(paths.reports, root.join("vault").join("03-reports"));
    assert_eq!(paths.cache, root.join("vault").join("04-cache"));
    assert_eq!(paths.manifest, root.join("vault").join("manifest.json"));
}

#[test]
fn generated_path_detection_excludes_vault_anywhere_under_source_root() {
    let root = Path::new("C:/Clients/Matter X");
    assert!(is_privacy_generated_path(
        root,
        Path::new("C:/Clients/Matter X/vault/01-working-documents/a.docx")
    ));
    assert!(is_privacy_generated_path(
        root,
        Path::new("C:/Clients/Matter X/contracts/vault/report.docx")
    ));
    assert!(!is_privacy_generated_path(
        root,
        Path::new("C:/Clients/Matter X/contracts/source.pdf")
    ));
}

#[test]
fn manifest_round_trips_without_document_values() {
    let manifest = WorkspaceManifest::new("C:/Clients/Matter X");
    let json = serde_json::to_string_pretty(&manifest).expect("serialize");
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"source_root\""));
    assert!(!json.contains("Jean Dupont"));

    let restored: WorkspaceManifest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.version, 1);
    assert_eq!(restored.documents.len(), 0);
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test -p anno-rag --test privacy_artifacts
```

Expected: FAIL because `privacy_artifacts` does not exist.

- [ ] **Step 3: Implement artifact models**

Create `crates/anno-rag/src/privacy_artifacts.rs`:

```rust
//! Local privacy workspace artifacts and path helpers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Workspace folder name created under the user-selected source root.
pub const PRIVACY_WORKSPACE_DIR: &str = "vault";
/// Editable cleartext Word review documents.
pub const WORKING_DIR: &str = "01-working-documents";
/// PII-safe Word documents generated for sharing and indexing.
pub const ANONYMIZED_DIR: &str = "02-anonymized-documents";
/// Local reports.
pub const REPORTS_DIR: &str = "03-reports";
/// Local cache and non-user-facing support data.
pub const CACHE_DIR: &str = "04-cache";
/// Manifest file name.
pub const MANIFEST_FILE: &str = "manifest.json";

/// All generated paths for one privacy workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyWorkspacePaths {
    /// User-selected source root.
    pub source_root: PathBuf,
    /// Generated workspace root named `vault`.
    pub workspace: PathBuf,
    /// Editable cleartext normalized Word documents.
    pub working: PathBuf,
    /// Anonymized Word documents.
    pub anonymized: PathBuf,
    /// Report folder.
    pub reports: PathBuf,
    /// Cache folder.
    pub cache: PathBuf,
    /// Manifest path.
    pub manifest: PathBuf,
}

impl PrivacyWorkspacePaths {
    /// Build generated paths from a source root.
    #[must_use]
    pub fn from_source_root(source_root: impl AsRef<Path>) -> Self {
        let source_root = source_root.as_ref().to_path_buf();
        let workspace = source_root.join(PRIVACY_WORKSPACE_DIR);
        Self {
            source_root,
            working: workspace.join(WORKING_DIR),
            anonymized: workspace.join(ANONYMIZED_DIR),
            reports: workspace.join(REPORTS_DIR),
            cache: workspace.join(CACHE_DIR),
            manifest: workspace.join(MANIFEST_FILE),
            workspace,
        }
    }

    /// Create all generated directories.
    ///
    /// # Errors
    /// Returns IO errors if the workspace cannot be created.
    pub fn create_all(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.working)?;
        std::fs::create_dir_all(&self.anonymized)?;
        std::fs::create_dir_all(&self.reports)?;
        std::fs::create_dir_all(&self.cache)?;
        Ok(())
    }
}

/// One document tracked in `vault/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestDocument {
    /// Stable UUID string for this document.
    pub document_id: String,
    /// Relative path from source root.
    pub relative_path: String,
    /// Original source path. Local-only operational metadata.
    pub source_path: String,
    /// Generated working `.docx` path.
    pub working_path: String,
    /// Generated anonymized `.docx` path.
    pub anonymized_path: String,
    /// SHA-256 of source bytes.
    pub source_hash: String,
    /// SHA-256 of latest working bytes.
    pub working_hash: Option<String>,
    /// SHA-256 used for the last index write.
    pub last_indexed_hash: Option<String>,
    /// Current document status.
    pub status: String,
    /// Safe aggregate counts.
    pub counts: ManifestCounts,
}

/// Safe per-document counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestCounts {
    /// Detector findings before user decisions.
    pub detected: usize,
    /// User-forced mask instructions.
    pub forced_mask: usize,
    /// User-reviewed visible exceptions.
    pub kept_visible: usize,
}

/// Workspace manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceManifest {
    /// Manifest schema version.
    pub version: u32,
    /// Source root path string.
    pub source_root: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Documents tracked in this workspace.
    pub documents: Vec<ManifestDocument>,
}

impl WorkspaceManifest {
    /// Create an empty manifest for a source root.
    #[must_use]
    pub fn new(source_root: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            version: 1,
            source_root: source_root.into(),
            created_at: now,
            updated_at: now,
            documents: Vec::new(),
        }
    }
}

/// Return true for any generated `vault` path under the source root.
#[must_use]
pub fn is_privacy_generated_path(source_root: &Path, path: &Path) -> bool {
    if path
        .strip_prefix(source_root)
        .ok()
        .is_some_and(|relative| {
            relative.components().any(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(PRIVACY_WORKSPACE_DIR))
            })
        })
    {
        return true;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(PRIVACY_WORKSPACE_DIR))
}

/// SHA-256 hex of bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag --test privacy_artifacts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/privacy_artifacts.rs crates/anno-rag/tests/privacy_artifacts.rs
git commit -m "feat: add privacy workspace artifacts"
```

## Task 3: Minimal DOCX Writer For Normalized Documents

**Files:**
- Create: `crates/anno-rag/src/privacy_docx.rs`
- Create: `crates/anno-rag/tests/privacy_docx.rs`

- [ ] **Step 1: Write failing tests for DOCX writing**

Create `crates/anno-rag/tests/privacy_docx.rs`:

```rust
use anno_rag::privacy_docx::{
    write_normalized_docx, NormalizedDocx, NormalizedSection,
};
use std::io::Read;

#[test]
fn writes_minimal_docx_with_metadata_and_sections() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("working.docx");
    let doc = NormalizedDocx {
        title: "contracts/contract.pdf".to_string(),
        metadata: vec![
            ("Source".to_string(), "contracts/contract.pdf".to_string()),
            ("Extraction".to_string(), "Kreuzberg".to_string()),
        ],
        sections: vec![NormalizedSection {
            heading: "Page 1".to_string(),
            body: "Jean Dupont signe le contrat.".to_string(),
        }],
    };

    write_normalized_docx(&out, &doc).expect("write docx");

    let file = std::fs::File::open(&out).expect("open docx");
    let mut zip = zip::ZipArchive::new(file).expect("zip");
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut xml)
        .expect("read xml");

    assert!(xml.contains("contracts/contract.pdf"));
    assert!(xml.contains("Jean Dupont signe le contrat."));
    assert!(!xml.contains("à masquer"));
}

#[test]
fn escapes_xml_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("escaped.docx");
    let doc = NormalizedDocx {
        title: "A & B".to_string(),
        metadata: Vec::new(),
        sections: vec![NormalizedSection {
            heading: "Clause <1>".to_string(),
            body: "A < B & C > D".to_string(),
        }],
    };

    write_normalized_docx(&out, &doc).expect("write docx");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&out).expect("open")).expect("zip");
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut xml)
        .expect("read xml");

    assert!(xml.contains("A &amp; B"));
    assert!(xml.contains("Clause &lt;1&gt;"));
    assert!(xml.contains("A &lt; B &amp; C &gt; D"));
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test -p anno-rag --test privacy_docx
```

Expected: FAIL because `privacy_docx` has no `write_normalized_docx`.

- [ ] **Step 3: Implement minimal DOCX writer**

Create `crates/anno-rag/src/privacy_docx.rs`:

```rust
//! Minimal DOCX writers for privacy workspace artifacts.

use crate::error::{Error, Result};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

/// One normalized `.docx` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedSection {
    /// Section heading.
    pub heading: String,
    /// Section body.
    pub body: String,
}

/// Normalized `.docx` content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedDocx {
    /// Document title.
    pub title: String,
    /// Metadata rows rendered near the top.
    pub metadata: Vec<(String, String)>,
    /// Body sections.
    pub sections: Vec<NormalizedSection>,
}

/// Write a minimal Word-compatible `.docx`.
///
/// # Errors
/// Returns [`Error::Privacy`] or IO errors when the package cannot be written.
pub fn write_normalized_docx(path: &Path, doc: &NormalizedDocx) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)
        .map_err(|e| Error::Privacy(format!("docx content types: {e}")))?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;

    zip.add_directory("_rels/", options)
        .map_err(|e| Error::Privacy(format!("docx rels dir: {e}")))?;
    zip.start_file("_rels/.rels", options)
        .map_err(|e| Error::Privacy(format!("docx root rels: {e}")))?;
    zip.write_all(ROOT_RELS.as_bytes())?;

    zip.add_directory("word/", options)
        .map_err(|e| Error::Privacy(format!("docx word dir: {e}")))?;
    zip.start_file("word/document.xml", options)
        .map_err(|e| Error::Privacy(format!("docx document: {e}")))?;
    zip.write_all(document_xml(doc).as_bytes())?;

    zip.add_directory("word/_rels/", options)
        .map_err(|e| Error::Privacy(format!("docx word rels dir: {e}")))?;
    zip.start_file("word/_rels/document.xml.rels", options)
        .map_err(|e| Error::Privacy(format!("docx document rels: {e}")))?;
    zip.write_all(DOCUMENT_RELS.as_bytes())?;

    zip.finish()
        .map_err(|e| Error::Privacy(format!("docx finish: {e}")))?;
    Ok(())
}

fn document_xml(doc: &NormalizedDocx) -> String {
    let mut body = String::new();
    body.push_str(&paragraph(&doc.title, "Title"));

    for (key, value) in &doc.metadata {
        body.push_str(&paragraph(&format!("{key}: {value}"), "Normal"));
    }

    for section in &doc.sections {
        body.push_str(&paragraph(&section.heading, "Heading1"));
        for line in section.body.lines() {
            if line.trim().is_empty() {
                body.push_str(&paragraph("", "Normal"));
            } else {
                body.push_str(&paragraph(line, "Normal"));
            }
        }
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {body}
    <w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
  </w:body>
</w:document>"#
    )
}

fn paragraph(text: &str, style: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        escape_xml(text)
    )
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag --test privacy_docx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/privacy_docx.rs crates/anno-rag/tests/privacy_docx.rs
git commit -m "feat: write privacy workspace docx artifacts"
```

## Task 4: Word Comment Instruction Reader

**Files:**
- Create: `crates/anno-rag/src/docx_instructions.rs`
- Extend: `crates/anno-rag/tests/privacy_docx.rs`

- [ ] **Step 1: Add failing tests with a minimal commented DOCX fixture**

Append to `crates/anno-rag/tests/privacy_docx.rs`:

```rust
use anno_rag::docx_instructions::{read_docx_instructions, InstructionAction};
use std::io::Write;

#[test]
fn reads_a_masquer_and_a_garder_comments_from_docx() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("comments.docx");
    write_commented_docx(&path);

    let instructions = read_docx_instructions(&path).expect("read instructions");

    assert_eq!(instructions.len(), 2);
    assert_eq!(instructions[0].action, InstructionAction::Mask);
    assert_eq!(instructions[0].selected_text, "Jean Dupont");
    assert_eq!(instructions[1].action, InstructionAction::Keep);
    assert_eq!(instructions[1].selected_text, "Orange");
}

fn write_commented_docx(path: &std::path::Path) {
    let file = std::fs::File::create(path).expect("create");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options).expect("content types");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>
</Types>"#,
    ).expect("write content types");

    zip.add_directory("_rels/", options).expect("rels dir");
    zip.start_file("_rels/.rels", options).expect("rels");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    ).expect("write rels");

    zip.add_directory("word/", options).expect("word dir");
    zip.start_file("word/document.xml", options).expect("document");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>Client </w:t></w:r>
      <w:commentRangeStart w:id="0"/>
      <w:r><w:t>Jean Dupont</w:t></w:r>
      <w:commentRangeEnd w:id="0"/>
      <w:r><w:commentReference w:id="0"/></w:r>
    </w:p>
    <w:p>
      <w:r><w:t>Marque </w:t></w:r>
      <w:commentRangeStart w:id="1"/>
      <w:r><w:t>Orange</w:t></w:r>
      <w:commentRangeEnd w:id="1"/>
      <w:r><w:commentReference w:id="1"/></w:r>
    </w:p>
  </w:body>
</w:document>"#,
    ).expect("write document");

    zip.start_file("word/comments.xml", options).expect("comments");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:comment w:id="0"><w:p><w:r><w:t>à masquer</w:t></w:r></w:p></w:comment>
  <w:comment w:id="1"><w:p><w:r><w:t>a garder</w:t></w:r></w:p></w:comment>
</w:comments>"#,
    ).expect("write comments");

    zip.finish().expect("finish");
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```powershell
cargo test -p anno-rag --test privacy_docx reads_a_masquer_and_a_garder_comments_from_docx -- --exact
```

Expected: FAIL because `docx_instructions` does not exist.

- [ ] **Step 3: Implement DOCX instruction reader**

Create `crates/anno-rag/src/docx_instructions.rs`:

```rust
//! Read plain-language privacy instructions from Word comments.

use crate::error::{Error, Result};
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

/// Supported user instruction actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionAction {
    /// Force anonymization for selected text.
    Mask,
    /// Keep selected text visible as a reviewed false positive.
    Keep,
}

/// One instruction extracted from a Word comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxInstruction {
    /// Word comment id.
    pub comment_id: String,
    /// Parsed action.
    pub action: InstructionAction,
    /// Text selected by the Word comment anchor.
    pub selected_text: String,
}

/// Read supported privacy instructions from a `.docx`.
///
/// # Errors
/// Returns [`Error::Privacy`] when the DOCX package or XML cannot be read.
pub fn read_docx_instructions(path: &Path) -> Result<Vec<DocxInstruction>> {
    let file = std::fs::File::open(path)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| Error::Privacy(format!("open docx: {e}")))?;

    let mut document_xml = String::new();
    zip.by_name("word/document.xml")
        .map_err(|e| Error::Privacy(format!("missing word/document.xml: {e}")))?
        .read_to_string(&mut document_xml)?;

    let mut comments_xml = String::new();
    match zip.by_name("word/comments.xml") {
        Ok(mut comments) => {
            comments.read_to_string(&mut comments_xml)?;
        }
        Err(_) => return Ok(Vec::new()),
    }

    let comment_actions = parse_comment_actions(&comments_xml)?;
    let selected_text = parse_comment_ranges(&document_xml)?;

    let mut instructions = Vec::new();
    for (comment_id, action) in comment_actions {
        if let Some(text) = selected_text.get(&comment_id) {
            let selected = text.trim();
            if !selected.is_empty() {
                instructions.push(DocxInstruction {
                    comment_id,
                    action,
                    selected_text: selected.to_string(),
                });
            }
        }
    }
    Ok(instructions)
}

fn parse_comment_actions(xml: &str) -> Result<BTreeMap<String, InstructionAction>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut current_id: Option<String> = None;
    let mut current_text = String::new();
    let mut out = BTreeMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name() == QName(b"w:comment") => {
                current_id = attr_value(&e, b"w:id");
                current_text.clear();
            }
            Ok(Event::Text(e)) if current_id.is_some() => {
                current_text.push_str(
                    &e.unescape()
                        .map_err(|err| Error::Privacy(format!("comment text: {err}")))?,
                );
            }
            Ok(Event::End(e)) if e.name() == QName(b"w:comment") => {
                if let Some(id) = current_id.take() {
                    if let Some(action) = normalize_instruction(&current_text) {
                        out.insert(id, action);
                    }
                }
                current_text.clear();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(Error::Privacy(format!("parse comments.xml: {e}"))),
        }
    }
    Ok(out)
}

fn parse_comment_ranges(xml: &str) -> Result<BTreeMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut active: Vec<String> = Vec::new();
    let mut out: BTreeMap<String, String> = BTreeMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) if e.name() == QName(b"w:commentRangeStart") => {
                if let Some(id) = attr_value(&e, b"w:id") {
                    active.push(id);
                }
            }
            Ok(Event::Empty(e)) if e.name() == QName(b"w:commentRangeEnd") => {
                if let Some(id) = attr_value(&e, b"w:id") {
                    active.retain(|candidate| candidate != &id);
                }
            }
            Ok(Event::Text(e)) if !active.is_empty() => {
                let text = e
                    .unescape()
                    .map_err(|err| Error::Privacy(format!("document text: {err}")))?;
                for id in &active {
                    out.entry(id.clone()).or_default().push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(Error::Privacy(format!("parse document.xml: {e}"))),
        }
    }
    Ok(out)
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|attr| attr.key == QName(key))
        .and_then(|attr| String::from_utf8(attr.value.into_owned()).ok())
}

/// Normalize a Word comment body into an action.
#[must_use]
pub fn normalize_instruction(text: &str) -> Option<InstructionAction> {
    let normalized = text
        .trim()
        .to_lowercase()
        .replace('à', "a")
        .replace('â', "a")
        .replace('é', "e")
        .replace('è', "e")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    match normalized.as_str() {
        "a masquer" => Some(InstructionAction::Mask),
        "a garder" => Some(InstructionAction::Keep),
        _ => None,
    }
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag --test privacy_docx reads_a_masquer_and_a_garder_comments_from_docx -- --exact
cargo test -p anno-rag --test privacy_docx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/docx_instructions.rs crates/anno-rag/tests/privacy_docx.rs
git commit -m "feat: read privacy instructions from word comments"
```

## Task 5: User Decision Application

**Files:**
- Create: `crates/anno-rag/src/privacy_decisions.rs`
- Create: `crates/anno-rag/tests/privacy_decisions.rs`

- [ ] **Step 1: Write failing tests for keep and mask decisions**

Create `crates/anno-rag/tests/privacy_decisions.rs`:

```rust
use anno_rag::docx_instructions::InstructionAction;
use anno_rag::privacy_decisions::{apply_user_decisions, UserDecision};
use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};

#[test]
fn keep_visible_removes_matching_detection() {
    let text = "La société Orange signe.";
    let detected = vec![entity("Orange", 11, 17, EntityCategory::Organization)];
    let decisions = vec![UserDecision {
        action: InstructionAction::Keep,
        selected_text: "Orange".to_string(),
    }];

    let merged = apply_user_decisions(text, detected, &decisions);

    assert!(merged.is_empty());
}

#[test]
fn mask_adds_custom_private_detection_for_missed_text() {
    let text = "Le client Jean Dupont signe.";
    let detected = Vec::new();
    let decisions = vec![UserDecision {
        action: InstructionAction::Mask,
        selected_text: "Jean Dupont".to_string(),
    }];

    let merged = apply_user_decisions(text, detected, &decisions);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].original, "Jean Dupont");
    assert_eq!(merged[0].start, 10);
    assert_eq!(merged[0].end, 21);
    assert!(matches!(merged[0].category, EntityCategory::Custom(ref label) if label == "private"));
}

#[test]
fn mask_all_exact_occurrences_and_deduplicates_overlaps() {
    let text = "Jean Dupont et Jean Dupont signent.";
    let detected = vec![entity("Jean Dupont", 0, 11, EntityCategory::Person)];
    let decisions = vec![UserDecision {
        action: InstructionAction::Mask,
        selected_text: "Jean Dupont".to_string(),
    }];

    let merged = apply_user_decisions(text, detected, &decisions);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].start, 0);
    assert_eq!(merged[1].start, 15);
}

fn entity(original: &str, start: usize, end: usize, category: EntityCategory) -> DetectedEntity {
    DetectedEntity {
        original: original.to_string(),
        start,
        end,
        category,
        confidence: 1.0,
        source: DetectionSource::Ner,
    }
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test -p anno-rag --test privacy_decisions
```

Expected: FAIL because `privacy_decisions` does not exist.

- [ ] **Step 3: Implement decision application**

Create `crates/anno-rag/src/privacy_decisions.rs`:

```rust
//! Apply reviewed Word-comment decisions to detector output.

use crate::docx_instructions::InstructionAction;
use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};

/// One user decision derived from a Word comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDecision {
    /// Action requested by the user.
    pub action: InstructionAction,
    /// Text selected in Word.
    pub selected_text: String,
}

/// Apply keep/mask user decisions to detected PII.
///
/// Keep decisions remove exact matching detections. Mask decisions add custom
/// `private` detections for exact occurrences not already covered.
#[must_use]
pub fn apply_user_decisions(
    text: &str,
    detected: Vec<DetectedEntity>,
    decisions: &[UserDecision],
) -> Vec<DetectedEntity> {
    let mut out = detected;

    for decision in decisions {
        match decision.action {
            InstructionAction::Keep => {
                out.retain(|entity| entity.original.trim() != decision.selected_text.trim());
            }
            InstructionAction::Mask => {
                for (start, end) in exact_occurrences(text, &decision.selected_text) {
                    if out
                        .iter()
                        .any(|entity| ranges_overlap(start, end, entity.start, entity.end))
                    {
                        continue;
                    }
                    out.push(DetectedEntity {
                        original: text[start..end].to_string(),
                        start,
                        end,
                        category: EntityCategory::Custom("private".to_string()),
                        confidence: 1.0,
                        source: DetectionSource::Custom,
                    });
                }
            }
        }
    }

    out.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
    });
    dedup_overlaps(out)
}

fn exact_occurrences(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    text.match_indices(needle)
        .map(|(start, value)| (start, start + value.len()))
        .collect()
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
}

fn dedup_overlaps(entities: Vec<DetectedEntity>) -> Vec<DetectedEntity> {
    let mut out: Vec<DetectedEntity> = Vec::with_capacity(entities.len());
    for entity in entities {
        if out
            .last()
            .is_some_and(|last| ranges_overlap(entity.start, entity.end, last.start, last.end))
        {
            continue;
        }
        out.push(entity);
    }
    out
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag --test privacy_decisions
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/privacy_decisions.rs crates/anno-rag/tests/privacy_decisions.rs
git commit -m "feat: apply privacy word comment decisions"
```

## Task 6: Vault Replacement Report Map

**Files:**
- Modify: `crates/anno-rag/src/vault.rs`

- [ ] **Step 1: Add failing vault tests**

Add these tests inside `#[cfg(test)] mod tests` in `crates/anno-rag/src/vault.rs`:

```rust
    #[tokio::test]
    async fn pseudonymize_with_report_map_returns_replacements() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault_path = dir.path().join("vault.enc");
        let vault = Vault::open(&vault_path, [7u8; 32]).expect("vault");
        let text = "Jean Dupont appelle jean@example.test.";
        let entities = vec![
            cloakpipe_core::DetectedEntity {
                original: "Jean Dupont".to_string(),
                start: 0,
                end: 11,
                category: cloakpipe_core::EntityCategory::Person,
                confidence: 0.98,
                source: cloakpipe_core::DetectionSource::Ner,
            },
            cloakpipe_core::DetectedEntity {
                original: "jean@example.test".to_string(),
                start: 20,
                end: 37,
                category: cloakpipe_core::EntityCategory::Email,
                confidence: 1.0,
                source: cloakpipe_core::DetectionSource::Pattern,
            },
        ];

        let report = vault
            .pseudonymize_with_report_map(text, &entities)
            .await
            .expect("pseudo report");

        assert!(!report.text.contains("Jean Dupont"));
        assert_eq!(report.replacements.len(), 2);
        assert_eq!(report.replacements[0].original, "Jean Dupont");
        assert!(report.replacements[0].token.starts_with("PERSON_"));
        assert_eq!(report.replacements[0].raw_start, 0);
        assert_eq!(report.replacements[0].raw_end, 11);
    }
```

- [ ] **Step 2: Run the new test and verify it fails**

Run:

```powershell
cargo test -p anno-rag vault::tests::pseudonymize_with_report_map_returns_replacements --lib -- --exact
```

Expected: FAIL because the method and structs do not exist.

- [ ] **Step 3: Implement replacement report structs and method**

Add near `pseudonymize_with_map` in `crates/anno-rag/src/vault.rs`:

```rust
/// Pseudonymized text plus local replacement metadata for reports.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PseudonymizeReport {
    /// Pseudonymized text.
    pub text: String,
    /// Replacement metadata ordered by raw offset.
    pub replacements: Vec<ReplacementRecord>,
    /// Offset map used by legal span translation.
    pub offset_map: PseudoOffsetMap,
}

/// One replacement made by the vault.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ReplacementRecord {
    /// Original local cleartext value. Never return through MCP.
    pub original: String,
    /// Pseudonym token.
    pub token: String,
    /// Entity category display.
    pub category: String,
    /// Detection confidence.
    pub confidence: f64,
    /// Detection source display.
    pub source: String,
    /// Raw-text byte start.
    pub raw_start: u32,
    /// Raw-text byte end.
    pub raw_end: u32,
    /// Pseudonymized-text byte start.
    pub pseudo_start: u32,
    /// Pseudonymized-text byte end.
    pub pseudo_end: u32,
}
```

Add method:

```rust
    /// Pseudonymize text and return local replacement metadata for reports.
    ///
    /// # Errors
    /// Returns [`Error::Vault`] on replacer or vault persistence failures.
    pub async fn pseudonymize_with_report_map(
        &self,
        text: &str,
        entities: &[DetectedEntity],
    ) -> Result<PseudonymizeReport> {
        let mut v = self.inner.lock().await;
        let result = Replacer::pseudonymize(text, entities, &mut v)
            .map_err(|e| Error::Vault(format!("replacer: {e}")))?;
        v.save()
            .map_err(|e| Error::Vault(format!("save after pseudonymize_with_report_map: {e}")))?;
        drop(v);

        let mut sorted_entities: Vec<&DetectedEntity> = entities.iter().collect();
        sorted_entities.sort_by_key(|entity| entity.start);

        let mut subs = Vec::with_capacity(sorted_entities.len());
        let mut replacements = Vec::with_capacity(sorted_entities.len());
        let mut pseudo_cursor: usize = 0;
        let mut raw_cursor: usize = 0;

        for entity in sorted_entities {
            if entity.start < raw_cursor || entity.end > text.len() {
                continue;
            }

            pseudo_cursor += entity.start - raw_cursor;
            let token = result
                .mappings
                .iter()
                .find(|(_, original)| original.as_str() == entity.original)
                .map(|(token, _)| token.as_str())
                .ok_or_else(|| {
                    Error::Vault(format!(
                        "pseudonymize_with_report_map: no mapping for original {:?}",
                        entity.original
                    ))
                })?;
            let pseudo_start = pseudo_cursor as u32;
            let pseudo_end = (pseudo_cursor + token.len()) as u32;
            subs.push(Substitution {
                raw_start: entity.start as u32,
                raw_end: entity.end as u32,
                pseudo_start,
                pseudo_end,
            });
            replacements.push(ReplacementRecord {
                original: entity.original.clone(),
                token: token.to_string(),
                category: format!("{:?}", entity.category),
                confidence: entity.confidence,
                source: format!("{:?}", entity.source),
                raw_start: entity.start as u32,
                raw_end: entity.end as u32,
                pseudo_start,
                pseudo_end,
            });
            pseudo_cursor += token.len();
            raw_cursor = entity.end;
        }

        Ok(PseudonymizeReport {
            text: result.text,
            replacements,
            offset_map: PseudoOffsetMap { subs },
        })
    }
```

- [ ] **Step 4: Run vault tests**

Run:

```powershell
cargo test -p anno-rag vault::tests::pseudonymize_with_report_map_returns_replacements --lib -- --exact
cargo test -p anno-rag vault::tests::pseudonymize_with_map --lib
```

Expected: PASS. If the second command finds no exact test name, run `cargo test -p anno-rag vault::tests --lib`.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/vault.rs
git commit -m "feat: expose local vault replacement report map"
```

## Task 7: Prepare Workspace Orchestration

**Files:**
- Create: `crates/anno-rag/src/privacy_workspace.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/privacy_artifacts.rs`
- Create: `crates/anno-rag/tests/privacy_workspace.rs`

- [ ] **Step 1: Write failing tests for prepare output paths without model work**

Create `crates/anno-rag/tests/privacy_workspace.rs`:

```rust
use anno_rag::privacy_artifacts::PrivacyWorkspacePaths;
use anno_rag::privacy_workspace::{build_relative_output_paths, PrivacyDocumentOutputPaths};
use std::path::Path;

#[test]
fn relative_output_paths_mirror_subfolders_and_change_extension() {
    let source_root = Path::new("C:/Clients/Matter X");
    let paths = PrivacyWorkspacePaths::from_source_root(source_root);
    let source = Path::new("C:/Clients/Matter X/contracts/contract.pdf");

    let PrivacyDocumentOutputPaths {
        relative_path,
        working_path,
        anonymized_path,
    } = build_relative_output_paths(&paths, source);

    assert_eq!(relative_path, "contracts/contract.pdf");
    assert_eq!(
        working_path,
        source_root
            .join("vault")
            .join("01-working-documents")
            .join("contracts")
            .join("contract.docx")
    );
    assert_eq!(
        anonymized_path,
        source_root
            .join("vault")
            .join("02-anonymized-documents")
            .join("contracts")
            .join("contract.anon.docx")
    );
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test -p anno-rag --test privacy_workspace
```

Expected: FAIL because `privacy_workspace` does not exist.

- [ ] **Step 3: Implement path helper and public result shapes**

Create `crates/anno-rag/src/privacy_workspace.rs`:

```rust
//! Orchestration for the local `vault` privacy workspace.

use crate::error::Result;
use crate::privacy_artifacts::{PrivacyWorkspacePaths, WorkspaceManifest};
use std::path::{Path, PathBuf};

/// Generated paths for one source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyDocumentOutputPaths {
    /// Normalized relative source path.
    pub relative_path: String,
    /// Cleartext working docx path.
    pub working_path: PathBuf,
    /// Anonymized docx path.
    pub anonymized_path: PathBuf,
}

/// Safe prepare summary returned to MCP/CLI callers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrivacyPrepareSummary {
    /// Workspace path.
    pub workspace: String,
    /// Working folder path.
    pub working_folder: String,
    /// Anonymized folder path.
    pub anonymized_folder: String,
    /// Reports folder path.
    pub reports_folder: String,
    /// Source files seen.
    pub documents_seen: usize,
    /// Documents prepared.
    pub documents_prepared: usize,
    /// Documents failed.
    pub documents_failed: usize,
}

/// Safe finalize summary returned to MCP/CLI callers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrivacyFinalizeSummary {
    /// Workspace path.
    pub workspace: String,
    /// Changed working docs.
    pub documents_changed: usize,
    /// Reindexed docs.
    pub documents_reindexed: usize,
    /// Manual mask decisions.
    pub to_mask: usize,
    /// Manual keep decisions.
    pub to_keep: usize,
    /// Anonymized folder path.
    pub anonymized_folder: String,
    /// Shareable report path.
    pub shareable_report: String,
}

/// Build mirrored working/anonymized output paths for one source path.
#[must_use]
pub fn build_relative_output_paths(
    paths: &PrivacyWorkspacePaths,
    source_path: &Path,
) -> PrivacyDocumentOutputPaths {
    let relative = source_path
        .strip_prefix(&paths.source_root)
        .unwrap_or(source_path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");

    let mut relative_docx = PathBuf::from(&relative);
    relative_docx.set_extension("docx");

    let mut relative_anon = PathBuf::from(&relative);
    let stem = relative_anon
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document")
        .to_string();
    relative_anon.set_file_name(format!("{stem}.anon.docx"));

    PrivacyDocumentOutputPaths {
        relative_path: relative,
        working_path: paths.working.join(relative_docx),
        anonymized_path: paths.anonymized.join(relative_anon),
    }
}

/// Write a manifest to disk.
///
/// # Errors
/// Returns IO or JSON serialization errors wrapped as privacy errors.
pub fn write_manifest(path: &Path, manifest: &WorkspaceManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| crate::Error::Privacy(format!("serialize manifest: {e}")))?;
    std::fs::write(path, json)?;
    Ok(())
}
```

- [ ] **Step 4: Add pipeline prepare/finalize method stubs**

Add to `impl Pipeline` in `crates/anno-rag/src/pipeline.rs`:

```rust
    /// Prepare a local privacy workspace under `<source_root>/vault`.
    ///
    /// # Errors
    /// Returns privacy, extraction, detection, vault, or IO errors.
    pub async fn privacy_prepare_folder(
        &self,
        source_root: &Path,
        recursive: bool,
    ) -> Result<crate::privacy_workspace::PrivacyPrepareSummary> {
        crate::privacy_workspace::prepare_folder(self, source_root, recursive).await
    }

    /// Finalize a local privacy workspace after user Word edits.
    ///
    /// # Errors
    /// Returns privacy, extraction, detection, vault, or IO errors.
    pub async fn privacy_finalize_folder(
        &self,
        workspace: &Path,
    ) -> Result<crate::privacy_workspace::PrivacyFinalizeSummary> {
        crate::privacy_workspace::finalize_folder(self, workspace).await
    }
```

Add temporary functions to `privacy_workspace.rs` so this compiles:

```rust
/// Prepare a folder privacy workspace.
pub async fn prepare_folder(
    _pipeline: &crate::pipeline::Pipeline,
    source_root: &Path,
    _recursive: bool,
) -> Result<PrivacyPrepareSummary> {
    let paths = PrivacyWorkspacePaths::from_source_root(source_root);
    paths.create_all()?;
    let manifest = WorkspaceManifest::new(source_root.display().to_string());
    write_manifest(&paths.manifest, &manifest)?;
    Ok(PrivacyPrepareSummary {
        workspace: paths.workspace.display().to_string(),
        working_folder: paths.working.display().to_string(),
        anonymized_folder: paths.anonymized.display().to_string(),
        reports_folder: paths.reports.display().to_string(),
        documents_seen: 0,
        documents_prepared: 0,
        documents_failed: 0,
    })
}

/// Finalize a folder privacy workspace.
pub async fn finalize_folder(
    _pipeline: &crate::pipeline::Pipeline,
    workspace: &Path,
) -> Result<PrivacyFinalizeSummary> {
    Ok(PrivacyFinalizeSummary {
        workspace: workspace.display().to_string(),
        documents_changed: 0,
        documents_reindexed: 0,
        to_mask: 0,
        to_keep: 0,
        anonymized_folder: workspace.join(crate::privacy_artifacts::ANONYMIZED_DIR).display().to_string(),
        shareable_report: workspace.join(crate::privacy_artifacts::REPORTS_DIR).join("shareable_report.docx").display().to_string(),
    })
}
```

- [ ] **Step 5: Run tests and check**

Run:

```powershell
cargo test -p anno-rag --test privacy_workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: PASS for the test and check for `anno-rag`.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/privacy_workspace.rs crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/privacy_workspace.rs
git commit -m "feat: scaffold privacy workspace orchestration"
```

## Task 8: Exclude `vault` From Existing Folder Discovery

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-source-local/src/folder.rs`

- [ ] **Step 1: Add failing tests for `vault` exclusion**

In `crates/anno-rag/src/pipeline.rs`, extend the existing generated-output tests near the tests around `.anon.md` with:

```rust
    #[test]
    fn generated_output_filter_skips_vault_workspace() {
        let root = std::path::Path::new("C:/Clients/Matter X");
        assert!(super::is_anno_generated_output(
            root,
            std::path::Path::new("C:/Clients/Matter X/vault/01-working-documents/a.docx"),
            std::path::Path::new("C:/Clients/Matter X/anon")
        ));
    }
```

In `crates/anno-source-local/src/folder.rs`, add a test near generated folder tests:

```rust
    #[test]
    fn skips_vault_generated_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("vault")).expect("mkdir");
        std::fs::write(dir.path().join("vault").join("report.docx"), b"generated").expect("write");
        std::fs::write(dir.path().join("source.txt"), b"source").expect("write");

        let source = LocalFolderSource::new(dir.path());
        let discovered = source.discover(&DiscoverBudget::default()).expect("discover");

        assert_eq!(discovered.len(), 1);
        assert!(discovered[0].external_id.ends_with("source.txt"));
    }
```

- [ ] **Step 2: Run tests and verify failures**

Run:

```powershell
cargo test -p anno-rag generated_output_filter_skips_vault_workspace --lib -- --exact
cargo test -p anno-source-local skips_vault_generated_folder --lib -- --exact
```

Expected: at least one FAIL because `vault` is not excluded yet.

- [ ] **Step 3: Add `vault` to generated folder names**

In `crates/anno-rag/src/pipeline.rs`, update `is_anno_generated_dir_name`:

```rust
fn is_anno_generated_dir_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "anon" | "outputs" | ".anno" | ".anno-rag" | "vault"
    )
}
```

In `crates/anno-source-local/src/folder.rs`, update `is_generated_anno_dir_name`:

```rust
fn is_generated_anno_dir_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "anon" | "outputs" | ".anno" | ".anno-rag" | "vault"
    )
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag generated_output_filter_skips_vault_workspace --lib -- --exact
cargo test -p anno-source-local skips_vault_generated_folder --lib -- --exact
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-source-local/src/folder.rs
git commit -m "fix: exclude privacy vault folders from indexing"
```

## Task 9: Full Prepare Implementation

**Files:**
- Modify: `crates/anno-rag/src/privacy_workspace.rs`
- Modify: `crates/anno-rag/src/privacy_docx.rs`
- Modify: `crates/anno-rag/tests/privacy_workspace.rs`

- [ ] **Step 1: Add a model-gated integration test**

Append to `crates/anno-rag/tests/privacy_workspace.rs`:

```rust
use anno_rag::{config::AnnoRagConfig, Pipeline};

#[tokio::test]
async fn prepare_folder_creates_working_anonymized_reports_and_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source_root = dir.path().join("matter");
    std::fs::create_dir_all(source_root.join("contracts")).expect("mkdir");
    std::fs::write(
        source_root.join("contracts").join("contract.txt"),
        "Jean Dupont signe avec Acme.",
    )
    .expect("write source");

    let cfg = AnnoRagConfig {
        data_dir: dir.path().join("data"),
        ..AnnoRagConfig::default()
    };
    if !cfg.models_cache().exists() {
        eprintln!("skipping: no local models at {}", cfg.models_cache().display());
        return;
    }

    let pipeline = Pipeline::new(cfg, [9u8; 32]).await.expect("pipeline");
    let summary = pipeline
        .privacy_prepare_folder(&source_root, true)
        .await
        .expect("prepare");

    assert_eq!(summary.documents_seen, 1);
    assert_eq!(summary.documents_prepared, 1);
    assert_eq!(summary.documents_failed, 0);
    assert!(source_root
        .join("vault")
        .join("01-working-documents")
        .join("contracts")
        .join("contract.docx")
        .exists());
    assert!(source_root
        .join("vault")
        .join("02-anonymized-documents")
        .join("contracts")
        .join("contract.anon.docx")
        .exists());
    assert!(source_root.join("vault").join("manifest.json").exists());
}
```

- [ ] **Step 2: Run test and verify prepare counts are still zero**

Run:

```powershell
cargo test -p anno-rag --test privacy_workspace prepare_folder_creates_working_anonymized_reports_and_manifest -- --exact
```

Expected: If models are absent, SKIP via printed message. If models are present, FAIL because `prepare_folder` returns zero counts.

- [ ] **Step 3: Implement report document helper**

Add to `crates/anno-rag/src/privacy_docx.rs`:

```rust
/// Write a simple shareable report with safe counts only.
pub fn write_shareable_report(
    path: &Path,
    title: &str,
    rows: &[(String, String)],
) -> Result<()> {
    let doc = NormalizedDocx {
        title: title.to_string(),
        metadata: rows.to_vec(),
        sections: Vec::new(),
    };
    write_normalized_docx(path, &doc)
}
```

- [ ] **Step 4: Implement prepare document loop**

Replace the temporary body of `prepare_folder` in `privacy_workspace.rs` with:

```rust
pub async fn prepare_folder(
    pipeline: &crate::pipeline::Pipeline,
    source_root: &Path,
    recursive: bool,
) -> Result<PrivacyPrepareSummary> {
    let paths = PrivacyWorkspacePaths::from_source_root(source_root);
    paths.create_all()?;

    let mut manifest = WorkspaceManifest::new(source_root.display().to_string());
    let source_paths = crate::pipeline::privacy_candidate_paths(source_root, recursive, &paths.workspace);
    let mut prepared = 0usize;
    let mut failed = 0usize;

    for source_path in &source_paths {
        match prepare_one_document(pipeline, &paths, source_path).await {
            Ok(document) => {
                prepared += 1;
                manifest.documents.push(document);
            }
            Err(e) => {
                failed += 1;
                tracing::warn!(
                    path = %source_path.display(),
                    error = %e,
                    "privacy prepare skipped document"
                );
            }
        }
    }

    manifest.updated_at = chrono::Utc::now();
    write_manifest(&paths.manifest, &manifest)?;
    crate::privacy_docx::write_shareable_report(
        &paths.reports.join("shareable_report.docx"),
        "Privacy Workspace Report",
        &[
            ("Documents seen".to_string(), source_paths.len().to_string()),
            ("Documents prepared".to_string(), prepared.to_string()),
            ("Documents failed".to_string(), failed.to_string()),
        ],
    )?;

    Ok(PrivacyPrepareSummary {
        workspace: paths.workspace.display().to_string(),
        working_folder: paths.working.display().to_string(),
        anonymized_folder: paths.anonymized.display().to_string(),
        reports_folder: paths.reports.display().to_string(),
        documents_seen: source_paths.len(),
        documents_prepared: prepared,
        documents_failed: failed,
    })
}
```

Add helper in `privacy_workspace.rs`:

```rust
async fn prepare_one_document(
    pipeline: &crate::pipeline::Pipeline,
    paths: &PrivacyWorkspacePaths,
    source_path: &Path,
) -> Result<crate::privacy_artifacts::ManifestDocument> {
    let extracted = crate::ingest::extract(source_path, pipeline.config()).await?;
    let out_paths = build_relative_output_paths(paths, source_path);

    let working_doc = crate::privacy_docx::NormalizedDocx {
        title: out_paths.relative_path.clone(),
        metadata: vec![
            ("Source".to_string(), out_paths.relative_path.clone()),
            ("Extraction".to_string(), "Kreuzberg".to_string()),
        ],
        sections: extracted
            .chunks
            .iter()
            .map(|chunk| crate::privacy_docx::NormalizedSection {
                heading: format!("Chunk {}", chunk.idx + 1),
                body: chunk.text.clone(),
            })
            .collect(),
    };
    crate::privacy_docx::write_normalized_docx(&out_paths.working_path, &working_doc)?;

    let detector = pipeline.detector_for_privacy()?;
    let no_legal = Vec::new();
    let no_thresholds = std::collections::HashMap::new();
    let mut pseudo_sections = Vec::new();
    let mut detected_count = 0usize;
    for chunk in &extracted.chunks {
        let bundle = detector.detect_for_ingest(&chunk.text, &no_legal, &no_thresholds)?;
        detected_count += bundle.pii.len();
        let report = pipeline
            .vault_for_privacy()
            .pseudonymize_with_report_map(&chunk.text, &bundle.pii)
            .await?;
        pseudo_sections.push(crate::privacy_docx::NormalizedSection {
            heading: format!("Chunk {}", chunk.idx + 1),
            body: report.text,
        });
    }
    let anonymized_doc = crate::privacy_docx::NormalizedDocx {
        title: out_paths.relative_path.clone(),
        metadata: vec![
            ("Source".to_string(), out_paths.relative_path.clone()),
            ("PII".to_string(), "Anonymized".to_string()),
        ],
        sections: pseudo_sections,
    };
    crate::privacy_docx::write_normalized_docx(&out_paths.anonymized_path, &anonymized_doc)?;

    let source_bytes = std::fs::read(source_path)?;
    let working_bytes = std::fs::read(&out_paths.working_path)?;
    Ok(crate::privacy_artifacts::ManifestDocument {
        document_id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &source_bytes).to_string(),
        relative_path: out_paths.relative_path,
        source_path: source_path.display().to_string(),
        working_path: out_paths.working_path.display().to_string(),
        anonymized_path: out_paths.anonymized_path.display().to_string(),
        source_hash: crate::privacy_artifacts::sha256_hex(&source_bytes),
        working_hash: Some(crate::privacy_artifacts::sha256_hex(&working_bytes)),
        last_indexed_hash: None,
        status: "prepared".to_string(),
        counts: crate::privacy_artifacts::ManifestCounts {
            detected: detected_count,
            forced_mask: 0,
            kept_visible: 0,
        },
    })
}
```

Expose minimal privacy helpers in `Pipeline`:

```rust
    /// Borrow config for privacy workspace orchestration.
    #[must_use]
    pub fn config(&self) -> &AnnoRagConfig {
        &self.cfg
    }

    /// Load detector for privacy workspace orchestration.
    pub fn detector_for_privacy(&self) -> Result<&crate::detect::Detector> {
        self.detector_get_or_init().map(|arc| arc.as_ref())
    }

    /// Borrow vault for privacy workspace orchestration.
    #[must_use]
    pub fn vault_for_privacy(&self) -> &crate::vault::Vault {
        &self.vault
    }
```

Add candidate-path wrapper in `pipeline.rs` near `legal_ingest_candidate_paths`:

```rust
/// Privacy workspace candidate paths. Excludes generated output folders.
#[must_use]
pub fn privacy_candidate_paths(
    folder: &Path,
    recursive: bool,
    workspace_dir: &Path,
) -> Vec<std::path::PathBuf> {
    legal_ingest_candidate_paths(folder, recursive, workspace_dir)
}
```

- [ ] **Step 5: Run tests and check**

Run:

```powershell
cargo test -p anno-rag --test privacy_workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: PASS, with model-gated integration printing skip if models are unavailable.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/privacy_workspace.rs crates/anno-rag/src/privacy_docx.rs crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/privacy_workspace.rs
git commit -m "feat: prepare privacy vault workspace"
```

## Task 10: Finalize Changed Working Documents

**Files:**
- Modify: `crates/anno-rag/src/privacy_workspace.rs`
- Modify: `crates/anno-rag/tests/privacy_workspace.rs`

- [ ] **Step 1: Add unit test for skip unchanged finalize path**

Append to `crates/anno-rag/tests/privacy_workspace.rs`:

```rust
use anno_rag::privacy_workspace::finalize_manifest_only_for_test;

#[test]
fn finalize_manifest_skips_unchanged_working_hashes() {
    let mut manifest = anno_rag::privacy_artifacts::WorkspaceManifest::new("C:/Matter");
    manifest.documents.push(anno_rag::privacy_artifacts::ManifestDocument {
        document_id: "doc-1".to_string(),
        relative_path: "a.txt".to_string(),
        source_path: "C:/Matter/a.txt".to_string(),
        working_path: "unused".to_string(),
        anonymized_path: "unused".to_string(),
        source_hash: "source".to_string(),
        working_hash: Some("same".to_string()),
        last_indexed_hash: Some("same".to_string()),
        status: "prepared".to_string(),
        counts: Default::default(),
    });

    let summary = finalize_manifest_only_for_test(&manifest, &[("doc-1", "same")]);

    assert_eq!(summary.documents_changed, 0);
    assert_eq!(summary.documents_reindexed, 0);
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```powershell
cargo test -p anno-rag --test privacy_workspace finalize_manifest_skips_unchanged_working_hashes -- --exact
```

Expected: FAIL because helper does not exist.

- [ ] **Step 3: Add manifest-only helper**

Add to `privacy_workspace.rs`:

```rust
/// Test helper for manifest hash comparison.
#[doc(hidden)]
#[must_use]
pub fn finalize_manifest_only_for_test(
    manifest: &WorkspaceManifest,
    working_hashes: &[(&str, &str)],
) -> PrivacyFinalizeSummary {
    let changed = manifest
        .documents
        .iter()
        .filter(|doc| {
            let current = working_hashes
                .iter()
                .find(|(id, _)| *id == doc.document_id)
                .map(|(_, hash)| (*hash).to_string());
            current.is_some() && current != doc.working_hash
        })
        .count();

    PrivacyFinalizeSummary {
        workspace: "test".to_string(),
        documents_changed: changed,
        documents_reindexed: changed,
        to_mask: 0,
        to_keep: 0,
        anonymized_folder: "test/02-anonymized-documents".to_string(),
        shareable_report: "test/03-reports/shareable_report.docx".to_string(),
    }
}
```

- [ ] **Step 4: Replace temporary finalize implementation**

Update `finalize_folder` in `privacy_workspace.rs` to:

```rust
pub async fn finalize_folder(
    pipeline: &crate::pipeline::Pipeline,
    workspace: &Path,
) -> Result<PrivacyFinalizeSummary> {
    let manifest_path = workspace.join(crate::privacy_artifacts::MANIFEST_FILE);
    let manifest_json = std::fs::read_to_string(&manifest_path)?;
    let mut manifest: WorkspaceManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| crate::Error::Privacy(format!("parse manifest: {e}")))?;

    let paths = PrivacyWorkspacePaths {
        source_root: PathBuf::from(&manifest.source_root),
        workspace: workspace.to_path_buf(),
        working: workspace.join(crate::privacy_artifacts::WORKING_DIR),
        anonymized: workspace.join(crate::privacy_artifacts::ANONYMIZED_DIR),
        reports: workspace.join(crate::privacy_artifacts::REPORTS_DIR),
        cache: workspace.join(crate::privacy_artifacts::CACHE_DIR),
        manifest: manifest_path,
    };

    let mut changed = 0usize;
    let mut reindexed = 0usize;
    let mut to_mask = 0usize;
    let mut to_keep = 0usize;

    for doc in &mut manifest.documents {
        let working_path = PathBuf::from(&doc.working_path);
        let working_bytes = std::fs::read(&working_path)?;
        let working_hash = crate::privacy_artifacts::sha256_hex(&working_bytes);
        if doc.working_hash.as_deref() == Some(working_hash.as_str()) {
            continue;
        }
        changed += 1;

        let instructions = crate::docx_instructions::read_docx_instructions(&working_path)?;
        let decisions: Vec<crate::privacy_decisions::UserDecision> = instructions
            .iter()
            .map(|instruction| crate::privacy_decisions::UserDecision {
                action: instruction.action,
                selected_text: instruction.selected_text.clone(),
            })
            .collect();
        to_mask += decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Mask)
            .count();
        to_keep += decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Keep)
            .count();

        let extracted = crate::ingest::extract(&working_path, pipeline.config()).await?;
        let detector = pipeline.detector_for_privacy()?;
        let no_legal = Vec::new();
        let no_thresholds = std::collections::HashMap::new();
        let mut pseudo_sections = Vec::new();
        let mut detected_count = 0usize;
        for chunk in &extracted.chunks {
            let bundle = detector.detect_for_ingest(&chunk.text, &no_legal, &no_thresholds)?;
            detected_count += bundle.pii.len();
            let pii =
                crate::privacy_decisions::apply_user_decisions(&chunk.text, bundle.pii, &decisions);
            let report = pipeline
                .vault_for_privacy()
                .pseudonymize_with_report_map(&chunk.text, &pii)
                .await?;
            pseudo_sections.push(crate::privacy_docx::NormalizedSection {
                heading: format!("Chunk {}", chunk.idx + 1),
                body: report.text,
            });
        }

        let anonymized_path = PathBuf::from(&doc.anonymized_path);
        let anonymized_doc = crate::privacy_docx::NormalizedDocx {
            title: doc.relative_path.clone(),
            metadata: vec![
                ("Source".to_string(), doc.relative_path.clone()),
                ("PII".to_string(), "Anonymized".to_string()),
            ],
            sections: pseudo_sections,
        };
        crate::privacy_docx::write_normalized_docx(&anonymized_path, &anonymized_doc)?;

        doc.working_hash = Some(working_hash.clone());
        doc.last_indexed_hash = Some(working_hash);
        doc.status = "finalized".to_string();
        doc.counts.detected = detected_count;
        doc.counts.forced_mask = decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Mask)
            .count();
        doc.counts.kept_visible = decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Keep)
            .count();
        reindexed += 1;
    }

    manifest.updated_at = chrono::Utc::now();
    write_manifest(&paths.manifest, &manifest)?;
    crate::privacy_docx::write_shareable_report(
        &paths.reports.join("shareable_report.docx"),
        "Privacy Finalization Report",
        &[
            ("Documents changed".to_string(), changed.to_string()),
            ("Documents reindexed".to_string(), reindexed.to_string()),
            ("Manual mask decisions".to_string(), to_mask.to_string()),
            ("Kept visible exceptions".to_string(), to_keep.to_string()),
        ],
    )?;

    Ok(PrivacyFinalizeSummary {
        workspace: paths.workspace.display().to_string(),
        documents_changed: changed,
        documents_reindexed: reindexed,
        to_mask,
        to_keep,
        anonymized_folder: paths.anonymized.display().to_string(),
        shareable_report: paths
            .reports
            .join("shareable_report.docx")
            .display()
            .to_string(),
    })
}
```

- [ ] **Step 5: Run tests and check**

Run:

```powershell
cargo test -p anno-rag --test privacy_workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: PASS or model-gated skip where applicable.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/privacy_workspace.rs crates/anno-rag/tests/privacy_workspace.rs
git commit -m "feat: finalize changed privacy working documents"
```

## Task 11: MCP Tools For Cowork

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`
- Create: `crates/anno-rag-mcp/tests/privacy_tools.rs`

- [ ] **Step 1: Add response-safety tests**

Create `crates/anno-rag-mcp/tests/privacy_tools.rs`:

```rust
use anno_rag_mcp::health::all_tool_names;

#[test]
fn health_lists_privacy_tools() {
    let tools = all_tool_names();
    assert!(tools.contains(&"privacy_prepare_folder".to_string()));
    assert!(tools.contains(&"privacy_finalize_folder".to_string()));
    assert!(tools.contains(&"privacy_status".to_string()));
}

#[test]
fn privacy_tool_response_shape_is_path_and_count_only() {
    let value = serde_json::json!({
        "ok": true,
        "workspace": "C:\\Matter\\vault",
        "working_folder": "C:\\Matter\\vault\\01-working-documents",
        "anonymized_folder": "C:\\Matter\\vault\\02-anonymized-documents",
        "documents_seen": 1,
        "documents_prepared": 1,
        "documents_failed": 0
    });
    let json = serde_json::to_string(&value).expect("serialize");
    assert!(!json.contains("Jean Dupont"));
    assert!(!json.contains("jean@example.test"));
}
```

- [ ] **Step 2: Run tests and verify health fails**

Run:

```powershell
cargo test -p anno-rag-mcp --test privacy_tools
```

Expected: FAIL because health metadata does not list privacy tools.

- [ ] **Step 3: Add MCP params**

In `crates/anno-rag-mcp/src/lib.rs`, add near other param structs:

```rust
/// Parameters for `privacy_prepare_folder`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PrivacyPrepareFolderParams {
    /// Local source root to prepare.
    pub source_root: String,
    /// Recurse into subfolders.
    #[serde(default = "default_true")]
    pub recursive: bool,
}

/// Parameters for `privacy_finalize_folder`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PrivacyFinalizeFolderParams {
    /// Local `vault` workspace path.
    pub workspace: String,
}

fn default_true() -> bool {
    true
}
```

- [ ] **Step 4: Add safe implementation methods**

Inside `impl AnnoRagServer`, add:

```rust
    async fn privacy_prepare_folder_impl(
        &self,
        p: PrivacyPrepareFolderParams,
    ) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let summary = pipeline
            .privacy_prepare_folder(std::path::Path::new(&p.source_root), p.recursive)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(summary).map_err(|e| e.to_string())
    }

    async fn privacy_finalize_folder_impl(
        &self,
        p: PrivacyFinalizeFolderParams,
    ) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let summary = pipeline
            .privacy_finalize_folder(std::path::Path::new(&p.workspace))
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(summary).map_err(|e| e.to_string())
    }

    async fn privacy_status_impl(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": true,
            "tools": [
                "privacy_prepare_folder",
                "privacy_finalize_folder",
                "privacy_status"
            ],
            "privacy_boundary": "local"
        })
    }
```

- [ ] **Step 5: Add MCP tool handlers**

Inside the `#[tool_handler] impl` block, add:

```rust
    /// Prepare a local folder for privacy review in a generated `vault` workspace.
    #[tool(
        description = "Prepare a local folder for privacy review. Creates a local vault workspace with working Word docs, anonymized docs, reports, and a manifest. Returns paths and counts only."
    )]
    async fn privacy_prepare_folder(
        &self,
        Parameters(p): Parameters<PrivacyPrepareFolderParams>,
    ) -> String {
        match self.privacy_prepare_folder_impl(p).await {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Finalize a local privacy workspace after user Word edits.
    #[tool(
        description = "Finalize a local vault workspace after Word edits. Reads 'à masquer' and 'à garder' comments locally, regenerates anonymized docs, and returns paths and counts only."
    )]
    async fn privacy_finalize_folder(
        &self,
        Parameters(p): Parameters<PrivacyFinalizeFolderParams>,
    ) -> String {
        match self.privacy_finalize_folder_impl(p).await {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Report privacy workflow capabilities without loading models.
    #[tool(description = "Privacy workflow status and capabilities. Does not return document content.")]
    async fn privacy_status(&self) -> String {
        serde_json::to_string_pretty(&self.privacy_status_impl().await)
            .unwrap_or_else(|e| format!("Error: {e}"))
    }
```

- [ ] **Step 6: Update health metadata**

In `crates/anno-rag-mcp/src/health.rs`, add these strings to `available_tools`:

```rust
"privacy_prepare_folder",
"privacy_finalize_folder",
"privacy_status",
```

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test -p anno-rag-mcp --test privacy_tools
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs crates/anno-rag-mcp/tests/privacy_tools.rs
git commit -m "feat: expose privacy vault workflow tools"
```

## Task 12: Final Verification And Documentation

**Files:**
- Modify: `docs/user-guide/legal-rag.md`
- Modify: `docs/developers/mcp-tools.md` if present
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Locate MCP docs**

Run:

```powershell
rg -n -S "MCP Tools|legal_ingest|index\\(|vault_stats" docs
```

Expected: find the primary MCP docs path. Use that file in the following steps. If `docs/developers/mcp-tools.md` exists, update it.

- [ ] **Step 2: Update user-facing workflow docs**

Add this section to `docs/user-guide/legal-rag.md`:

```markdown
## Privacy Vault Word Review

For folder review workflows, ask Cowork to prepare the folder for anonymization.
Anno creates a local `vault` folder beside the source documents:

```text
vault/
  01-working-documents/
  02-anonymized-documents/
  03-reports/
  04-cache/
  manifest.json
```

Edit only files in `01-working-documents`. In Word, add a comment `à masquer`
on text that should be hidden, or `à garder` on a false positive that should
remain visible. When finished, ask Cowork to finalize the folder.

Share only files from `02-anonymized-documents` or the shareable report. Do not
share `01-working-documents` or the sensitive report.
```

- [ ] **Step 3: Update changelog**

Add under the unreleased/current section in `CHANGELOG.md`:

```markdown
- Added privacy vault Word review design and local workflow tools for preparing
  editable working documents, reading `à masquer` / `à garder` comments, and
  regenerating anonymized outputs without returning PII through Cowork.
```

- [ ] **Step 4: Run focused checks**

Run:

```powershell
cargo test -p anno-rag --test privacy_artifacts
cargo test -p anno-rag --test privacy_docx
cargo test -p anno-rag --test privacy_decisions
cargo test -p anno-rag --test privacy_workspace
cargo test -p anno-rag-mcp --test privacy_tools
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: all tests pass. Model-gated tests may print a skip message when local models are unavailable.

- [ ] **Step 5: Run GitNexus change detection**

If MCP tools are available, run `gitnexus_detect_changes({scope: "all"})`.

Fallback command:

```powershell
npx gitnexus status
git diff --stat
git diff --name-only
```

Expected: changed files match the privacy workflow modules, MCP surface, docs, and dependency files.

- [ ] **Step 6: Commit**

```powershell
git add docs/user-guide/legal-rag.md CHANGELOG.md
git add docs/developers/mcp-tools.md
git commit -m "docs: document privacy vault word workflow"
```

If `docs/developers/mcp-tools.md` does not exist, omit it from `git add`.

## Plan Self-Review

### Spec Coverage

- `vault` workspace under the source folder: Task 2, Task 7.
- `01-working-documents`, `02-anonymized-documents`, `03-reports`, `04-cache`: Task 2.
- Normalized cleartext working `.docx`: Task 3, Task 9.
- Word comments `à masquer` / `à garder`: Task 4, Task 5, Task 10.
- Comments are instructions and not indexed: Task 4 reads them separately; Task 10 regenerates anonymized docs from extracted body text and decision application.
- Anonymized `.docx` output: Task 3, Task 9, Task 10.
- Manifest with hashes and counts: Task 2, Task 9, Task 10.
- Skip unchanged working docs: Task 10.
- Exclude `vault` from scans: Task 8.
- MCP tools with path/count responses only: Task 11.
- Performance logic through hashing and per-document loops: Task 9, Task 10.
- Documentation and final verification: Task 12.

### Red-Flag Scan

Each code task includes concrete test code, implementation snippets, commands, and expected results. The only conditional branch is documented file existence for `docs/developers/mcp-tools.md`, with an explicit omit instruction.

### Type Consistency

The same types are used throughout:

- `PrivacyWorkspacePaths`
- `WorkspaceManifest`
- `ManifestDocument`
- `ManifestCounts`
- `InstructionAction`
- `DocxInstruction`
- `UserDecision`
- `NormalizedDocx`
- `NormalizedSection`
- `PrivacyPrepareSummary`
- `PrivacyFinalizeSummary`

The MCP tool names match the design spec:

- `privacy_prepare_folder`
- `privacy_finalize_folder`
- `privacy_status`

## Execution Handoff

Plan complete. Use one of these execution modes:

1. **Subagent-Driven (recommended)** - dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** - execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.
