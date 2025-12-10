# Dataset Registry Design

This document describes the design of Anno's dataset registry system, including download fallback strategies, caching, and metadata fields.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Dataset Registry                             │
│  (anno/src/eval/dataset_registry.rs - define_datasets! macro)   │
│                                                                  │
│  Fields: name, url, entity_types, language, domain, license,    │
│          hf_id, alt_sources, s3_path, version, annotation_quality│
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Dataset Loader                               │
│              (anno/src/eval/loader.rs)                          │
│                                                                  │
│  - Cache management (local + S3)                                │
│  - Multi-source download with fallbacks                         │
│  - Format parsing (CoNLL, JSONL, BIO, etc.)                     │
└─────────────────────────────────────────────────────────────────┘
```

## Download Strategy

### Priority Order

1. **Local Cache** (instant, no network)
2. **S3 Mirror** (fast, reliable, if `ANNO_S3_CACHE=1`)
3. **Primary URL** (from registry)
4. **Alternative Sources** (from `alt_sources` field)

### Fallback Logic

```rust
fn download(&self, id: DatasetId) -> Result<String> {
    let urls = id.all_urls();  // primary + alt_sources
    
    for url in urls {
        match self.download_with_retries(id, url) {
            Ok(content) => return Ok(content),
            Err(e) => continue,  // Try next URL
        }
    }
    
    Err("All sources failed")
}
```

Each URL gets 3 retries with exponential backoff (1s, 2s, 4s).

## Metadata Fields

### Core Fields (Required)

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Human-readable name |
| `description` | string | Brief description |
| `url` | string | Primary download URL |
| `entity_types` | [string] | Supported entity types |
| `language` | string | Primary language code |
| `domain` | string | Domain category |
| `categories` | [ident] | Category tags (ner, coref, etc.) |

### Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `license` | string | License type (MIT, CC-BY, LDC, etc.) |
| `citation` | string | BibTeX citation |
| `year` | u32 | Publication year |
| `format` | string | File format (CoNLL, JSONL, etc.) |
| `hf_id` | string | HuggingFace dataset ID |
| `alt_sources` | [string] | Alternative download URLs |
| `s3_path` | string | Our S3 mirror path |
| `version` | string | Dataset version |
| `annotation_quality` | string | "gold", "silver", or "weak" |
| `train_size` | u32 | Training set size |
| `dev_size` | u32 | Dev/validation set size |
| `test_size` | u32 | Test set size |

### Field Usage Guidelines

**When to add `alt_sources`:**
- Primary URL has been unreliable
- Dataset exists on multiple mirrors
- HuggingFace version available

**When to add `s3_path`:**
- Dataset has been downloaded and verified
- Want fast/reliable access for CI/CD

**`annotation_quality` values:**
- `"gold"`: Human-annotated, high quality
- `"silver"`: Automatically generated, some noise
- `"weak"`: Distant supervision, significant noise

## URL Health Monitoring

The `scripts/enrich_dataset_registry.py` script checks URL health:

```bash
uv run python scripts/enrich_dataset_registry.py --check-urls
```

Output includes:
- Working URLs (HTTP 200)
- Broken URLs (404, 401, timeout)
- Suggestions for alt_sources

## S3 Cache Structure

```
s3://arc-anno-data/
├── datasets/           # NER datasets
│   ├── conll2003_sample.conll
│   ├── wnut17.conll
│   └── ...
├── models/             # Pretrained models
│   ├── gliner/
│   └── ...
├── manifest.json       # Auto-generated inventory
└── README.md           # Documentation
```

## Design Decisions

### Why `alt_sources` as array?
Different mirrors have different reliability. By storing multiple alternatives, we can:
1. Try the most reliable first
2. Gracefully degrade when sources fail
3. Support regional mirrors

### Why separate `s3_path` from `alt_sources`?
S3 URLs require different handling (AWS credentials, s5cmd). Keeping them separate allows:
1. Clear distinction in download logic
2. Easy inventory of what we've mirrored
3. Automatic S3 fallback when local cache misses

### Why not store health status in registry?
Health status is dynamic (URLs go down/up). Better to:
1. Check periodically via script
2. Store results in external manifest
3. Use cached data when available

## Future Improvements

1. **Automatic S3 upload**: When downloading from primary URL, automatically mirror to S3
2. **Health-based priority**: Prefer URLs with better recent health
3. **Regional mirrors**: Support geographic distribution
4. **Checksum verification**: Add SHA256 for all datasets
5. **Version tracking**: Track which versions we have cached

