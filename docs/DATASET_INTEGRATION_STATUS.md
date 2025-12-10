# Dataset Integration Status

*Updated: 2025-12-10*

## Summary

| Metric | Count |
|--------|-------|
| Datasets in registry (`dataset_registry.rs`) | **479** |
| DatasetId unified | **Yes** (loader re-exports from registry) |
| With HuggingFace IDs | 32 |
| With `alt_sources` (fallback URLs) | **151** |
| With `s3_path` (S3 mirror) | **148** |
| With explicit `tasks` field | ~170 |
| With `categories` field | 479 |
| Downloadable (with URLs) | ~350 |
| In S3 cache | **183 files (7.9GB)** |
| Models in S3 cache | **351 files (53GB)** |
| URL health (last check) | 308 OK, 140 broken (31%) |

## Recent Additions (2025-12-09)

Added 24 datasets from the [entity-recognition-datasets](https://github.com/juand-r/entity-recognition-datasets) comprehensive list:

| Dataset | Language | Domain | Notes |
|---------|----------|--------|-------|
| GMB (Groningen Meaning Bank) | en | general | Multi-layer semantic annotation |
| WorldWide Newswire | en | news | Stanford 2023; geographic NER bias evaluation |
| suralk/multiNER | en/ta/si | government | English/Tamil/Sinhala parallel |
| NCHLT (4 languages) | af/zu/xh/ve | general | South African: Afrikaans, Zulu, Xhosa, Venda |
| RONEC | ro | news | Romanian; OntoNotes schema |
| KIND | it | news | Italian; FBK |
| DaNE | da | news | Danish; DaNLP project |
| TurkuNER | fi | news | Finnish; agglutinative |
| factRuEval | ru | news | Russian; fact extraction |
| KazNERD | kk | news | Kazakh; Turkic language |
| Thai-NNER | th | news | Thai nested NER |
| PhoNER_COVID19 | vi | biomedical | Vietnamese COVID-19 |
| KLUE-NER | ko | news | Korean benchmark |
| TLUnified | tl | general | Tagalog/Filipino |
| idner-news-2k | id | news | Indonesian |
| Wojood | ar | general | Arabic nested NER (21 types) |
| LeNER-Br | pt | legal | Brazilian Portuguese legal |
| CANTEMIST | es | biomedical | Spanish tumor morphology |
| L3Cube-MahaNER | mr | news | Marathi |
| pioNER | hy | news | Armenian |

### Additional datasets (later 2025-12-09)

| Dataset | Language | Domain | Notes |
|---------|----------|--------|-------|
| BBN | en | news | Fine-grained 29-type (LDC) |
| NCHLTisiNdebele | nr | general | South African isiNdebele |
| NCHLTSepedi | nso | general | Northern Sotho |
| NCHLTSesotho | st | general | Southern Sotho |
| NCHLTSetswana | tn | general | Tswana |
| NCHLTSiswati | ss | general | Swazi |
| NCHLTXitsonga | ts | general | Tsonga |
| IgboNER | ig | general | Igbo (LREC 2022) |
| MphayaNER | ve | general | Tshivenḓa (AfricaNLP 2023) |

**Total NCHLT South African coverage: 10/11 languages** (all except isiSwahili which is covered by MasakhaNER)

## New Fields: Alternative Sources & Acquisition Notes (2025-12-09)

The `define_datasets!` macro now supports two new optional fields for handling datasets with broken URLs or restricted access:

### `alt_sources`

Array of alternative download URLs (GitHub mirrors, Kaggle, etc.):

```rust
alt_sources: [
    "https://github.com/patverga/torch-ner-nlp-from-scratch/tree/master/data/conll2003/",
    "https://github.com/synalp/NER/tree/master/corpus/CoNLL-2003"
],
```

Use `DatasetId::alt_sources()` to get alternatives when primary URL fails.

### `acquisition_note`

Instructions for obtaining restricted datasets:

```rust
acquisition_note: "Requires LDC membership: https://catalog.ldc.upenn.edu/LDC2013T19",
```

Use `DatasetId::acquisition_note()` to show users how to obtain data requiring licenses.

## New Fields: Version, S3 Mirror, Quality & Sizes (2025-12-09)

The `define_datasets!` macro now supports additional optional fields for improved dataset management:

### `version`

Dataset version string for reproducibility:

```rust
version: "2.0",
```

Use `DatasetId::version()` to get the dataset version.

### `s3_path`

Our S3 mirror path for reliable downloads:

```rust
s3_path: "datasets/conll2003.conll",
```

Use `DatasetId::s3_path()` to get the S3 path. Full URL: `s3://arc-anno-data/{s3_path}`.

### `annotation_quality`

Annotation supervision level: `"gold"`, `"silver"`, or `"weak"`:

```rust
annotation_quality: "gold",
```

Use `DatasetId::annotation_quality()` to filter by annotation quality.

### `train_size`, `dev_size`, `test_size`

Number of examples in each split:

```rust
train_size: 14987,
dev_size: 3466,
test_size: 3684,
```

Use `DatasetId::train_size()`, `dev_size()`, `test_size()` to get split sizes.

### `superseded_by`

Reference to newer dataset version if deprecated:

```rust
superseded_by: "CleanCoNLL",
```

Use `DatasetId::superseded_by()` to check for newer versions.

### `all_urls()` Helper

New method returns all download URLs in priority order (primary URL + alt_sources):

```rust
let urls = dataset_id.all_urls();
for url in urls {
    if download(url).is_ok() {
        break;  // Success!
    }
}
```

### Datasets with Acquisition Notes

| Dataset | License | Note |
|---------|---------|------|
| OntoNotes 5.0 | LDC | LDC2013T19 membership |
| BBN | LDC | LDC2005T33 membership |
| ACE 2005 | LDC | LDC2006T06 membership |
| i2b2 2014 | DUA | Sign DUA at i2b2.org |

## Architectural Note: DatasetId Unification (2025-12-09)

**DatasetId is now unified.** The `loader.rs` file re-exports from `dataset_registry.rs`:

```rust
// In loader.rs
pub use super::dataset_registry::DatasetId;
```

This ensures `dataset_registry.rs` is the **single source of truth** for dataset identifiers.

### Unification Complete (2025-12-09)

The unification removed ~5,200 lines of duplicate code from `loader.rs`:
- **Before**: `loader.rs` was ~10,800 lines (duplicate enum + impl)
- **After**: `loader.rs` is ~5,300 lines (loading functionality only)
- **Savings**: ~5,500 lines eliminated

---

## CLI Commands (2025-12-10)

New first-class CLI commands for dataset management:

### `anno dataset cache-info`

Show cache statistics:

```bash
$ anno dataset cache-info

Cache Information

  Cache directory: /Users/arc/Library/Caches/anno/datasets
  Cached files:    182
  Total size:      7978.5 MB
  Largest file:    natural_questions.json (6637.1 MB)
  S3 enabled:      false

  Manifest:
    Entries:     5

# Export manifest as JSON
$ anno dataset cache-info --export > manifest.json
```

### `anno dataset check-health`

Check URL health for dataset sources:

```bash
# Check specific dataset
$ anno dataset check-health --dataset wikigold
  OK WikiGold (200)

# Check all datasets from manifest
$ anno dataset check-health
  Summary: 308 healthy, 140 unhealthy
```

### `anno dataset checksums`

Compute checksums for cached datasets:

```bash
# Summary (default) - just count files
$ anno dataset checksums
  182 files, 7978.5 MB total

# List mode - show checksums with sizes
$ anno dataset checksums --mode list
  f89c5544643ec722    1.1MB  GAP.cache
  ...

# Skip large files (default: 100MB)
$ anno dataset checksums --mode list --skip-large-mb 50
  179 files checked, 3 skipped (>50MB), 7978.5 MB total
```

### `anno dataset download`

Download datasets to local cache:

```bash
# Download specific dataset
$ anno dataset download --dataset wikigold
  WikiGold ... OK (1696 sentences)

# Force re-download
$ anno dataset download --dataset wikigold --force
```

### `anno dataset prune`

Remove cached files not in manifest or matching a pattern:

```bash
# Dry run - show what would be deleted (files not in manifest)
$ anno dataset prune --dry-run
  Would delete: old_file.json (4.6 MB)
  Would delete 1 files, free 4.6 MB

# Delete files matching a pattern
$ anno dataset prune --pattern "natural_questions"
  Deleted: natural_questions.json
  Deleted 1 files, freed 6637.1 MB
```

### `anno dataset sync-manifest`

Update manifest with checksums for all cached files (useful after bulk downloads via Python scripts):

```bash
$ anno dataset sync-manifest
  Added: file1.json
  Added: file2.conll
  ...
  Added 178 entries, updated 0
  Manifest saved to /Users/arc/Library/Caches/anno/datasets/manifest.json
```

---

## Module Boundaries

### `dataset_registry.rs` — The Single Source of Truth

**Owns:**
- `DatasetId` enum (generated by `define_datasets!` macro)
- Static metadata: `name()`, `description()`, `download_url()`, `license()`, `citation()`
- Category predicates from macro: `is_ner()`, `is_coref()`, `is_biomedical()`, etc.
- Static accessors: `entity_types()`, `language()`, `domain()`, `format()`
- Collection methods: `all()`, `quick()`, `medium()`

**When to modify:**
- Adding new datasets
- Changing dataset metadata (URL, license, entity types)
- Adding new category flags to the macro

### `loader.rs` — Runtime Operations & Extensions

**Owns:**
- `DatasetLoader` - HTTP download, local caching, format parsing
- `LoadedDataset`, `AnnotatedSentence`, `AnnotatedToken` - parsed data types
- `DatasetMetadata`, `CacheManifest` - runtime metadata
- `DatasetCounts`, `ValidationResult` - validation infrastructure

**Re-exports:**
- `DatasetId` from `dataset_registry.rs`

**Extension methods on `DatasetId`:**
- Computed predicates: `is_intra_doc_coref()`, `is_temporal_ner()`, etc.
- Runtime helpers: `default_metadata()`, `canonical_counts()`, `expected_checksum()`
- Language helpers: `african_language_codes()`, `african_language_url()`

**When to modify:**
- Adding new parsing formats
- Changing download/caching behavior
- Adding computed predicates that depend on multiple base predicates
- Adding runtime validation logic

### Boundary Rules

| Question | Answer |
|----------|--------|
| Where to add a new dataset? | `dataset_registry.rs` (in `define_datasets!` macro) |
| Where to add a simple `is_X()` predicate? | `dataset_registry.rs` (add to macro categories) |
| Where to add a computed predicate like `is_joint_ner_re()`? | `loader.rs` (extension impl on DatasetId) |
| Where to add download URL? | `dataset_registry.rs` (in macro `url:` field) |
| Where to add parsing logic? | `loader.rs` (in `DatasetLoader`) |
| Where to add validation counts? | `loader.rs` (in `canonical_counts()`) |

### Import Pattern

```rust
// To get DatasetId and its methods:
use crate::eval::loader::DatasetId;  // Re-exported from registry

// To use the loader:
use crate::eval::loader::{DatasetLoader, LoadedDataset};

// Direct registry access (rarely needed):
use crate::eval::dataset_registry::DatasetId;
```

---

### Remaining Work

1. **Add HuggingFace IDs** - For automated downloads where available
2. **S3 mirroring** - For datasets with permissive licenses

## Integration Status by System

### ✅ Completed

- [x] Dataset registry expanded to **458 datasets**
- [x] **DatasetId unified** - `loader.rs` now re-exports from registry
- [x] Generated TOML/JSON/Markdown documentation
- [x] S3 upload of generated metadata
- [x] Randomized matrix test expanded to 67 datasets
- [x] Download config generator created (147 downloadable)
- [x] All tests pass (992+ tests)

### ⚠️ Partial

- [ ] Download scripts use separate hardcoded configs
- [ ] SHA256 hashes missing for most datasets
- [ ] Not all registry datasets have loader implementations

### ❌ Not Started

- [ ] Auto-generate loader implementations from registry
- [ ] Validate all download URLs are accessible
- [ ] Add SHA256 hashes to registry entries

## File Locations

| File | Purpose |
|------|---------|
| `anno/src/eval/dataset_registry.rs` | Source of truth for dataset metadata |
| `anno/src/eval/loader.rs` | Dataset loading implementations |
| `datasets_generated.toml` | Generated TOML export |
| `datasets_generated.json` | Generated JSON export |
| `docs/DATASETS_GENERATED.md` | Generated markdown documentation |
| `download_configs_generated.json` | Generated download configs |
| `scripts/download_extended_datasets.py` | Dataset downloader |
| `scripts/generate_download_configs.py` | Config generator |

## Language Coverage

With the 2025-12-09 additions, Anno now covers:

| Region | Languages | Notes |
|--------|-----------|-------|
| European | English, German, Dutch, French, Spanish, Italian, Portuguese, Romanian, Danish, Finnish, Swedish, Norwegian, Russian, Ukrainian, Polish, Czech, Slovak, Slovene, Croatian, Serbian, Bulgarian, Hungarian, Greek, Latvian, Lithuanian, Estonian, Icelandic, Basque, Catalan, Galician | 30+ languages |
| Asian | Chinese (simplified/traditional), Japanese, Korean, Thai, Vietnamese, Indonesian, Tagalog, Hindi, Bengali, Telugu, Tamil, Malayalam, Marathi, Punjabi, Urdu, Sanskrit, Nepali, Sinhala | 18+ languages |
| Middle Eastern | Arabic, Persian, Turkish, Kazakh, Armenian, Coptic, Hebrew | 7 languages |
| African | Amharic, Swahili, Yoruba, Igbo, Hausa, Zulu, Xhosa, Afrikaans, Tshivenda, Sesotho, Setswana, Sepedi, Siswati, Xitsonga, isiNdebele | 15+ languages |
| Indigenous/Low-Resource | Cherokee, Nahuatl, Hawaiian, Quechua | 4+ languages |

## Tasks vs Categories Design

The registry distinguishes between:

- **`tasks`**: NLP tasks the dataset supports (ner, coref, re, el, event_coref, slot_filling)
- **`categories`**: Discovery/filtering properties (domain: biomedical, literary; language: multilingual; annotation: nested)

Many datasets have task-like categories (ner, coref) for backwards compatibility. Use `tasks_or_inferred()` to get tasks with fallback to category inference.

```rust
// Explicit tasks if defined, otherwise infer from categories
let tasks = dataset_id.tasks_or_inferred();

// Convenience checks
if dataset_id.supports_ner() { ... }
if dataset_id.supports_coref() { ... }
```

## Next Steps

1. **Immediate**: Add `tasks` field to datasets that only have task-like categories
2. **Short-term**: Add more HuggingFace IDs for automated downloads
3. **Long-term**: Refactor to single DatasetId enum

## Commands

```bash
# Regenerate derived files
cargo test -p anno --lib --features eval -- generate_datasets --ignored

# Upload to S3
just s3-upload

# Run matrix tests
just test-matrix

# Generate download configs
uv run scripts/generate_download_configs.py --dry-run
```

