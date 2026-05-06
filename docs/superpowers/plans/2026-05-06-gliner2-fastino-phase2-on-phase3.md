# gliner2_fastino — Phase 2 (structure extraction, on Phase 3 architecture) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `GLiNER2Fastino::extract_structure(text, schema) -> Vec<ExtractedStructure>` on top of Phase 3's 8-session ONNX pipeline. Reuses the existing scorer output shape `[MAX_COUNT=20, num_words, MAX_WIDTH, num_labels]` — the `MAX_COUNT` axis IS the per-instance dimension. No new ONNX sessions, no Python export script changes, no count-predictor MLP work (Phase 3 already shipped that as `count_pred_argmax`).

**Architecture:** A new `SchemaTask::Structures(name, fields)` variant emits `[P] task_name ( [C] field [C] field ... )` in the prompt (per the GLiNER2 paper §3.3). The existing 8-session pipeline (encoder → token_gather → span_rep → schema_gather → count_pred_argmax → count_lstm_fixed → scorer) runs unchanged. A new `decode_structure` reads the same `ScorerOutput` that `decode_entities` already consumes, but walks the instance axis (`c_idx 0..pred_count`) and picks the best span per (instance, field) pair to assemble `Vec<ExtractedStructure>`. Schema types are re-exported from `gliner_multitask::schema` so users get a uniform API across backends.

**Tech Stack:** Rust 2021, `ort` rc.12, `tokenizers`, `ndarray`, `half`, `serde_json` (already a transitive dep). No new external crates.

**Spec:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` §5 Phase 2.
**Roadmap:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md` Track C.
**Supersedes:** `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase2.md` (written pre-Phase 3 merge; ~80% of its scope was already shipped by Phase 3 commit `96cfe1d7`).
**Phase 3 base:** merged at `96cfe1d7` on `main`. Phase 1.5 polish merged at `674119d1`. This plan stacks on `main` post-1.5.

---

## Why this plan replaces the previous Phase 2 plan

The previous plan was written 2026-05-04 morning, before Phase 3 merged. It assumed:
- ONNX export had a single graph with two outputs (`scores`, `spans`) — Phase 3 ships **8 sessions, the scorer's output is `[MAX_COUNT, num_words, MAX_WIDTH, M]`**.
- `Classifications` arm was deferred — Phase 3 **already implemented it** (lines 213-238 of `processor.rs`).
- Need to extend `scripts/gliner2_export_onnx.py` to add a count-predictor head — Phase 3 **already exports `count_pred_argmax` and `count_lstm_fixed`** as separate ONNX graphs and uses them in `extract_ner`.
- A separate `count_logits` ONNX output is needed — **wrong**: the per-instance dimension is baked into the existing scorer output's first axis.

What's actually left is: add a `Structures` task variant, write a structure-aware decoder, expose an `extract_structure` method, parity-test it. ~3 days, not 2 weeks.

---

## Pre-flight

- [ ] **Phase 1.5 merged into `main`.** Verify with `git log --oneline -3` — expect `674119d1 chore(gliner2_fastino): silence dead-code warnings on Phase-2-reserved fields`.
- [ ] **WSL Ubuntu-C is healthy.** Same setup that drove Phases 3 and 1.5. Build cache lives at `~/cargo-target-anno-phase15` (Phase 1.5 worktree had a fresh one); a fresh Phase 2 cache will go to `~/cargo-target-anno-phase2`. Cold first-build will be ~5 min.
- [ ] **`SemplificaAI/gliner2-multi-v1-onnx` is cached** (~6 GB). Phase 3 already verified this snapshot; we reuse it for Phase 2 integration tests. The 8-session set already supports structure extraction — the scorer's `MAX_COUNT` axis is what we'll walk.
- [ ] **`.cargo/config.toml` shipped in the worktree.** Already done at worktree creation — file at `.cargo/config.toml` contains `[build] rustflags = ["--cap-lints", "allow"]` (rustc 1.95 ICE workaround). Verify:
  ```bash
  cat /c/Users/NMarchitecte/anno-gliner2-phase2/.cargo/config.toml
  ```
- [ ] **The worktree exists at** `C:/Users/NMarchitecte/anno-gliner2-phase2` on branch `feat/gliner2-fastino-phase2` off `main` (`674119d1`). Already created.
- [ ] **Skim Phase 1.5's `extract_with_label_descriptions` body** in `crates/anno/src/backends/gliner2_fastino/mod.rs` (~line 332). It's the closest existing template for `extract_structure` — same orchestration shape (build SchemaTask → transform → 8-session pipeline → decode), only the `SchemaTask` variant and decoder differ.
- [ ] **Read** `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` §5 Phase 2 once more for context on the prompt format.

---

## File structure (locked)

| File | Action | Purpose |
|---|---|---|
| `crates/anno/src/backends/gliner2_fastino/processor.rs` | modify | Add `SchemaTask::Structures(String, Vec<(String, FieldType)>)` variant + match arm in `transform` (uses `C_TOKEN` per field, mirrors `Classifications`'s shape) |
| `crates/anno/src/backends/gliner2_fastino/schema.rs` | create | Re-exports `TaskSchema`, `StructureTask`, `StructureField`, `FieldType`, `ExtractedStructure`, `StructureValue` from `gliner_multitask::schema`. Single source of truth for the schema-extraction API across backends |
| `crates/anno/src/backends/gliner2_fastino/pipeline.rs` | modify | Add `decode_structure(text, record, task_map, scorer_out, pred_count, fields) -> ExtractedStructure` reading the existing `ScorerOutput` shape. Walks `c_idx 0..pred_count` as the instance axis |
| `crates/anno/src/backends/gliner2_fastino/mod.rs` | modify | Add `extract_structure(text, &TaskSchema) -> Vec<ExtractedStructure>` public method. Loops the schema's structure tasks, calls the existing 8-session pipeline once per task (same way `extract_ner` and `extract_with_label_thresholds` do), dispatches to `decode_structure` |
| `crates/anno/tests/gliner2_fastino_integration.rs` | modify | Add three `#[ignore]`-gated Tier-2 tests: single-instance, multi-instance, mixed-with-NER |
| `docs/BACKENDS.md` | modify | Update `gliner2_fastino` row description to include "structure extraction" |
| `crates/anno/src/backends/catalog.rs` | modify | Update the description string to match BACKENDS.md |

**Out of scope (deferred):**
- Python parity fixture and harness — original plan's P6. Useful but not blocking. Track separately if a regression appears. Adds a Python dependency that the WSL cargo workflow doesn't have wired up.
- `Relations` arm completion — placeholder variant exists with `// TODO(Phase 2): port Relations arm from upstream`. Deferred until a real workload needs it; the `Structures` variant covers the headline GLiNER2 paper feature.
- `FieldType::List` and `FieldType::Choice` decoding — Phase 2 ships `FieldType::String` only (single best span per field). The plan emits `// TODO(Phase 2.5)` markers where the other two field types would plug in.
- `ExtractionResult` (the combined entities + classifications + structures result type from `gliner_multitask`) — Phase 2 keeps the methods separate (`extract_ner`, `classify`, `extract_structure`) and lets callers compose. A combined `extract` would be a Phase 5 feature.

---

## Milestone P2.M1 — Schema re-export (~half day)

Goal: schema types accessible from the `gliner2_fastino` namespace so Phase 2 code doesn't have to import from `gliner_multitask` (which is a different backend).

### Task M1.1: Create `schema` submodule

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino/schema.rs`
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Create the re-export module.**

  Write `crates/anno/src/backends/gliner2_fastino/schema.rs`:

  ```rust
  //! Re-exports of structure-extraction schema types from
  //! [`crate::backends::gliner_multitask::schema`].
  //!
  //! Phase 2 of `gliner2_fastino` consumes the same shape as the GLiNER v1
  //! multi-task backend; users can move between backends with a single
  //! `use` change. If a future Phase 4 (Candle path) needs different
  //! semantics, fork the types here.

  pub use crate::backends::gliner_multitask::schema::{
      ExtractedStructure, FieldType, StructureField, StructureTask,
      StructureValue, TaskSchema,
  };
  ```

  Note: `gliner_multitask::schema` exposes more types (`EntityTask`, `ClassificationTask`, `ExtractionResult`, etc.) but Phase 2 only needs the structure-extraction subset. Keeping the re-export tight prevents the public surface from growing accidentally.

- [ ] **Step 2: Register the module.**

  Open `crates/anno/src/backends/gliner2_fastino/mod.rs`, find the existing `pub(crate) mod ...;` declarations near the top (around line 41-47), and add:

  ```rust
  pub mod schema;
  ```

  Make this `pub` (not `pub(crate)`) — schema types are part of the user-facing API.

- [ ] **Step 3: Verify the module compiles AND its re-exports are reachable.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo check -p anno --features gliner2-fastino 2>&1 | tail -5'
  ```

  Expected: `Finished` with no errors.

- [ ] **Step 4: Add a smoke test for the re-export path.**

  Append to the existing `mod from_local_tests` block in `crates/anno/src/backends/gliner2_fastino/mod.rs`:

  ```rust
  #[test]
  fn schema_types_reachable_via_gliner2_fastino_path() {
      // Compile-time check: the re-export at gliner2_fastino::schema
      // makes the structure-extraction types reachable.
      use crate::backends::gliner2_fastino::schema::{
          ExtractedStructure, FieldType, StructureTask, TaskSchema,
      };
      let _schema: TaskSchema = TaskSchema::new()
          .with_structure(
              StructureTask::new("invoice")
                  .with_field("vendor", FieldType::String)
                  .with_field("amount", FieldType::String),
          );
      // Assert ExtractedStructure has the expected shape.
      let _es: ExtractedStructure = ExtractedStructure {
          structure_type: "invoice".to_string(),
          fields: std::collections::HashMap::new(),
      };
  }
  ```

- [ ] **Step 5: Run the test.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo test -p anno --features gliner2-fastino backends::gliner2_fastino::from_local_tests::schema_types_reachable 2>&1 | grep -E "test result:|FAILED|^error"'
  ```

  Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 6: Commit.**

  ```bash
  cd C:/Users/NMarchitecte/anno-gliner2-phase2
  git add crates/anno/src/backends/gliner2_fastino/schema.rs \
          crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): re-export schema types from gliner_multitask"
  ```

---

## Milestone P2.M2 — `SchemaTask::Structures` variant (~1 day)

Goal: `processor::SchemaTask` gains a `Structures(name, Vec<(field_name, FieldType)>)` variant, and `transform` learns to assemble the structure prompt `[P] task_name ( [C] field1 [C] field2 ... )`. Three unit tests cover prompt shape.

### Task M2.1: Add the variant and prompt-assembly arm

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/processor.rs`

- [ ] **Step 1: Verify `C_TOKEN` is in the stub fixture.**

  ```bash
  grep '"\[C\]"' /c/Users/NMarchitecte/anno-gliner2-phase2/testdata/gliner2_fastino/stub_tokenizer.json
  ```

  Expected: shows `[C]` in `added_tokens` (id 4) and `vocab` (id 4). It IS already there from Phase 1.

- [ ] **Step 2: Update the `SchemaTask` enum.**

  Open `crates/anno/src/backends/gliner2_fastino/processor.rs`. Find:

  ```rust
  #[derive(Debug, Clone)]
  pub enum SchemaTask {
      Entities(Vec<String>),
      /// Phase 1.5: entities with per-label descriptions for accuracy boost.
      /// Each tuple is (label, description). Emits
      /// `[E] <label> [DESCRIPTION] <description>` per pair in the prompt.
      EntitiesDescribed(Vec<(String, String)>),
      /// Phase 3: classification task. (task_name, labels). Uses [L] tokens.
      Classifications(String, Vec<String>),
      // TODO(Phase 2): port Relations arm from upstream
      Relations(String, Vec<String>),
  }
  ```

  Replace with (insert `Structures` between `Classifications` and `Relations`):

  ```rust
  #[derive(Debug, Clone)]
  pub enum SchemaTask {
      Entities(Vec<String>),
      /// Phase 1.5: entities with per-label descriptions for accuracy boost.
      /// Each tuple is (label, description). Emits
      /// `[E] <label> [DESCRIPTION] <description>` per pair in the prompt.
      EntitiesDescribed(Vec<(String, String)>),
      /// Phase 3: classification task. (task_name, labels). Uses [L] tokens.
      Classifications(String, Vec<String>),
      /// Phase 2: structured-data extraction. (task_name, [(field_name,
      /// field_type)]). Uses [C] tokens per field. The model treats each
      /// field as an attribute that may appear 0..MAX_COUNT times in the
      /// text; the scorer's `MAX_COUNT` axis decodes to per-instance
      /// occurrences (see [`super::pipeline::decode_structure`]).
      Structures(String, Vec<(String, super::schema::FieldType)>),
      // TODO(Phase 2.5): port Relations arm from upstream when a workload
      // requests it. Phase 2 ships Structures only.
      Relations(String, Vec<String>),
  }
  ```

- [ ] **Step 3: Add the prompt-assembly match arm.**

  Find the `match task` block in `pub fn transform(...)` (around line 151). Insert a new arm BEFORE the `Relations(..)` no-op:

  ```rust
                  SchemaTask::Structures(task_name, fields) => {
                      combined_tokens.push("(");
                      let prompt_idx = combined_tokens.len();
                      combined_tokens.push(P_TOKEN);
                      combined_tokens.push(task_name.as_str());
                      combined_tokens.push("(");
                      for (field_name, _ftype) in fields {
                          combined_tokens.push(C_TOKEN);
                          field_indices.push(combined_tokens.len());
                          combined_tokens.push(field_name.as_str());
                          labels.push(field_name.clone());
                      }
                      combined_tokens.push(")");
                      combined_tokens.push(")");

                      task_mappings_temp.push((
                          task_name.clone(),
                          "structures".to_string(),
                          labels,
                          prompt_idx,
                          field_indices,
                      ));
                  }
  ```

  Notes:
  - `_ftype` is intentionally unused in Phase 2 — the prompt format is identical regardless of field type. Field type is consumed in the decoder (M3) when composing the JSON value.
  - `task_type = "structures"` (string) is the dispatch key the decoder uses. Keep this string stable; it appears in `record.tasks[i].task_type`.

- [ ] **Step 4: Verify compile.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo check -p anno --features gliner2-fastino --tests 2>&1 | grep -E "^error|Finished"'
  ```

  Expected: `Finished`.

### Task M2.2: Unit tests for the new arm

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/processor.rs`

- [ ] **Step 1: Add three unit tests.**

  Append to `mod transformer_tests` at the bottom of `processor.rs`:

  ```rust
  #[test]
  fn structures_arm_emits_c_tokens_per_field() {
      let tok = stub();
      let xfm = SchemaTransformer::new(tok).expect("transformer build");

      let fields: Vec<(String, super::FieldType)> = vec![
          ("name".into(), super::FieldType::String),
          ("price".into(), super::FieldType::String),
      ];
      let task = SchemaTask::Structures("product".into(), fields);
      let rec = xfm.transform("Acme Corp Paris .", &[task]).unwrap();
      let ids = &rec.input_ids;

      // [C] = id 4. Two fields → exactly 2 [C] markers.
      let c_count = ids.iter().filter(|&&i| i == 4).count();
      assert_eq!(c_count, 2, "expected 2 [C] markers, got {c_count} in {ids:?}");

      // [P] = id 2 — exactly one (single task).
      let p_count = ids.iter().filter(|&&i| i == 2).count();
      assert_eq!(p_count, 1, "expected 1 [P] marker, got {p_count}");

      // [SEP_TEXT] = id 8 — present.
      assert!(ids.contains(&8), "missing [SEP_TEXT]");

      // No [E] (entity, id 3) and no [L] (label, id 5) — this is a structure prompt.
      assert!(!ids.contains(&3), "unexpected [E] in structures prompt: {ids:?}");
      assert!(!ids.contains(&5), "unexpected [L] in structures prompt: {ids:?}");
  }

  #[test]
  fn structures_task_mapping_records_field_count() {
      let tok = stub();
      let xfm = SchemaTransformer::new(tok).expect("transformer build");
      let fields: Vec<(String, super::FieldType)> = vec![
          ("name".into(), super::FieldType::String),
          ("price".into(), super::FieldType::String),
          ("vendor".into(), super::FieldType::String),
      ];
      let task = SchemaTask::Structures("invoice".into(), fields);
      let rec = xfm.transform("Acme Corp paid 100 dollars to Globex.", &[task]).unwrap();

      // One task mapping, 3 labels, task_type = "structures".
      assert_eq!(rec.tasks.len(), 1);
      let t = &rec.tasks[0];
      assert_eq!(t.task_name, "invoice");
      assert_eq!(t.task_type, "structures");
      assert_eq!(t.labels, vec!["name", "price", "vendor"]);
      assert_eq!(t.field_tok_indices.len(), 3);
  }

  #[test]
  fn mixed_entities_and_structures_separated_by_sep_struct() {
      let tok = stub();
      let xfm = SchemaTransformer::new(tok).expect("transformer build");

      let entities_task = SchemaTask::Entities(vec!["person".into()]);
      let struct_task = SchemaTask::Structures(
          "invoice".into(),
          vec![("vendor".into(), super::FieldType::String)],
      );
      let rec = xfm.transform("Acme Corp paid Globex.", &[entities_task, struct_task]).unwrap();
      let ids = &rec.input_ids;

      // [SEP_STRUCT] = id 7, must appear between the two tasks.
      let sep_struct_pos = ids.iter().position(|&i| i == 7);
      assert!(sep_struct_pos.is_some(), "expected [SEP_STRUCT] between tasks, got {ids:?}");

      // [E] = id 3 from Entities task — exactly one.
      let e_count = ids.iter().filter(|&&i| i == 3).count();
      assert_eq!(e_count, 1, "expected 1 [E] marker (one entity label)");

      // [C] = id 4 from Structures task — exactly one.
      let c_count = ids.iter().filter(|&&i| i == 4).count();
      assert_eq!(c_count, 1, "expected 1 [C] marker (one field)");

      // Two task mappings recorded.
      assert_eq!(rec.tasks.len(), 2);
      assert_eq!(rec.tasks[0].task_type, "entities");
      assert_eq!(rec.tasks[1].task_type, "structures");
  }
  ```

  Note: the tests reference `super::FieldType` because the test module is INSIDE `processor.rs` and the FieldType is at `gliner2_fastino::schema::FieldType`. The path is `super::super::schema::FieldType`. Adjust if the path differs:

  ```rust
  use super::super::schema::FieldType;
  ```

  Actually correct: `super::FieldType` if `FieldType` is in scope through a `use` at the top of `processor.rs`. Check first — if not, prepend each test with `use crate::backends::gliner2_fastino::schema::FieldType;`. Keep the tests self-contained.

- [ ] **Step 2: Run.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo test -p anno --features gliner2-fastino backends::gliner2_fastino::processor::transformer_tests 2>&1 | grep -E "test result:|FAILED|^error|^test "'
  ```

  Expected: 3 new tests pass + 4 pre-existing pass (entities_arm_assembles_expected_prompt_shape, entities_described_arm_emits_desc_tokens, empty_labels_still_returns_well_formed_record, plus any others). Total 6+ green.

- [ ] **Step 3: Commit.**

  ```bash
  cd C:/Users/NMarchitecte/anno-gliner2-phase2
  git add crates/anno/src/backends/gliner2_fastino/processor.rs
  git commit -m "feat(gliner2_fastino): SchemaTask::Structures + [C]-token prompt assembly"
  ```

---

## Milestone P2.M3 — `decode_structure` (~1 day)

Goal: a new `pub(crate) fn decode_structure` in `pipeline.rs` that consumes the existing `ScorerOutput` (shape `[MAX_COUNT, num_words, MAX_WIDTH, num_fields]`), walks `c_idx 0..pred_count` as the instance axis, and assembles `Vec<ExtractedStructure>` for one task. NMS is applied per (instance, field) pair to pick the best span.

### Task M3.1: Implement `decode_structure`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Locate the existing `decode_entities_with_thresholds` body.**

  ```bash
  grep -n "pub(crate) fn decode_entities_with_thresholds\|pub(crate) fn decode_entities" /c/Users/NMarchitecte/anno-gliner2-phase2/crates/anno/src/backends/gliner2_fastino/pipeline.rs
  ```

  Expected: `decode_entities_with_thresholds` around line 437, `decode_entities` around line 497.

  Read the body of `decode_entities_with_thresholds` — it's the closest template. The decode_structure body shares the (c_idx, start, width_idx, m) loop structure, but:
  - **Output is `Vec<ExtractedStructure>` instead of `Vec<Entity>`.**
  - **Per-instance grouping**: collect (best span, field_name) tuples grouped by `c_idx`, then emit one `ExtractedStructure` per non-empty instance.
  - **Best-span-per-(instance, field)**: instead of NMS over all candidates, find argmax over (start, width_idx) for each (c_idx, field).
  - **Threshold filter**: drop candidates below a single global threshold (Phase 2 doesn't ship per-field thresholds; that's a 2.5 polish if requested).
  - **FieldType only matters for value shape**: Phase 2 uses `StructureValue::Single(surface)` for every field regardless of FieldType. `List` and `Choice` are TODO markers.

- [ ] **Step 2: Add the function.**

  Append to `pipeline.rs` after `decode_entities` (around line 540):

  ```rust
  /// Decode the scorer's `[MAX_COUNT, num_words, MAX_WIDTH, num_fields]`
  /// tensor as a structure-extraction result. Walks the `MAX_COUNT` axis
  /// as the instance axis: for each predicted instance `c_idx ∈ 0..pred_count`,
  /// pick the best span for each field and assemble one
  /// [`crate::backends::gliner2_fastino::schema::ExtractedStructure`].
  ///
  /// Phase 2: ships `FieldType::String` only. `List` / `Choice` field types
  /// receive the same single-best-span treatment as `String` — see the
  /// `// TODO(Phase 2.5)` markers below for where they'd specialize.
  ///
  /// Threshold semantics: a (instance, field) candidate is dropped only if
  /// its best score is `<= threshold`. An instance with all fields dropped
  /// becomes an empty `fields` map; the caller decides whether to keep
  /// such instances (this fn keeps them — see `extract_structure` for the
  /// emptiness filter).
  pub(crate) fn decode_structure(
      text: &str,
      record: &ProcessedRecord,
      task: &crate::backends::gliner2_fastino::processor::TaskMapping,
      scorer_out: &ScorerOutput,
      pred_count: usize,
      threshold: f32,
      fields: &[(String, crate::backends::gliner2_fastino::schema::FieldType)],
  ) -> Vec<crate::backends::gliner2_fastino::schema::ExtractedStructure> {
      use crate::backends::gliner2_fastino::schema::{
          ExtractedStructure, StructureValue,
      };
      use std::collections::HashMap;

      let num_words = record.word_to_char_maps.len();
      let num_fields = task.labels.len();
      debug_assert_eq!(
          num_fields, fields.len(),
          "decode_structure: task.labels.len() = {} but fields.len() = {}",
          num_fields, fields.len(),
      );
      let scores = &scorer_out.scores;

      let mut out: Vec<ExtractedStructure> = Vec::with_capacity(pred_count);
      for c_idx in 0..pred_count.min(MAX_COUNT) {
          let mut field_values: HashMap<String, StructureValue> = HashMap::new();
          for (m, (field_name, _ftype)) in fields.iter().enumerate().take(num_fields) {
              // Find the best (start, width_idx) for this (instance, field).
              let mut best: Option<(f32, usize, usize)> = None;
              for start in 0..num_words {
                  for width_idx in 0..MAX_WIDTH {
                      let prob = scores[[c_idx, start, width_idx, m]];
                      if prob <= threshold {
                          continue;
                      }
                      let end_word = (start + width_idx + 1).min(num_words);
                      let (byte_start, _) = record.word_to_char_maps[start];
                      let (_, byte_end) = record.word_to_char_maps[end_word - 1];
                      if byte_end > text.len() || byte_start > byte_end {
                          continue;
                      }
                      let surface = text[byte_start..byte_end].trim();
                      if surface.is_empty() {
                          continue;
                      }
                      match best {
                          Some((b, _, _)) if b >= prob => {}
                          _ => best = Some((prob, start, width_idx)),
                      }
                  }
              }
              if let Some((_prob, start, width_idx)) = best {
                  let end_word = (start + width_idx + 1).min(num_words);
                  let (byte_start, _) = record.word_to_char_maps[start];
                  let (_, byte_end) = record.word_to_char_maps[end_word - 1];
                  let surface = text[byte_start..byte_end].trim().to_string();
                  // Phase 2: every field, regardless of FieldType, becomes
                  // StructureValue::Single. TODO(Phase 2.5): branch on
                  // _ftype here for List (collect top-K) / Choice (snap
                  // surface to nearest choice via edit distance).
                  field_values.insert(field_name.clone(), StructureValue::Single(surface));
              }
          }
          out.push(ExtractedStructure {
              structure_type: task.task_name.clone(),
              fields: field_values,
          });
      }
      out
  }
  ```

  **Why no NMS step**: structure decoding's "winner" per (instance, field) is already the best by construction (we argmax over spans). NMS in `decode_entities_with_thresholds` exists to deduplicate overlapping spans of the SAME label — that's a different problem.

- [ ] **Step 3: Verify compile.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo check -p anno --features gliner2-fastino --tests 2>&1 | grep -E "^error|Finished"'
  ```

  Expected: `Finished`.

### Task M3.2: Synthetic-input unit tests for `decode_structure`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Add tests.**

  Append to the existing `mod tests` block (the one that has `decode_entities_respects_per_label_thresholds`):

  ```rust
  #[test]
  fn decode_structure_single_instance_picks_best_span_per_field() {
      use crate::backends::gliner2_fastino::processor::{ProcessedRecord, TaskMapping};
      use crate::backends::gliner2_fastino::schema::{FieldType, StructureValue};
      use ndarray::Array4;

      // 3 words: "Acme Corp Paris" (indices 0, 1, 2 with byte ranges).
      let record = ProcessedRecord {
          input_ids: vec![],
          attention_mask: vec![],
          tasks: vec![],
          text_start: 0,
          text_end: 0,
          word_to_token_maps: vec![(0, 1), (1, 2), (2, 3)],
          word_to_char_maps: vec![(0, 4), (5, 9), (10, 15)],
      };
      let task = TaskMapping {
          task_name: "company_loc".to_string(),
          task_type: "structures".to_string(),
          labels: vec!["vendor".into(), "city".into()],
          prompt_tok_idx: 0,
          field_tok_indices: vec![0, 0],
      };
      // Scorer: [MAX_COUNT, num_words=3, MAX_WIDTH, num_fields=2].
      // Instance 0:
      //   field 0 (vendor) best at start=0, width=1 ("Acme Corp"): 0.9
      //   field 1 (city)   best at start=2, width=0 ("Paris"):     0.85
      let mut scores = Array4::<f32>::zeros((MAX_COUNT, 3, MAX_WIDTH, 2));
      scores[[0, 0, 1, 0]] = 0.9;
      scores[[0, 2, 0, 1]] = 0.85;
      let scorer_out = ScorerOutput { scores };

      let fields = vec![
          ("vendor".to_string(), FieldType::String),
          ("city".to_string(), FieldType::String),
      ];
      let result = decode_structure(
          "Acme Corp Paris",
          &record,
          &task,
          &scorer_out,
          /* pred_count = */ 1,
          /* threshold  = */ 0.5,
          &fields,
      );

      assert_eq!(result.len(), 1, "expected 1 instance, got {}", result.len());
      let inst = &result[0];
      assert_eq!(inst.structure_type, "company_loc");
      match inst.fields.get("vendor") {
          Some(StructureValue::Single(s)) => assert_eq!(s, "Acme Corp"),
          other => panic!("expected vendor=Single(\"Acme Corp\"), got {other:?}"),
      }
      match inst.fields.get("city") {
          Some(StructureValue::Single(s)) => assert_eq!(s, "Paris"),
          other => panic!("expected city=Single(\"Paris\"), got {other:?}"),
      }
  }

  #[test]
  fn decode_structure_zero_pred_count_returns_empty() {
      use crate::backends::gliner2_fastino::processor::{ProcessedRecord, TaskMapping};
      use crate::backends::gliner2_fastino::schema::FieldType;
      use ndarray::Array4;

      let record = ProcessedRecord {
          input_ids: vec![],
          attention_mask: vec![],
          tasks: vec![],
          text_start: 0,
          text_end: 0,
          word_to_token_maps: vec![(0, 1)],
          word_to_char_maps: vec![(0, 4)],
      };
      let task = TaskMapping {
          task_name: "x".to_string(),
          task_type: "structures".to_string(),
          labels: vec!["a".into()],
          prompt_tok_idx: 0,
          field_tok_indices: vec![0],
      };
      let scorer_out = ScorerOutput {
          scores: Array4::<f32>::zeros((MAX_COUNT, 1, MAX_WIDTH, 1)),
      };
      let fields = vec![("a".to_string(), FieldType::String)];

      let result = decode_structure("Acme", &record, &task, &scorer_out, 0, 0.5, &fields);
      assert!(result.is_empty(), "expected 0 instances when pred_count=0, got {result:?}");
  }

  #[test]
  fn decode_structure_multi_instance_separates_by_c_idx() {
      use crate::backends::gliner2_fastino::processor::{ProcessedRecord, TaskMapping};
      use crate::backends::gliner2_fastino::schema::{FieldType, StructureValue};
      use ndarray::Array4;

      // 3 words: "Marie Albert physicist".
      let record = ProcessedRecord {
          input_ids: vec![],
          attention_mask: vec![],
          tasks: vec![],
          text_start: 0,
          text_end: 0,
          word_to_token_maps: vec![(0, 1), (1, 2), (2, 3)],
          word_to_char_maps: vec![(0, 5), (6, 12), (13, 22)],
      };
      let task = TaskMapping {
          task_name: "person".to_string(),
          task_type: "structures".to_string(),
          labels: vec!["name".into()],
          prompt_tok_idx: 0,
          field_tok_indices: vec![0],
      };
      let mut scores = Array4::<f32>::zeros((MAX_COUNT, 3, MAX_WIDTH, 1));
      scores[[0, 0, 0, 0]] = 0.9; // instance 0, name = "Marie"
      scores[[1, 1, 0, 0]] = 0.8; // instance 1, name = "Albert"
      let scorer_out = ScorerOutput { scores };
      let fields = vec![("name".to_string(), FieldType::String)];

      let result = decode_structure(
          "Marie Albert physicist", &record, &task, &scorer_out, 2, 0.5, &fields,
      );

      assert_eq!(result.len(), 2, "expected 2 instances");
      let names: Vec<&String> = result
          .iter()
          .filter_map(|s| match s.fields.get("name") {
              Some(StructureValue::Single(n)) => Some(n),
              _ => None,
          })
          .collect();
      assert_eq!(names, vec![&"Marie".to_string(), &"Albert".to_string()]);
  }

  #[test]
  fn decode_structure_below_threshold_drops_field() {
      use crate::backends::gliner2_fastino::processor::{ProcessedRecord, TaskMapping};
      use crate::backends::gliner2_fastino::schema::FieldType;
      use ndarray::Array4;

      let record = ProcessedRecord {
          input_ids: vec![],
          attention_mask: vec![],
          tasks: vec![],
          text_start: 0,
          text_end: 0,
          word_to_token_maps: vec![(0, 1)],
          word_to_char_maps: vec![(0, 4)],
      };
      let task = TaskMapping {
          task_name: "t".to_string(),
          task_type: "structures".to_string(),
          labels: vec!["f".into()],
          prompt_tok_idx: 0,
          field_tok_indices: vec![0],
      };
      let mut scores = Array4::<f32>::zeros((MAX_COUNT, 1, MAX_WIDTH, 1));
      scores[[0, 0, 0, 0]] = 0.4; // below threshold 0.5
      let scorer_out = ScorerOutput { scores };
      let fields = vec![("f".to_string(), FieldType::String)];

      let result = decode_structure("Acme", &record, &task, &scorer_out, 1, 0.5, &fields);
      assert_eq!(result.len(), 1, "instance is still emitted (with empty fields)");
      assert!(
          result[0].fields.is_empty(),
          "field below threshold should be dropped, got {:?}", result[0].fields,
      );
  }
  ```

- [ ] **Step 2: Run.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo test -p anno --features gliner2-fastino backends::gliner2_fastino::pipeline::tests 2>&1 | grep -E "test result:|FAILED|^error|^test "'
  ```

  Expected: 4 new `decode_structure_*` tests pass + 3 pre-existing pass = 7+ tests, 0 failed.

- [ ] **Step 3: Commit.**

  ```bash
  cd C:/Users/NMarchitecte/anno-gliner2-phase2
  git add crates/anno/src/backends/gliner2_fastino/pipeline.rs
  git commit -m "feat(gliner2_fastino): decode_structure (instance-axis decoder for structures)"
  ```

---

## Milestone P2.M4 — `extract_structure` end-to-end (~1 day)

Goal: public method on `GLiNER2Fastino` that loops over the schema's `structures`, runs the existing 8-session pipeline once per task, dispatches to `decode_structure`, and returns `Vec<ExtractedStructure>`.

### Task M4.1: Implement `extract_structure`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Locate the existing `extract_with_label_descriptions` method.**

  ```bash
  grep -n "fn extract_with_label_descriptions\|fn classify\b" /c/Users/NMarchitecte/anno-gliner2-phase2/crates/anno/src/backends/gliner2_fastino/mod.rs
  ```

  Expected: `extract_with_label_descriptions` around line 332, `classify` around line 455. The new method goes after `extract_with_label_thresholds` (or anywhere in the same `impl` block).

- [ ] **Step 2: Add the method.**

  Insert into the `impl GLiNER2Fastino` block (the second one, where `extract_with_label_descriptions` lives):

  ```rust
      /// Extract structured data per the given schema.
      ///
      /// Each [`schema::StructureTask`] in `schema.structures` triggers
      /// one ONNX inference pass through the 8-session pipeline (encoder →
      /// token_gather → span_rep → schema_gather → count_pred_argmax →
      /// count_lstm_fixed → scorer). The scorer's `MAX_COUNT` axis is
      /// walked as the instance axis: an `[ExtractedStructure; pred_count]`
      /// is appended to the result for each task.
      ///
      /// **Phase 2 / experimental.** Returns instances even when all
      /// fields drop below threshold (with empty `fields` map). Phase 2.5
      /// may add an opt-in filter for empty instances.
      ///
      /// `FieldType::String` is the only fully-supported field type in
      /// Phase 2. `FieldType::List` and `FieldType::Choice` decode the
      /// same single-best-span treatment as `String` — see
      /// [`pipeline::decode_structure`] for the TODO markers.
      pub fn extract_structure(
          &self,
          text: &str,
          schema: &schema::TaskSchema,
          threshold: f32,
      ) -> crate::Result<Vec<schema::ExtractedStructure>> {
          use pipeline::*;
          if schema.structures.is_empty() {
              return Ok(vec![]);
          }
          let mut all_results: Vec<schema::ExtractedStructure> = Vec::new();
          for st in &schema.structures {
              if st.fields.is_empty() {
                  continue; // skip degenerate task
              }
              let fields_owned: Vec<(String, schema::FieldType)> = st
                  .fields
                  .iter()
                  .map(|f| (f.name.clone(), f.field_type))
                  .collect();
              let task = processor::SchemaTask::Structures(
                  st.name.clone(),
                  fields_owned.clone(),
              );
              let record = self.transformer.transform(text, &[task])?;
              let num_words = record.word_to_char_maps.len();
              if num_words == 0 {
                  continue;
              }
              let task_map = record.tasks.first().ok_or_else(|| {
                  crate::Error::Backend(
                      "gliner2_fastino: transformer produced no task mapping".into(),
                  )
              })?;

              let enc = run_encoder(&self.sessions, &record)?;
              let tg = run_token_gather(&self.sessions, &enc, &record)?;
              let sr = run_span_rep(&self.sessions, &tg, num_words)?;
              let sg = run_schema_gather(&self.sessions, &enc, task_map)?;
              let pred_count = run_count_pred_argmax(&self.sessions, &sg)?;
              if pred_count == 0 {
                  continue;
              }
              let cl = run_count_lstm_fixed(&self.sessions, &sg)?;
              let scorer_out = run_scorer(&self.sessions, &sr, &cl)?;

              let task_results = decode_structure(
                  text,
                  &record,
                  task_map,
                  &scorer_out,
                  pred_count,
                  threshold,
                  &fields_owned,
              );
              all_results.extend(task_results);
          }
          Ok(all_results)
      }
  ```

  Notes on the design choice:
  - **One inference pass per structure task.** We could batch all structures into a single combined prompt (using `[SEP_STRUCT]`-separated tasks), but that complicates decoding because each task has its own field count and `MAX_COUNT` axis. Per-task inference is simpler and matches Phase 1.5's per-task pattern in `extract_with_label_thresholds`. Optimization candidate for Phase 5 if measured to be a bottleneck.
  - **Concatenated `Vec<ExtractedStructure>` return.** The caller can group by `structure_type` if they need a per-task partition. Matches `gliner_multitask`'s `ExtractionResult.structures` shape.
  - **Empty-instance behavior**: instances with no fields above threshold are kept (with empty `fields` map). The decoder's contract documents this; the public method preserves it. Filtering is the caller's choice.

- [ ] **Step 3: Verify compile.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo check -p anno --features gliner2-fastino --tests 2>&1 | grep -E "^error|Finished"'
  ```

  Expected: `Finished`.

- [ ] **Step 4: Commit.**

  ```bash
  cd C:/Users/NMarchitecte/anno-gliner2-phase2
  git add crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): extract_structure (per-task pipeline + decoder dispatch)"
  ```

---

## Milestone P2.M5 — Tier-2 integration tests (~half day)

Goal: three `#[ignore]`-gated integration tests against the real `SemplificaAI/gliner2-multi-v1-onnx` model. Compile-verified now; can be run on any host with the model cached.

### Task M5.1: Integration tests

**Files:**
- Modify: `crates/anno/tests/gliner2_fastino_integration.rs`

- [ ] **Step 1: Add the tests.**

  Append to `crates/anno/tests/gliner2_fastino_integration.rs` (after the existing tests):

  ```rust
  #[test]
  #[ignore]
  fn fastino_extract_structure_invoice_single_instance() {
      // Phase 2: single-instance structure extraction.
      use anno::backends::gliner2_fastino::schema::{
          FieldType, StructureTask, StructureValue, TaskSchema,
      };

      let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("load gliner2-multi-v1");
      let schema = TaskSchema::new().with_structure(
          StructureTask::new("invoice")
              .with_field("vendor", FieldType::String)
              .with_field("amount", FieldType::String),
      );

      let text = "Invoice from Acme Corp for $4,250.00 dated January 15, 2026.";
      let result = model.extract_structure(text, &schema, 0.5).expect("extract");

      eprintln!("invoice extraction: {result:#?}");

      // Loose assertions: at least one instance returned, and "Acme" surfaces
      // as the vendor (or somewhere in the result). We don't pin to exact
      // count because the model's count predictor can return 1 or more
      // depending on tokenization.
      assert!(!result.is_empty(), "expected at least 1 instance, got {result:#?}");
      let serialized = serde_json::to_string(&result).expect("serialize");
      assert!(
          serialized.contains("Acme"),
          "expected 'Acme' somewhere in result, got {serialized}",
      );

      // structure_type is the task name.
      assert_eq!(result[0].structure_type, "invoice");

      // If vendor is present, it should be a Single value.
      if let Some(v) = result[0].fields.get("vendor") {
          assert!(
              matches!(v, StructureValue::Single(_)),
              "vendor should be Single, got {v:?}",
          );
      }
  }

  #[test]
  #[ignore]
  fn fastino_extract_structure_multi_instance_people() {
      // Phase 2: multi-instance structure extraction. Two clear people in
      // the text → expect at least 2 instances.
      use anno::backends::gliner2_fastino::schema::{FieldType, StructureTask, TaskSchema};

      let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("load");
      let schema = TaskSchema::new().with_structure(
          StructureTask::new("person_record")
              .with_field("name", FieldType::String)
              .with_field("role", FieldType::String),
      );

      let text =
          "Marie Curie was a physicist. Albert Einstein was also a physicist.";
      let result = model.extract_structure(text, &schema, 0.3).expect("extract");

      eprintln!("multi-instance: {result:#?}");
      assert!(
          result.len() >= 2,
          "expected at least 2 person_record instances, got {result:#?}",
      );

      // Every result has structure_type == "person_record".
      for r in &result {
          assert_eq!(r.structure_type, "person_record");
      }
  }

  #[test]
  #[ignore]
  fn fastino_extract_structure_empty_schema_returns_empty() {
      // Defensive: empty schema → empty vec, no inference passes.
      use anno::backends::gliner2_fastino::schema::TaskSchema;

      let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("load");
      let schema = TaskSchema::new(); // no structures
      let result = model
          .extract_structure("anything", &schema, 0.5)
          .expect("extract");
      assert!(result.is_empty());
  }
  ```

- [ ] **Step 2: Compile-check (model not required — tests are #[ignore]).**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo check -p anno --features gliner2-fastino --tests 2>&1 | grep -E "^error|Finished"'
  ```

  Expected: `Finished`.

- [ ] **Step 3: (Optional) Run on a host with the model cached.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo test -p anno --features gliner2-fastino --test gliner2_fastino_integration -- --ignored --nocapture fastino_extract_structure'
  ```

  Expected: 3 tests pass. `fastino_extract_structure_invoice_single_instance` and `..._multi_instance_people` exercise the full pipeline; `..._empty_schema_returns_empty` is defensive.

- [ ] **Step 4: Commit.**

  ```bash
  cd C:/Users/NMarchitecte/anno-gliner2-phase2
  git add crates/anno/tests/gliner2_fastino_integration.rs
  git commit -m "test(gliner2_fastino): integration tests for extract_structure (single, multi, empty)"
  ```

---

## Milestone P2.M6 — Docs + catalog (~half day)

Goal: BACKENDS.md and the catalog reflect the new capability.

### Task M6.1: Update BACKENDS.md

**Files:**
- Modify: `docs/BACKENDS.md`

- [ ] **Step 1: Find the `gliner2_fastino` row.**

  ```bash
  grep -n "gliner2_fastino" /c/Users/NMarchitecte/anno-gliner2-phase2/docs/BACKENDS.md
  ```

  Expected: row at line 13 (or thereabouts) with description starting "fastino-ai GLiNER2 (NER + classification...".

- [ ] **Step 2: Update the description.**

  In `docs/BACKENDS.md`, replace the existing `gliner2_fastino` description text:

  Find:
  ```
  fastino-ai GLiNER2 (NER + classification; Zaratiana 2025). Feature `gliner2-fastino`. Phase 3 multi-session pipeline (8 ONNX graphs: encoder/token_gather/span_rep/schema_gather/count_pred_argmax/count_lstm_fixed/scorer/classifier). Runtime LoRA hot-swap NOT implemented. GPU: opt-in via `gliner2-fastino-cuda` / `gliner2-fastino-coreml` features. Issue [#18](https://github.com/arclabs561/anno/issues/18).
  ```

  Replace with:
  ```
  fastino-ai GLiNER2 (NER + classification + structure extraction; Zaratiana 2025). Feature `gliner2-fastino`. Phase 3 multi-session pipeline (8 ONNX graphs: encoder/token_gather/span_rep/schema_gather/count_pred_argmax/count_lstm_fixed/scorer/classifier). Phase 2 structure extraction via `extract_structure(text, &TaskSchema, threshold)` returning `Vec<ExtractedStructure>`. Runtime LoRA hot-swap NOT implemented. GPU: opt-in via `gliner2-fastino-cuda` / `gliner2-fastino-coreml` features. Issue [#18](https://github.com/arclabs561/anno/issues/18).
  ```

### Task M6.2: Update catalog

**Files:**
- Modify: `crates/anno/src/backends/catalog.rs`

- [ ] **Step 1: Find the description.**

  ```bash
  grep -n "fastino-ai GLiNER2 (NER" /c/Users/NMarchitecte/anno-gliner2-phase2/crates/anno/src/backends/catalog.rs
  ```

- [ ] **Step 2: Update.** Replace:

  ```rust
          description: "fastino-ai GLiNER2 (NER + classification, multi-session pipeline) — experimental, issue #18",
  ```

  With:

  ```rust
          description: "fastino-ai GLiNER2 (NER + classification + structure extraction) — experimental, issue #18",
  ```

### Task M6.3: Module-level rustdoc

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Add a Structure extraction section.**

  In `mod.rs`'s top-of-file rustdoc, after the existing `# Architecture deltas` and `# LoRA` sections, add:

  ```rust
  //! # Structure extraction (Phase 2)
  //!
  //! [`GLiNER2Fastino::extract_structure`] returns a `Vec<schema::ExtractedStructure>`
  //! given a [`schema::TaskSchema`]. Each `StructureTask` in the schema runs
  //! one inference pass through the 8-session pipeline; the scorer's
  //! `MAX_COUNT` axis is walked as the per-instance dimension.
  //!
  //! ```rust,no_run
  //! use anno::backends::gliner2_fastino::GLiNER2Fastino;
  //! use anno::backends::gliner2_fastino::schema::{
  //!     FieldType, StructureTask, TaskSchema,
  //! };
  //! use std::path::Path;
  //!
  //! let model = GLiNER2Fastino::from_local(Path::new("./model")).unwrap();
  //! let schema = TaskSchema::new().with_structure(
  //!     StructureTask::new("invoice")
  //!         .with_field("vendor", FieldType::String)
  //!         .with_field("amount", FieldType::String),
  //! );
  //! let result = model
  //!     .extract_structure("Invoice from Acme Corp for $4,250.", &schema, 0.5)
  //!     .unwrap();
  //! for instance in result {
  //!     println!("{}: {:?}", instance.structure_type, instance.fields);
  //! }
  //! ```
  //!
  //! Phase 2 ships [`schema::FieldType::String`] only. `List` and `Choice`
  //! field types decode with the same single-best-span treatment as `String`.
  ```

- [ ] **Step 2: Build docs.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && cargo doc -p anno --features gliner2-fastino --no-deps 2>&1 | grep -E "^error|^warning|Finished" | head -10'
  ```

  Expected: `Finished` with no doc errors.

- [ ] **Step 3: Commit M6.1 + M6.2 + M6.3 together.**

  ```bash
  cd C:/Users/NMarchitecte/anno-gliner2-phase2
  git add docs/BACKENDS.md \
          crates/anno/src/backends/catalog.rs \
          crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "docs(gliner2_fastino): mark Phase 2 structure extraction shipped"
  ```

---

## Milestone P2.M7 — Final sweep + finishing (~half day)

Goal: cargo matrix sweep, then hand off to finishing-a-development-branch skill.

### Task M7.1: Cargo matrix

- [ ] **Step 1: Run all 5 feature configurations + lib unit tests for gliner2_fastino.**

  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase2 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=$HOME/cargo-target-anno-phase2 && \
    echo "=== 1. no-default-features ===" && cargo check -p anno --no-default-features 2>&1 | grep -E "^error|Finished" | tail -3 && \
    echo "=== 2. gliner2-fastino ===" && cargo check -p anno --features gliner2-fastino 2>&1 | grep -E "^error|Finished" | tail -3 && \
    echo "=== 3. gliner2-fastino --tests ===" && cargo check -p anno --features gliner2-fastino --tests 2>&1 | grep -E "^error|Finished" | tail -3 && \
    echo "=== 4. gliner2-fastino-cuda ===" && cargo check -p anno --features gliner2-fastino-cuda 2>&1 | grep -E "^error|Finished" | tail -3 && \
    echo "=== 5. gliner2-fastino-coreml ===" && cargo check -p anno --features gliner2-fastino-coreml 2>&1 | grep -E "^error|Finished" | tail -3 && \
    echo "=== 6. gliner2_fastino unit tests ===" && cargo test -p anno --features gliner2-fastino backends::gliner2_fastino 2>&1 | grep -E "Running unittests|test result:|FAILED|^error"'
  ```

  Expected: all 6 sections show `Finished` and the test section shows `test result: ok. N passed; 0 failed` where N ≥ 30 (Phase 1.5 had 27; Phase 2 adds at least 7 from M2.2 + M3.2 = 34+).

  **If any cargo check fails or any test fails, STOP** — debug before continuing. Phase 2 must not regress any existing tests.

- [ ] **Step 2: (Optional, slow) Full lib test.**

  Phase 1.5 documented that the full lib test (~1599 tests, no filter) hangs on pre-existing slow `annotated::tests::*` and others — unrelated to this work. The filtered run in step 1 covers everything `gliner2_fastino`-related. Skip the full run unless something in this PR could plausibly affect those modules (it shouldn't — Phase 2 only touches the gliner2_fastino subtree).

### Task M7.2: Push to fork

- [ ] **Step 1: Switch to the main worktree, fast-forward merge, push to fork.**

  This step delegates to the **superpowers:finishing-a-development-branch** skill. Invoke it at the end of M7.1, present the four options, and choose option 1 (merge locally) followed by `git push fork main`. Same pattern as Phase 1.5's finish.

  After the user chooses option 1:
  ```bash
  cd C:/Users/NMarchitecte/anno
  git checkout main
  git merge --ff-only feat/gliner2-fastino-phase2
  git push fork main
  git worktree remove --force ../anno-gliner2-phase2
  git branch -d feat/gliner2-fastino-phase2
  ```

---

## Acceptance for Phase 2

- [ ] `extract_structure(text, &TaskSchema, threshold)` exists and returns `Vec<ExtractedStructure>`.
- [ ] `SchemaTask::Structures(name, fields)` variant compiles, prompt assembly emits `[C]` per field, three unit tests for prompt shape pass.
- [ ] `decode_structure` walks the `MAX_COUNT` axis as the instance axis; four synthetic-input unit tests cover single-instance, zero-pred-count, multi-instance, below-threshold paths.
- [ ] All 5 cargo check feature configs (`no-default-features`, `gliner2-fastino`, `gliner2-fastino --tests`, `gliner2-fastino-cuda`, `gliner2-fastino-coreml`) pass clean.
- [ ] Filtered lib test `cargo test -p anno --features gliner2-fastino backends::gliner2_fastino` shows ≥34 tests pass, 0 fail.
- [ ] Three `#[ignore]`-gated integration tests added (single-instance, multi-instance, empty schema) — compile-clean; running them needs the cached SemplificaAI snapshot.
- [ ] BACKENDS.md, catalog.rs, and module rustdoc reflect Phase 2 shipped.

---

## Out of scope (Phase 2.5 / Phase 5 candidates)

- **Python parity fixture** — the original plan's P6. Useful for catching subtle decoder regressions but adds Python toolchain dependency. Track separately.
- **`Relations` arm** — placeholder enum variant remains; arm is unimplemented. Defer until a real workload requests it.
- **`FieldType::List` decoding (top-K spans)** — Phase 2 uses single-best-span for all field types. List would collect `c_idx`-grouped spans above threshold instead of just argmax.
- **`FieldType::Choice` snapping** — extract the surface, then nearest-match against the choice list (edit distance or exact). Requires `choices: Option<Vec<String>>` on `StructureField` (already present in the type).
- **Empty-instance filtering** — current behavior keeps instances with empty `fields` map. An opt-in filter (`extract_structure_drop_empty`) is a one-liner in the public method if a user requests it.
- **Combined `extract` method** — running entities + classifications + structures in a single multi-task prompt. Would require collapsing the per-task inference loop in `extract_structure` into a single combined `transform` call. Phase 5 candidate when measured to be a perf win.
- **Per-field thresholds** — Phase 2 uses one global threshold. Phase 1.5 added per-label thresholds for entities; the equivalent for structures would extend `decode_structure` to take `&[(field_name, threshold)]`. Cheap to add when needed.

---

## Self-review

- [x] **Spec coverage** (`design.md` §5 Phase 2):
  - Spec wants `extract_structure(text, schema)` → JSON-or-equivalent. **Implemented as `Vec<ExtractedStructure>`** (the typed equivalent; `serde_json::to_value(...)` round-trips trivially). M4.
  - Spec wants count predictor + occurrence ID embeddings + per-attribute span scoring. **Phase 3 already shipped count_pred_argmax + count_lstm_fixed + scorer.** M3 walks the resulting tensor.
  - Spec wants Relations + Classifications arms restored. **Classifications already restored in Phase 3; Relations placeholder remains, deferred per "Out of scope".** M2 verifies non-regression.
  - Spec wants multi-instance test. M5.1 includes `fastino_extract_structure_multi_instance_people`.
- [x] **Type consistency**: `ExtractedStructure { structure_type: String, fields: HashMap<String, StructureValue> }` consistent across M3 (synth tests construct it), M4 (extract_structure returns it), M5 (integration tests assert it), M6.3 (rustdoc example uses it). `StructureValue::Single(String)` consistent everywhere.
- [x] **No placeholders**: every code block is complete; the two TODO markers (`Phase 2.5` for List/Choice, `Phase 2.5` for Relations arm) are explicit deferrals with rationale, not "TBD".
- [x] **Method signature consistency**: `extract_structure(&self, text: &str, schema: &TaskSchema, threshold: f32) -> Result<Vec<ExtractedStructure>>` consistent in M4, M5, and the rustdoc example.
- [x] **Test naming**: prefixes match Phase 1.5's convention (`fastino_extract_*` for integration, `decode_structure_*` for unit, `structures_*_assembles*` for processor). Easy to find, easy to filter.

---

## References

- Phase 3 design + plan: `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md`, `docs/superpowers/plans/2026-05-05-gliner2-fastino-phase3.md`
- Roadmap Track C (this track): `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md`
- Previous Phase 2 plan (superseded): `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase2.md`
- Phase 1.5 plan (recently shipped, similar shape): `docs/superpowers/plans/2026-05-05-gliner2-fastino-phase1.5-polish.md`
- GLiNER2 paper: Zaratiana et al. 2025, [arXiv:2507.18546](https://arxiv.org/abs/2507.18546). §3.3 covers the structure-extraction prompt format.
- gliner_multitask schema source-of-truth: `crates/anno/src/backends/gliner_multitask/schema.rs`
