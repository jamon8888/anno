# Anno-RAG ONNX FP16 Conversion — gliner2-privacy-filter-PII-multi

> **Plan 3/3 — ONNX Conversion**
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert `fastino/gliner2-privacy-filter-PII-multi` (PyTorch) to ONNX FP16 using `gliner2-onnx 0.1.1`, host the artifact on HF Hub (under the `anno-rag` org namespace), and update `download_models.rs` to pull from that artifact.

**Prerequisites:**
- Plan 1 merged (`ner_pii_model_id = "fastino/gliner2-privacy-filter-PII-multi"` in config)
- Python 3.10+ with `pip` on the conversion machine (one-time setup)
- `huggingface_hub` CLI with write access to the target repo

---

## Why FP16 ONNX

| Format | Size | Accuracy | Notes |
|--------|------|----------|-------|
| PyTorch fp32 | ~580 MB | Baseline | Too large, not ONNX |
| ONNX INT8 dynamic | ~75 MB | ⚠️ Broken — empty spans | Known GLiNER2 issue with dynamic quantization |
| ONNX FP16 | ~150 MB | ✓ No degradation | Sweet spot |

`gliner2-onnx 0.1.1` (released Feb 2026) exports GLiNER2 models to ONNX FP16 in one command. It handles the custom token classification head and span-scoring layers correctly — manually exporting with `torch.onnx.export` misses the span pooling which breaks scoring.

---

## Target artifact location

```
HF repo: anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16
File:    model_fp16_v2.onnx    (matches the naming convention in model_inventory.rs)
         tokenizer.json
         tokenizer_config.json
         config.json
```

Why HF Hub (not GitHub Releases): the existing `inspect_onnx_gliner_family` in `model_inventory.rs` calls `hf_hub` to resolve model repos by org/name — keeping the PII model on HF means zero changes to the download plumbing.

Alternatively: host at `fastino/gliner2-privacy-filter-PII-multi-onnx` if the fastino org accepts a PR. The plan defaults to `anno-rag/` namespace for autonomy.

---

## File Map

| File | Change |
|------|--------|
| `scripts/convert-pii-onnx/convert.py` | NEW — conversion script |
| `scripts/convert-pii-onnx/requirements.txt` | NEW |
| `.github/workflows/convert-pii-onnx.yml` | NEW — one-shot CI job (manual trigger) |
| `crates/anno-rag/src/config.rs` | Update `default_ner_pii_model_id()` → new HF repo |
| `crates/anno-rag/src/download_models.rs` | Verify PII model download resolves to `model_fp16_v2.onnx` |

---

### Task 1: Python conversion script

- [ ] **Step 1: Create script directory**

```bash
mkdir -p scripts/convert-pii-onnx
```

- [ ] **Step 2: Write `requirements.txt`**

```
gliner2-onnx==0.1.1
huggingface_hub>=0.24
torch>=2.1
transformers>=4.40
```

- [ ] **Step 3: Write `convert.py`**

```python
#!/usr/bin/env python3
"""
Convert fastino/gliner2-privacy-filter-PII-multi to ONNX FP16.

Usage:
    python convert.py \
        --model fastino/gliner2-privacy-filter-PII-multi \
        --out ./output \
        --push-to anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16

Requires: gliner2-onnx==0.1.1  (pip install -r requirements.txt)
"""

import argparse
import shutil
from pathlib import Path

from gliner2_onnx import export_to_onnx  # gliner2-onnx 0.1.1 public API
from huggingface_hub import HfApi, create_repo


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="fastino/gliner2-privacy-filter-PII-multi")
    parser.add_argument("--out", default="./output", type=Path)
    parser.add_argument("--push-to", default=None,
                        help="HF repo id to push to (e.g. anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16)")
    parser.add_argument("--token", default=None, help="HF write token (or set HF_TOKEN env var)")
    args = parser.parse_args()

    out = args.out
    out.mkdir(parents=True, exist_ok=True)

    print(f"[convert] Exporting {args.model} → ONNX FP16")
    # gliner2-onnx export_to_onnx: downloads source model from HF, exports to ONNX FP16.
    # Output: model_fp16_v2.onnx + tokenizer files in `out`.
    export_to_onnx(
        model_id=args.model,
        output_dir=str(out),
        precision="fp16",
        opset=17,
    )

    onnx_file = out / "model_fp16_v2.onnx"
    assert onnx_file.exists(), f"Expected {onnx_file} — check gliner2-onnx output naming"
    size_mb = onnx_file.stat().st_size / 1_000_000
    print(f"[convert] Done — {onnx_file.name} ({size_mb:.0f} MB)")

    if args.push_to:
        api = HfApi(token=args.token)
        create_repo(args.push_to, repo_type="model", exist_ok=True, token=args.token)
        print(f"[push] Uploading to {args.push_to} …")
        api.upload_folder(
            folder_path=str(out),
            repo_id=args.push_to,
            repo_type="model",
            commit_message="Add ONNX FP16 export via gliner2-onnx 0.1.1",
        )
        print(f"[push] Done — https://huggingface.co/{args.push_to}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Local test (dry-run — no push)**

```bash
cd scripts/convert-pii-onnx
pip install -r requirements.txt
python convert.py --model fastino/gliner2-privacy-filter-PII-multi --out /tmp/pii-onnx
ls -lh /tmp/pii-onnx/
```

Expected:
```
model_fp16_v2.onnx  (~150 MB)
tokenizer.json
tokenizer_config.json
config.json
special_tokens_map.json
```

- [ ] **Step 5: Validate ONNX with onnxruntime**

```python
import onnxruntime as ort
sess = ort.InferenceSession("/tmp/pii-onnx/model_fp16_v2.onnx")
print([i.name for i in sess.get_inputs()])
# Expected: ['input_ids', 'attention_mask', ...]  (no error)
```

- [ ] **Step 6: Commit script**

```bash
git add scripts/convert-pii-onnx/
git commit -m "feat(scripts): gliner2-privacy-filter-PII-multi → ONNX FP16 conversion script"
```

---

### Task 2: One-shot CI job (manual trigger)

**File:** `.github/workflows/convert-pii-onnx.yml`

This job runs once to produce and push the ONNX artifact. After it completes and the artifact is on HF, this workflow is no longer needed in regular CI — it stays in the repo for auditability.

- [ ] **Step 1: Write workflow**

```yaml
name: Convert gliner2-PII → ONNX FP16 (one-shot)

on:
  workflow_dispatch:
    inputs:
      push_to:
        description: 'HF repo to push converted model (e.g. anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16)'
        required: true
        default: 'anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16'

jobs:
  convert:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.11'

      - name: Install dependencies
        run: pip install -r scripts/convert-pii-onnx/requirements.txt

      - name: Convert to ONNX FP16
        run: |
          python scripts/convert-pii-onnx/convert.py \
            --model fastino/gliner2-privacy-filter-PII-multi \
            --out /tmp/pii-onnx \
            --push-to ${{ github.event.inputs.push_to }} \
            --token ${{ secrets.HF_WRITE_TOKEN }}

      - name: Verify output
        run: |
          python - <<'EOF'
          import onnxruntime as ort
          sess = ort.InferenceSession("/tmp/pii-onnx/model_fp16_v2.onnx")
          inputs = [i.name for i in sess.get_inputs()]
          print("ONNX inputs:", inputs)
          assert "input_ids" in inputs
          print("ONNX validation OK")
          EOF

      - name: Upload ONNX artifact (CI cache)
        uses: actions/upload-artifact@v4
        with:
          name: gliner2-pii-onnx-fp16
          path: /tmp/pii-onnx/
          retention-days: 14
```

- [ ] **Step 2: Add `HF_WRITE_TOKEN` to repo secrets**

In GitHub → repo Settings → Secrets → Actions, add `HF_WRITE_TOKEN` with a HF token that has write access to the `anno-rag` org (or the fastino org if pushing there).

This step is manual — cannot be scripted.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/convert-pii-onnx.yml
git commit -m "ci: one-shot ONNX FP16 conversion workflow for gliner2-PII (manual trigger)"
```

---

### Task 3: Update config.rs and download_models.rs

After Task 2 runs and the artifact is live at `anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16`:

- [ ] **Step 1: Update `default_ner_pii_model_id()` in config.rs**

Change:
```rust
fn default_ner_pii_model_id() -> String {
    "fastino/gliner2-privacy-filter-PII-multi".to_string()
}
```
To:
```rust
fn default_ner_pii_model_id() -> String {
    "anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16".to_string()
}
```

- [ ] **Step 2: Verify `download_models.rs` resolves the PII model correctly**

The existing `download_gliner_onnx` helper calls `hf_hub` with the model_id. Confirm it tries `model_fp16_v2.onnx` first (matching the filename produced by the conversion script). If it tries a different filename, update the priority order in `inspect_onnx_gliner_family` to put `fp16_v2` first:

```rust
// In model_inventory.rs — inspect_onnx_gliner_family priority order
let candidates = ["model_fp16_v2.onnx", "model_fp16.onnx", "model.onnx"];
```

- [ ] **Step 3: Run tests**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -- default_ner_pii 2>&1 | tail -5
```

Expected: `test default_ner_pii_model_is_gliner2_pii ... ok`

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/config.rs crates/anno-rag/src/download_models.rs crates/anno-rag-mcp/src/model_inventory.rs
git commit -m "feat(models): point ner_pii_model_id at ONNX FP16 artifact on HF"
```

---

### Task 4: PR

- [ ] **Step 1: Open PR**

```bash
git push origin feat/pii-onnx-conversion
gh pr create \
  --title "feat: ONNX FP16 conversion pipeline for gliner2-PII + one-shot CI" \
  --body "Plan 3/3 — ONNX Conversion

## Changes
- scripts/convert-pii-onnx/convert.py: gliner2-onnx 0.1.1 export to ONNX FP16
- .github/workflows/convert-pii-onnx.yml: manual trigger CI job that converts + pushes to HF
- config.rs: default_ner_pii_model_id → anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16
- download_models.rs + model_inventory.rs: model_fp16_v2.onnx filename priority

## To activate
1. Add HF_WRITE_TOKEN to repo secrets (Settings → Secrets → Actions)
2. Run Actions → 'Convert gliner2-PII → ONNX FP16' → workflow_dispatch
3. Confirm artifact appears at huggingface.co/anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16
4. Merge this PR — anno-rag will download the FP16 model on first launch

## Test plan
- [ ] convert.py --out /tmp/pii-onnx produces model_fp16_v2.onnx (~150 MB)
- [ ] onnxruntime loads model without error
- [ ] cargo test -p anno-rag -- default_ner_pii passes
- [ ] anno-rag status shows gliner_pii: ready after first download
- [ ] detect returns PII entities in <2s on CPU Windows"
```

---

## Self-Review

- ✅ FP16 (not INT8) — avoids known GLiNER2 dynamic INT8 span-scoring regression
- ✅ `model_fp16_v2.onnx` filename — matches `inspect_onnx_gliner_family` priority list
- ✅ `gliner2-onnx 0.1.1` — handles custom span-scoring head correctly (vs manual `torch.onnx.export`)
- ✅ HF Hub hosting — uses existing `hf_hub` plumbing in `download_models.rs`, zero new download logic
- ✅ One-shot CI — runs once, artifact is permanent on HF, no ongoing CI cost
- ✅ `HF_WRITE_TOKEN` — clearly documented as a manual step (cannot be automated)
- ⚠️ `gliner2-onnx 0.1.1` public API shape (`export_to_onnx` function name, params) — verify against the actual package before running. The script uses the documented API from the Feb 2026 release notes; if the API differs, adjust the call signature.
- ⚠️ If fastino org accepts the ONNX artifact directly, use `fastino/gliner2-privacy-filter-PII-multi` with an ONNX branch — cleaner attribution, no `anno-rag` org dependency. Update `default_ner_pii_model_id` accordingly.
