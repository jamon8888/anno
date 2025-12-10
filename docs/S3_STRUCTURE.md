# S3 Bucket Structure

Organized structure for `s3://arc-anno-data/`.

## Directory Layout

```
s3://arc-anno-data/
│
├── datasets/                    # Evaluation and training datasets
│   ├── ner/                     # NER datasets by task
│   │   ├── conll2003/           # CoNLL-2003 (train/dev/test)
│   │   ├── ontonotes5/          # OntoNotes 5.0 (requires LDC)
│   │   ├── fewnerd/             # Few-NERD (fine-grained)
│   │   ├── multinerd/           # MultiNERD (multilingual)
│   │   ├── wnut17/              # WNUT-17 (social media)
│   │   └── biomedical/          # BC5CDR, NCBI Disease, GENIA, etc.
│   │
│   ├── coref/                   # Coreference datasets
│   │   ├── gap/                 # GAP (gendered pronouns)
│   │   ├── preco/               # PreCo (reading comprehension)
│   │   ├── litbank/             # LitBank (literary)
│   │   ├── ecbplus/             # ECB+ (cross-document)
│   │   └── wikicoref/           # WikiCoref
│   │
│   ├── relation/                # Relation extraction
│   │   ├── docred/              # DocRED
│   │   ├── tacred/              # TACRED (requires LDC)
│   │   └── scierc/              # SciERC
│   │
│   └── legacy/                  # Flat files (pre-reorganization)
│       └── *.conll, *.json, etc.
│
├── models/                      # Pre-trained model weights
│   ├── ner/                     # NER models
│   │   ├── gliner/              # GLiNER variants (ONNX)
│   │   │   ├── gliner-small-v2.1/
│   │   │   ├── gliner-base-v2.1/
│   │   │   └── gliner-multi-v2.1/
│   │   ├── nuner/               # NuNER variants
│   │   │   └── nuner-zero-onnx/
│   │   └── bert-ner/            # BERT-based NER
│   │       └── bert-base-ner-onnx/
│   │
│   ├── coref/                   # Coreference models
│   │   ├── flan-t5-coref/       # T5-based coref
│   │   └── spanbert-coref/      # SpanBERT coref
│   │
│   └── encoders/                # Embedding models
│       ├── modernbert-base/
│       ├── deberta-v3-base/
│       └── bge-large-en/
│
├── scripts/                     # Utility scripts
│   ├── convert_pytorch_to_safetensors.py
│   ├── download_datasets.sh
│   └── validate_checksums.sh
│
├── eval-results/                # Evaluation outputs
│   └── YYYY-MM-DD/              # Date-organized
│       ├── ner/
│       ├── coref/
│       └── summary.json
│
├── experiments/                 # Experimental artifacts
│   └── YYYY-MM-DD-description/
│
└── manifests/                   # Data manifests
    ├── datasets.json            # Dataset checksums, sizes
    ├── models.json              # Model checksums, versions
    └── provenance.json          # Full pipeline provenance
```

## Model Priorities

Models to upload (by importance):

### Critical (for basic functionality)
1. `protectai/bert-base-NER-onnx` (~400MB) - Default ONNX NER
2. `knowledgator/gliner-x-small` (~700MB) - Fast GLiNER
3. `juampahc/gliner_multi-v2.1-onnx` (~1.1GB) - Multilingual GLiNER

### Important (for full features)
4. `deepanwa/NuNerZero_onnx` (~1.7GB) - Zero-shot NER
5. `dbmdz/bert-large-cased-finetuned-conll03-english` (~1.2GB) - High-accuracy NER
6. `answerdotai/ModernBERT-base` (~573MB) - Modern encoder

### Nice to have
7. `dslim/bert-base-NER` (~825MB) - Alternative BERT NER
8. `BAAI/bge-large-en-v1.5` (~1.2GB) - Embeddings
9. `hkunlp/instructor-large` (~2.5GB) - Instruction-tuned embeddings

## Upload Commands

```bash
# Upload a model directory
s5cmd sync ~/.cache/huggingface/hub/models--knowledgator--gliner-x-small/ \
  s3://arc-anno-data/models/ner/gliner/gliner-x-small/

# Upload all models (careful - large!)
s5cmd sync ~/.cache/huggingface/hub/models--* \
  s3://arc-anno-data/models/cache/

# Sync datasets to organized structure
s5cmd cp 's3://arc-anno-data/datasets/*.conll' \
  s3://arc-anno-data/datasets/legacy/
```

## Checksum Verification

All files should have SHA256 checksums recorded in manifests:

```json
{
  "models/ner/gliner/gliner-x-small/model.safetensors": {
    "sha256": "abc123...",
    "size_bytes": 700000000,
    "uploaded_at": "2024-12-08T00:00:00Z"
  }
}
```

