# Dataset Fixes Summary

> Date: 2025-01-27
> 
> Summary of fixes applied and remaining work

## Completed Fixes

### 1. Documentation Updated ✅

**File**: `docs/DATASETS.md`

**Changes**:
- Updated Quick Reference table to show 451 datasets (was 20)
- Added comprehensive dataset statistics section
- Added dataset discovery examples with code
- Documented all 23+ categories for filtering
- Added status indicators (✅ Loadable) to key datasets

**Impact**: Users can now discover the full 451-dataset registry instead of thinking only 20 exist.

### 2. URL Validation Completed ✅

**Action**: Ran full URL validation via `scripts/validate_urls.py --all`

**Results**:
- ✅ 251 valid URLs (working, accessible)
- ❌ 118 broken URLs (404, 401, 403, SSL errors, timeouts)
- 📄 34 paper-only URLs (DOI/arXiv links, not direct downloads)
- ⚪ 47 no URL (may require licenses or contact authors)

**Output**: `url_validation_report.json` with full results

**Impact**: Clear visibility into which datasets are actually downloadable.

### 3. Comprehensive Review Document ✅

**File**: `docs/notes/DATASET_REVIEW.md`

**Contents**:
- Executive summary with key statistics
- Detailed breakdown of synthetic datasets (28+ domains)
- Complete analysis of real datasets (451 total)
- Architecture documentation
- Issues & gaps analysis
- Recommendations prioritized by impact

**Impact**: Single source of truth for dataset system status.

### 4. Added Loader Implementations for 12 Datasets ✅

**File**: `anno/src/eval/loader.rs`

**Changes**:
- Added 12 datasets with public URLs and parseable formats to `registry_hint_plan()`
- Fixed auto-detection logic for RE datasets that also have NER tasks
- All datasets now have proper parse plans

**Datasets Added**:
- **CoNLL format**: OntoNotes50, GUM, CoNLL04RE
- **JSONL NER**: SCINERNested, AgriNER, MOFDataset, SolidStateDoping
- **JSONL RE**: SciERC, SciER, SciERCNER, PolyIE, EnzChemRED

**Impact**: 12 more datasets are now loadable (228 → 240, 51% → 53%)

## Remaining Work

### High Priority

#### 1. Add Loader Implementations (211 datasets remaining)

**Current**: 240/451 datasets (53%) are loadable (was 228/51%)

**Progress**: ✅ Added 12 datasets today (SCINERNested, EnzChemRED, CoNLL04RE, SciERC, OntoNotes50, GUM, SciER, SciERCNER, PolyIE, MOFDataset, SolidStateDoping, AgriNER)

**Approach**:
- Many datasets can use existing parsers (CoNLL, JSONL, TSV)
- The loader has `registry_hint_plan()` that auto-detects format from metadata
- Need to add explicit matches for datasets that don't match hints

**Estimated effort**: 
- Common formats (CoNLL/JSONL): ~100 datasets could be added quickly
- Specialized formats: Require custom parsers

**Files to modify**:
- `anno/src/eval/loader.rs` - Add to `parse_plan()` match statement
- Or add format metadata to registry entries to enable auto-detection

#### 2. Fix Broken URLs (118 datasets)

**Current**: 118 broken URLs identified

**Approach**:
- Research alternative sources (HuggingFace, GitHub mirrors, etc.)
- Update registry entries with new URLs
- Mark as `ContactAuthors` or `Registration` when permanently unavailable

**Estimated effort**: 
- Research + update: ~2-3 hours per 10 datasets
- Total: ~30-40 hours for all 118

**Files to modify**:
- `anno/src/eval/dataset_registry.rs` - Update `url:` fields

#### 3. Expand S3 Cache (223 datasets)

**Current**: 165/228 loadable datasets cached (40%)

**Approach**:
- Download remaining datasets to S3
- Prioritize frequently-used datasets first
- Use `scripts/download_extended_datasets.py` or similar

**Estimated effort**: 
- Depends on dataset sizes and download speeds
- Can be automated with batch scripts

### Medium Priority

#### 4. Add Example Snippets

**Current**: 18/451 datasets (4%) have examples

**Target**: 135+ datasets (30%)

**Approach**:
- Add `example:` field to registry entries
- Focus on datasets with unusual formats first
- Extract examples from actual data files

**Files to modify**:
- `anno/src/eval/dataset_registry.rs` - Add `example:` fields

#### 5. Add Expected F1 Scores

**Current**: 21/451 datasets (5%) have expected F1

**Target**: 225+ datasets (50%)

**Approach**:
- Research published papers for baseline scores
- Add `expected_f1:` field to registry entries
- Focus on major benchmarks first (CoNLL, OntoNotes, etc.)

**Files to modify**:
- `anno/src/eval/dataset_registry.rs` - Add `expected_f1:` fields

### Low Priority

#### 6. Add SHA256 Hashes

**Current**: Missing for most datasets

**Approach**:
- Calculate hashes for downloaded datasets
- Add `sha256:` field to registry entries
- Use for integrity verification

#### 7. Add Temporal Metadata

**Current**: Missing for historical datasets

**Approach**:
- Add temporal metadata for time-sensitive datasets
- Enable temporal stratification in evaluation

## Quick Wins

These can be done quickly to improve coverage:

1. **Add CoNLL-format datasets** (~50-100 datasets)
   - Many datasets in registry use CoNLL format
   - Just need to add to `parse_plan()` match statement
   - Estimated: 1-2 hours

2. **Add JSONL-format datasets** (~30-50 datasets)
   - Similar to CoNLL, just add to match statement
   - Estimated: 1 hour

3. **Fix obvious URL issues** (~20-30 datasets)
   - GitHub repos that moved (update URLs)
   - HuggingFace datasets (add HF IDs)
   - Estimated: 2-3 hours

## Architecture Notes

The loader uses a smart two-tier system:

1. **Registry hints** (`registry_hint_plan()`)
   - Auto-detects format from registry metadata
   - Checks `format:` field and other metadata
   - Returns `None` if not confident

2. **Explicit matches** (`parse_plan()`)
   - Fallback for special cases
   - Handles datasets with custom formats
   - Currently ~228 datasets mapped

**To add a dataset**:
- If it uses a common format (CoNLL, JSONL, TSV), add format metadata to registry
- If it needs special handling, add explicit match in `parse_plan()`

## Next Steps

1. **Immediate**: Add common-format datasets to loader (quick wins)
2. **Short-term**: Fix broken URLs for top 50 most-used datasets
3. **Medium-term**: Expand loader coverage to 80%+
4. **Long-term**: Add examples and expected F1 for all major benchmarks

## Files Modified

- ✅ `docs/DATASETS.md` - Updated to reflect full registry
- ✅ `docs/notes/DATASET_REVIEW.md` - Comprehensive review document
- ✅ `docs/notes/DATASET_FIXES_SUMMARY.md` - This file

## Validation

- ✅ Documentation builds without errors
- ✅ URL validation completed (251 valid, 118 broken)
- ✅ Review document created with accurate statistics

