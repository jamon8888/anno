# Hacienda Tauri Client Workbench - App autonome offline

Date: 2026-05-28
Status: Design approved, pending user spec review

## Context

Hacienda already provides the core local privacy stack: PII detection, pseudonymisation, encrypted vault, local RAG, legal graph groundwork, tabular legal extraction, and release planning for native installers. The missing product layer is a usable desktop application for legal teams that need to understand a client folder globally, edit anonymized work products, and run legal workflows without sending sensitive data outside the machine.

This design defines a new autonomous Tauri application. It is not only a thin UI over `anno-rag`. It is a desktop product with its own workspace, update model, document editor, connector layer, workflow system, and embedded local engine. Existing Hacienda crates should be reused where they fit, but the app boundary is the product.

## Product Goal

Build a Windows and macOS desktop app for lawyers, DPOs, and legal operations teams that can:

- open local client folders, network cabinet folders, and SharePoint/OneDrive libraries;
- show a global view of documents, anonymization status, PII categories, confidence, provenance, and audit history;
- generate editable anonymized working documents while keeping originals read-only;
- let users edit normalized anonymized content, not source files;
- propose legal workflows with checklists, extracted fields, citations, missing-piece alerts, and human validation;
- run offline after installation, including bundled models and legal workflow resources;
- update the app and resource packs through signed artifacts.

## Non-Goals

- No modification or replacement of original source documents.
- No faithful in-place editor for every original format in v1.
- No cloud dependency for core anonymization, editing, indexing, or workflow execution.
- No proprietary DMS integration such as iManage or NetDocuments in v1.
- No full visual workflow-builder engine in v1. Workflows are declarative and versioned; expert editing can be added after the assisted workflow loop is stable.

## Scope Decisions

### App Shape

Use approach B: a Tauri shell with a local embedded Rust engine.

The UI owns product navigation, review ergonomics, and document editing. The Rust engine owns ingestion, format extraction, OCR orchestration, PII detection, pseudonymisation, vault access, indexing, workflow execution, audit, exports, and resource-pack management.

Avoid a local HTTP server for v1. It adds support risk through ports, firewall prompts, antivirus behaviour, and process lifecycle complexity. Tauri commands or an internal IPC boundary are sufficient for a desktop-only product.

### Data Sources

Support three source classes in v1:

- local folders selected by the user;
- network folders mounted by the operating system, including Windows shared drives and NAS paths;
- SharePoint/OneDrive through a connector layer.

All sources feed the same normalized ingestion pipeline. The connector layer should expose a small interface: enumerate, stat, fetch, hash, watch or poll, and record provenance. A future DMS connector must plug into the same interface without changing the anonymization or workflow layers.

### Document Formats

The v1 target is broad: PDF, DOCX, XLSX, PPTX, email files, and OCR-backed images or scanned PDFs. The editing model stays narrow: every source is converted to a normalized working document made of sections, paragraphs, tables, pages, attachments, metadata, citations, and source spans.

Users edit the normalized anonymized document. They do not edit PDF internals, Word XML, spreadsheet formulas, slide layouts, or email source files directly.

### Installation and Updates

The default installer is complete and offline:

- Windows: signed MSI or NSIS installer.
- macOS: signed and notarized DMG.
- Payload: app, Rust engine, base GLiNER2 model, LoRA adapters, embedder, OCR resources where legally distributable, workflow templates, PII rules, export templates, and connector code.

Tauri v2 supports updater artifacts with mandatory signatures and can use a static JSON file or an update server. The app should use this for app-level updates. Resource updates should be separate signed packs so a new LoRA adapter, workflow template, PII rule, or export template does not require reinstalling the whole app.

Offline cabinets can import signed resource packs from local files.

References:

- Tauri updater: https://v2.tauri.app/fr/plugin/updater/
- Tauri Windows installers: https://v2.tauri.app/distribute/windows-installer/

## Architecture

```text
Tauri desktop app
  UI shell
  command boundary
  embedded Rust engine
    source connectors
    format extraction
    OCR
    normalized document store
    PII and legal extraction
    encrypted vault
    local indexes
    workflow runtime
    audit log
    export engine
    resource-pack manager
```

The app should be autonomous, but its internals must stay modular. Each engine subsystem should have a stable Rust interface and focused tests. The UI should not know implementation details such as GLiNER2 loading, vault internals, LanceDB or SQLite tables, OCR tools, or connector-specific API details.

## Data Model

Each client matter has four separated areas.

### Source References

Store source metadata only:

- source kind;
- absolute path or connector reference;
- content hash;
- size, modified time, author or owner when available;
- source version or SharePoint drive item version;
- import time;
- last scan status.

The original file remains where it is and is opened read-only.

### Anonymized Working Documents

Store normalized editable content:

- document tree;
- paragraphs, sections, tables, pages, attachments, comments;
- source span links and citations;
- anonymized text containing tokens or approved replacements;
- revision history;
- review status;
- export status.

Every edit creates a new revision or patch record. The app can present a clean current view, but the underlying store remains auditable.

### PII Vault

Store encrypted mappings separately from working documents:

- clear value;
- token;
- category;
- detection source;
- confidence;
- source document and span provenance;
- model base, adapter, labels, thresholds, and rule versions;
- creation and modification audit references.

The vault is protected through OS keyring or a strong local secret. Rehydration is explicit, permissioned, and logged.

### Audit Log

Use append-only audit events for:

- import;
- scan;
- anonymization;
- user edit;
- PII rehydration;
- workflow validation;
- workflow correction;
- export;
- resource-pack import;
- connector sync.

The audit log should be hash chained to detect tampering. It does not replace a legal record management system, but it gives strong local traceability for RGPD and professional secrecy workflows.

## GLiNER2 and LoRA Strategy

GLiNER2 is the base extraction model for PII and legal entities. Domain specialization is handled with LoRA adapters instead of separate full model copies.

```text
GLiNER2 base model
  + contract commercial adapter
  + litigation adapter
  + employment law adapter
  + real estate adapter
  + corporate adapter
```

A workflow selects an extraction profile:

- active LoRA adapter;
- labels for PII and legal entities;
- per-category thresholds;
- normalization rules;
- confidence policy;
- workflow field schema;
- export schema.

Adapter swapping should happen per workflow session or document batch, not per single detection call. Results must record the model base version, adapter version, labels, thresholds, and resource-pack hash. This is required for the product promise: showing what was anonymized and how.

The offline installer bundles the base model and initial adapters. Later adapters are delivered as signed resource packs.

## User Experience

The app opens on an operational dashboard, not a marketing screen.

Primary screens:

- Cabinet dashboard: matters, anonymization progress, alerts, workflow state.
- Client matter: source documents, working documents, global PII view, audits, exports.
- Document atelier: read-only source viewer, normalized anonymized editor, PII panel, citations.
- Global PII view: all detected PII for the matter, type, confidence, source, status, usage count.
- Assisted legal workflow: checklist, extracted fields, missing documents, contradictions, validation state.
- Export center: DOCX, PDF, Markdown, CSV/XLSX tables, anonymization report, audit export.

Core flow:

1. User adds a local folder, network folder, or SharePoint/OneDrive library.
2. The app scans and hashes sources.
3. The app extracts normalized working documents.
4. The engine detects PII and legal entities with the selected or inferred profile.
5. The app creates anonymized editable documents.
6. The user reviews and edits the anonymized working documents.
7. The workflow engine proposes checklists and extracted fields.
8. The user validates, corrects, or rejects items.
9. The app exports anonymized deliverables and reports.

## Workflows

Workflows are assisted and declarative in v1. They are not a full free-form automation builder.

Initial workflow families:

- commercial contract;
- litigation;
- employment law;
- real estate;
- corporate.

Each workflow defines:

- expected document types;
- field schema;
- GLiNER2 labels;
- LoRA adapter preference;
- extraction thresholds;
- checklist items;
- validation rules;
- missing-piece rules;
- contradiction checks;
- export templates.

Each workflow item has a state:

- proposed;
- validated;
- corrected;
- rejected;
- needs source;
- blocked.

The runtime must preserve citations to source spans. If a field has no support in the source material, it should be marked as missing or inferred only with a low-trust state. Legal users must be able to correct the proposed answer without changing the original source.

## Storage Recommendation

Use local storage with clear ownership boundaries:

- SQLite for metadata, workflow state, audit references, normalized document structure, and resource pack registry.
- Encrypted vault file or encrypted SQLite database for PII mappings.
- LanceDB or the existing local vector store for semantic indexes where useful.
- File-backed blob store for normalized extracted text, OCR text, page images, and export artifacts when large.

This keeps queryable state in SQLite, sensitive mappings in a separately protected vault, and large content outside oversized database rows.

## Security Requirements

- Originals are read-only.
- Clear PII is not stored in anonymized working documents.
- Vault access is explicit and audited.
- Rehydration requires a deliberate user action.
- Resource packs are signed and versioned.
- Connectors store only the minimal tokens required by the OS or provider integration.
- No silent network calls during offline mode.
- Error messages must not leak sensitive document text into logs.
- Logs and crash reports are local by default and must be scrubbed before export.

## Testing Strategy

Unit tests:

- source connector interface contracts;
- normalized document transformations;
- PII token mapping and vault rules;
- workflow schema validation;
- adapter profile resolution;
- audit hash chaining.

Integration tests:

- local folder ingest;
- network-folder-like ingest through fixture paths;
- SharePoint/OneDrive connector with mocked API responses;
- PDF, DOCX, XLSX, PPTX, email, and OCR fixture conversion;
- anonymized document edit and revision history;
- workflow run with cited fields;
- export generation.

Security and privacy tests:

- originals are never modified;
- anonymized working documents contain no clear PII;
- vault access is required for rehydration;
- audit logs are append-only and tamper-detectable;
- resource packs fail closed when unsigned or hash mismatched.

Packaging tests:

- Windows installer installs, launches, updates, and uninstalls cleanly;
- macOS DMG launches after signing and notarization;
- offline first-run works without model download;
- app update and resource-pack update are separate.

## Phased Delivery

### Phase 1 - Walking Skeleton

- Tauri app shell.
- Embedded Rust engine boundary.
- Local folder ingestion.
- Normalized text document model.
- Basic PII detection and anonymized editor.
- SQLite metadata store.
- Separate encrypted vault.
- Basic audit log.

### Phase 2 - Broad Format Ingestion

- PDF, DOCX, XLSX, PPTX, email, image/OCR conversion.
- Source viewer with read-only provenance.
- Revisioned anonymized working documents.
- Global PII view.

### Phase 3 - GLiNER2/LoRA Profiles

- Bundle base GLiNER2 model.
- Load LoRA adapters from resource packs.
- Add extraction profiles per workflow family.
- Persist model, adapter, labels, thresholds, and pack hashes for every extraction.

### Phase 4 - Assisted Legal Workflows

- Commercial contract, litigation, employment, real estate, and corporate workflow templates.
- Checklist runtime.
- Cited field extraction.
- Human validation and correction loop.
- Missing-piece and contradiction alerts.

### Phase 5 - Connectors and Packaging

- SharePoint/OneDrive connector.
- Network folder hardening.
- Signed Windows installer.
- Signed and notarized macOS DMG.
- Tauri updater for app updates.
- Signed resource-pack import and update.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Offline installer becomes too large | Separate app updates from signed resource packs; keep base model plus adapters, not many full models |
| Broad file format support slows v1 | Normalize all formats into one editable model; avoid native faithful editing |
| Legal extraction appears more certain than it is | Preserve citations, confidence, and validation states |
| LoRA adapter swaps add latency | Load adapters per workflow session or batch |
| SharePoint integration complicates offline story | Treat SharePoint as ingestion and sync; core work remains local once content is fetched |
| Vault and audit logic become hard to reason about | Keep vault separate, append-only audit, and test privacy invariants |
| Proprietary DMS expectations appear early | Support mounted network folders first; keep connector interface ready for future DMS |

## Success Criteria

- A user can add a client folder and see all documents with anonymization status.
- Originals remain unchanged after scan, anonymization, editing, and export.
- The app can show every PII token with category, source, confidence, model or rule source, and current status.
- The user can edit an anonymized normalized document and keep revision history.
- A workflow can propose a cited checklist for at least one legal family.
- GLiNER2 base plus at least one LoRA adapter can be selected by workflow profile.
- The first offline installer launches without downloading models.
- App updates and resource-pack updates are signed and handled separately.

