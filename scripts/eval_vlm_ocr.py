"""
eval_vlm_ocr.py — VLM-OCR evaluation harness for French legal fixtures
=======================================================================

Loads synthetic PNG fixtures from crates/anno-rag/tests/fixtures/vlm_ocr_eval/,
calls each VLM candidate via its OpenAI-compatible vLLM API, computes CER
(Character Error Rate) per class, and prints a summary table.

Results are saved as JSON under scripts/eval_vlm_ocr_results/.

## Requirements

    pip install openai jiwer Pillow

## VLM servers (start before running)

    # LightOnOCR-2-1B
    vllm serve lightonai/LightOnOCR-2-1B --port 8000

    # olmOCR-7B
    vllm serve allenai/olmOCR-7B-0225-preview --port 8001

    # PaddleOCR-VL
    vllm serve PaddlePaddle/PaddleOCR-VL-1.6 --port 8002

## Usage

    # Run all models (requires all three vLLM servers running)
    python scripts/eval_vlm_ocr.py

    # Run a single candidate
    python scripts/eval_vlm_ocr.py --model lighton
    python scripts/eval_vlm_ocr.py --model olmocr
    python scripts/eval_vlm_ocr.py --model paddle

    # Override fixture root
    python scripts/eval_vlm_ocr.py --fixtures path/to/vlm_ocr_eval

    # Dry-run: print fixture list without calling APIs
    python scripts/eval_vlm_ocr.py --dry-run
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Optional dependency checks with friendly errors
# ---------------------------------------------------------------------------
try:
    from openai import OpenAI
except ImportError:
    sys.exit("ERROR: 'openai' package not found.  Run: pip install openai")

try:
    import jiwer
except ImportError:
    sys.exit("ERROR: 'jiwer' package not found.  Run: pip install jiwer")

try:
    from PIL import Image  # noqa: F401 — just validate install
except ImportError:
    sys.exit("ERROR: 'Pillow' package not found.  Run: pip install Pillow")

# ---------------------------------------------------------------------------
# Model registry
# ---------------------------------------------------------------------------

MODELS: dict[str, dict[str, str]] = {
    "lighton": {
        "label":   "LightOnOCR-2-1B",
        "model_id": "lightonai/LightOnOCR-2-1B",
        "base_url": "http://127.0.0.1:8000/v1",
    },
    "olmocr": {
        "label":   "olmOCR-7B",
        "model_id": "allenai/olmOCR-7B-0225-preview",
        "base_url": "http://127.0.0.1:8001/v1",
    },
    "paddle": {
        "label":   "PaddleOCR-VL-1.6",
        "model_id": "PaddlePaddle/PaddleOCR-VL-1.6",
        "base_url": "http://127.0.0.1:8002/v1",
    },
}

FIXTURE_CLASSES = ["printed", "handwritten", "tables", "stamps"]

OCR_PROMPT = (
    "You are an OCR engine for French legal documents.  "
    "Transcribe ALL text visible in this image, exactly as it appears, "
    "preserving line breaks.  Do NOT summarise or interpret.  "
    "Output ONLY the transcribed text, nothing else."
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _find_fixture_root(override: str | None) -> Path:
    if override:
        p = Path(override)
        if not p.is_dir():
            sys.exit(f"ERROR: fixture root not found: {p}")
        return p

    # Walk up from this script to find crates/anno-rag/tests/fixtures/vlm_ocr_eval
    here = Path(__file__).resolve().parent
    candidates = [
        here.parent / "crates" / "anno-rag" / "tests" / "fixtures" / "vlm_ocr_eval",
        here / "crates" / "anno-rag" / "tests" / "fixtures" / "vlm_ocr_eval",
    ]
    for c in candidates:
        if c.is_dir():
            return c
    sys.exit(
        "ERROR: Could not locate vlm_ocr_eval fixture directory.\n"
        "Run: python scripts/eval_vlm_ocr.py --fixtures <path>"
    )


def _encode_image(path: Path) -> str:
    """Return base64-encoded PNG data URI for inclusion in a chat message."""
    data = path.read_bytes()
    b64 = base64.b64encode(data).decode("ascii")
    return f"data:image/png;base64,{b64}"


def _load_ref(png_path: Path) -> str:
    """Load .ref.txt sidecar next to a PNG.  Returns empty string if missing."""
    ref_path = png_path.with_suffix(".ref.txt")
    if ref_path.is_file():
        return ref_path.read_text(encoding="utf-8").strip()
    return ""


def _compute_cer(hypothesis: str, reference: str) -> float | None:
    """Character Error Rate via jiwer.  Returns None if reference is empty."""
    if not reference.strip():
        return None
    # jiwer.cer operates on word sequences by default when given strings;
    # for CER we compare char-by-char by splitting into individual chars.
    ref_chars  = list(reference.replace(" ", ""))
    hyp_chars  = list(hypothesis.replace(" ", ""))
    if not ref_chars:
        return None
    ref_str = " ".join(ref_chars)
    hyp_str = " ".join(hyp_chars)
    return jiwer.wer(ref_str, hyp_str)  # WER on chars == CER


def _call_vlm(client: OpenAI, model_id: str, image_b64: str) -> tuple[str, float]:
    """Call the VLM API and return (transcription, latency_seconds)."""
    t0 = time.monotonic()
    response = client.chat.completions.create(
        model=model_id,
        messages=[
            {
                "role": "user",
                "content": [
                    {"type": "text",      "text": OCR_PROMPT},
                    {"type": "image_url", "image_url": {"url": image_b64}},
                ],
            }
        ],
        max_tokens=2048,
        temperature=0.0,
    )
    latency = time.monotonic() - t0
    text = response.choices[0].message.content or ""
    return text.strip(), latency


# ---------------------------------------------------------------------------
# Per-model evaluation
# ---------------------------------------------------------------------------

def _eval_model(
    key: str,
    fixture_root: Path,
    dry_run: bool,
    out_dir: Path,
) -> dict[str, Any]:
    cfg = MODELS[key]
    label = cfg["label"]
    print(f"\n{'='*60}")
    print(f"  Model: {label}")
    print(f"  API  : {cfg['base_url']}")
    print(f"{'='*60}")

    client = None
    if not dry_run:
        client = OpenAI(base_url=cfg["base_url"], api_key="EMPTY")

    results_per_class: dict[str, list[dict[str, Any]]] = {c: [] for c in FIXTURE_CLASSES}

    for cls in FIXTURE_CLASSES:
        cls_dir = fixture_root / cls
        if not cls_dir.is_dir():
            print(f"  [SKIP] class directory not found: {cls_dir}")
            continue

        pngs = sorted(cls_dir.glob("*.png"))
        if not pngs:
            print(f"  [WARN] no PNGs found in {cls_dir} — run generate_fixtures.py first")
            continue

        print(f"\n  Class: {cls}  ({len(pngs)} file(s))")

        for png in pngs:
            ref_text = _load_ref(png)
            entry: dict[str, Any] = {
                "file":      png.name,
                "class":     cls,
                "reference": ref_text,
                "hypothesis": "",
                "cer":       None,
                "latency_s": None,
                "error":     None,
            }

            if dry_run:
                print(f"    [dry-run] {png.name}  ref_chars={len(ref_text)}")
                results_per_class[cls].append(entry)
                continue

            try:
                image_b64 = _encode_image(png)
                hyp, latency = _call_vlm(client, cfg["model_id"], image_b64)
                cer = _compute_cer(hyp, ref_text)
                entry["hypothesis"] = hyp
                entry["cer"]        = cer
                entry["latency_s"]  = round(latency, 3)
                cer_str = f"{cer:.3f}" if cer is not None else "n/a"
                print(f"    {png.name:45s}  CER={cer_str}  lat={latency:.1f}s")
            except Exception as exc:
                entry["error"] = str(exc)
                print(f"    {png.name:45s}  ERROR: {exc}")

            results_per_class[cls].append(entry)

    # Persist JSON
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out_file = out_dir / f"{key}_{ts}.json"
    payload = {
        "model":       label,
        "model_id":    cfg["model_id"],
        "base_url":    cfg["base_url"],
        "timestamp":   ts,
        "dry_run":     dry_run,
        "results":     results_per_class,
    }
    out_dir.mkdir(parents=True, exist_ok=True)
    out_file.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"\n  Results saved: {out_file}")

    return payload


# ---------------------------------------------------------------------------
# Summary table
# ---------------------------------------------------------------------------

def _print_summary(all_results: list[dict[str, Any]]) -> None:
    print("\n" + "=" * 80)
    print("  VLM-OCR EVALUATION SUMMARY — per-class CER (lower is better)")
    print("=" * 80)

    header = f"{'Model':<25} {'printed':>10} {'handwritten':>14} {'tables':>10} {'stamps':>10} {'Overall':>10}"
    print(header)
    print("-" * len(header))

    for payload in all_results:
        label = payload["model"]
        row: dict[str, list[float]] = {c: [] for c in FIXTURE_CLASSES}
        for cls, entries in payload["results"].items():
            for e in entries:
                if e.get("cer") is not None:
                    row[cls].append(e["cer"])

        def fmt(vals: list[float]) -> str:
            if not vals:
                return "  —"
            return f"{sum(vals)/len(vals):.3f}"

        all_vals = [v for vs in row.values() for v in vs]
        overall  = f"{sum(all_vals)/len(all_vals):.3f}" if all_vals else "  —"

        print(
            f"{label:<25} "
            f"{fmt(row['printed']):>10} "
            f"{fmt(row['handwritten']):>14} "
            f"{fmt(row['tables']):>10} "
            f"{fmt(row['stamps']):>10} "
            f"{overall:>10}"
        )

    print("=" * 80)
    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="VLM-OCR evaluation harness for French legal fixtures",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--model",
        choices=list(MODELS.keys()),
        default=None,
        help="Run a single model key (lighton | olmocr | paddle). Default: all.",
    )
    parser.add_argument(
        "--fixtures",
        default=None,
        metavar="DIR",
        help="Override path to vlm_ocr_eval fixture directory.",
    )
    parser.add_argument(
        "--out-dir",
        default=None,
        metavar="DIR",
        help="Directory to write JSON results (default: scripts/eval_vlm_ocr_results/).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="List fixtures and skip API calls (for CI smoke tests).",
    )
    return parser.parse_args()


def main() -> None:
    args = _parse_args()

    fixture_root = _find_fixture_root(args.fixtures)
    print(f"Fixture root : {fixture_root}")

    # Resolve output directory relative to this script's parent (repo root)
    if args.out_dir:
        out_dir = Path(args.out_dir)
    else:
        out_dir = Path(__file__).resolve().parent / "eval_vlm_ocr_results"

    model_keys = [args.model] if args.model else list(MODELS.keys())

    all_results: list[dict[str, Any]] = []
    for key in model_keys:
        result = _eval_model(key, fixture_root, dry_run=args.dry_run, out_dir=out_dir)
        all_results.append(result)

    _print_summary(all_results)


if __name__ == "__main__":
    main()
