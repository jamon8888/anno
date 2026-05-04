# gliner2_fastino — Track A: Phase 1 finalization plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the existing `feat/gliner2-fastino` branch (21 commits, code complete) actually demonstrably-working: resolve the Windows linker blocker, verify the `// VERIFY` comments against a real model, run the test suite end-to-end on Linux/CI, fill in the parity fixture, and open the PR.

**Architecture:** No new code architecture. This is a finalization track — environment fixes, verification work, CI plumbing, and one Python harness.

**Tech Stack:** Rust 2021, GitHub Actions (or equivalent CI), Python 3.10+ (`onnx`, `gliner2`, `peft`, `torch`).

**Spec:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md`
**Phase 1 plan:** `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase1.md`
**Roadmap:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md`

---

## Pre-flight

- [ ] **Confirm starting state.** Branch `feat/gliner2-fastino` exists in worktree `C:/Users/NMarchitecte/anno-gliner2`, 21 commits ahead of `arclabs561/anno:main`. `cargo check --features gliner2-fastino --tests` passes; `cargo test --features onnx` fails on Windows due to MSVC CRT mismatch.
- [ ] **Read the roadmap** so the boundary between this track and Tracks B/C/D/E is clear.
- [ ] **Choose the host that runs Track A.** Linux or macOS strongly preferred (avoids the MSVC blocker entirely). If forced to Windows, F1 is mandatory; otherwise it can be deferred.

---

## Milestone F1 — Windows MSVC linker fix (~half day to 2 days)

Goal: `cargo test --features onnx` links on Windows MSVC.

This is environmental work. It belongs as its own commit (or its own PR) on `main`, NOT on the `feat/gliner2-fastino` branch — the feature branch shouldn't carry Windows infra fixes.

### Task F1.1: Reproduce and bound the failure

**Files:** none yet (reproduction).

- [ ] **Step 1: Reproduce on a clean checkout.** On the Windows host, clone fresh, run `cargo clean && cargo test -p anno --features onnx`. Confirm the failure mode is `LNK1319 779 discordances` with `MD_DynamicRelease` vs `MT_StaticRelease` from `esaxx-rs` and `ort_sys`.
- [ ] **Step 2: Identify the offending crate.** Run `cargo tree -p anno --features onnx -i esaxx-rs` to confirm what pulls it in (likely `tokenizers`). Note: `esaxx-rs` builds C++ code via a build script.
- [ ] **Step 3: Inspect the build script.** `cargo show esaxx-rs --remote` (or look at the published source on crates.io) to find the build.rs that calls `cc::Build`. Note whether it explicitly sets `.static_crt(true)` or relies on defaults.

### Task F1.2: Try fixes in increasing-cost order

- [ ] **Attempt 1: Override CRT via `.cargo/config.toml` rustflags + `CFLAGS_*` env.** Try:

  ```toml
  [target.x86_64-pc-windows-msvc]
  rustflags = ["-C", "target-feature=-crt-static"]
  ```

  Combined with `CFLAGS_x86_64-pc-windows-msvc="-MD"` and `CXXFLAGS_x86_64-pc-windows-msvc="-MD"` env vars. `cc-rs` honors these. Run `cargo clean && cargo test -p anno --features onnx`. Already-attempted variants (failed): plain `-crt-static`, plain `/NODEFAULTLIB:LIBCMT`. The CFLAGS/CXXFLAGS env path is the unattempted one and is the most promising.

- [ ] **Attempt 2: Pin a different `ort` revision.** Check if `ort` rc.13 or later changes the prebuilt CRT. If so, bump the workspace pin. Run the test matrix.

- [ ] **Attempt 3: Use `ort` with `load-dynamic` feature.** Removes the static linkage to onnxruntime — the prebuilt is loaded at runtime via LoadLibrary. Sidesteps the entire CRT-mismatch class. Cost: requires onnxruntime.dll on the user's PATH at runtime; semver impact on existing onnx users.

- [ ] **Attempt 4: File an upstream issue.** If none of the above land, file an issue against `esaxx-rs` requesting a build-script flag for dynamic CRT, and document the Windows-only test skip in `BACKENDS.md` until upstream resolves.

### Task F1.3: Land the fix

**Files:** depending on which attempt succeeded:
- `.cargo/config.toml` (committed at workspace root) — if Attempt 1 worked
- `Cargo.toml` (workspace `[workspace.dependencies]` change) — if Attempt 2 or 3
- `docs/dev-notes/windows-msvc-build-notes.md` — capture the fix and why

- [ ] **Step 1: Commit the fix.** Branch off `main`, NOT off `feat/gliner2-fastino`. Branch name: `fix/msvc-ort-linker-conflict`.

  ```bash
  git checkout main
  git checkout -b fix/msvc-ort-linker-conflict
  # apply the fix
  git add <files>
  git commit -m "fix(build): resolve MSVC LIBCMT/MSVCP140 conflict for ort tests"
  ```

  Commit body should reference the linker error verbatim so future archaeologists find it via `git log --grep`.

- [ ] **Step 2: Verify on Windows.**

  ```bash
  cargo clean
  cargo test -p anno --features onnx --lib -- --test-threads=1
  ```

  Real test count should run (not 0-filtered).

- [ ] **Step 3: PR and merge before resuming Track A.** Once `main` carries the fix, rebase `feat/gliner2-fastino` onto the updated main.

---

## Milestone F2 — Verify `// VERIFY` comments (~2 hours)

Goal: replace the speculative tensor names + sigmoid handling in `extract_ner` with verified-against-real-model values.

### Task F2.1: Introspect a real fastino ONNX export

- [ ] **Step 1: Obtain a real fastino ONNX export.** Either:
  - Use the SemplificaAI pin: `python -c "from huggingface_hub import snapshot_download; print(snapshot_download('SemplificaAI/gliner2-multi-v1-onnx'))"` and find `model.onnx` in the printed path.
  - Or self-export via `uv run scripts/gliner2_export_onnx.py --base fastino/gliner2-multi-v1 --output dist/test`.

- [ ] **Step 2: Run the introspection script.**

  ```bash
  python - <<'PY'
  import onnx
  m = onnx.load("/path/to/model.onnx")
  print("INPUTS:")
  for i in m.graph.input:
      print(" ", i.name, [d.dim_value or d.dim_param for d in i.type.tensor_type.shape.dim])
  print("OUTPUTS:")
  for o in m.graph.output:
      print(" ", o.name, [d.dim_value or d.dim_param for d in o.type.tensor_type.shape.dim])
  PY
  ```

- [ ] **Step 3: Record the actual names + shapes** as a reference dataset. Save to `testdata/gliner2_fastino/onnx_io_layout.txt` (or as a doc comment in `mod.rs`).

### Task F2.2: Update `extract_ner` to match

**Files:** `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Update tensor names** in the `ort::inputs![...]` macro and the `outputs.get(...)` calls. Replace the `// VERIFY` comments with `// Verified against fastino/gliner2-multi-v1 (commit hash, date)`.

- [ ] **Step 2: Update the sigmoid handling.** If the fastino export already applies sigmoid in-graph, leave the score read as-is. If logits, wrap with `1.0 / (1.0 + (-score).exp())`. Determine empirically: run a known fixture text, observe the score range; logits produce values outside [0, 1].

- [ ] **Step 3: Update the shape assertions.** The plan currently asserts `score_shape.len() == 3` and `span_shape[2] == 2`. If the actual export shapes differ (e.g., `[num_spans, num_labels]` without the leading batch dim), adjust both the assertion and the indexing math.

- [ ] **Step 4: Run the integration test.**

  ```bash
  cargo test -p anno --features gliner2-fastino \
      --test gliner2_fastino_integration -- --ignored fastino_multi_v1_extracts_org_and_loc
  ```

  Expected: PASS. Fix any panics by reading the assertion failure and adjusting names/shapes/sigmoid.

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "fix(gliner2_fastino): verified ONNX I/O names + sigmoid handling against fastino/gliner2-multi-v1"
  ```

---

## Milestone F3 — CI workflow for Linux test execution (~half day)

Goal: every push to `feat/gliner2-fastino` (and the eventual PR) runs `cargo test --features gliner2-fastino` on Linux. Catches everything Windows test-blind mode missed.

### Task F3.1: GitHub Actions workflow file

**Files:** `.github/workflows/gliner2-fastino-test.yml` (new)

- [ ] **Step 1: Read existing workflows.** `ls .github/workflows/` and inspect the existing CI file (likely `ci.yml` or similar) to absorb the cargo install + cache pattern anno uses.

- [ ] **Step 2: Add a new job (or extend the existing matrix)** that runs:

  ```yaml
  - name: Test gliner2_fastino (unit only)
    run: cargo test -p anno --features gliner2-fastino --lib
  - name: Test gliner2_fastino (integration, ignored disabled)
    run: cargo test -p anno --features gliner2-fastino --tests
  ```

  Note: integration tests are `#[ignore]`-gated so they won't run by default; that's intentional. They'll only run on a manual dispatch (see F3.2).

- [ ] **Step 3: Trigger on PR + push to feature branches.**

  ```yaml
  on:
    pull_request:
      paths:
        - 'crates/anno/src/backends/gliner2_fastino/**'
        - 'crates/anno/Cargo.toml'
    push:
      branches:
        - 'feat/gliner2-fastino*'
  ```

- [ ] **Step 4: Run the workflow** by pushing to a test branch (or using `act` locally if available). Confirm green.

### Task F3.2: Manual-dispatch nightly job for `--ignored` tests

**Files:** `.github/workflows/gliner2-fastino-nightly.yml` (new)

- [ ] **Step 1: Add a workflow_dispatch + cron.**

  ```yaml
  on:
    workflow_dispatch:
    schedule:
      - cron: '0 6 * * *'  # 06:00 UTC daily
  ```

- [ ] **Step 2: HF cache priming step.** Cache `~/.cache/huggingface` keyed on the model id, so the integration tests don't re-download on every run.

- [ ] **Step 3: Run the integration tests.**

  ```yaml
  - run: cargo test -p anno --features gliner2-fastino --test gliner2_fastino_integration -- --ignored
  ```

- [ ] **Step 4: Commit both workflows.**

  ```bash
  git add .github/workflows/gliner2-fastino-*.yml
  git commit -m "ci(gliner2_fastino): unit job per push + nightly integration job"
  ```

---

## Milestone F4 — Python parity fixture (~1 day)

Goal: replace the smoke parity test (currently asserts only "non-empty when reference is non-empty") with a real `max_abs_diff < 5e-3` comparison.

### Task F4.1: Generate the fixture

**Files:** `scripts/gliner2_generate_parity_fixture.py` (new), `testdata/gliner2_fastino/parity/scores_multi_v1.json` (generated)

- [ ] **Step 1: Read the existing T20 stub** in the Phase 1 plan (M10) for the harness skeleton.

- [ ] **Step 2: Discover the actual gliner2 Python API** for getting raw scores. The PyPI `gliner2` package exposes `m.extract_entities(...)`. Determine if it has a `return_scores=True` kwarg or if you need to hook the underlying `model.forward(...)` call. Possible introspection:

  ```bash
  uv run python -c "import gliner2; help(gliner2.GLiNER2.extract_entities)"
  ```

- [ ] **Step 3: Write the harness.** Based on the API discovered:

  ```python
  # See Phase 1 plan T20 for the skeleton; key adjustment is using
  # whatever returns the raw span+score list, not the post-thresholded
  # entity list.
  ```

- [ ] **Step 4: Run it once and check in the JSON.**

  ```bash
  uv run scripts/gliner2_generate_parity_fixture.py \
      --model fastino/gliner2-multi-v1 \
      --output testdata/gliner2_fastino/parity/scores_multi_v1.json
  ```

  Inspect the file. Should be a JSON dict with `text`, `labels`, and a list of `{start_word, end_word, label_idx, score}` entries.

### Task F4.2: Tighten the Rust parity test

**Files:** `crates/anno/tests/gliner2_fastino_integration.rs`

- [ ] **Step 1: Expose raw span scores from anno.** Currently `extract_with_types` thresholds and produces `Vec<Entity>`. The parity test needs the pre-threshold `Vec<decoder::Span>` so it can compare to the Python fixture entry-by-entry.

  Add a `pub(crate) fn extract_raw_spans(&self, text: &str, types: &[&str]) -> Result<Vec<Span>>` to `GLiNER2Fastino` that returns the decoder's raw output. Mark `#[doc(hidden)]` so it's not part of the stable API.

- [ ] **Step 2: Replace the smoke test body** in `parity_against_python_reference_multi_v1`:

  ```rust
  // Pseudocode — fill in after F4.1's JSON shape is known.
  let rust_spans = model.extract_raw_spans(&fixture.text, &labels)?;
  for ref_span in &fixture.scores {
      let rust = rust_spans.iter().find(|s|
          s.start_word == ref_span.start_word
          && s.end_word == ref_span.end_word
          && s.label_idx == ref_span.label_idx
      ).expect("missing matching rust span");
      assert!(
          (rust.score - ref_span.score).abs() < 5e-3,
          "score mismatch at ({},{},{}): rust={}, py={}",
          ref_span.start_word, ref_span.end_word, ref_span.label_idx,
          rust.score, ref_span.score
      );
  }
  ```

- [ ] **Step 3: Run.**

  ```bash
  cargo test -p anno --features gliner2-fastino \
      --test gliner2_fastino_integration -- --ignored parity_against_python_reference_multi_v1
  ```

  Expected: PASS. If diff is consistently > 5e-3 across most spans, root-cause is likely either the sigmoid handling (F2) or a different prompt assembly (F2 inputs).

- [ ] **Step 4: Commit fixture + tightened test.**

  ```bash
  git add scripts/gliner2_generate_parity_fixture.py \
          testdata/gliner2_fastino/parity/scores_multi_v1.json \
          crates/anno/tests/gliner2_fastino_integration.rs \
          crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "test(gliner2_fastino): tightened parity test with python reference fixture"
  ```

---

## Milestone F5 — Open the PR (~30 min)

### Task F5.1: Branch hygiene

- [ ] **Step 1: Rebase onto current `main`.**

  ```bash
  cd C:/Users/NMarchitecte/anno-gliner2
  git fetch origin
  git rebase origin/main
  ```

  Resolve any conflicts. The rebase pulls in F1's linker fix if it landed first.

- [ ] **Step 2: Squash review.** Look at `git log main..HEAD` and decide whether to squash any of the 21+ commits. The commit history is intentionally fine-grained (one commit per task) for review readability. Default: don't squash.

### Task F5.2: Fork + push (cross-repo PR)

**Only run this when the user (`jamon8888`) has explicitly approved publishing.** This step creates a public artifact.

- [ ] **Step 1: Fork the repo.**

  ```bash
  gh repo fork arclabs561/anno --clone=false --remote=false
  ```

- [ ] **Step 2: Add the fork as a remote and push.**

  ```bash
  git remote add fork https://github.com/jamon8888/anno.git
  git push -u fork feat/gliner2-fastino
  ```

- [ ] **Step 3: Open the cross-repo PR.**

  ```bash
  gh pr create \
      --repo arclabs561/anno \
      --head jamon8888:feat/gliner2-fastino \
      --base main \
      --title "feat(gliner2_fastino): Phase 1 — NER + classification (issue #18)" \
      --body-file <(cat docs/superpowers/plans/2026-05-04-gliner2-fastino-phase1.md | head -50)
  ```

  PR body should reference both the spec and the plan, and explicitly call out:
  - Phase 1 only — Phase 2/3/4 deferred per spec §5
  - LoRA hot-swap NOT implemented (Phase 4 if demand surfaces)
  - `classify` is NER-head approximation (Phase 1.5 follow-up)
  - WIP status, no SLA

---

## Acceptance for Track A

- [ ] `cargo test --features gliner2-fastino --lib` passes on at least one host.
- [ ] `cargo test --features gliner2-fastino --test gliner2_fastino_integration -- --ignored` passes against `fastino/gliner2-multi-v1` on a host with the model in HF cache.
- [ ] `// VERIFY` comments removed from `extract_ner`; replaced with `// Verified against ...` comments referencing a known model + date.
- [ ] CI workflow runs on every push to `feat/gliner2-fastino*` and passes.
- [ ] Parity test asserts `max_abs_diff < 5e-3` and passes.
- [ ] PR opened and CI green.

When all six checkboxes are ticked, Phase 1 is genuinely shippable and Track A is closed.
