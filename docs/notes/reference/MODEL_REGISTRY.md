# Anno Model Registry

Models cached in `s3://arc-anno-data/models/` for reproducible evaluation.

## Current Inventory (December 2025)

### NER Models

| Model | S3 Path | Size | Task | Status |
|-------|---------|------|------|--------|
| GLiNER x-small | `ner/gliner/gliner-x-small/` | ~200MB | Zero-shot NER | Synced |
| GLiNER small v2.1 | `ner/gliner/gliner_small-v2.1/` | ~400MB | Zero-shot NER | Synced |
| GLiNER medium v2.1 | `ner/gliner/gliner_medium-v2.1/` | ~700MB | Zero-shot NER | Synced |
| GLiNER multi v2.1 (ONNX) | `ner/gliner/gliner_multi-v2.1-onnx/` | ~1.1GB | Multilingual NER | Synced |
| GLiNER bi-small v1.0 | `ner/gliner/gliner-bi-small-v1.0/` | ~400MB | Zero-shot NER | Synced |
| GLiNER modern-bi-large | `ner/gliner/modern-gliner-bi-large-v1.0/` | ~1.3GB | Zero-shot NER | Synced |
| GLiNER multitask large | `ner/gliner/gliner-multitask-large-v0.5/` | ~1.3GB | NER + RE | Synced |
| GLiNER PII | `ner/gliner/gliner_multi_pii-v1/` | ~700MB | PII detection | Synced |
| NuNER Zero (ONNX) | `ner/nuner/nuner-zero-onnx/` | ~1.7GB | Zero-shot NER | Synced |
| BERT base NER | `ner/bert-base-ner/` | ~420MB | Supervised NER | Synced |
| BERT base NER (ONNX) | `ner/bert-base-ner-onnx/` | ~400MB | Supervised NER | Synced |
| BERT large CoNLL03 | `ner/bert-large-conll03/` | ~1.2GB | Supervised NER | Synced |

### Encoders

| Model | S3 Path | Size | Use Case | Status |
|-------|---------|------|----------|--------|
| ModernBERT base | `encoders/modernbert-base/` | ~573MB | General encoding | Synced |
| BGE large en v1.5 | `encoders/bge-large-en-v1.5/` | ~1.2GB | Embeddings | Synced |
| all-MiniLM-L6-v2 | `encoders/all-MiniLM-L6-v2/` | ~90MB | Fast embeddings | Synced |
| Instructor large | `encoders/instructor-large/` | ~2.5GB | Task-specific embeddings | Synced |

### Rerankers

| Model | S3 Path | Size | Use Case | Status |
|-------|---------|------|----------|--------|
| ms-marco-MiniLM-L-6-v2 | `rerankers/ms-marco-MiniLM-L-6-v2/` | ~90MB | Cross-encoder reranking | Synced |
| mxbai-rerank-base-v2 | `rerankers/mxbai-rerank-base-v2/` | ~400MB | High-quality reranking | Synced |

### Vision

| Model | S3 Path | Size | Use Case | Status |
|-------|---------|------|----------|--------|
| SigLIP base | `vision/siglip-base-patch16-224/` | ~350MB | Image-text matching | Synced |

### Coreference (TODO)

| Model | HuggingFace ID | Size | Status |
|-------|----------------|------|--------|
| SpanBERT coref large | `shtoshni/spanbert_coreference_large` | ~1.5GB | **Not synced** |
| SpanBERT coref base | `shtoshni/spanbert_coreference_base` | ~500MB | **Not synced** |
| Longformer coref | `shtoshni/longformer_coreference_ontonotes` | ~600MB | **Not synced** |
| Qwen2 0.5B coref | `hsiehpinghan/Qwen2-0.5B-Instruct-Coreference-Resolution` | ~1GB | **Not synced** |
| LingMess coref | `biu-nlp/lingmess-coref` | ~1.5GB | **Not synced** |

### Relation Extraction (TODO)

| Model | HuggingFace ID | Size | Status |
|-------|----------------|------|--------|
| REBEL | `Babelscape/rebel-large` | ~1.5GB | **Not synced** |
| mREBEL large | `Babelscape/mrebel-large` | ~1.7GB | **Not synced** |
| UniRel | `biu-nlp/UniRel-base` | ~500MB | **Not synced** |

### Entity Linking (TODO)

| Model | HuggingFace ID | Size | Status |
|-------|----------------|------|--------|
| BLINK | `facebook/BLINK` | ~2GB | **Not synced** |
| ReFinED | `amazon/ReFinED` | ~1GB | **Not synced** |
| mGENRE | `facebook/mGENRE` | ~2GB | **Not synced** |

## Download Commands

```bash
# Download specific model from HuggingFace
huggingface-cli download shtoshni/spanbert_coreference_large

# Sync all local HF models to S3
./scripts/upload_models_s3.sh

# Download from S3 to local cache
./scripts/sync_datasets_s3.sh download
```

## Model Selection Guidelines

### For NER Tasks

| Scenario | Recommended Model |
|----------|-------------------|
| Fast inference, English | GLiNER x-small |
| High accuracy, English | BERT large CoNLL03 |
| Zero-shot (new entity types) | NuNER Zero or GLiNER multi |
| Multilingual | GLiNER multi v2.1 |
| PII detection | GLiNER PII |
| Joint NER + RE | GLiNER multitask large |

### For Coreference

| Scenario | Recommended Model |
|----------|-------------------|
| Long documents | Longformer coref |
| Short documents | SpanBERT coref base |
| High accuracy | SpanBERT coref large |

### For Embeddings

| Scenario | Recommended Model |
|----------|-------------------|
| Fast similarity search | all-MiniLM-L6-v2 |
| High quality embeddings | BGE large |
| Task-specific embeddings | Instructor large |

## Sync Status

Last sync: December 9, 2025

Total files on S3: ~351
Total size: ~20GB (estimated)

Categories:
- NER: 12 model families (GLiNER variants, NuNER, BERT)
- Encoders: 4 model families (ModernBERT, BGE, MiniLM, Instructor)
- Rerankers: 2 model families (ms-marco, mxbai)
- Vision: 1 model family (SigLIP)
- Coreference: 3 model families (SpanBERT base/large, Longformer)
- Relation Extraction: 1 model family (REBEL large)


