# Hacienda Tauri Connectors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a connector layer for network folders and SharePoint/OneDrive so all external sources feed the same local ingestion pipeline.

**Architecture:** Keep local and mounted network paths on the existing folder ingestion path first. Add provenance fields to persisted source documents. Introduce a `SourceConnector` trait only when the SharePoint/Microsoft Graph slice exists and needs the same ingestion interface as filesystem sources.

**Tech Stack:** Rust 1.95, `reqwest`, `serde`, Microsoft Graph REST, Tauri commands, existing normalized ingestion pipeline.

---

## Lean Validation

The connector plan is useful, but a trait hierarchy is premature for Phase 1. The walking skeleton already has local folder ingestion, and mounted network folders can use the same path-based flow.

Apply these reductions before implementing:

- Do not add `connectors/mod.rs`, `local.rs`, and `sharepoint.rs` until a real non-filesystem connector is implemented.
- Keep local and network folder support as `SourceKind` plus concrete path ingestion in `ingest.rs`.
- Add a `SourceConnector` trait only when SharePoint/Graph fetching is actually built and tested.
- For SharePoint, start as a separate later slice with a mocked Graph client and explicit OS-managed token storage. Do not mix OAuth and local folder work in the same implementation step.
- Keep provenance fields on `SourceDocument`; avoid a separate provenance model unless multiple connector types need it.

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

- Modify `model.rs`, `store.rs`, `engine.rs`
- Modify Tauri commands and frontend source picker.
- Create connector modules only in the later SharePoint slice, when a non-filesystem connector exists.

## Tasks

### Task 1: Local and Network Paths

- [ ] Keep the current folder scanner in `ingest.rs`.
- [ ] Treat UNC paths and mounted drives as `NetworkFolder` when path prefix indicates network.
- [ ] Store provenance fields directly on `SourceDocument`: source kind, source id/path/url when available, and content hash.
- [ ] Test local/network folder scanning does not follow symlinks by default.

### Task 2: SharePoint Connector Client

- [ ] Add Microsoft Graph client wrapper with methods:

```text
list_drive_items
download_drive_item
get_delta
```

- [ ] Use mocked HTTP responses in tests.
- [ ] Store only remote item id, drive id, site id, eTag/cTag, and display path in provenance.
- [ ] Do not store document content outside workspace cache.
- [ ] At this point only, introduce `SourceConnector` if the Graph path needs the same ingestion interface as local files.

### Task 3: Sync Integration

- [ ] Add `sync_source(matter_id)` engine method.
- [ ] Fetch changed remote files into workspace cache.
- [ ] Run normalized ingestion on cached files.
- [ ] Mark deleted remote files as unavailable, not physically deleted from audit history.

### Task 4: UI

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
