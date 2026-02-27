# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "torch>=2.0",
#     "transformers>=4.30",
#     "fastcoref>=2.0",
#     "onnx>=1.14",
#     "onnxruntime>=1.15",
#     "safetensors>=0.4",
# ]
# ///
"""Export biu-nlp/f-coref to ONNX encoder + safetensors scorer weights.

Produces:
  <output_dir>/
    encoder.onnx          -- DistilRoBERTa encoder (input_ids, attention_mask -> last_hidden_state)
    scorer_weights.safetensors -- All mention/antecedent scorer MLP weights
    tokenizer.json        -- HuggingFace tokenizer
    config.json           -- Model config with coref head params

Usage:
    uv run scripts/export_fcoref.py [--output-dir fcoref_onnx] [--quantize]
"""

import argparse
import json
import sys
from collections import OrderedDict
from pathlib import Path

import onnx
import torch
from safetensors.torch import save_file
from transformers import AutoTokenizer


def load_fcoref_model():
    """Load the f-coref model from HuggingFace."""
    from fastcoref import FCoref

    model = FCoref(device="cpu", enable_progress_bar=False)
    return model


def export_encoder(model, output_dir: Path, opset: int = 14):
    """Export the DistilRoBERTa encoder to ONNX."""
    encoder = model.model.base_model

    # Dummy inputs matching DistilRoBERTa's expected input
    dummy_input_ids = torch.ones(1, 128, dtype=torch.long)
    dummy_attention_mask = torch.ones(1, 128, dtype=torch.long)

    encoder_path = output_dir / "encoder.onnx"

    torch.onnx.export(
        encoder,
        (dummy_input_ids, dummy_attention_mask),
        str(encoder_path),
        opset_version=opset,
        input_names=["input_ids", "attention_mask"],
        output_names=["last_hidden_state"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "seq_len"},
            "attention_mask": {0: "batch", 1: "seq_len"},
            "last_hidden_state": {0: "batch", 1: "seq_len"},
        },
    )

    # Validate the exported model
    onnx_model = onnx.load(str(encoder_path))
    onnx.checker.check_model(onnx_model)
    print(f"  encoder.onnx: {encoder_path.stat().st_size / 1024 / 1024:.1f} MB")
    return encoder_path


def export_scorer_weights(model, output_dir: Path):
    """Export all scorer head weights to safetensors."""
    coref_model = model.model

    tensors = OrderedDict()

    # Mention detection MLPs
    for name, param in coref_model.start_mention_mlp.named_parameters():
        tensors[f"start_mention_mlp.{name}"] = param.data.clone()
    for name, param in coref_model.end_mention_mlp.named_parameters():
        tensors[f"end_mention_mlp.{name}"] = param.data.clone()

    # Mention classifiers
    for name, param in coref_model.mention_start_classifier.named_parameters():
        tensors[f"mention_start_classifier.{name}"] = param.data.clone()
    for name, param in coref_model.mention_end_classifier.named_parameters():
        tensors[f"mention_end_classifier.{name}"] = param.data.clone()
    for name, param in coref_model.mention_s2e_classifier.named_parameters():
        tensors[f"mention_s2e_classifier.{name}"] = param.data.clone()

    # Coreference MLPs
    for name, param in coref_model.start_coref_mlp.named_parameters():
        tensors[f"start_coref_mlp.{name}"] = param.data.clone()
    for name, param in coref_model.end_coref_mlp.named_parameters():
        tensors[f"end_coref_mlp.{name}"] = param.data.clone()

    # Antecedent classifiers
    for name, param in coref_model.antecedent_s2s_classifier.named_parameters():
        tensors[f"antecedent_s2s_classifier.{name}"] = param.data.clone()
    for name, param in coref_model.antecedent_e2e_classifier.named_parameters():
        tensors[f"antecedent_e2e_classifier.{name}"] = param.data.clone()
    for name, param in coref_model.antecedent_s2e_classifier.named_parameters():
        tensors[f"antecedent_s2e_classifier.{name}"] = param.data.clone()
    for name, param in coref_model.antecedent_e2s_classifier.named_parameters():
        tensors[f"antecedent_e2s_classifier.{name}"] = param.data.clone()

    weights_path = output_dir / "scorer_weights.safetensors"
    save_file(tensors, str(weights_path))
    print(f"  scorer_weights.safetensors: {weights_path.stat().st_size / 1024:.1f} KB")
    print(f"  tensors: {list(tensors.keys())}")
    return weights_path


def export_tokenizer(output_dir: Path):
    """Save the tokenizer."""
    tokenizer = AutoTokenizer.from_pretrained("distilroberta-base")
    tokenizer.save_pretrained(str(output_dir))
    # Keep only tokenizer.json (remove auxiliary files)
    for f in output_dir.iterdir():
        if f.name not in ("tokenizer.json", "encoder.onnx", "scorer_weights.safetensors", "config.json"):
            f.unlink()
    print(f"  tokenizer.json saved")


def export_config(model, output_dir: Path):
    """Save model config with coref head params."""
    config = model.model.config.to_dict()
    coref_head = config.get("coref_head", {})
    export_config = {
        "model_type": "fcoref",
        "encoder": "distilroberta-base",
        "hidden_size": config.get("hidden_size", 768),
        "coref_head": {
            "ffnn_size": coref_head.get("ffnn_size", 1024),
            "max_span_length": coref_head.get("max_span_length", 30),
            "top_lambda": coref_head.get("top_lambda", 0.25),
            "max_segment_len": coref_head.get("max_segment_len", 512),
        },
    }
    config_path = output_dir / "config.json"
    with open(config_path, "w") as f:
        json.dump(export_config, f, indent=2)
    print(f"  config.json saved")
    return config_path


def quantize_onnx(encoder_path: Path):
    """Optionally quantize the encoder to INT8."""
    try:
        from onnxruntime.quantization import QuantType, quantize_dynamic

        quantized_path = encoder_path.parent / "encoder_quantized.onnx"
        quantize_dynamic(
            str(encoder_path),
            str(quantized_path),
            weight_type=QuantType.QInt8,
        )
        print(f"  encoder_quantized.onnx: {quantized_path.stat().st_size / 1024 / 1024:.1f} MB")
        return quantized_path
    except ImportError:
        print("  quantization skipped (onnxruntime.quantization not available)")
        return None


def verify_export(output_dir: Path):
    """Verify the exported model produces valid output."""
    import onnxruntime as ort
    from safetensors import safe_open

    # Verify encoder
    session = ort.InferenceSession(str(output_dir / "encoder.onnx"))
    import numpy as np

    dummy_ids = np.ones((1, 10), dtype=np.int64)
    dummy_mask = np.ones((1, 10), dtype=np.int64)
    outputs = session.run(None, {"input_ids": dummy_ids, "attention_mask": dummy_mask})
    hidden = outputs[0]
    assert hidden.shape == (1, 10, 768), f"unexpected shape: {hidden.shape}"
    print(f"  encoder verification: OK (output shape {hidden.shape})")

    # Verify scorer weights
    with safe_open(str(output_dir / "scorer_weights.safetensors"), framework="pt") as f:
        keys = list(f.keys())
        assert len(keys) > 0, "no tensors in scorer_weights"
        print(f"  scorer weights verification: OK ({len(keys)} tensors)")

    # Verify config
    with open(output_dir / "config.json") as f:
        cfg = json.load(f)
        assert cfg["coref_head"]["ffnn_size"] == 1024
        print(f"  config verification: OK")


def main():
    parser = argparse.ArgumentParser(description="Export f-coref to ONNX + safetensors")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("fcoref_onnx"),
        help="Output directory (default: fcoref_onnx)",
    )
    parser.add_argument(
        "--quantize",
        action="store_true",
        help="Also produce INT8 quantized encoder",
    )
    parser.add_argument(
        "--opset",
        type=int,
        default=14,
        help="ONNX opset version (default: 14)",
    )
    args = parser.parse_args()

    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    print("Loading f-coref model...")
    model = load_fcoref_model()

    print("Exporting encoder to ONNX...")
    encoder_path = export_encoder(model, output_dir, opset=args.opset)

    print("Exporting scorer weights to safetensors...")
    export_scorer_weights(model, output_dir)

    print("Saving tokenizer...")
    export_tokenizer(output_dir)

    print("Saving config...")
    export_config(model, output_dir)

    if args.quantize:
        print("Quantizing encoder...")
        quantize_onnx(encoder_path)

    print("Verifying export...")
    verify_export(output_dir)

    print(f"\nExport complete: {output_dir}/")
    return 0


if __name__ == "__main__":
    sys.exit(main())
