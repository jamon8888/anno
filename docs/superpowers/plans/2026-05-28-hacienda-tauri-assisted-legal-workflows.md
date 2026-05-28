# Hacienda Tauri Assisted Legal Workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add declarative assisted legal workflows that propose cited checklist items and fields for a matter, with human validation and correction.

**Architecture:** Represent workflows as versioned TOML manifests. A workflow runtime evaluates expected document types, field schemas, extraction profiles, checklist items, missing-piece rules, and contradiction rules against normalized working documents and tabular extraction outputs.

**Tech Stack:** Rust 1.95, `hacienda-workbench-core`, `anno-rag-tabular` for cited field extraction, TOML manifests, SQLite workflow state, Tauri UI.

---

## Scope

In scope:

- Workflow manifests for commercial contract, litigation, employment, real estate, corporate.
- Checklist item states: proposed, validated, corrected, rejected, needs source, blocked.
- Cited field output with source chunk/document references.
- Human validation and correction.

Out of scope:

- Free-form visual workflow builder.
- Multi-user assignment.
- Court deadline calculation with jurisdiction-specific calendars.

## Files

- Create `crates/hacienda-workbench-core/src/workflows/mod.rs`
- Create `crates/hacienda-workbench-core/src/workflows/manifest.rs`
- Create `crates/hacienda-workbench-core/src/workflows/runtime.rs`
- Add manifests under `crates/hacienda-workbench-core/src/workflows/templates/*.toml`
- Modify `store.rs`, `engine.rs`, Tauri commands, frontend UI.

## Tasks

### Task 1: Workflow Manifest

- [ ] Define:

```rust
pub struct WorkflowManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub extraction_profile_id: String,
    pub expected_documents: Vec<ExpectedDocument>,
    pub fields: Vec<WorkflowField>,
    pub checklist: Vec<ChecklistTemplateItem>,
}
```

- [ ] Load TOML manifests from built-ins.
- [ ] Test `commercial-contract-v1` parses and references an existing extraction profile.

### Task 2: Workflow State Store

- [ ] Add SQLite tables `workflow_runs`, `workflow_items`, `workflow_fields`.
- [ ] Persist item state, corrected value, citations JSON, and updated time.
- [ ] Test creating a workflow run and updating one item to validated.

### Task 3: Runtime

- [ ] Implement `start_workflow(matter_id, workflow_id)`.
- [ ] Detect missing expected documents from source file names and normalized metadata.
- [ ] Seed checklist items as `proposed` or `needs_source`.
- [ ] Add simple field extraction from existing `WorkingDocument` text using keywords before wiring tabular.
- [ ] Test a matter with one contract file produces party/date checklist items.

### Task 4: Tabular Integration

- [ ] Map workflow fields to `anno-rag-tabular` columns.
- [ ] Use local extraction first; route `LlmRequired` fields only when provider is configured.
- [ ] Store citations with every field.
- [ ] Test no field is marked validated without a citation.

### Task 5: UI

- [ ] Add workflow tab to matter page.
- [ ] Add workflow picker.
- [ ] Render checklist with states and citations.
- [ ] Add validate/correct/reject actions.
- [ ] Run frontend build.

## Verification

Run:

```powershell
cargo test -p hacienda-workbench-core workflows --lib
cargo test -p anno-rag-tabular
cargo check -p hacienda-workbench-tauri
Set-Location apps\hacienda-workbench; npm run build; Set-Location ..\..
```

Acceptance:

```text
User can start commercial-contract-v1 on a matter.
Checklist appears with proposed and needs_source states.
Validated item keeps citation metadata.
Correction creates audit event and does not edit source files.
```
