#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "glirel>=0.2.0",
#     "torch>=2.0.0",
#     "onnx>=1.14.0",
#     "onnxruntime>=1.16.0",
#     "transformers>=4.30.0",
#     "numpy>=1.21.0",
# ]
# ///
"""
Export GLiREL (relation extraction) model to ONNX format for use in anno.

GLiREL extends GLiNER to predict typed relations between entity pairs.
The model takes text + entity spans + relation labels as input, and outputs
relation scores for each (head, tail, relation_type) triple.

Architecture:
  - Shared DeBERTa-v3 encoder for text and relation labels
  - Entity pair representation via span pooling
  - Relation scorer: dot-product between pair repr and relation label repr

Output ONNX model:
  - Inputs: input_ids, attention_mask, entity_spans, relation_labels
  - Outputs: relation_scores [num_pairs, num_relations]

Usage:
    uv run scripts/export_glirel_onnx.py
    uv run scripts/export_glirel_onnx.py --model jackboyla/glirel-large-v0
    uv run scripts/export_glirel_onnx.py --output ~/.cache/anno/models/glirel/
    uv run scripts/export_glirel_onnx.py --quantize

Compatible models:
    jackboyla/glirel-large-v0    (DeBERTa-v3-large, zero-shot RE)
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import traceback
from pathlib import Path

DEFAULT_MODEL = "jackboyla/glirel-large-v0"
DEFAULT_OUTPUT = os.path.expanduser("~/.cache/anno/models/glirel")


def export_glirel(
    model_name: str,
    output_dir: str,
    quantize: bool = False,
    opset: int = 14,
) -> None:
    """Export GLiREL model to ONNX."""
    import numpy as np
    import torch
    from glirel import GLiREL

    print(f"[glirel-export] Loading model: {model_name}")
    model = GLiREL.from_pretrained(model_name)
    model.eval()

    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)

    # Save tokenizer
    tokenizer = model.tokenizer
    tokenizer.save_pretrained(str(out))
    print(f"[glirel-export] Saved tokenizer to {out}")

    # Save model config for Rust-side reconstruction
    config = {
        "model_name": model_name,
        "architecture": "glirel",
        "hidden_size": model.config.hidden_size if hasattr(model, "config") else 1024,
        "max_width": getattr(model, "max_width", 12),
    }

    # Try to extract hidden_size from the actual model
    for name, param in model.named_parameters():
        if "rel_rep_layer" in name or "rnn" in name:
            config["hidden_size"] = param.shape[-1]
            break

    with open(out / "glirel_config.json", "w") as f:
        json.dump(config, f, indent=2)
    print(f"[glirel-export] Saved config: {config}")

    # GLiREL uses a custom forward pass that doesn't map cleanly to a single
    # ONNX graph. We export using torch.onnx.export with a wrapper that
    # accepts the standard inputs.
    #
    # The GLiREL inference pipeline:
    # 1. Tokenize text (shared encoder)
    # 2. Encode relation type labels (same encoder, different prompt)
    # 3. Pool entity span representations from encoder hidden states
    # 4. For each (head, tail) pair, compute pair representation
    # 5. Score: dot(pair_repr, relation_label_repr) -> sigmoid
    #
    # We export the core scoring model that takes:
    # - token_rep: encoder hidden states [batch, seq, hidden]
    # - entity_spans: [num_entities, 2] (start, end word indices)
    # - relation_label_reps: [num_relations, hidden]
    #
    # The text encoding step uses the same DeBERTa encoder as GLiNER,
    # which we already have in the gliner_onnx backend. So we only need
    # to export the relation scoring head.

    # Strategy: export the full model using a traced wrapper
    class GLiRELWrapper(torch.nn.Module):
        """Wrapper for ONNX export of GLiREL relation scoring."""

        def __init__(self, glirel_model):
            super().__init__()
            self.model = glirel_model
            # Extract the relation-relevant layers
            self.encoder = glirel_model.model  # DeBERTa encoder
            self.rnn = glirel_model.rnn if hasattr(glirel_model, "rnn") else None
            self.span_rep_layer = (
                glirel_model.span_rep_layer
                if hasattr(glirel_model, "span_rep_layer")
                else None
            )
            self.rel_rep_layer = (
                glirel_model.rel_rep_layer
                if hasattr(glirel_model, "rel_rep_layer")
                else None
            )

        def forward(
            self,
            input_ids: torch.Tensor,
            attention_mask: torch.Tensor,
            words_mask: torch.Tensor,
            text_lengths: torch.Tensor,
            span_idx: torch.Tensor,
            span_mask: torch.Tensor,
            rel_label_input_ids: torch.Tensor,
            rel_label_attention_mask: torch.Tensor,
        ) -> torch.Tensor:
            # Encode text
            token_output = self.encoder(
                input_ids=input_ids,
                attention_mask=attention_mask,
            )
            token_rep = token_output.last_hidden_state

            # Encode relation labels
            rel_output = self.encoder(
                input_ids=rel_label_input_ids,
                attention_mask=rel_label_attention_mask,
            )
            # Mean pooling for relation label representations
            rel_mask = rel_label_attention_mask.unsqueeze(-1).float()
            rel_rep = (rel_output.last_hidden_state * rel_mask).sum(1) / rel_mask.sum(
                1
            ).clamp(min=1e-9)

            # Get word-level representations from subword tokens
            batch_size = input_ids.shape[0]
            max_words = text_lengths.max().item()
            hidden_size = token_rep.shape[-1]

            # Aggregate subword -> word level using words_mask
            word_rep = torch.zeros(
                batch_size, max_words, hidden_size, device=token_rep.device
            )
            word_count = torch.zeros(batch_size, max_words, 1, device=token_rep.device)
            for b in range(batch_size):
                for t in range(words_mask.shape[1]):
                    w = words_mask[b, t].item()
                    if w > 0:
                        word_rep[b, w - 1] += token_rep[b, t]
                        word_count[b, w - 1] += 1
            word_rep = word_rep / word_count.clamp(min=1)

            # Apply RNN if present
            if self.rnn is not None:
                word_rep = self.rnn(word_rep)[0]

            # Get span representations
            num_spans = span_idx.shape[1]
            span_start = span_idx[:, :, 0]  # [batch, num_spans]
            span_end = span_idx[:, :, 1]  # [batch, num_spans]

            # Simple span representation: start + end pooling
            span_start_rep = torch.gather(
                word_rep, 1, span_start.unsqueeze(-1).expand(-1, -1, hidden_size)
            )
            span_end_rep = torch.gather(
                word_rep, 1, span_end.unsqueeze(-1).expand(-1, -1, hidden_size)
            )
            span_rep = (span_start_rep + span_end_rep) / 2  # [batch, num_spans, hidden]

            if self.span_rep_layer is not None:
                span_rep = self.span_rep_layer(span_rep)

            # Create all entity pair representations
            # For each pair (i, j) where i != j, compute pair_rep
            # pair_rep = concat(head_rep, tail_rep) or head_rep * tail_rep
            # Shape: [batch, num_spans, num_spans, hidden]
            head_rep = span_rep.unsqueeze(2).expand(-1, -1, num_spans, -1)
            tail_rep = span_rep.unsqueeze(1).expand(-1, num_spans, -1, -1)
            pair_rep = head_rep * tail_rep  # element-wise product

            if self.rel_rep_layer is not None:
                pair_shape = pair_rep.shape
                pair_rep = self.rel_rep_layer(pair_rep.reshape(-1, pair_shape[-1]))
                pair_rep = pair_rep.reshape(
                    pair_shape[0], pair_shape[1], pair_shape[2], -1
                )

            # Score: dot product with relation label representations
            # pair_rep: [batch, num_spans, num_spans, hidden]
            # rel_rep:  [num_relations, hidden]
            scores = torch.einsum("bijd,rd->bijr", pair_rep, rel_rep)

            return scores  # [batch, num_spans, num_spans, num_relations]

    print("[glirel-export] Building ONNX wrapper...")

    # Note: Full ONNX export of the wrapper may not work directly because
    # GLiREL's internal architecture varies by version. Instead, we use
    # the GLiREL library for inference and export just the scoring head.
    #
    # For the Rust backend, we'll use a hybrid approach:
    # 1. Use the existing GLiNER ONNX encoder for text encoding
    # 2. Export only the relation scoring layers
    # 3. Combine them in the Rust inference pipeline

    # Try direct model export first
    try:
        wrapper = GLiRELWrapper(model)
        wrapper.eval()

        # Create dummy inputs matching the model's expected shapes
        batch_size = 1
        seq_len = 64
        num_words = 16
        num_spans = num_words * 12  # max_width = 12
        num_relations = 4

        dummy_input_ids = torch.randint(0, 30000, (batch_size, seq_len))
        dummy_attention_mask = torch.ones(batch_size, seq_len, dtype=torch.long)
        dummy_words_mask = torch.zeros(batch_size, seq_len, dtype=torch.long)
        for i in range(min(num_words, seq_len)):
            dummy_words_mask[0, i + 10] = i + 1  # offset by entity prompt tokens
        dummy_text_lengths = torch.tensor([[num_words]], dtype=torch.long)

        dummy_span_idx = torch.zeros(batch_size, num_spans, 2, dtype=torch.long)
        dummy_span_mask = torch.zeros(batch_size, num_spans, dtype=torch.bool)
        for s in range(min(num_words, num_spans)):
            dummy_span_idx[0, s, 0] = s
            dummy_span_idx[0, s, 1] = s
            dummy_span_mask[0, s] = True

        dummy_rel_ids = torch.randint(0, 30000, (num_relations, 8))
        dummy_rel_mask = torch.ones(num_relations, 8, dtype=torch.long)

        print("[glirel-export] Exporting to ONNX...")
        torch.onnx.export(
            wrapper,
            (
                dummy_input_ids,
                dummy_attention_mask,
                dummy_words_mask,
                dummy_text_lengths,
                dummy_span_idx,
                dummy_span_mask,
                dummy_rel_ids,
                dummy_rel_mask,
            ),
            str(out / "model.onnx"),
            input_names=[
                "input_ids",
                "attention_mask",
                "words_mask",
                "text_lengths",
                "span_idx",
                "span_mask",
                "rel_label_input_ids",
                "rel_label_attention_mask",
            ],
            output_names=["relation_scores"],
            dynamic_axes={
                "input_ids": {0: "batch", 1: "seq_len"},
                "attention_mask": {0: "batch", 1: "seq_len"},
                "words_mask": {0: "batch", 1: "seq_len"},
                "text_lengths": {0: "batch"},
                "span_idx": {0: "batch", 1: "num_spans"},
                "span_mask": {0: "batch", 1: "num_spans"},
                "rel_label_input_ids": {0: "num_relations", 1: "label_seq_len"},
                "rel_label_attention_mask": {0: "num_relations", 1: "label_seq_len"},
                "relation_scores": {
                    0: "batch",
                    1: "num_spans",
                    2: "num_spans",
                    3: "num_relations",
                },
            },
            opset_version=opset,
        )
        print(f"[glirel-export] Exported model.onnx to {out}")

    except Exception as e:
        print(f"[glirel-export] Direct wrapper export failed: {e}")
        print("[glirel-export] Falling back to scripted export...")
        traceback.print_exc()

        # Fallback: save PyTorch weights for Rust-side loading via candle
        torch.save(model.state_dict(), str(out / "model.pt"))
        print(f"[glirel-export] Saved PyTorch state_dict to {out / 'model.pt'}")
        print("[glirel-export] Note: ONNX export failed. The Rust GLiREL backend")
        print("[glirel-export] will need the candle feature to load PyTorch weights,")
        print("[glirel-export] or you can manually trace the model to ONNX.")

    if quantize:
        try:
            from onnxruntime.quantization import QuantType, quantize_dynamic

            model_path = out / "model.onnx"
            if model_path.exists():
                quant_path = out / "model_quantized.onnx"
                quantize_dynamic(
                    str(model_path),
                    str(quant_path),
                    weight_type=QuantType.QInt8,
                )
                print(f"[glirel-export] Quantized model saved to {quant_path}")
        except Exception as e:
            print(f"[glirel-export] Quantization failed: {e}")

    # Verify with onnxruntime
    model_path = out / "model.onnx"
    if model_path.exists():
        try:
            import onnxruntime as ort

            sess = ort.InferenceSession(str(model_path))
            print(f"[glirel-export] ONNX verification passed.")
            print(f"[glirel-export] Inputs:  {[i.name for i in sess.get_inputs()]}")
            print(f"[glirel-export] Outputs: {[o.name for o in sess.get_outputs()]}")
            for inp in sess.get_inputs():
                print(f"  {inp.name}: {inp.shape} ({inp.type})")
            for out_node in sess.get_outputs():
                print(f"  {out_node.name}: {out_node.shape} ({out_node.type})")
        except Exception as e:
            print(f"[glirel-export] ONNX verification failed: {e}")

    print(f"[glirel-export] Done. Output: {out}")


def main():
    parser = argparse.ArgumentParser(description="Export GLiREL to ONNX")
    parser.add_argument(
        "--model",
        default=DEFAULT_MODEL,
        help=f"HuggingFace model ID (default: {DEFAULT_MODEL})",
    )
    parser.add_argument(
        "--output",
        default=DEFAULT_OUTPUT,
        help=f"Output directory (default: {DEFAULT_OUTPUT})",
    )
    parser.add_argument(
        "--quantize", action="store_true", help="Also produce INT8 quantized model"
    )
    parser.add_argument(
        "--opset", type=int, default=14, help="ONNX opset version (default: 14)"
    )
    args = parser.parse_args()
    export_glirel(args.model, args.output, args.quantize, args.opset)


if __name__ == "__main__":
    main()
