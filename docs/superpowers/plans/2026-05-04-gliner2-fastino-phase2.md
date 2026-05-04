# gliner2_fastino — Track C: Phase 2 (structure extraction) plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `extract_structure(text, schema) -> serde_json::Value` to the `gliner2_fastino` backend. Implements the count-predictor MLP (20-class) from `[P]` embedding plus occurrence ID embeddings + per-attribute span scoring, per Zaratiana et al. 2025 (arXiv:2507.18546).

**Architecture:** Builds on Phase 1. Restores the `Relations` and `Classifications` arms of `SchemaTask` that Phase 1 deferred. Adds `decoder::decode_structure` that consumes a different ONNX output set (count tensor + occurrence IDs + attribute scores) and assembles JSON. Public API: a method on `GLiNER2Fastino` (no new trait — same posture as Phase 1's `classify`).

**Tech Stack:** Same as Phase 1 — Rust 2021, `ort` rc.12, `tokenizers`, `serde_json`. No new external deps.

**Spec:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` §5 Phase 2.
**Phase 1 plan:** `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase1.md`
**Roadmap:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md`

---

## Pre-flight

- [ ] **Track A is complete.** `// VERIFY` comments resolved, parity test passing, CI green. Don't start Phase 2 on speculative Phase 1 code.
- [ ] **Read the GLiNER2 paper** (arXiv:2507.18546), specifically §3.2 ("Structured Data Extraction") and §3.3 ("Schema-Driven Output Assembly"). The count-predictor and occurrence ID embeddings are documented there with the exact tensor shapes.
- [ ] **Re-read `gliner2_multitask::schema`** at `crates/anno/src/backends/gliner_multitask/schema.rs`. Phase 2's API surface should align with the existing `TaskSchema` / `ExtractedStructure` / `StructureValue` types where possible — anno already has these for the GLiNER v1 multitask backend.
- [ ] **Inspect the fastino ONNX export's structure-extraction outputs.** Phase 1 only used `scores` and `spans`. Phase 2 needs at least three more output tensors:

  ```bash
  python - <<'PY'
  import onnx
  m = onnx.load("/path/to/fastino/gliner2-multi-v1/model.onnx")
  for o in m.graph.output:
      print(o.name, [d.dim_value or d.dim_param for d in o.type.tensor_type.shape.dim])
  PY
  ```

  Document the actual output names and shapes in `docs/dev-notes/gliner2-fastino-onnx-io.md` — Phase 2 will reference this throughout.

  **Note:** the lmoe/gliner2-onnx community export script may NOT export the structure-extraction heads. If the introspection output shows only NER outputs, the export script needs extension first (see Task P1 below). Self-export via `scripts/gliner2_export_onnx.py` is the path to use; the SemplificaAI pre-export pin probably won't have these heads either.

- [ ] **Create a worktree** off the latest `feat/gliner2-fastino` (post-Track A merge): `git worktree add ../anno-gliner2-phase2 -b feat/gliner2-fastino-phase2`.

---

## File structure

| File | Action | Purpose |
|---|---|---|
| `crates/anno/src/backends/gliner2_fastino/processor.rs` | modify | Restore `Relations` + `Classifications` arms; add `Structures` arm |
| `crates/anno/src/backends/gliner2_fastino/schema.rs` | create | `Schema`, `Field`, `FieldType` types — fastino-specific |
| `crates/anno/src/backends/gliner2_fastino/decoder.rs` | modify | Add `decode_structure` from count + occurrence + score tensors |
| `crates/anno/src/backends/gliner2_fastino/mod.rs` | modify | Add `extract_structure` method; wire ONNX outputs |
| `scripts/gliner2_export_onnx.py` | possibly modify | Ensure structure-extraction output tensors are in the export graph |
| `crates/anno/tests/gliner2_fastino_integration.rs` | modify | Add structure-extraction integration tests |
| `testdata/gliner2_fastino/parity/structure_multi_v1.json` | create | Python-reference parity fixture for structure |
| `docs/dev-notes/gliner2-fastino-onnx-io.md` | create | Document the full ONNX I/O layout (NER + structure) |

---

## Milestone P1 — Verify ONNX exports include structure heads (~half day)

Goal: confirm `scripts/gliner2_export_onnx.py` produces a graph with the count-predictor + occurrence-ID outputs. If not, extend the script. This is a prerequisite for everything downstream.

### Task P1.1: Inspect a fresh export

- [ ] **Step 1: Self-export via the Phase 1 script.**

  ```bash
  uv run scripts/gliner2_export_onnx.py \
      --base fastino/gliner2-multi-v1 \
      --output /tmp/test-multi-v1
  ```

- [ ] **Step 2: Introspect the outputs.**

  ```bash
  python - <<'PY'
  import onnx
  m = onnx.load("/tmp/test-multi-v1/model.onnx")
  for o in m.graph.output:
      print(o.name, [d.dim_value or d.dim_param for d in o.type.tensor_type.shape.dim])
  PY
  ```

- [ ] **Step 3: Save the result.** Create `docs/dev-notes/gliner2-fastino-onnx-io.md` with the captured I/O layout. This becomes the source of truth that subsequent tasks reference for tensor names + shapes.

### Task P1.2: Extend the export script if heads are missing

**Files:** `scripts/gliner2_export_onnx.py`

- [ ] **Step 1:** If the introspection shows only `scores` + `spans` (no count-predictor output), extend the export.

  The fallback `torch.onnx.export(...)` path in `scripts/gliner2_export_onnx.py` uses a static `output_names=["scores", "spans"]`. Extend to include the structure outputs:

  ```python
  output_names=[
      "scores",          # NER span scores [B, num_spans, num_labels]
      "spans",           # NER span coords [B, num_spans, 2]
      "count_logits",    # count-predictor [B, num_tasks, 20] (0-19 instances)
      "occ_embeddings",  # occurrence ID embeddings [B, num_tasks, max_count, hidden]
      "attr_scores",     # attribute span scores [B, num_attrs, num_spans]
  ]
  ```

  (The exact names and shapes depend on the `gliner2` Python package's forward signature. Cross-check with the Python package's source.)

- [ ] **Step 2: Test the extended script.** Re-run P1.1 step 1; confirm all five (or whatever the actual count is) outputs appear in the graph.

- [ ] **Step 3: Commit.**

  ```bash
  git add scripts/gliner2_export_onnx.py docs/dev-notes/gliner2-fastino-onnx-io.md
  git commit -m "feat(scripts): export structure-extraction heads in addition to NER"
  ```

---

## Milestone P2 — Schema types (~half day)

Goal: define the public types that describe an extraction schema. Mirror the existing `gliner_multitask::schema` shape where possible so users moving between backends have a familiar API.

### Task P2.1: Schema definitions

**Files:** `crates/anno/src/backends/gliner2_fastino/schema.rs` (new)

- [ ] **Step 1: Read existing schema types.** `crates/anno/src/backends/gliner_multitask/schema.rs` — note `TaskSchema`, `StructureTask`, `Field`, `FieldType`, `StructureValue`, `ExtractedStructure`. Goal: reuse these where the semantics match; introduce fastino-specific types only where genuine architectural differences require it.

- [ ] **Step 2: Decision point.** Two paths:

  - **Path A — reuse `gliner_multitask` types directly.** Re-export from `gliner2_fastino`. Cheapest. Risk: if Phase 4 (Candle path) wants schema differences, we're locked.
  - **Path B — define fastino-local types.** Independent shapes; possibly stricter (e.g., max instance count = 19 baked into the type). Pure but more code.

  **Recommended: Path A** — re-export. anno's posture is "shared types where possible." Add a `// TODO(phase 4)` comment if Candle later needs divergence.

- [ ] **Step 3: Re-export.** In `crates/anno/src/backends/gliner2_fastino/schema.rs`:

  ```rust
  //! Re-exports of structure-extraction schema types from
  //! `gliner_multitask::schema`. Phase 2 of gliner2_fastino consumes the
  //! same shape; if Phase 4 (Candle path) needs divergence, fork here.

  pub use crate::backends::gliner_multitask::schema::{
      TaskSchema, StructureTask, Field, FieldType,
      StructureValue, ExtractedStructure,
  };
  ```

- [ ] **Step 4: Register the module** in `mod.rs` (alongside `pub mod errors;` etc.):

  ```rust
  pub mod schema;
  ```

- [ ] **Step 5: Verify and commit.**

  ```bash
  cargo check -p anno --features gliner2-fastino
  git add crates/anno/src/backends/gliner2_fastino/schema.rs crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): re-export schema types from gliner_multitask"
  ```

---

## Milestone P3 — Restore Relations + Classifications + add Structures arm (~2 days)

Goal: `processor::SchemaTask` regains the variants Phase 1 deferred, plus a new `Structures` variant. Prompt assembly handles all four.

### Task P3.1: Read the upstream port for the omitted arms

- [ ] **Step 1: Re-pull the upstream source** (already in `/tmp/gliner2-rs-ref/processor.rs` from Phase 1):

  ```bash
  curl -fsSL https://raw.githubusercontent.com/SemplificaAI/gliner2-rs/main/rust_component/src/processor.rs \
      -o /tmp/gliner2-rs-ref/processor.rs
  ```

- [ ] **Step 2: Identify the variant arms.** In upstream's `transform`:
  - `SchemaTask::Relations(rel_name, fields)` — uses `R_TOKEN` per field
  - `SchemaTask::Classifications(task_name, cls_labels)` — uses `L_TOKEN` per label (note upstream's inline TODO comment about C_TOKEN vs L_TOKEN; preserve it)

- [ ] **Step 3: Identify the structure arm.** Upstream's `gliner2-rs` does NOT have a `Structures` variant. We're adding it for Phase 2. The pattern from the Python reference (`fastino-ai/GLiNER2`):
  - `SchemaTask::Structures(task_name, fields)` — uses `C_TOKEN` per field. Each field is a `(name, FieldType)` pair.
  - The structure arm pushes `[C]` per field instead of `[E]` (entity) or `[L]` (label).

  Cross-verify with Python `gliner2.GLiNER2` source if access is available; otherwise infer from the paper's prompt-format figure.

### Task P3.2: Restore the omitted arms + add Structures

**Files:** `crates/anno/src/backends/gliner2_fastino/processor.rs`

- [ ] **Step 1: Update the enum.**

  ```rust
  #[derive(Debug, Clone)]
  pub enum SchemaTask {
      Entities(Vec<String>),
      Relations(String, Vec<String>),
      Classifications(String, Vec<String>),
      // Phase 2 addition. (task_name, [(field_name, FieldType)])
      Structures(String, Vec<(String, super::schema::FieldType)>),
  }
  ```

- [ ] **Step 2: Restore the Relations + Classifications match arms** in `transform`. Copy from upstream verbatim, applying the same mechanical translation we did in Phase 1 (`anyhow!` → `Error::Tokenizer`, etc.). Reference: upstream lines ~110–195.

- [ ] **Step 3: Add the Structures arm.** Pattern (port from Python reference):

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

  Confirm `C_TOKEN` is correct via the upstream Python `processor.py` (search for "structures" or "STRUCT" in `fastino-ai/GLiNER2`).

- [ ] **Step 4: Add tests for each new arm** in the existing `transformer_tests` module:

  - `relations_arm_assembles_expected_prompt_shape` — assert `[R]` markers count equals fields count
  - `classifications_arm_assembles_expected_prompt_shape` — assert `[L]` markers count equals labels count
  - `structures_arm_assembles_expected_prompt_shape` — assert `[C]` markers count equals fields count
  - `multiple_tasks_separated_by_sep_struct` — test with one Entities + one Structures task; assert `[SEP_STRUCT]` (id 7) appears between them

  Use the same stub fixture (`testdata/gliner2_fastino/stub_tokenizer.json`).

- [ ] **Step 5: Verify and commit.**

  ```bash
  cargo check -p anno --features gliner2-fastino --tests
  cargo test -p anno --features gliner2-fastino backends::gliner2_fastino::processor
  git add crates/anno/src/backends/gliner2_fastino/processor.rs
  git commit -m "feat(gliner2_fastino): port Relations/Classifications + add Structures task arm"
  ```

---

## Milestone P4 — Structure decoder (~2 days)

Goal: convert the count-predictor + occurrence ID + attribute score tensors into a `serde_json::Value`.

### Task P4.1: Decoder design

**Files:** `crates/anno/src/backends/gliner2_fastino/decoder.rs`

- [ ] **Step 1: Sketch the data flow.** Given:
  - `count_logits`: `[B, num_tasks, 20]` — argmax along the last axis gives the predicted instance count (0–19) per structure task.
  - `occ_embeddings`: `[B, num_tasks, max_count, hidden]` — one embedding per (task, predicted_instance).
  - `attr_scores`: `[B, num_attrs, num_spans]` — score for each attribute (across all tasks' fields, indexed) on each span. Higher score = better match for that attribute.

  For each structure task with predicted count N:
    - For each instance in 0..N:
      - For each attribute (field) in the task:
        - Pick the span with highest score for (instance, attribute) — argmax along spans, weighted by `occ_embeddings[i] · span_embedding`.
      - Build an object: `{ field_name: span_text, ... }`.
    - Collect into a list.
  - Result: `{ task_name: [instance_obj, ...], ... }`.

- [ ] **Step 2: Define the decoder function.**

  ```rust
  pub fn decode_structure(
      text: &str,
      word_offsets: &[(usize, usize)],
      record: &super::processor::ProcessedRecord,
      count_logits: ndarray::ArrayView<f32, ndarray::Ix3>,    // [1, num_tasks, 20]
      attr_scores: ndarray::ArrayView<f32, ndarray::Ix3>,     // [1, num_attrs, num_spans]
      span_coords: ndarray::ArrayView<i64, ndarray::Ix3>,     // [1, num_spans, 2]
  ) -> Result<serde_json::Value, super::errors::Error> {
      // ... see step 3
  }
  ```

  (`occ_embeddings` is consumed via `attr_scores` — the upstream graph does the dot product internally and we just read the resulting attribute-vs-span scores. Verify this against the actual ONNX export shape.)

- [ ] **Step 3: Implementation.** Fully spelled out in the actual file when the implementer dispatches; structure:

  1. Read `record.tasks` to get the task list (names + types).
  2. For each task with `task_type == "structures"`:
     - Read `count_logits[0, task_idx, :]`, argmax → instance count N.
     - For each instance `i` in 0..N:
       - Build an object.
       - For each attribute belonging to this task (look up via `record.tasks[task_idx].field_tok_indices`):
         - Find the best span: `argmax_s attr_scores[0, attr_global_idx, s]`.
         - Convert (start_word, end_word) to char offsets via `word_offsets`.
         - Extract the surface form.
         - Insert into the instance object.
     - Push the instance object into a list under `task_name`.
  3. Return `serde_json::Value::Object`.

- [ ] **Step 4: Tests.** Synthetic-input tests as in Phase 1's decoder:

  - `decodes_single_structure_task_with_one_instance`
  - `decodes_multi_instance_structure` (count = 3, two attributes per instance)
  - `decodes_zero_instances` (count = 0 → empty list)
  - `out_of_range_attr_indices_dropped`

- [ ] **Step 5: Verify and commit.**

  ```bash
  cargo check -p anno --features gliner2-fastino --tests
  git add crates/anno/src/backends/gliner2_fastino/decoder.rs
  git commit -m "feat(gliner2_fastino): structure decoder (count + occurrence + attr scores -> JSON)"
  ```

---

## Milestone P5 — `extract_structure` end-to-end (~1.5 days)

Goal: public method on `GLiNER2Fastino` that runs the ONNX session with a structures schema, reads the new outputs, calls `decode_structure`, and returns JSON.

### Task P5.1: Add `extract_structure`

**Files:** `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Read Phase 1's `extract_ner`** in the same file. Use it as the template — same input building, same `with_session` closure pattern, same word-offsets table construction. The differences are:

  - Build a `SchemaTask::Structures(...)` (or a mix with `Entities`) rather than only `Entities`.
  - Read three or four output tensors (`count_logits`, `attr_scores`, `spans`, possibly `occ_embeddings`) instead of two.
  - Call `decoder::decode_structure` instead of `decode_spans`.

- [ ] **Step 2: Method signature.**

  ```rust
  /// Extract structured data per the given schema.
  ///
  /// **Phase 2 / experimental.** Returns a `serde_json::Value` with keys
  /// matching the schema's structure-task names; each value is an array
  /// of instance objects.
  ///
  /// Not behind a public trait — promote when a second backend implements
  /// the same shape.
  pub fn extract_structure(
      &self,
      text: &str,
      schema: &schema::TaskSchema,
  ) -> crate::Result<serde_json::Value> {
      // ...
  }
  ```

- [ ] **Step 3: Implementation outline.**

  1. Build `Vec<SchemaTask>` from `schema` (translate `TaskSchema::structures` to `SchemaTask::Structures` etc.).
  2. Call `self.transformer.transform(text, &tasks)`.
  3. Build `input_ids` / `attention_mask` ndarray inputs (reuse Phase 1's pattern).
  4. Run ONNX inside `with_session`; extract `count_logits`, `attr_scores`, `spans` as owned tensors.
  5. Build `word_offsets` table.
  6. Call `decoder::decode_structure(text, &word_offsets, &record, count_view, attr_view, spans_view)`.
  7. Return.

- [ ] **Step 4: Add a basic compile-time test.** Mirror Phase 1's pattern — a short test that constructs a `TaskSchema`, calls `extract_structure` with the stub setup (will fail at ONNX time without a real model, but the call site compiles).

  Mark the real-model test `#[ignore]` for the integration test file.

- [ ] **Step 5: Verify and commit.**

  ```bash
  cargo check -p anno --features gliner2-fastino --tests
  git add crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): extract_structure end-to-end (schema -> JSON)"
  ```

### Task P5.2: Integration test

**Files:** `crates/anno/tests/gliner2_fastino_integration.rs`

- [ ] **Step 1: Add an `#[ignore]`-gated structure test.**

  ```rust
  #[test]
  #[ignore]
  fn fastino_extract_structure_invoice() {
      use anno::backends::gliner2_fastino::schema::{TaskSchema, StructureTask, FieldType};

      let model = GLiNER2Fastino::from_pretrained("fastino/gliner2-multi-v1")
          .expect("load");
      let schema = TaskSchema::new().with_structure(
          StructureTask::new("invoice")
              .with_field("vendor", FieldType::String)
              .with_field("amount", FieldType::String),
      );

      let text = "Invoice from Acme Corp for $4,250.00 dated January 15, 2026.";
      let result = model.extract_structure(text, &schema).unwrap();

      // Loose assertions: just verify shape and that "Acme Corp" surfaces somewhere.
      let invoices = result.get("invoice").and_then(|v| v.as_array())
          .expect("missing invoice array");
      assert!(!invoices.is_empty(), "expected at least one invoice instance");
      let serialized = serde_json::to_string(&result).unwrap();
      assert!(serialized.contains("Acme"), "expected Acme in {serialized}");
  }
  ```

- [ ] **Step 2: Add a multi-instance test.**

  ```rust
  #[test]
  #[ignore]
  fn fastino_extract_structure_multiple_instances() {
      // Schema with one task; text with two clear instances.
      let model = GLiNER2Fastino::from_pretrained("fastino/gliner2-multi-v1").unwrap();
      let schema = TaskSchema::new().with_structure(
          StructureTask::new("person_record")
              .with_field("name", FieldType::String)
              .with_field("role", FieldType::String),
      );
      let text = "Marie Curie was a physicist. Albert Einstein was also a physicist.";
      let result = model.extract_structure(text, &schema).unwrap();
      let records = result.get("person_record").and_then(|v| v.as_array()).unwrap();
      assert!(records.len() >= 2, "expected at least 2 person_record, got {records:?}");
  }
  ```

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/tests/gliner2_fastino_integration.rs
  git commit -m "test(gliner2_fastino): structure extraction integration tests (ignored)"
  ```

---

## Milestone P6 — Parity test for structure (~1 day)

Goal: stored Python-reference fixture for structure outputs; Rust comparison asserts byte-exact JSON match (or close-to-it for floating-point fields).

### Task P6.1: Generate the structure fixture

**Files:** `scripts/gliner2_generate_structure_parity.py` (new), `testdata/gliner2_fastino/parity/structure_multi_v1.json` (generated)

- [ ] **Step 1: Write the harness.** Mirror Track A's parity harness (`scripts/gliner2_generate_parity_fixture.py`), but call the Python reference's structure-extraction API:

  ```python
  m = GLiNER2.from_pretrained("fastino/gliner2-multi-v1")
  schema = {
      "structures": [{"name": "invoice", "fields": [
          {"name": "vendor", "type": "string"},
          {"name": "amount", "type": "string"},
      ]}],
  }
  result = m.extract_structure(FIXTURE_TEXT, schema)
  json.dump({"text": FIXTURE_TEXT, "schema": schema, "expected": result}, ...)
  ```

- [ ] **Step 2: Run once and check in.**

  ```bash
  uv run scripts/gliner2_generate_structure_parity.py \
      --model fastino/gliner2-multi-v1 \
      --output testdata/gliner2_fastino/parity/structure_multi_v1.json
  ```

### Task P6.2: Rust parity test

**Files:** `crates/anno/tests/gliner2_fastino_integration.rs`

- [ ] **Step 1: Add the parity comparison.**

  ```rust
  #[test]
  #[ignore]
  fn parity_structure_against_python_reference() {
      let fixture: serde_json::Value = serde_json::from_str(
          &std::fs::read_to_string("testdata/gliner2_fastino/parity/structure_multi_v1.json").unwrap()
      ).unwrap();

      let model = GLiNER2Fastino::from_pretrained("fastino/gliner2-multi-v1").unwrap();
      // Translate fixture.schema → TaskSchema (helper function in test module)
      let schema = task_schema_from_json(fixture.get("schema").unwrap());
      let text = fixture["text"].as_str().unwrap();
      let result = model.extract_structure(text, &schema).unwrap();
      let expected = fixture.get("expected").unwrap();

      // Byte-exact JSON comparison. If false-positive failures arise from
      // ordering, switch to a structural deep-equal helper.
      assert_eq!(&result, expected, "structure output diverged from python reference");
  }
  ```

- [ ] **Step 2: Add the `task_schema_from_json` test helper.** Translates the Python-style schema dict into anno's `TaskSchema` builder calls.

- [ ] **Step 3: Run.**

  ```bash
  cargo test -p anno --features gliner2-fastino \
      --test gliner2_fastino_integration -- --ignored parity_structure_against_python_reference
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add scripts/gliner2_generate_structure_parity.py \
          testdata/gliner2_fastino/parity/structure_multi_v1.json \
          crates/anno/tests/gliner2_fastino_integration.rs
  git commit -m "test(gliner2_fastino): structure parity fixture + comparison test"
  ```

---

## Milestone P7 — Documentation + status updates (~half day)

### Task P7.1: Update existing docs

**Files:**
- `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` — update §5 status from "Phase 2: not started" to "Phase 2: shipped".
- `docs/BACKENDS.md` — update the `gliner2_fastino` row's description from "(NER + classification)" to "(NER + classification + structure extraction)".
- `crates/anno/src/backends/gliner2_fastino/mod.rs` — module-level rustdoc adds a `# Structure extraction` section.
- `crates/anno/src/backends/catalog.rs` — update the description string to match.

- [ ] **Step 1: Update each file.** Small text changes, no logic.
- [ ] **Step 2: Commit.**

  ```bash
  git add docs/ crates/anno/src/backends/
  git commit -m "docs(gliner2_fastino): mark phase 2 (structure extraction) shipped"
  ```

### Task P7.2: Issue #18 status check

- [ ] **Step 1: Update issue #18.** Comment on the GitHub issue noting Phase 2 has shipped, link to the PR. Phase 2 acceptance from the original issue:
  - [x] `extract_structure(text, schema)` end-to-end (count predictor + occurrence ID embeddings + per-attribute span scoring → JSON)
  - [x] Tests cover at least one structure schema with multiple instances

- [ ] **Step 2: Re-evaluate whether the issue should close.** Phase 1 + Phase 2 satisfy the original ask. Phase 3 (perf) and Phase 4 (Candle/LoRA) are improvements, not closures of the issue. Decision: close after Phase 2 lands; Phase 3/4 can be tracked as separate issues if/when they're scheduled.

---

## Acceptance for Track C

- [ ] `extract_structure(text, schema)` returns well-formed JSON for a single-instance schema.
- [ ] Multi-instance test passes (returns ≥ 2 instances for a clearly-multi-instance text).
- [ ] Python parity test passes (byte-exact JSON match against the stored fixture, or documented-tolerance match for any floating-point fields).
- [ ] All four `SchemaTask` arms (`Entities`, `Relations`, `Classifications`, `Structures`) compile + their unit tests pass.
- [ ] CI workflow (Track A's F3) is green on the new tests.
- [ ] Documentation reflects shipped state.

When all six are ticked, Phase 2 is complete and `gliner2_fastino` covers the full GLiNER2 paper feature set (modulo the perf and Candle tracks).

---

## Sequencing notes

- **P1 first.** If the export script doesn't produce the structure heads, nothing else can be tested. P1.2 may turn into a half-day of digging through the `gliner2` Python source to get the right output names.
- **P2 + P3 can be parallelized.** P2 is just a re-export module; P3 is the prompt-assembly port. They don't share files.
- **P4 + P5 are sequential.** Decoder must exist before `extract_structure` can call it.
- **P6 depends on a Linux/macOS host with `gliner2` installed.** Same constraint as Track A's F4.

## Risk register

1. **Output tensor names/shapes don't match the paper.** Highest risk. Mitigation: P1's introspection output is the source of truth; if it diverges from this plan, update P4 + P5 accordingly.
2. **`occ_embeddings` is consumed inside the graph (no separate output).** Possible. Mitigation: P4 step 3 covers either case.
3. **C_TOKEN vs L_TOKEN ambiguity in upstream.** The upstream `gliner2-rs` has a TODO comment. Mitigation: cross-check with Python `fastino-ai/GLiNER2` source before P3.
4. **JSON ordering causes false parity failures.** Mitigation: use `BTreeMap` ordering in the decoder, or write a structural-equal helper instead of byte-equal.
5. **Phase 2 reveals a bug in Phase 1's prompt assembly.** Possible if the prompt format changes per task type. Mitigation: P3's tests for `Structures` should catch shape divergence early.
