# Code Quality Implementation Plan (P2 + P6 + P7)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update stale docstrings, propagate workspace lints to 4 remaining crates, and decompose 3 god-files (17,018 lines total) into focused modules.

**Architecture:** P2 and P6 are trivial config changes. P7 uses Rust's internal module pattern: private `mod` declarations + `pub use *` re-exports. Zero public API changes. Each god-file split is a separate task to keep commits atomic.

**Tech Stack:** Rust, Cargo workspace lints

**Build/test commands:**
```powershell
# Targeted check:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package <crate> -Mode check -Profile dev-fast
# Targeted tests:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package <crate>
```

---

### Task 1: Update stale docstrings (P2)

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs:1-3`

- [ ] **Step 1: Replace the stale docstring**

In `crates/anno/src/backends/gliner2_fastino/mod.rs`, replace lines 1-3:

```rust
// Before:
//! gliner2_fastino — fastino-ai GLiNER2 backend (issue #18).
//!
//! **Status:** experimental / WIP. No API stability guarantees in Phase 1.

// After:
//! gliner2_fastino — fastino-ai GLiNER2 backend (issue #18).
//!
//! **Status:** Shipped (Phase 4). Candle + LoRA NER backend with merge-at-load.
```

- [ ] **Step 2: Run check**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno -Mode check -Profile dev-fast
```

Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/anno/src/backends/gliner2_fastino/mod.rs
git commit -m "docs(gliner2): update status from experimental to shipped Phase 4"
```

---

### Task 2: Workspace lint propagation (P6)

**Files:**
- Modify: `crates/anno/Cargo.toml`
- Modify: `crates/anno-cli/Cargo.toml`
- Modify: `crates/anno-eval/Cargo.toml`
- Modify: `crates/anno-corpus-core/Cargo.toml`

The workspace lints defined in the root `Cargo.toml` are:
```toml
[workspace.lints.rust]
missing_docs = "warn"

[workspace.lints.clippy]
unwrap_used = "warn"
```

- [ ] **Step 1: Add `[lints]` to all 4 crates**

Append to the bottom of each `Cargo.toml` (before any `[package.metadata]` section if present):

**`crates/anno/Cargo.toml`** — add:
```toml

[lints]
workspace = true
```

**`crates/anno-cli/Cargo.toml`** — add:
```toml

[lints]
workspace = true
```

**`crates/anno-eval/Cargo.toml`** — add:
```toml

[lints]
workspace = true
```

**`crates/anno-corpus-core/Cargo.toml`** — add:
```toml

[lints]
workspace = true
```

- [ ] **Step 2: Run check and collect warnings**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno -Mode check -Profile dev-fast 2>&1 | Select-String "warning"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-cli -Mode check -Profile dev-fast 2>&1 | Select-String "warning"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-eval -Mode check -Profile dev-fast 2>&1 | Select-String "warning"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-corpus-core -Mode check -Profile dev-fast 2>&1 | Select-String "warning"
```

Expect new `missing_docs` and `clippy::unwrap_used` warnings. Do NOT fix them in this step — just confirm the lints are active.

- [ ] **Step 3: Fix warnings mechanically**

Rules:
- `missing_docs` on private items: add `#[allow(missing_docs)]` at the module level with `// TODO(lint): add docs` if the module is large, or add doc comments if the item is small
- `clippy::unwrap_used` in production code: replace with `.expect("reason")` or proper error handling
- `clippy::unwrap_used` in test code: add `#[allow(clippy::unwrap_used)]` at the test module level
- `unused_imports`: remove them
- `dead_code`: add `#[allow(dead_code)]` with `// TODO(lint): remove or use`
- Do NOT refactor or change behavior — mechanical fixes only

- [ ] **Step 4: Run check to verify zero warnings**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-cli -Mode check -Profile dev-fast
```

Expected: compiles with zero warnings (or only pre-existing workspace-level warnings).

- [ ] **Step 5: Commit**

```bash
git add crates/anno/Cargo.toml crates/anno-cli/Cargo.toml crates/anno-eval/Cargo.toml crates/anno-corpus-core/Cargo.toml
git add -u  # stage all lint fixes
git commit -m "chore(lint): propagate workspace lints to anno, anno-cli, anno-eval, anno-corpus-core

Add [lints] workspace = true to the 4 remaining crates. Fix resulting
warnings: missing_docs, unwrap_used, unused_imports."
```

---

### Task 3: Split `anno-rag-mcp/src/lib.rs` (P7a)

**Files:**
- Create: `crates/anno-rag-mcp/src/params.rs`
- Create: `crates/anno-rag-mcp/src/wire.rs`
- Create: `crates/anno-rag-mcp/src/search.rs`
- Create: `crates/anno-rag-mcp/src/legal.rs`
- Create: `crates/anno-rag-mcp/src/review.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

This task is a pure mechanical move. No logic changes. The strategy:

1. Extract types to new module files
2. Add `mod` + `pub use` in `lib.rs`
3. Verify compilation

- [ ] **Step 1: Create `params.rs`**

Create `crates/anno-rag-mcp/src/params.rs`. Move ALL `*Params` structs from `lib.rs` into this file:

- `KnowledgeAddFolderParams`, `IndexParams`, `PrivacyPrepareFolderParams`, `PrivacyFinalizeFolderParams`
- `KnowledgeSyncParams`, `KnowledgeForgetParams`, `ForgetParams`, `CorpusGetParams`
- `SearchParams`, `SearchUnifiedParams`, `RehydrateParams`, `DetectParams`
- `InitVaultParams`, `MemorySaveParams`, `MemoryRecallParams`, `MemoryGraphRecallParams`
- `MemoryInvalidateParams`, `MemoryForgetParams`, `MemoryListParams`
- Helper functions: `validate_profile()`, `default_index_profile()`, `default_true()`, `default_top_k()`, `default_search_unified_top_k()`, `default_max_hops()`, `default_per_hop_limit()`, `default_forget_limit()`, `default_list_limit()`, `parse_kind()`

Add the necessary `use` imports at the top:

```rust
use serde::Deserialize;
// ... other imports as needed by the moved types
```

- [ ] **Step 2: Create `wire.rs`**

Create `crates/anno-rag-mcp/src/wire.rs`. Move ALL `*Wire`, `*Result` response structs:

- `SearchHitWire`, `SearchResult`, `RehydrateResult`, `DetectResult`, `EntityInfo`
- `VaultStatsResult`, `MemorySaveResultWire`, `MemoryHitWire`, `MemoryRecallResultWire`
- `MemoryInvalidateResultWire`, `MemoryForgetResultWire`, `MemoryListResultWire`
- `DownloadModelsResult`

- [ ] **Step 3: Create `search.rs`**

Create `crates/anno-rag-mcp/src/search.rs`. Move:

- `SearchBackendMode` enum + `impl`
- `SearchExecutionPlan` struct
- `search_execution_plan()` function
- `normalize_search_scope()`, `filter_string()`, `filter_string_vec()`, `filter_f32()`

This module uses types from `params.rs` — import via `use crate::SearchUnifiedParams;` (re-exported).

- [ ] **Step 4: Create `legal.rs`**

Create `crates/anno-rag-mcp/src/legal.rs`. Move ALL `Legal*` types:

- `LegalIngestParams`, `LegalIngestResult`, `LegalSearchParams`, `LegalSearchHitWire`, `LegalSearchResult`
- `LegalGraphQueryParams`, `LegalGraphQueryResult`
- `LegalRehydrateCitationParams`, `LegalRehydrateCitationResult`
- `LegalExtractContractParams`, `LegalExtractCaseFileParams`
- `LegalTimelineParams`, `LegalRiskReviewParams`
- `LegalMandatoryClauseAuditParams`, `LegalPrescriptionCheckParams`
- `LegalInterruptingEventWire`, `LegalValidateFieldParams`
- `build_legal_search_params()`, `knowledge_sync_issue()`

- [ ] **Step 5: Create `review.rs`**

Create `crates/anno-rag-mcp/src/review.rs`. Move:

- `ReviewCreateParams`, `ReviewAddRowsParams`, `ReviewExtractParams`
- Related types

- [ ] **Step 6: Update `lib.rs` with mod declarations and re-exports**

Add these lines near the top of `crates/anno-rag-mcp/src/lib.rs` (after the existing `mod` declarations):

```rust
mod legal;
mod params;
mod review;
mod search;
mod wire;

pub use legal::*;
pub use params::*;
pub use review::*;
pub use search::*;
pub use wire::*;
```

Remove the moved types/functions from `lib.rs`. The tool handler `impl` blocks stay in `lib.rs`.

- [ ] **Step 7: Run check**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: compiles with zero errors. If there are import issues, fix them (typically `use crate::SomeType;` in the new modules).

- [ ] **Step 8: Run tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```

Expected: ALL existing tests pass unchanged.

- [ ] **Step 9: Commit**

```bash
git add crates/anno-rag-mcp/src/params.rs crates/anno-rag-mcp/src/wire.rs crates/anno-rag-mcp/src/search.rs crates/anno-rag-mcp/src/legal.rs crates/anno-rag-mcp/src/review.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "refactor(mcp): split lib.rs (6079L) into 5 focused modules

Extract params, wire types, search logic, legal types, and review types
into separate modules. Public API preserved via re-exports. No behavior
changes — pure code organization."
```

---

### Task 4: Split `anno/src/core/grounded.rs` (P7b)

**Files:**
- Create: `crates/anno/src/core/signal.rs`
- Create: `crates/anno/src/core/track.rs`
- Create: `crates/anno/src/core/identity.rs`
- Create: `crates/anno/src/core/html.rs`
- Create: `crates/anno/src/core/eval_render.rs`
- Modify: `crates/anno/src/core/grounded.rs`
- Modify: `crates/anno/src/core/mod.rs` (if `grounded` is declared there)

- [ ] **Step 1: Create `signal.rs`**

Create `crates/anno/src/core/signal.rs`. Move from `grounded.rs`:

- `Modality` enum (line 122)
- `Signal<L>` struct (line 315) + all `impl Signal` blocks
- `SignalRef` struct (line 681)
- `Quantifier` enum (line 348)
- `SignalValidationError` enum (line 577) + `Display` + `Error` impls
- `impl From<&Entity> for Signal<Location>` (line 656)

Add at the top:
```rust
use super::{Location, grounded::GroundedDocument};
// ... other needed imports
```

- [ ] **Step 2: Create `track.rs`**

Move from `grounded.rs`:

- `Track` struct (line 721) + `impl Track` (line 741)
- `TrackRef` struct (line 694)
- `TrackStats` struct (line 1001)

- [ ] **Step 3: Create `identity.rs`**

Move from `grounded.rs`:

- `IdentitySource` enum (line 1038)
- `Identity` struct (line 1087) + `impl Identity` (line 1114)

- [ ] **Step 4: Create `html.rs`**

Move from `grounded.rs`:

- `render_document_html()` function (line 2642)
- `html_escape()` function (line 3333)
- `annotate_text_html()` function (line 3340)
- Any helper functions used only by HTML rendering

- [ ] **Step 5: Create `eval_render.rs`**

Move from `grounded.rs`:

- `EvalComparison` struct (line 3527)
- `EvalMatch` enum (line 3540)
- `impl EvalComparison` (line 3580)
- `render_eval_html()` function (line 3755)
- `render_eval_html_with_title()` function (line 3763)
- `EvalHtmlSpan` struct (line 4167)
- `annotate_text_spans()` function (line 4175)

- [ ] **Step 6: Update `grounded.rs` with mod declarations and re-exports**

Add at the top of `grounded.rs`:

```rust
mod signal;
mod track;
mod identity;
mod html;
mod eval_render;

pub use signal::*;
pub use track::*;
pub use identity::*;
pub use html::*;
pub use eval_render::*;
```

The remaining content in `grounded.rs`:
- `Location` enum + impls
- `GroundedDocument` struct + impl
- `GroundedDocumentWire`
- `TextSpatialIndex`, `IntervalNode`
- `DocumentStats`
- `ProcessOptions`, `ProcessResult`, `Corpus`

- [ ] **Step 7: Run check**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno -Mode check -Profile dev-fast
```

Fix any import issues. Common patterns:
- Types in new modules that reference `GroundedDocument` → `use super::GroundedDocument;`
- Types that reference `Location` → `use super::Location;`
- Cross-module references → use `crate::` paths

- [ ] **Step 8: Run tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno
```

Expected: ALL existing tests pass.

- [ ] **Step 9: Cross-crate check**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -AllAffected
```

Expected: all downstream crates that import from `anno::core::grounded` still compile.

- [ ] **Step 10: Commit**

```bash
git add crates/anno/src/core/signal.rs crates/anno/src/core/track.rs crates/anno/src/core/identity.rs crates/anno/src/core/html.rs crates/anno/src/core/eval_render.rs crates/anno/src/core/grounded.rs
git commit -m "refactor(core): split grounded.rs (6005L) into 5 focused modules

Extract signal, track, identity, HTML rendering, and eval rendering
into separate modules under core/. Public API preserved via re-exports."
```

---

### Task 5: Split `anno/src/backends/coref/mention_ranking/algorithm.rs` (P7c)

**Files:**
- Create: `crates/anno/src/backends/coref/mention_ranking/features.rs`
- Create: `crates/anno/src/backends/coref/mention_ranking/scoring.rs`
- Modify: `crates/anno/src/backends/coref/mention_ranking/algorithm.rs`
- Modify: `crates/anno/src/backends/coref/mention_ranking/mod.rs`

- [ ] **Step 1: Create `features.rs`**

Create `crates/anno/src/backends/coref/mention_ranking/features.rs`. Move from `algorithm.rs`:

- `is_type_incompatible()` function (line 35)
- `animacy_from_pronoun()` function (line 51)
- `animacy_from_entity_type()` function (line 73)
- Feature extraction methods from the `impl MentionRankingCoref` block — identify methods whose name contains `feature`, `animacy`, `gender`, or `type_compat`

Use `impl MentionRankingCoref` blocks in the new file (Rust allows split impl across child modules):

```rust
use super::MentionRankingCoref;

pub(super) fn is_type_incompatible(mention_a: &str, mention_b: &str) -> bool {
    // ... moved code
}

pub(super) fn animacy_from_pronoun(text_lower: &str) -> Animacy {
    // ... moved code
}

pub(super) fn animacy_from_entity_type(entity_type: &crate::EntityType) -> Animacy {
    // ... moved code
}

impl MentionRankingCoref {
    // Move feature extraction methods here as pub(super)
}
```

- [ ] **Step 2: Create `scoring.rs`**

Create `crates/anno/src/backends/coref/mention_ranking/scoring.rs`. Move scoring/ranking methods from the main `impl MentionRankingCoref` block — identify methods whose name contains `score`, `rank`, `candidate`, or `cluster_score`.

```rust
use super::MentionRankingCoref;

impl MentionRankingCoref {
    // Move scoring methods here as pub(super)
}
```

- [ ] **Step 3: Update `algorithm.rs` with mod declarations**

Add at the top of `algorithm.rs`:

```rust
mod features;
mod scoring;
```

No `pub use` needed — the methods are `impl MentionRankingCoref` blocks that are automatically available on the struct.

The free functions (`is_type_incompatible`, etc.) if called from `algorithm.rs` need:
```rust
use features::{is_type_incompatible, animacy_from_pronoun, animacy_from_entity_type};
```

- [ ] **Step 4: Update `mod.rs` if needed**

Check `crates/anno/src/backends/coref/mention_ranking/mod.rs` — it should already declare `pub mod algorithm;`. No change needed unless the new modules need to be visible outside `mention_ranking`.

- [ ] **Step 5: Run check**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno -Mode check -Profile dev-fast
```

Fix visibility issues. Common: methods called from `algorithm.rs` that were moved need `pub(super)` visibility.

- [ ] **Step 6: Run tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno
```

Expected: ALL tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/anno/src/backends/coref/mention_ranking/features.rs crates/anno/src/backends/coref/mention_ranking/scoring.rs crates/anno/src/backends/coref/mention_ranking/algorithm.rs
git commit -m "refactor(coref): split algorithm.rs (4934L) into features + scoring modules

Extract feature extraction and scoring methods into separate modules.
MentionRankingCoref struct stays in algorithm.rs with impl blocks split
across child modules."
```

---

## Verification

After all 5 tasks:

- [ ] **Full cross-crate check**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -AllAffected
```

- [ ] **Verify line counts decreased**

```bash
wc -l crates/anno-rag-mcp/src/lib.rs crates/anno/src/core/grounded.rs crates/anno/src/backends/coref/mention_ranking/algorithm.rs
```

Expected: all three under 4,000 lines.
