# Dataset System Improvements - 2025-01-27

## Summary

Continued improvements to the dataset registry, loader, and documentation system. Focused on adding expected F1 scores, improving code documentation, and identifying opportunities for further improvements.

## Completed Work

### 1. Expected F1 Scores

Added expected F1 scores for 10+ additional benchmarks:

- **Coreference**: GAP (85.0), PreCo (82.0), LitBank (75.0)
- **Relation Extraction**: DocRED (58.0), ReTACRED (72.0)
- **Biomedical**: CRAFT (82.0), BC2GMFull (84.0)
- **Multilingual**: CoNLL2002 (88.0), GermEval2014 (86.0)
- **Domain-specific**: LegalNER (83.0), FinNER (81.0)
- **Social media**: TweetNER7 (72.0)
- **Historical/classical**: SanskritUD (68.0), ClassicalChineseUD (72.0)

**Total expected F1 coverage**: ~31/451 datasets (7%, up from 5%)

### 2. Code Documentation

Added comprehensive comments to `loader.rs`:

- **Architecture notes** in `registry_hint_plan()` explaining:
  - Two-tier detection system (auto-detection vs explicit matches)
  - Detection logic priority order
  - Guidelines for adding new datasets

- **Section headers** in `parse_plan()` match statement:
  - CoNLL/BIO format datasets
  - JSONL format datasets
  - JSON format (Relation extraction)
  - Each section includes notes on when to add datasets

- **Inline comments** explaining:
  - Why certain datasets are in explicit matches vs auto-detection
  - Multi-task dataset handling
  - Format detection logic

### 3. Loader Improvements

- Fixed GUM dataset mapping (removed from GapTsv, handled by CoNLL auto-detection)
- Added comments explaining the two-tier system
- Documented when to use explicit matches vs auto-detection

### 4. URL Analysis

Identified 44 datasets with broken GitHub URLs that might have HuggingFace alternatives:

- AIONER, AgCNER, AstroNER, BioNERLLaMA
- ChineseNestedNER, EnzChemRED, LongDocNER
- MultiBioNERLong, SCINERNested
- And 35 more...

**Next steps**: Research HuggingFace alternatives for these datasets and update registry with HF IDs and URLs.

### 5. Scripts

Created `scripts/find_missing_loaders.py` to help identify datasets that could be added to the loader based on format metadata.

## Current Status

### Loader Coverage
- **Loadable datasets**: 240/451 (53%)
- **Auto-detected**: ~150+ via `registry_hint_plan()`
- **Explicit matches**: ~90 in `parse_plan()`

### Expected F1 Coverage
- **With expected F1**: ~31/451 (7%)
- **Target**: 225+ (50%)

### Example Coverage
- **With examples**: ~18/451 (4%)
- **Target**: 135+ (30%)

### URL Health
- **Valid URLs**: 251/451 (56%)
- **Broken URLs**: 118/451 (26%)
- **Paper-only URLs**: 34/451 (8%)
- **No URL**: 47/451 (10%)

## Remaining Work

### High Priority

1. **Fix broken URLs** (118 datasets)
   - Research HuggingFace alternatives for 44 GitHub-based datasets
   - Update registry with HF IDs and URLs
   - Mark permanently unavailable datasets as `ContactAuthors` or `Registration`

2. **Add more loader implementations** (211 datasets remaining)
   - Many can use existing parsers (CoNLL, JSONL, TSV)
   - Focus on datasets with clear format metadata and public URLs

3. **Add expected F1 scores** (target: 225+ datasets)
   - Research published papers for baseline scores
   - Focus on major benchmarks first

4. **Add example snippets** (target: 135+ datasets)
   - Extract examples from actual data files
   - Focus on datasets with unusual formats first

### Medium Priority

5. **Expand S3 cache** (223 datasets)
   - Download remaining datasets to S3
   - Prioritize frequently-used datasets first

6. **Add SHA256 hashes** for integrity verification

## Architecture Notes

The loader uses a smart two-tier system:

1. **Registry hints** (`registry_hint_plan()`)
   - Auto-detects format from registry metadata
   - Checks `format:` field, tasks, annotation schemes
   - Returns `None` if not confident (conservative)

2. **Explicit matches** (`parse_plan()`)
   - Fallback for special cases
   - Handles datasets with custom formats
   - Multi-task datasets (conservative auto-detection)

**To add a dataset**:
- **Common format + single task**: Add format metadata to registry, auto-detection will work
- **Special format**: Add to explicit match in `parse_plan()`
- **Multi-task**: Add to explicit match (auto-detection is conservative)

## Files Modified

- `anno/src/eval/dataset_registry.rs`: Added expected F1 scores
- `anno/src/eval/loader.rs`: Added comprehensive comments and documentation
- `scripts/find_missing_loaders.py`: Created helper script

## Next Session Priorities

1. Research and fix broken URLs (start with 44 GitHub-based datasets)
2. Add more expected F1 scores from published papers
3. Add example snippets to key datasets
4. Continue adding loader implementations for common-format datasets

