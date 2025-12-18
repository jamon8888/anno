# S3 Data Provenance

This document tracks the S3 bucket history and data lineage for the `anno` project.

## Current Bucket

| Property | Value |
|----------|-------|
| Name | `arc-anno-data` |
| Naming | `arc` (global namespace) → `anno` (project) → `data` |
| Region | us-east-1 |
| Access | Private (owner-only) |
| Created | 2024-12-08 |

### Security Configuration

```json
{
  "BlockPublicAcls": true,
  "IgnorePublicAcls": true,
  "BlockPublicPolicy": true,
  "RestrictPublicBuckets": true
}
```

## Bucket History

### `arc-anno-data` (current)
- Created: 2024-12-08
- Purpose: General data storage (datasets, models, scripts, eval results)
- Naming: `arc` as global namespace prefix, then project name

### `anno-arc-data` (deprecated)
- Created: 2024-12-08
- Deprecated: 2024-12-08 (same day - naming correction)
- Status: Data migrated to `arc-anno-data`, bucket deleted

### `anno-datasets-cache` (deprecated)
- Created: 2024-11-28
- Deprecated: 2024-12-08
- Status: Data migrated to `arc-anno-data`
- TODO: Delete after verification period (2025-01-08)

### `anno-data` (never created)
- Attempted: 2024-12-08
- Status: Name globally unavailable (owned by another AWS account)

## Directory Structure

```
s3://arc-anno-data/
├── datasets/           # Cached evaluation datasets
│   ├── *.conll         # CoNLL format files
│   ├── *.json          # JSON format datasets
│   └── *.tsv           # TSV coreference data
├── models/             # Pre-trained model artifacts
│   └── safetensors/    # SafeTensor model weights
├── scripts/            # Utility scripts
│   └── convert_pytorch_to_safetensors.py
├── eval-results/       # Evaluation run outputs
│   └── YYYY-MM-DD/     # Date-organized results
└── experiments/        # Experimental data
```

## Data Migration Commands

```bash
# Verify current bucket data
s5cmd ls s3://arc-anno-data/

# Sync from old bucket (if needed)
s5cmd sync 's3://anno-datasets-cache/*' s3://arc-anno-data/

# Delete old bucket (after verification)
# aws s3 rb s3://anno-datasets-cache --force
```

## Environment Configuration

Add to `.env`:

```bash
# S3 cache configuration
ANNO_S3_CACHE=1
ANNO_S3_BUCKET=arc-anno-data

# HuggingFace token (for gated datasets)
HF_TOKEN=hf_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

## Related Files

- `datasets.toml` - Dataset configuration (URLs, metadata, splits)
- `scripts/sync_datasets_s3.sh` - S3 sync script
- `justfile` - Contains S3 targets (s3-sync, s3-download, s3-upload)

