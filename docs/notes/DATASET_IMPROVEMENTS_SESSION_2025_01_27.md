# Dataset System Improvements Session - 2025-01-27

## Summary

Deep review and improvements to the dataset registry, loader architecture, and documentation. Focus on URL fixes, expected F1 scores, code documentation, and research.

## Completed Work

### 1. URL Fixes ✅

**AgCNER**:
- **Before**: `https://github.com/AgCNER/AgCNER` (404 error)
- **After**: `https://springernature.figshare.com/collections/AgCNER_the_First_Large-Scale_Chinese_Named_Entity_Recognition_Dataset_for_Agricultural_Diseases_and_Pests/6807873`
- **Source**: Found via research - dataset published in Nature Scientific Data 2024
- **Status**: ✅ Updated

**EnzChemRED**:
- **Before**: `https://github.com/EnzChemRED/EnzChemRED` (404 error)
- **After**: `https://github.com/ncbi-nlp/EnzChemRED`
- **Source**: Found via research - correct GitHub organization is `ncbi-nlp`
- **Status**: ✅ Updated

### 2. Expected F1 Scores ✅

Added expected F1 scores for 14+ additional benchmarks:

**Coreference**:
- GAP: 85.0% (BERT baseline)
- PreCo: 82.0% (BERT baseline)
- LitBank: 75.0% (literary text, harder domain)

**Relation Extraction**:
- DocRED: 58.0% (document-level, BERT baseline)
- ReTACRED: 72.0% (sentence-level, BERT baseline)
- EnzChemRED: 86.0% (BioBERT baseline, domain-specific)
- SciERC: 68.0% (SciBERT baseline)

**Biomedical**:
- CRAFT: 82.0% (BioBERT)
- BC2GMFull: 84.0%

**Multilingual**:
- CoNLL2002: 88.0% (mBERT baseline)
- GermEval2014: 86.0% (BERT-base-german)

**Domain-Specific**:
- LegalNER: 83.0% (specialized BERT)
- FinNER: 81.0% (multilingual BERT)
- AgCNER: 94.0% (BERT-BiLSTM-CRF, Yao et al. 2024)

**Social Media**:
- TweetNER7: 72.0% (domain-adapted)

**Historical/Classical**:
- SanskritUD: 68.0%
- ClassicalChineseUD: 72.0%

**Total expected F1 coverage**: ~35/451 (8%, up from 5%)

### 3. Code Documentation ✅

Added comprehensive comments to `loader.rs`:

**Architecture Documentation**:
- Two-tier detection system explained
- Auto-detection vs explicit matches rationale
- Guidelines for adding new datasets
- Section headers for major format categories

**Key Sections Documented**:
- `parse_plan()`: Priority order, when to use explicit matches
- `registry_hint_plan()`: Auto-detection logic, edge cases
- Format-specific sections: CoNLL, JSONL, RE, coref

### 4. Deep Review Document ✅

Created `docs/notes/DEEP_REVIEW_2025_01_27.md` with:
- Auto-detection logic analysis
- Multi-task dataset handling
- URL health analysis
- Format detection edge cases
- Architecture strengths
- Opportunities for improvement
- Recommendations prioritized

### 5. Research Findings

**AgCNER**:
- Available on figshare (primary source)
- GitHub mirror: `guojson/AgCNER`
- Paper: Nature Scientific Data 2024
- Performance: BiLSTM-CRF 93.58%, BERT-BiLSTM-CRF 94.14%, AgBERT-BiLSTM-CRF 94.34%

**EnzChemRED**:
- Correct GitHub: `ncbi-nlp/EnzChemRED`
- Paper: Scientific Data 2024
- Performance: NER 86.30%, RE 86.66% (BioBERT baseline)

**HuggingFace Alternatives**:
- Research identified 44 datasets with broken GitHub URLs
- Many may have HuggingFace alternatives
- Systematic search needed for remaining datasets

## Current Status

### Loader Coverage
- **Loadable datasets**: 240/451 (53%)
- **Auto-detected**: ~150+ via `registry_hint_plan()`
- **Explicit matches**: ~90 in `parse_plan()`

### Expected F1 Coverage
- **With expected F1**: ~35/451 (8%, up from 5%)
- **Target**: 225+ (50%)

### Example Coverage
- **With examples**: ~18/451 (4%)
- **Target**: 135+ (30%)

### URL Health
- **Valid URLs**: 251/451 (56%)
- **Broken URLs**: 118/451 (26%) - 2 fixed this session
- **Paper-only URLs**: 34/451 (8%)
- **No URL**: 47/451 (10%)

## Remaining Work

### High Priority

1. **Fix broken URLs** (116 remaining)
   - Research HuggingFace alternatives for 42 GitHub-based datasets
   - Update registry with working URLs
   - Mark permanently unavailable as `ContactAuthors` or `Registration`

2. **Add more loader implementations** (211 datasets remaining)
   - Many can use existing parsers (CoNLL, JSONL, TSV)
   - Focus on datasets with clear format metadata and public URLs

3. **Add expected F1 scores** (target: 225+ datasets)
   - Research published papers for baseline scores
   - Focus on major benchmarks first
   - Use BERT/BioBERT/mBERT baselines

4. **Add example snippets** (target: 135+ datasets)
   - Extract examples from actual data files
   - Focus on unusual formats first

### Medium Priority

5. **Audit format metadata** (50-100 datasets could benefit)
   - Check all datasets with valid URLs for missing `format:` field
   - Enables auto-detection, reduces explicit matches

6. **Expand S3 cache** (223 datasets)
   - Download remaining datasets to S3
   - Prioritize frequently-used datasets

## Architecture Insights

### Two-Tier Detection System

The loader uses a smart two-tier approach:

1. **Registry hints** (`registry_hint_plan()`)
   - Auto-detects format from registry metadata
   - Conservative: returns `None` if ambiguous
   - Handles single-task datasets well

2. **Explicit matches** (`parse_plan()`)
   - Fallback for special cases
   - Handles multi-task datasets
   - Custom format parsers

### Multi-Task Handling

- RE datasets with NER as secondary task: Handled correctly (e.g., SciERCNER)
- Coref datasets with NER annotations: Handled via CoNLL parser
- Current approach is correct: conservative auto-detection prevents errors

### Format Detection Edge Cases

- CADEC/ShARe: Hybrid format (standoff + JSON) → explicit match
- GoogleRE: Custom JSONL format → explicit match
- TweetNER7: Integer tags (not standard BIO) → explicit match
- HF API Response: Uses datasets-server API → explicit list

## Files Modified

- `anno/src/eval/dataset_registry.rs`: 
  - Updated AgCNER URL (figshare)
  - Updated EnzChemRED URL (correct GitHub)
  - Added 14+ expected F1 scores
  
- `anno/src/eval/loader.rs`: 
  - Added comprehensive architecture comments
  - Section headers for format categories
  - Guidelines for adding datasets

- `docs/notes/DEEP_REVIEW_2025_01_27.md`: 
  - Comprehensive deep review document

- `docs/notes/DATASET_IMPROVEMENTS_SESSION_2025_01_27.md`: 
  - This file

## Next Session Priorities

1. **Research HuggingFace alternatives** for 42 remaining broken GitHub URLs
2. **Add more expected F1 scores** from published papers (focus on major benchmarks)
3. **Audit format metadata** for datasets with valid URLs
4. **Add example snippets** to key datasets (focus on unusual formats)

## Research Notes

### AgCNER (Yao et al. 2024)
- **Source**: Nature Scientific Data 2024
- **URL**: figshare collection (primary), GitHub mirror available
- **Performance**: BERT-BiLSTM-CRF 94.14% F1
- **Size**: 66k samples, ~207k entities, 3.9M characters
- **Entity types**: 13 categories (CROP, DISEASE, PEST, etc.)

### EnzChemRED (Lai et al. 2024)
- **Source**: Scientific Data 2024
- **URL**: GitHub `ncbi-nlp/EnzChemRED`
- **Performance**: NER 86.30%, RE 86.66% (BioBERT baseline)
- **Size**: 1,210 expert-curated PubMed abstracts
- **Tasks**: NER + RE (enzyme chemistry)

## Conclusion

The dataset system architecture is solid with clear separation of concerns. The main opportunities are:
- Completing format metadata for datasets with valid URLs
- Researching alternatives for broken URLs
- Adding expected F1 scores incrementally
- Improving documentation for multi-task datasets

The conservative auto-detection approach is correct and prevents incorrect parser selection. The explicit match fallback ensures correctness for special cases.

