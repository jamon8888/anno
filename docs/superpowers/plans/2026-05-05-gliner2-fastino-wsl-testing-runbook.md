# gliner2_fastino — WSL end-to-end testing runbook

Picks up after Ubuntu has been relocated to `D:\WSL\Ubuntu` (or wherever).
Resolves the test-blind state from Phase 1 implementation by actually
running `cargo test --features gliner2-fastino` on Linux, validating the
integration, and closing out the `// VERIFY` comments in `extract_ner`.

This is a runbook (concrete shell commands), not a plan document.

**Prerequisites:**
- Ubuntu running on D: (post-relocation)
- The Phase 1 branch `feat/gliner2-fastino` exists in the worktree at
  `C:\Users\NMarchitecte\anno-gliner2`
- ~10 GB free on D: for cargo target + HF cache

---

## Step 0 — Get into Ubuntu

From PowerShell (Windows side):

```powershell
wsl -d Ubuntu
```

Drops you into a bash shell inside Ubuntu. From here, everything is
Linux. The Windows worktree is reachable via `/mnt/c/Users/NMarchitecte/anno-gliner2`,
but **don't build there** — cross-filesystem cargo builds are 5–10×
slower than native ext4. Clone the branch into the WSL filesystem
instead.

---

## Step 1 — Stage the worktree inside Ubuntu

```bash
# Inside WSL Ubuntu:
mkdir -p ~/dev
cd ~/dev

# Clone the local Windows worktree into WSL ext4
git clone /mnt/c/Users/NMarchitecte/anno anno
cd anno

# Check out the Phase 1 branch
git fetch /mnt/c/Users/NMarchitecte/anno-gliner2 feat/gliner2-fastino:feat/gliner2-fastino
git checkout feat/gliner2-fastino
git log --oneline -5
```

You should see the 21 Phase 1 commits ending in `d4b22da5` (or whatever
the latest is).

**Alternative if the above is awkward:** clone fresh from GitHub once
the branch is pushed. The point is just to get the tree onto an ext4
filesystem.

---

## Step 2 — Install the Rust toolchain + ONNX deps

```bash
# Rust (if not already installed)
which cargo || (curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y)
source $HOME/.cargo/env

# Build deps for ort / onnxruntime
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev cmake

# Verify Rust version matches anno's MSRV
rustc --version
```

anno's MSRV should be in `Cargo.toml` or `rust-toolchain.toml`. If
there's a `rust-toolchain.toml`, rustup auto-installs the right
toolchain on first cargo invocation.

---

## Step 3 — Smoke-build with the new feature

```bash
cd ~/dev/anno

# This is the moment of truth — does ort link on Linux?
cargo check -p anno --features gliner2-fastino
```

**Expected:** clean check with only the dead-code warnings we already
see on Windows. **No** `LNK` / linker errors. If anything weird shows
up, post the output and we triage.

---

## Step 4 — Run all gliner2_fastino unit tests

This is the test-blind validation we couldn't do on Windows. Every
unit test in `crates/anno/src/backends/gliner2_fastino/**` should now
actually execute.

```bash
cargo test -p anno --features gliner2-fastino backends::gliner2_fastino -- --nocapture
```

**Expected pass list** (count: ~14 unit tests):
- `errors::tests::lora_error_message_contains_script_path_and_phase4_pointer`
- `processor::tests::resolve_special_tokens_from_stub_fixture`
- `processor::tests::missing_special_token_returns_typed_error`
- `processor::splitter_tests::whitespace_splitter_basic`
- `processor::splitter_tests::whitespace_splitter_offsets_are_byte_offsets`
- `processor::splitter_tests::whitespace_splitter_unicode_offsets`
- `processor::transformer_tests::entities_arm_assembles_expected_prompt_shape`
- `processor::transformer_tests::empty_labels_still_returns_well_formed_record`
- `config::tests::parses_minimal_config`
- `config::tests::parses_all_three_counting_variants`
- `config::tests::missing_counting_layer_is_optional_for_phase1`
- `session::tests::session_load_failure_returns_error_for_missing_file`
- `decoder::tests::decodes_two_spans_with_char_offsets`
- `decoder::tests::decodes_unicode_with_char_offsets`
- `decoder::tests::out_of_range_spans_are_dropped`
- `from_local_tests::from_local_rejects_lora_adapter_dir`
- `from_local_more_tests::from_local_missing_tokenizer_returns_typed_error`
- `tests::catalog_includes_gliner2_fastino_wip` (in `catalog.rs`)
- `tests::check_model_id_is_supported_rejects_fastino_models` (in `gliner_multitask`)
- `tests::check_model_id_is_supported_accepts_supported_models` (in `gliner_multitask`)

**If anything fails:** copy the assertion output verbatim. Most likely
failure modes:
- `transformer_tests` — fixture path resolution. If `tokenizer.json`
  isn't found, the test path is relative to crate root in cargo's view;
  switch to `CARGO_MANIFEST_DIR` resolution.
- `decoder::tests::decodes_unicode_with_char_offsets` — if `Entity`'s
  `start()`/`end()` accessors are private, adapt to whatever's public.

Fix locally, commit, move on.

---

## Step 5 — Resolve the `// VERIFY` comments in `extract_ner`

This requires a real fastino ONNX export.

### 5.1 — Get the model

```bash
# Install Python + huggingface_hub if not present
sudo apt-get install -y python3-pip
pip install --user huggingface_hub onnx

# Download the SemplificaAI pre-export
python3 -c "
from huggingface_hub import snapshot_download
p = snapshot_download('SemplificaAI/gliner2-multi-v1-onnx')
print('Cached at:', p)
"
```

The print line gives you the path to the snapshot. Copy that path —
you'll need it.

### 5.2 — Introspect the ONNX I/O

```bash
SNAPSHOT="<path-from-step-5.1>"

python3 - <<'PY'
import onnx, sys
m = onnx.load(f"{SNAPSHOT}/model.onnx")
print("=== INPUTS ===")
for i in m.graph.input:
    shape = [d.dim_value or d.dim_param for d in i.type.tensor_type.shape.dim]
    print(f"  {i.name}: {shape}")
print("=== OUTPUTS ===")
for o in m.graph.output:
    shape = [d.dim_value or d.dim_param for d in o.type.tensor_type.shape.dim]
    print(f"  {o.name}: {shape}")
PY
```

(`SNAPSHOT` substituted in.)

**Compare to the assumptions in `extract_ner`:**

| Assumption | Actual? |
|---|---|
| Input names: `input_ids`, `attention_mask` | check |
| Output names: `scores`, `spans` | check |
| Score shape: `[batch, num_spans, num_labels]` | check rank=3 |
| Span shape: `[batch, num_spans, 2]` | check rank=3, last dim=2 |

If anything differs, edit
`crates/anno/src/backends/gliner2_fastino/mod.rs` to match. Replace
each `// VERIFY` comment with `// Verified against SemplificaAI/gliner2-multi-v1-onnx (2026-05-05)`.

### 5.3 — Determine sigmoid handling

Run a single inference and observe the score range:

```bash
cargo test -p anno --features gliner2-fastino \
    --test gliner2_fastino_integration -- --ignored fastino_multi_v1_extracts_org_and_loc \
    --nocapture
```

Add a `dbg!(scores_view)` temporarily in `extract_ner` if needed.

- **Range [0, 1]** → already-sigmoided. Leave the score read as-is.
- **Range outside [0, 1] (e.g., -3..3)** → logits. Wrap with
  `1.0 / (1.0 + (-score).exp())` before passing to the decoder.

Remove the `dbg!`, commit the verification.

---

## Step 6 — Tier-2 integration tests

```bash
cargo test -p anno --features gliner2-fastino \
    --test gliner2_fastino_integration -- --ignored
```

The three tests:
1. `fastino_multi_v1_extracts_org_and_loc` — must find "Acme Corp"
   (organization) and "Paris" (location) in the fixture text.
2. `fastino_classify_smoke` — must return a stable shape (2 entries
   for 2 labels). Phase 1's `classify` is a NER-head approximation; we
   don't assert exact scores.
3. `semplifica_external_pin_loads` — sanity check that the docs' fast
   path resolves.

If 1 or 3 fail with assertion errors about missing entities, the
likely cause is sigmoid handling (5.3) or the prompt format. Iterate.

---

## Step 7 — Python parity fixture (T20 from the original plan)

```bash
# Install the Python reference impl
pip install --user gliner2 torch

# Generate the fixture
mkdir -p testdata/gliner2_fastino/parity
python3 scripts/gliner2_generate_parity_fixture.py \
    --model fastino/gliner2-multi-v1 \
    --output testdata/gliner2_fastino/parity/scores_multi_v1.json
```

If `gliner2_generate_parity_fixture.py` doesn't exist yet (Phase 1
deferred T20), copy the skeleton from the Phase 1 plan §M10 and adapt
to whatever API the installed `gliner2` package actually exposes.

Re-run the integration test to exercise the parity comparison:

```bash
cargo test -p anno --features gliner2-fastino \
    --test gliner2_fastino_integration -- --ignored \
    parity_against_python_reference_multi_v1
```

The Phase 1 stub of this test only asserts non-emptiness. If you want
the tighter `max_abs_diff < 5e-3` bound, also implement Track A's F4.2
(expose `extract_raw_spans` from the backend).

---

## Step 8 — Final sweep

```bash
# Full test suite
cargo test -p anno --features gliner2-fastino

# Clippy clean (warnings allowed for Phase-2-reserved fields)
cargo clippy -p anno --features gliner2-fastino --tests

# Doc build
cargo doc -p anno --features gliner2-fastino --no-deps
```

If all three pass, Phase 1 is genuinely shippable. Time to:

1. Push the branch (fork + cross-repo PR per Phase 1 plan M12 / Track A
   F5).
2. Update the issue #18 comment with the test results.
3. Mark the WSL relocation + linker-fix task done.

---

## Common stumbles (cheat sheet)

| Symptom | Cause | Fix |
|---|---|---|
| `error: linker 'cc' not found` | missing build-essential | `sudo apt install build-essential` |
| `cargo` builds but tests fail `not enough memory` | WSL2 default RAM cap | `~/.wslconfig` on Windows side, set `memory=8GB` (or whatever) |
| HF cache fills D: | cache lives under `~/.cache/huggingface` | symlink to `/mnt/d/hf-cache` if needed |
| Tokenizer fixture path in tests | tests run from crate root, not workspace root | already accounts for it; if not, `env!("CARGO_MANIFEST_DIR")` |
| `gliner2-fastino` feature not picking up | feature requires `--features gliner2-fastino` explicitly (not in default) | check the cargo invocation |
| Slow build on `/mnt/c` | crossing Linux↔Windows FS | clone into ext4, not under `/mnt/c/...` |

---

## Time estimate

| Step | Minutes |
|---|---|
| Step 0–1 (stage worktree) | 5 |
| Step 2 (toolchain install) | 5–15 (depends on cache) |
| Step 3 (smoke build) | 3–10 (first build is cold) |
| Step 4 (unit tests) | 1–3 |
| Step 5 (VERIFY resolution) | 15–30 |
| Step 6 (integration tests) | 5–10 (with HF cache prime) |
| Step 7 (parity fixture) | 30–60 |
| Step 8 (final sweep) | 5 |

**Total: ~1.5–2 hours of focused work** to close out Phase 1 from
test-blind state to fully verified.
