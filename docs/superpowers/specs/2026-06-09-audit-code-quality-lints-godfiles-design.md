# Code Quality — Lint Propagation, Docstrings, God-file Decomposition (P2 + P6 + P7)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Propagate workspace lints to all crates, update stale docstrings, and decompose three god-files (17,018 lines total) into focused modules.

**Priority:** P3 — code quality and maintainability. No runtime behavior change.

**Crates touched:** `anno`, `anno-cli`, `anno-eval`, `anno-corpus-core`, `anno-rag-mcp`

---

## P2 — Stale Docstrings

### Problem

`crates/anno/src/backends/gliner2_fastino/mod.rs:3` still says `"experimental / WIP"`. Phase 4 (Candle + LoRA) shipped on main and is the default backend.

### Fix

Replace:
```rust
//! experimental / WIP. No API stability guarantees in Phase 1.
```
With:
```rust
//! Candle + LoRA NER backend (GLiNER2-Fastino). Shipped in Phase 4.
//! Uses native Rust inference via the candle framework with LoRA adapter merge-at-load.
```

### Files

- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs:3`

---

## P6 — Workspace Lint Propagation

### Problem

9 of 13 crates inherit `[lints] workspace = true`. Four crates are missing it:

| Crate | Lines | Impact |
|-------|-------|--------|
| `anno` | ~80 source files | Largest crate — most lint gaps |
| `anno-cli` | CLI entry point | |
| `anno-eval` | Evaluation framework | 1664 symbols |
| `anno-corpus-core` | Corpus data types | |

### Fix

Add to each crate's `Cargo.toml`:

```toml
[lints]
workspace = true
```

### Lint warnings

After adding the `[lints]` section, new warnings will appear. The approach:

1. Add `[lints] workspace = true` to all 4 crates
2. Run `cargo check -p anno -p anno-cli -p anno-eval -p anno-corpus-core`
3. Fix warnings in batches:
   - `unused_imports` — remove
   - `dead_code` — add `#[allow(dead_code)]` with a `// TODO: remove or use` comment for genuinely dead code, or remove if clearly unused
   - `unused_variables` — prefix with `_`
   - Other warnings — fix case by case
4. Do NOT refactor or change behavior while fixing warnings — mechanical fixes only

### Files

- Modify: `crates/anno/Cargo.toml`
- Modify: `crates/anno-cli/Cargo.toml`
- Modify: `crates/anno-eval/Cargo.toml`
- Modify: `crates/anno-corpus-core/Cargo.toml`
- Modify: various `.rs` files to fix lint warnings

---

## P7 — God-file Decomposition

### Principle

Each god-file is split into internal modules (`mod name;`, not `pub mod`). Public API is preserved via re-exports from the parent file. Zero breaking changes to downstream consumers.

The pattern for each split:

```rust
// In the parent file (e.g., lib.rs):
mod params;    // private module
mod wire;      // private module

pub use params::*;  // re-export all public items
pub use wire::*;
```

### Pre-work: Impact analysis

Before any split, run `gitnexus_impact` on the major public symbols in each file to understand the blast radius. The splits must not change any public API.

---

### P7a — `anno-rag-mcp/src/lib.rs` (6,079 lines → 6 files)

#### Current structure

The file contains:
- `AnnoRagServer` struct + two `impl` blocks (tool handlers)
- ~25 `*Params` request structs with serde derives and validation helpers
- ~15 `*Wire` / `*Result` response types
- Search execution plan types and logic
- Legal-domain types and helpers
- Memory-domain types and helpers
- 4 already-extracted modules: `allowed_roots`, `corpus_sync`, `indexer`, `legal_maintenance`

#### Split plan

| New file | Contents to extract | Approx lines |
|----------|-------------------|-------------|
| `src/params.rs` | All `*Params` structs: `KnowledgeAddFolderParams`, `IndexParams`, `PrivacyPrepareFolderParams`, `PrivacyFinalizeFolderParams`, `KnowledgeSyncParams`, `KnowledgeForgetParams`, `ForgetParams`, `CorpusGetParams`, `SearchParams`, `SearchUnifiedParams`, `RehydrateParams`, `DetectParams`, `InitVaultParams`, `MemorySaveParams`, `MemoryRecallParams`, `MemoryGraphRecallParams`, `MemoryInvalidateParams`, `MemoryForgetParams`, `MemoryListParams`, `DownloadModelsResult`. Also: `validate_profile()`, `default_*()` helpers, `parse_kind()`. | ~600 |
| `src/wire.rs` | All `*Wire` / `*Result` response types: `SearchHitWire`, `SearchResult`, `RehydrateResult`, `DetectResult`, `EntityInfo`, `VaultStatsResult`, `MemorySaveResultWire`, `MemoryHitWire`, `MemoryRecallResultWire`, `MemoryInvalidateResultWire`, `MemoryForgetResultWire`, `MemoryListResultWire`, `DownloadModelsResult`. | ~400 |
| `src/search.rs` | `SearchBackendMode` enum + `impl`, `SearchExecutionPlan` struct, `search_execution_plan()` function, `normalize_search_scope()`, `filter_string()`, `filter_string_vec()`, `filter_f32()`. | ~300 |
| `src/legal.rs` | All `Legal*` types: `LegalIngestParams`, `LegalIngestResult`, `LegalSearchParams`, `LegalSearchHitWire`, `LegalSearchResult`, `LegalGraphQueryParams`, `LegalGraphQueryResult`, `LegalRehydrateCitationParams`, `LegalRehydrateCitationResult`, `LegalExtractContractParams`, `LegalExtractCaseFileParams`, `LegalTimelineParams`, `LegalRiskReviewParams`, `LegalMandatoryClauseAuditParams`, `LegalPrescriptionCheckParams`, `LegalInterruptingEventWire`, `LegalValidateFieldParams`, `build_legal_search_params()`, `knowledge_sync_issue()`. | ~500 |
| `src/review.rs` | `ReviewCreateParams`, `ReviewAddRowsParams`, `ReviewExtractParams`, and related types. | ~200 |
| `src/lib.rs` | `AnnoRagServer` struct + `impl` blocks with tool handlers + `allowed_roots_from_env()` + `mod` declarations + re-exports. | ~4,000 |

#### Dependencies between modules

- `params.rs` and `wire.rs` are leaf modules — no internal dependencies
- `search.rs` uses types from `params.rs` (`SearchUnifiedParams`)
- `legal.rs` uses types from `params.rs` (`SearchUnifiedParams`) and `wire.rs`
- `review.rs` is independent
- `lib.rs` imports from all of the above

#### Import strategy

Each extracted module gets the minimal `use` statements it needs from external crates (`serde`, `serde_json`, `anno_rag`, etc.). Internal cross-references use `crate::` paths.

---

### P7b — `anno/src/core/grounded.rs` (6,005 lines → 6 files)

#### Current structure

The file contains:
- Core types: `Modality` enum, `Location` enum, `Signal<L>` struct, `Quantifier` enum
- `SignalRef`, `TrackRef` reference types
- `Track` struct with large `impl` block
- `TrackStats` struct
- `Identity` struct with `IdentitySource` enum
- `GroundedDocument` struct with massive `impl` block (~1,500 lines)
- `TextSpatialIndex` (interval tree)
- `DocumentStats`
- HTML rendering: `render_document_html()` (~700 lines), `html_escape()`, `annotate_text_html()`
- Eval rendering: `EvalComparison`, `EvalMatch`, `render_eval_html()` (~400 lines)
- Processing: `ProcessOptions`, `ProcessResult`, `Corpus`

#### Split plan

| New file | Contents to extract | Approx lines |
|----------|-------------------|-------------|
| `core/signal.rs` | `Modality`, `Signal<L>`, `SignalRef`, `Quantifier`, `SignalValidationError`, `impl Signal`, `impl From<&Entity> for Signal` | ~650 |
| `core/track.rs` | `Track`, `TrackRef`, `TrackStats`, `impl Track` | ~350 |
| `core/identity.rs` | `Identity`, `IdentitySource`, `impl Identity` | ~200 |
| `core/html.rs` | `render_document_html()`, `html_escape()`, `annotate_text_html()`, HTML rendering helpers | ~700 |
| `core/eval_render.rs` | `EvalComparison`, `EvalMatch`, `EvalHtmlSpan`, `render_eval_html()`, `render_eval_html_with_title()`, `annotate_text_spans()` | ~800 |
| `core/grounded.rs` | `Location`, `GroundedDocument`, `GroundedDocumentWire`, `TextSpatialIndex`, `IntervalNode`, `DocumentStats`, `ProcessOptions`, `ProcessResult`, `Corpus` | ~3,300 |

#### Public API preservation

`grounded.rs` is imported via `crate::core::grounded::*` (glob re-export from `core/mod.rs`). The new modules are declared as `mod signal;` etc. in `grounded.rs` and re-exported:

```rust
// core/grounded.rs (after split)
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

// ... remaining GroundedDocument, Location, etc.
```

---

### P7c — `anno/src/backends/coref/mention_ranking/algorithm.rs` (4,934 lines → 3 files)

#### Current structure

The file is dominated by one struct `MentionRankingCoref` with a massive `impl` block. Contents:
- Helper functions: `is_type_incompatible()`, `animacy_from_pronoun()`, `animacy_from_entity_type()`
- `MentionRankingCoref` struct (line 92) with configuration fields
- Main `impl MentionRankingCoref` (line 118) — ~1,900 lines of methods including:
  - Feature extraction methods (mention features, pair features)
  - Scoring/ranking methods
  - Cluster management methods
- `impl Default` (line 2063)
- Second `impl MentionRankingCoref` (line 2073) — additional methods
- `spans_overlap()` helper
- `impl CoreferenceResolver` trait (line 2215)

#### Split plan

| New file | Contents to extract | Approx lines |
|----------|-------------------|-------------|
| `mention_ranking/features.rs` | `is_type_incompatible()`, `animacy_from_pronoun()`, `animacy_from_entity_type()`, and feature extraction methods from the main `impl` block (mention feature computation, pair feature computation) | ~800 |
| `mention_ranking/scoring.rs` | Scoring and ranking methods from the main `impl` block (score computation, candidate ranking, cluster scoring) | ~1,200 |
| `mention_ranking/algorithm.rs` | `MentionRankingCoref` struct, `new()`, `Default`, `CoreferenceResolver` trait impl, cluster management, `spans_overlap()`, re-exports | ~2,900 |

#### Method extraction strategy

Methods are extracted by moving them to `impl MentionRankingCoref` blocks in the new module files. The struct stays in `algorithm.rs`. The extracted methods reference `self` and `Self`, so they need the struct to be visible. Pattern:

```rust
// features.rs
use super::MentionRankingCoref;

impl MentionRankingCoref {
    pub(super) fn compute_mention_features(&self, ...) -> ... { ... }
    pub(super) fn compute_pair_features(&self, ...) -> ... { ... }
}

pub(super) fn is_type_incompatible(...) -> bool { ... }
pub(super) fn animacy_from_pronoun(...) -> Animacy { ... }
```

This is Rust's orphan-safe `impl` split — methods on a type can be defined in child modules of the type's defining module.

---

## Execution order

1. **P2 (docstrings)** — trivial, do first as warm-up
2. **P6 (lints)** — do before P7 so lint warnings are visible during refactoring
3. **P7a (lib.rs)** — largest file, most impactful split
4. **P7b (grounded.rs)** — second largest
5. **P7c (algorithm.rs)** — most complex split (impl block splitting)

## Testing strategy

For all P7 splits:
- **No new tests needed** — these are pure refactoring moves
- **Existing tests must pass unchanged** — if any test breaks, the split changed public API
- Run `cargo test -p anno-rag-mcp` after P7a, `cargo test -p anno` after P7b and P7c
- Run `cargo check --workspace` after all splits to verify no cross-crate breakage

## Non-goals

- **Changing the public API** — all splits use internal modules + re-exports
- **Reducing line count** — the total lines stay the same, they're just better organized
- **Refactoring the logic** — no behavior changes during split
- **Splitting further** — the target is 3,000-4,000 lines per file max, not 500. Over-splitting creates navigation overhead.

## Risk assessment

| Change | Blast radius | Risk |
|--------|-------------|------|
| P2 docstrings | None | NONE |
| P6 lint propagation | May surface warnings in 4 crates | LOW — mechanical fixes |
| P7a lib.rs split | `anno-rag-mcp` internal | LOW — re-exports preserve API |
| P7b grounded.rs split | `anno::core` internal, many downstream users | MEDIUM — `GroundedDocument` has 155 symbols in the graph |
| P7c algorithm.rs split | `mention_ranking` internal | LOW — 1 struct, contained |
