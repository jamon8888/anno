# Hacienda Tauri Assisted Legal Workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add declarative assisted legal workflows that propose cited checklist items and fields for a matter, with human validation and correction.

**Architecture:** Start from existing `anno-rag-tabular` TOML templates and `anno-rag::legal::extract` workflow functions. The workbench stores lightweight workflow run state and maps template fields/checklist items to cited extraction outputs; a new manifest/runtime layer is deferred until existing templates cannot represent a real workflow.

**Tech Stack:** Rust 1.95, `hacienda-workbench-core`, `anno-rag-tabular` for cited field extraction, TOML manifests, SQLite workflow state, Tauri UI.

---

## Lean Validation

The workflow goal is valid, but the repo already has two relevant foundations: `anno-rag-tabular` has TOML templates and cited extraction columns, and `anno-rag::legal::extract` already exposes contract/case-file/timeline/risk workflow functions.

Apply these reductions before implementing:

- Do not introduce a new workflow DSL in Phase 1.
- Prefer adapting existing `anno-rag-tabular::schema::Template` and built-in templates for checklist/field schemas.
- Prefer calling existing `anno-rag::legal::extract` functions for contract, case-file, timeline, and risk surfaces before creating new runtime logic.
- Start with one `workflow.rs` module or add methods to `engine.rs`; create `workflows/manifest.rs` and `workflows/runtime.rs` only after the first workflow proves the split is needed.
- Store workflow state in the existing workbench SQLite schema; do not add a separate workflow store.

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

- Modify `crates/hacienda-workbench-core/src/model.rs`
- Modify `crates/hacienda-workbench-core/src/store.rs`
- Modify `crates/hacienda-workbench-core/src/engine.rs`
- Modify Tauri commands and frontend UI.
- Optionally create one `crates/hacienda-workbench-core/src/workflow.rs` if workflow code makes `engine.rs` too large.

## Tasks

### Task 1: Reuse Existing Templates

- [ ] Start from `anno-rag-tabular::schema::Template` and `Template::list_builtin()`.
- [ ] Map built-in template columns to checklist/field display rows.
- [ ] Add only a thin workbench workflow descriptor if the UI needs names/stages that tabular templates do not carry.
- [ ] Do not add a new manifest format until one workflow cannot be represented by existing templates.

If a descriptor is needed, keep it minimal:

```rust
pub struct WorkflowDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub template_id: Option<String>,
}
```

- [ ] Test unknown workflow/template id and version parsing.

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
