#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "torch>=2.0.0",
#     "transformers>=4.30.0",
#     "onnx>=1.14.0",
#     "optimum>=1.16.0",
# ]
# ///
"""
Export biomedical NER model to ONNX format.

Uses d4data/biomedical-ner-all which is pre-trained on merged biomedical NER datasets
for detecting entities like: Disease, Chemical, Drug, Gene, Species, etc.

Usage:
    uv run scripts/export_biomedical_ner_to_onnx.py --output ~/.cache/anno/models/biomedical-ner/
    
    # Then use with anno (requires adding BiomedicalNEROnnx backend):
    BIOMEDICAL_MODEL_PATH=~/.cache/anno/models/biomedical-ner anno extract --model biomedical "Your text"

Supported entity types from d4data/biomedical-ner-all:
- Medication/Drug
- MedicalCondition  
- AnatomicalStructure
- BiologicalProcess
- ClinicalAttribute
- BodySubstance
- Gene
- Disease
- Chemical
"""

import argparse
import os
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(description="Export biomedical NER model to ONNX")
    parser.add_argument(
        "--model",
        type=str,
        default="d4data/biomedical-ner-all",
        help="HuggingFace model ID for biomedical NER"
    )
    parser.add_argument(
        "--output",
        type=str,
        default=os.path.expanduser("~/.cache/anno/models/biomedical-ner"),
        help="Output directory for ONNX model"
    )
    parser.add_argument(
        "--quantize",
        action="store_true",
        help="Apply INT8 quantization for smaller/faster model"
    )
    args = parser.parse_args()

    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"Exporting {args.model} to ONNX...")
    print(f"Output: {output_dir}")

    try:
        from optimum.onnxruntime import ORTModelForTokenClassification
        from transformers import AutoTokenizer

        # Load tokenizer
        print("Loading tokenizer...")
        tokenizer = AutoTokenizer.from_pretrained(args.model)
        
        # Export to ONNX
        print("Converting model to ONNX (this may take a minute)...")
        ort_model = ORTModelForTokenClassification.from_pretrained(
            args.model,
            export=True,
        )
        
        # Save ONNX model and tokenizer
        print("Saving ONNX model...")
        ort_model.save_pretrained(output_dir)
        tokenizer.save_pretrained(output_dir)

        if args.quantize:
            print("Applying INT8 quantization...")
            from optimum.onnxruntime import ORTQuantizer
            from optimum.onnxruntime.configuration import AutoQuantizationConfig
            
            quantizer = ORTQuantizer.from_pretrained(output_dir)
            qconfig = AutoQuantizationConfig.avx512_vnni(is_static=False, per_channel=True)
            quantizer.quantize(save_dir=output_dir / "quantized", quantization_config=qconfig)
            print(f"Quantized model saved to {output_dir / 'quantized'}")

        print(f"\n✓ Export complete!")
        print(f"  Model: {output_dir / 'model.onnx'}")
        print(f"  Config: {output_dir / 'config.json'}")
        print(f"  Tokenizer: {output_dir / 'tokenizer.json'}")
        
        # Print entity types
        from transformers import AutoConfig
        config = AutoConfig.from_pretrained(output_dir)
        if hasattr(config, 'id2label'):
            print(f"\n  Entity types ({len(config.id2label)}):")
            for idx, label in config.id2label.items():
                print(f"    {idx}: {label}")

    except ImportError as e:
        print(f"Error: Missing dependency. Please install with:")
        print(f"  pip install optimum[onnxruntime] transformers")
        raise e

if __name__ == "__main__":
    main()

