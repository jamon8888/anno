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
import os
from pathlib import Path

from gliner2_onnx import export_to_onnx  # gliner2-onnx 0.1.1 public API
from huggingface_hub import HfApi, create_repo


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="fastino/gliner2-privacy-filter-PII-multi")
    parser.add_argument("--out", default="./output", type=Path)
    parser.add_argument("--push-to", default=None,
                        help="HF repo id to push to (e.g. anno-rag/gliner2-privacy-filter-PII-multi-onnx-fp16)")
    args = parser.parse_args()
    # Token is read from HF_TOKEN env var — never pass secrets as CLI args.
    token = os.environ.get("HF_TOKEN") or None

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

    # gliner2-onnx exports into fp16_v2/ with one file per graph base.
    # The runtime downloader (download_models.rs) fetches fp16_v2/{base}_fp16.onnx
    # for each of the 8 NER graph bases — validate at least the encoder graph exists.
    onnx_file = out / "fp16_v2" / "encoder_fp16.onnx"
    if not onnx_file.exists():
        raise FileNotFoundError(
            f"Expected {onnx_file} — check gliner2-onnx output layout "
            "(should produce fp16_v2/{{base}}_fp16.onnx for each graph)"
        )
    size_mb = onnx_file.stat().st_size / 1_000_000
    print(f"[convert] Done — fp16_v2/ layout, encoder ({size_mb:.0f} MB)")

    if args.push_to:
        api = HfApi(token=token)
        create_repo(args.push_to, repo_type="model", exist_ok=True, token=token)
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
