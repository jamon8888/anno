# Hacienda Tauri Connectors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a connector layer for network folders and SharePoint/OneDrive so all external sources feed the same local ingestion pipeline.

**Architecture:** Define a `SourceConnector` trait with enumerate/stat/fetch/hash/provenance operations. Implement `LocalFolderConnector` for local and mounted network paths first, then `SharePointConnector` using Microsoft Graph with explicit user login and local token storage managed by OS facilities.

**Tech Stack:** Rust 1.95, `reqwest`, `serde`, Microsoft Graph REST, Tauri commands, existing normalized ingestion pipeline.

---

## Scope

In scope:

- Connector abstraction.
- Local/mounted network connector.
- SharePoint/OneDrive connector design and mocked implementation tests.
- Incremental sync by remote id/version/hash.
- Provenance recording.

Out of scope:

- iManage/NetDocuments.
- Background daemon sync.
- Multi-tenant admin consent automation.

## Files

- Create `crates/hacienda-workbench-core/src/connectors/mod.rs`
- Create `crates/hacienda-workbench-core/src/connectors/local.rs`
- Create `crates/hacienda-workbench-core/src/connectors/sharepoint.rs`
- Modify `model.rs`, `store.rs`, `engine.rs`
- Modify Tauri commands and frontend source picker.

## Tasks

### Task 1: Connector Trait

- [ ] Define:

```rust
#[async_trait::async_trait]
pub trait SourceConnector: Send + Sync {
    async fn enumerate(&self, root: &SourceRef) -> Result<Vec<SourceEntry>>;
    async fn fetch(&self, entry: &SourceEntry, dest: &std::path::Path) -> Result<FetchedSource>;
}
```

- [ ] Add `SourceRef`, `SourceEntry`, `SourceProvenance`.
- [ ] Test serialization of provenance for local and SharePoint.

### Task 2: Local and Network Connector

- [ ] Move current folder scanner behind `LocalFolderConnector`.
- [ ] Treat UNC paths and mounted drives as `NetworkFolder` when path prefix indicates network.
- [ ] Test local connector does not follow symlinks by default.

### Task 3: SharePoint Connector Client

- [ ] Add Microsoft Graph client wrapper with methods:

```text
list_drive_items
download_drive_item
get_delta
```

- [ ] Use mocked HTTP responses in tests.
- [ ] Store only remote item id, drive id, site id, eTag/cTag, and display path in provenance.
- [ ] Do not store document content outside workspace cache.

### Task 4: Sync Integration

- [ ] Add `sync_source(matter_id)` engine method.
- [ ] Fetch changed remote files into workspace cache.
- [ ] Run normalized ingestion on cached files.
- [ ] Mark deleted remote files as unavailable, not physically deleted from audit history.

### Task 5: UI

- [ ] Add source type segmented control: Local, Network, SharePoint.
- [ ] Local/Network uses folder picker.
- [ ] SharePoint shows connect button and library selector.
- [ ] Show sync status and last sync time.

## Verification

Run:

```powershell
cargo test -p hacienda-workbench-core connectors --lib
cargo check -p hacienda-workbench-tauri
Set-Location apps\hacienda-workbench; npm run build; Set-Location ..\..
```

Acceptance:

```text
Local and network folders use the same connector interface.
SharePoint mocked sync imports changed files into workspace cache.
Provenance is visible on SourceDocument.
Core work remains local after content is fetched.
```
