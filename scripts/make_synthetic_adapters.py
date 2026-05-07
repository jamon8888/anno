#!/usr/bin/env python3
# ruff: noqa: T201
"""
Generate two PEFT-format synthetic LoRA adapter directories for testing
anno's `gliner2_fastino_candle::load_adapter` end-to-end.

These adapters are NOT trained — they're random deltas with the exact
PEFT directory layout that `anno`'s loader expects:

    <out>/
    ├── adapter_config.json
    └── adapter_model.safetensors

Adapter A and Adapter B use different RNG seeds, so they produce
distinct deltas. Targeting Q/K/V projections in 3 encoder layers (0-2)
keeps each adapter ~600 KB.

The point is to prove anno's Phase 4 plumbing works on real PEFT
directories — not to demonstrate domain expertise.

Usage:
    python scripts/make_synthetic_adapters.py
    # Produces ./adapter_A/  and  ./adapter_B/

Then in Rust:
    cargo run -p anno --features gliner2-fastino-candle \\
        --example gliner2_candle_lora_demo

Dependencies (install once):
    pip install numpy safetensors

Total runtime: ~10 seconds. No GPU.
"""

import json
import os

import numpy as np
from safetensors.numpy import save_file

R = 4                  # LoRA rank
ALPHA = 8.0            # LoRA scaling factor (effective scale = alpha/r = 2.0)
HIDDEN = 768           # mDeBERTa-v3-base hidden size
NUM_TARGET_LAYERS = 3  # Layers 0, 1, 2 of the 12-layer encoder

# Module paths inside the gliner2 model. Three projections per layer.
TARGET_PROJS = ["query_proj", "key_proj", "value_proj"]


def build_adapter(out_dir: str, seed: int, base_model: str = "fastino/gliner2-multi-v1") -> dict:
    """Write a PEFT-format adapter directory with deterministic random deltas."""
    rng = np.random.default_rng(seed)
    os.makedirs(out_dir, exist_ok=True)

    # adapter_config.json — minimal PEFT schema that anno's lora.rs reads.
    config = {
        "peft_type": "LORA",
        "task_type": "TOKEN_CLS",
        "r": R,
        "lora_alpha": ALPHA,
        "lora_dropout": 0.0,
        "bias": "none",
        "fan_in_fan_out": False,
        "target_modules": TARGET_PROJS,
        "base_model_name_or_path": base_model,
    }
    config_path = os.path.join(out_dir, "adapter_config.json")
    with open(config_path, "w") as f:
        json.dump(config, f, indent=2)

    # adapter_model.safetensors — keys must follow the PEFT pattern
    # `base_model.model.<module_path>.lora_{A,B}.weight`.
    tensors: dict[str, np.ndarray] = {}
    for layer in range(NUM_TARGET_LAYERS):
        for proj in TARGET_PROJS:
            module_path = f"encoder.encoder.layer.{layer}.attention.self.{proj}"
            # Small random values (~normal(0, 0.01)) so the merged delta is
            # measurable but doesn't blow up the network's output.
            lora_a = rng.standard_normal((R, HIDDEN), dtype=np.float32) * 0.01
            lora_b = rng.standard_normal((HIDDEN, R), dtype=np.float32) * 0.01
            tensors[f"base_model.model.{module_path}.lora_A.weight"] = lora_a
            tensors[f"base_model.model.{module_path}.lora_B.weight"] = lora_b

    weights_path = os.path.join(out_dir, "adapter_model.safetensors")
    save_file(tensors, weights_path)

    return {
        "dir": out_dir,
        "config_path": config_path,
        "weights_path": weights_path,
        "num_tensors": len(tensors),
        "weights_size_bytes": os.path.getsize(weights_path),
    }


def main() -> None:
    print("Generating two PEFT-format synthetic adapters for anno's gliner2_fastino_candle...")
    print(f"  Base model: fastino/gliner2-multi-v1 (also works with -base-v1, -large-v1)")
    print(f"  Rank: {R}, Alpha: {ALPHA}, Layers: 0..{NUM_TARGET_LAYERS - 1}, Projections: {TARGET_PROJS}")
    print()

    a = build_adapter("./adapter_A", seed=42)
    b = build_adapter("./adapter_B", seed=1337)

    print(f"✅ {a['dir']}/")
    print(f"   {a['num_tensors']} tensors, {a['weights_size_bytes']:,} bytes")
    print(f"✅ {b['dir']}/")
    print(f"   {b['num_tensors']} tensors, {b['weights_size_bytes']:,} bytes")

    print()
    print("Next: run the demo in Rust")
    print()
    print("    cargo run -p anno --features gliner2-fastino-candle \\")
    print("        --example gliner2_candle_lora_demo")
    print()
    print("(or pass the adapter paths via env vars if the example was built")
    print(" elsewhere)")


if __name__ == "__main__":
    main()
