# Hacienda Tauri GLiNER2 LoRA Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add extraction profiles that select GLiNER2 labels, thresholds, and LoRA metadata per legal workflow or matter.

**Architecture:** Introduce one `profiles.rs` module in `hacienda-workbench-core`. It loads built-in profile manifests, records the base GLiNER2 model plus optional merged-model or adapter metadata, and stores profile metadata on every anonymization/extraction result. Runtime raw LoRA loading is feature-gated behind `gliner2-fastino-candle` and is not part of the default path.

**Tech Stack:** Rust 1.95, `anno-rag::detect::Detector::detect_with_labels`, GLiNER2/Fastino APIs already in `anno`, TOML/JSON manifests, SQLite profile metadata.

---

## Lean Validation

GLiNER2 and LoRA are aligned with the repo, but the implementation must respect the current feature split:

- `anno-rag` currently depends on `anno` with `gliner2-fastino` ONNX support.
- Runtime LoRA adapter loading exists on the experimental Candle backend behind `anno` feature `gliner2-fastino-candle`.
- The ONNX `GLiNER2Fastino` path explicitly rejects raw LoRA adapter directories and expects adapters to be merged/exported first.

Apply these reductions before implementing:

- Phase 1 should store and display profile metadata and support profile labels/thresholds using existing GLiNER2 extraction paths.
- For LoRA, start with merged local model directories or metadata-only adapter records unless the work explicitly enables and validates `gliner2-fastino-candle`.
- Keep this in one `profiles.rs` module initially. Split only if runtime model loading, manifest parsing, and UI state become independently large.
- Do not add a separate model registry service; use local paths, built-in manifests, and explicit cache-missing warnings.

## Scope

In scope:

- Built-in profiles: generic, contract, litigation, employment, real estate, corporate.
- Profile fields: labels, thresholds, adapter id/path, model id, version.
- Persist selected profile per matter.
- Record profile metadata per working document extraction.

Out of scope:

- Training LoRA adapters.
- Hot-swapping adapters per single detection call.
- Resource-pack signing; packaging plan covers signed distribution.

## Files

- Create `crates/hacienda-workbench-core/src/profiles.rs`
- Modify `crates/hacienda-workbench-core/src/anonymize.rs`
- Modify `crates/hacienda-workbench-core/src/engine.rs`
- Modify `crates/hacienda-workbench-core/src/store.rs`
- Add profile manifests under `crates/hacienda-workbench-core/src/profiles/*.toml`
- Modify Tauri commands and UI profile selector.

## Tasks

### Task 1: Profile Manifest Type

- [ ] Add:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExtractionProfile {
    pub id: String,
    pub name: String,
    pub version: String,
    pub base_model: String,
    pub adapter: Option<AdapterRef>,
    pub labels: Vec<ProfileLabel>,
}
```

- [ ] Include `threshold: f32` per label.
- [ ] Test TOML round-trip for `contract-v1`.
- [ ] Run `cargo test -p hacienda-workbench-core profiles --lib`.

### Task 2: Built-In Profiles

- [ ] Add built-in TOML manifests for the six profiles.
- [ ] Contract profile labels include `contract_party`, `obligation`, `effective_date`, `deadline`, `amount`, `clause_type`.
- [ ] Employment profile labels include employee/employer role labels plus dates and compensation.
- [ ] Test `ProfileRegistry::builtin().get("contract-v1")`.

### Task 3: Detection Integration

- [ ] Extend anonymization request with optional `profile_id`.
- [ ] When profile is generic, keep `detect_patterns` + default PII labels.
- [ ] When profile has labels, call `Detector::detect_with_labels` if the model is available.
- [ ] If GLiNER2 model is unavailable, fall back to `detect_patterns` and record warning.
- [ ] Test fallback path without model download.

### Task 4: Persistence

- [ ] Add `profile_id`, `profile_version`, `base_model`, `adapter_id` to working document extraction metadata.
- [ ] Store metadata as JSON if schema churn is too high for separate columns.
- [ ] Test that `MatterDetail` exposes extraction profile metadata.

### Task 5: UI

- [ ] Add profile selector on matter page.
- [ ] Disable profile changes while ingestion is running.
- [ ] Show model/profile metadata in PII panel.
- [ ] Run frontend build.

## Verification

Run:

```powershell
cargo test -p hacienda-workbench-core profiles anonymize --lib
cargo check -p hacienda-workbench-tauri
Set-Location apps\hacienda-workbench; npm run build; Set-Location ..\..
```

Acceptance:

```text
Matter can select contract profile.
Working document stores profile id/version/base model/adapter id.
If model cache is absent, app falls back to pattern PII and shows warning.
No extraction result lacks profile metadata.
```
