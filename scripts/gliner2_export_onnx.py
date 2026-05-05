#!/usr/bin/env python3
"""Export a fastino-ai GLiNER2 model to ONNX, optionally merging a LoRA adapter.

This script is the canonical export path for the gliner2_fastino backend
(issue #18). It mirrors lmoe/gliner2-onnx's approach and additionally
supports merging a PEFT/LoRA adapter into the base before export.

Usage:
    # Stock model
    uv run scripts/gliner2_export_onnx.py \\
        --base fastino/gliner2-multi-v1 \\
        --output dist/gliner2-multi-v1

    # LoRA-merged model
    uv run scripts/gliner2_export_onnx.py \\
        --base fastino/gliner2-multi-v1 \\
        --lora-adapter ./my_legal_adapter \\
        --output dist/gliner2-multi-v1-legal

The output directory will contain:
    - model.onnx         (the merged exported model)
    - tokenizer.json     (copied from base)
    - config.json        (copied from base, with `lora_merged: true` if applicable)
"""
from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path


def main() -> int:
    p = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "--base",
        required=True,
        help="HF model id or local path of the base model",
    )
    p.add_argument(
        "--lora-adapter",
        default=None,
        help="path to a PEFT/LoRA adapter directory (optional)",
    )
    p.add_argument(
        "--output",
        type=Path,
        required=True,
        help="output directory (will contain model.onnx etc.)",
    )
    p.add_argument(
        "--opset",
        type=int,
        default=17,
        help="ONNX opset (default: 17)",
    )
    args = p.parse_args()

    args.output.mkdir(parents=True, exist_ok=True)

    try:
        import torch
        from gliner2 import GLiNER2  # type: ignore
    except ImportError as e:
        print(
            f"error: {e}\nInstall: pip install gliner2 torch peft optimum",
            file=sys.stderr,
        )
        return 2

    print(f"loading base model {args.base!r}...")
    model = GLiNER2.from_pretrained(args.base)

    if args.lora_adapter:
        print(f"merging LoRA adapter from {args.lora_adapter!r}...")
        model.load_adapter(args.lora_adapter)
        # PEFT-merge: depending on gliner2's API, this may be on the model
        # or on its underlying nn.Module. Try both.
        if hasattr(model, "merge_and_unload"):
            model = model.merge_and_unload()
        elif hasattr(model.encoder, "merge_and_unload"):
            model.encoder = model.encoder.merge_and_unload()
        else:
            print(
                "warning: gliner2 model does not expose merge_and_unload(); "
                "the adapter is loaded but may not be merged for ONNX export. "
                "Inspect model.named_modules() for a peft.tuners.lora.layer.LoraLayer.",
                file=sys.stderr,
            )

    print(f"exporting to {args.output / 'model.onnx'} (opset={args.opset})...")
    # GLiNER2's export path. If the model exposes a `.export_onnx(path, opset=...)`
    # method, prefer it. Otherwise fall back to torch.onnx.export with a stub.
    if hasattr(model, "export_onnx"):
        model.export_onnx(args.output / "model.onnx", opset=args.opset)
    else:
        # Generic fallback. Caller should override this if their gliner2
        # version differs.
        dummy_text = "The quick brown fox."
        dummy_labels = ["animal", "color"]
        encoded = model.tokenize(dummy_text, dummy_labels)
        # Switch the module to inference mode (disables dropout / batchnorm
        # tracking). Equivalent to the standard nn.Module inference toggle.
        model.train(False)
        torch.onnx.export(
            model,
            (encoded["input_ids"], encoded["attention_mask"]),
            str(args.output / "model.onnx"),
            input_names=["input_ids", "attention_mask"],
            output_names=["scores", "spans"],
            dynamic_axes={
                "input_ids": {0: "batch", 1: "seq"},
                "attention_mask": {0: "batch", 1: "seq"},
                "scores": {0: "batch", 1: "num_spans"},
                "spans": {0: "batch", 1: "num_spans"},
            },
            opset_version=args.opset,
        )

    # Copy tokenizer + config.
    src_dir = Path(model.model_path) if hasattr(model, "model_path") else None
    if src_dir and src_dir.exists():
        for f in ("tokenizer.json", "config.json"):
            src = src_dir / f
            if src.exists():
                shutil.copy(src, args.output / f)

    # Stamp config so anno can detect a merged-LoRA model.
    cfg_path = args.output / "config.json"
    if args.lora_adapter and cfg_path.exists():
        cfg = json.loads(cfg_path.read_text())
        cfg["lora_merged"] = True
        cfg["lora_adapter_source"] = str(args.lora_adapter)
        cfg_path.write_text(json.dumps(cfg, indent=2))

    print(f"done. wrote: {sorted(args.output.iterdir())}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
