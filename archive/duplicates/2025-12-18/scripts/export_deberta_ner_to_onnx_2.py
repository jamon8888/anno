#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "torch>=2.0.0",
#     "transformers>=4.30.0",
#     "onnx>=1.14.0",
#     "onnxscript>=0.1.0",
#     "sentencepiece>=0.1.99",
#     "protobuf>=3.20.0",
# ]
# ///
"""
Export DeBERTa-v3 NER model to ONNX format.

DeBERTa-v3 uses disentangled attention and enhanced mask decoder for better
understanding of position and content. This creates a token classification
model for NER.

Usage:
    uv run scripts/export_deberta_ner_to_onnx.py --output /path/to/deberta-ner/model.onnx
    
    # Then use with anno:
    DEBERTA_MODEL_PATH=/path/to/deberta-ner anno extract --model deberta-v3 "Your text"
"""

import argparse
import os

import torch
import torch.nn as nn
from transformers import AutoModel, AutoTokenizer, AutoConfig


class DeBERTaNER(nn.Module):
    """DeBERTa-v3 for token classification (NER)."""
    
    def __init__(self, model_name: str = "microsoft/deberta-v3-base", num_labels: int = 9):
        super().__init__()
        self.config = AutoConfig.from_pretrained(model_name)
        self.deberta = AutoModel.from_pretrained(model_name)
        self.dropout = nn.Dropout(0.1)
        self.classifier = nn.Linear(self.config.hidden_size, num_labels)
        
        # Initialize classifier
        nn.init.xavier_normal_(self.classifier.weight)
        nn.init.zeros_(self.classifier.bias)
    
    def forward(
        self,
        input_ids: torch.Tensor,
        attention_mask: torch.Tensor,
    ) -> torch.Tensor:
        """
        Forward pass for token classification.
        
        Args:
            input_ids: [batch, seq_len]
            attention_mask: [batch, seq_len]
            
        Returns:
            logits: [batch, seq_len, num_labels]
        """
        outputs = self.deberta(input_ids, attention_mask=attention_mask)
        sequence_output = outputs.last_hidden_state
        sequence_output = self.dropout(sequence_output)
        logits = self.classifier(sequence_output)
        return logits


def export_to_onnx(
    model: nn.Module,
    tokenizer,
    output_path: str,
    max_length: int = 128,
):
    """Export model to ONNX format."""
    model.eval()
    
    # Create dummy inputs
    dummy_text = "This is a sample sentence for export."
    encoding = tokenizer(
        dummy_text,
        max_length=max_length,
        padding="max_length",
        truncation=True,
        return_tensors="pt",
    )
    
    input_ids = encoding["input_ids"]
    attention_mask = encoding["attention_mask"]
    
    # Create output directory
    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)
    
    # Export
    print(f"Exporting to {output_path}...")
    torch.onnx.export(
        model,
        (input_ids, attention_mask),
        output_path,
        input_names=["input_ids", "attention_mask"],
        output_names=["logits"],
        dynamic_axes={
            "input_ids": {0: "batch_size", 1: "sequence"},
            "attention_mask": {0: "batch_size", 1: "sequence"},
            "logits": {0: "batch_size", 1: "sequence"},
        },
        opset_version=14,
        do_constant_folding=True,
    )
    
    print(f"✓ Exported to {output_path}")
    
    # Verify
    import onnx
    model_onnx = onnx.load(output_path)
    onnx.checker.check_model(model_onnx)
    print("✓ ONNX model verified")


def main():
    parser = argparse.ArgumentParser(description="Export DeBERTa-v3 NER to ONNX")
    parser.add_argument(
        "--model",
        default="microsoft/deberta-v3-base",
        help="Base DeBERTa model to use",
    )
    parser.add_argument(
        "--num-labels",
        type=int,
        default=9,  # O, B-PER, I-PER, B-ORG, I-ORG, B-LOC, I-LOC, B-MISC, I-MISC
        help="Number of NER labels",
    )
    parser.add_argument(
        "--output",
        default="deberta-ner.onnx",
        help="Output ONNX file path",
    )
    parser.add_argument(
        "--max-length",
        type=int,
        default=128,
        help="Maximum sequence length",
    )
    args = parser.parse_args()
    
    print(f"Creating DeBERTa NER model with {args.model}...")
    
    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(args.model)
    
    # Create model
    model = DeBERTaNER(
        model_name=args.model,
        num_labels=args.num_labels,
    )
    
    # Export
    export_to_onnx(model, tokenizer, args.output, args.max_length)
    
    # Save tokenizer alongside
    from pathlib import Path
    tokenizer_dir = Path(args.output).parent
    tokenizer.save_pretrained(tokenizer_dir)
    print(f"✓ Tokenizer saved to {tokenizer_dir}")
    
    print("\nTo use with anno:")
    print(f"  DEBERTA_MODEL_PATH={tokenizer_dir} anno extract --model deberta-v3 'Your text here'")


if __name__ == "__main__":
    main()

