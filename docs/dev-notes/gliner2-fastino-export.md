# gliner2_fastino — ONNX export workflow

Two paths to a usable ONNX model for the `gliner2_fastino` backend.

## Fast path: SemplificaAI pre-export

> **Phase 1 caveat (2026-05-05):** The `SemplificaAI/gliner2-multi-v1-onnx`
> repo is a **5-graph pipeline** (separate `encoder_fp32.onnx`,
> `span_rep_fp32.onnx`, `classifier_fp32.onnx`, `count_pred_fp32.onnx`,
> `count_lstm_fp32.onnx`, plus `scorer_fp32.onnx` in `fp32_v2/`). Phase 1
> of `gliner2_fastino` implements a **single-graph** load path and
> rejects this layout with a typed error pointing at Phase 3. To use the
> SemplificaAI pin end-to-end, wait for Phase 3 (multi-session IOBinding
> chain, port from `SemplificaAI/gliner2-rs/lib_v2.rs`).
>
> Phase 1 is reachable today only with a unified single-graph export —
> the script path below.

Future (Phase 3+):

    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")?;

## Script path: scripts/gliner2_export_onnx.py

Covers all fastino variants and LoRA-merged models.

### Stock fastino model

    uv run scripts/gliner2_export_onnx.py \
        --base fastino/gliner2-multi-v1 \
        --output dist/gliner2-multi-v1

The output directory will contain `model.onnx`, `tokenizer.json`, and
`config.json` — exactly what `GLiNER2Fastino::from_local` expects.

### LoRA-fine-tuned model

If you have a PEFT/LoRA adapter trained on top of a fastino base, merge
it before export:

    uv run scripts/gliner2_export_onnx.py \
        --base fastino/gliner2-multi-v1 \
        --lora-adapter ./my_legal_adapter \
        --output dist/gliner2-multi-v1-legal

The output directory's `config.json` is stamped with
`"lora_merged": true` and the adapter source path. Future versions of
the loader can use this stamp to gate per-domain behavior.

## Loading the export in anno

    let model = GLiNER2Fastino::from_local(Path::new("dist/gliner2-multi-v1"))?;

## Why no runtime adapter loading?

Phase 1 of the `gliner2_fastino` backend supports ONLY merged ONNX
models. Loading a directory containing `adapter_config.json` returns
`Error::LoraAdapterNotSupported` with a pointer to this script.

Runtime hot-swap is tracked as Phase 4 (see
`docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` §5).

For now, generate one merged ONNX per domain and load them via separate
`GLiNER2Fastino` instances. The 450 MB-per-domain cost is a Phase 1
trade-off.

## Verifying the script is in place

    python scripts/gliner2_export_onnx.py --help | head -5

Expected: usage text starting with `usage: gliner2_export_onnx.py`.

## Dependencies

    pip install gliner2 torch peft optimum

The script auto-detects whether `gliner2.GLiNER2` exposes a high-level
`.export_onnx(...)` method; if not, it falls back to a generic
`torch.onnx.export` with hardcoded input/output names. If your gliner2
version uses different conventions, the fallback section is the place
to adjust.

## Related

- Spec: `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md`
- Plan: `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase1.md`
- Issue: [arclabs561/anno#18](https://github.com/arclabs561/anno/issues/18)
- Upstream port source: `SemplificaAI/gliner2-rs` (Apache-2.0)
- Community ONNX tooling: `lmoe/gliner2-onnx`
