# Deep Review of Dataset System - 2025-01-27

## Executive Summary

Comprehensive review of the dataset registry, loader architecture, and opportunities for improvement. Focus areas: auto-detection logic, multi-task handling, URL health, and expected F1 scores.

## Key Findings

### 1. Auto-Detection Logic Analysis

**Current State:**
- Two-tier system: `registry_hint_plan()` (auto-detection) + `parse_plan()` (explicit matches)
- Auto-detection is intentionally conservative (returns `None` if ambiguous)
- Handles single-task datasets well (NER-only, RE-only, coref-only)
- Multi-task datasets require explicit matches

**Edge Cases Identified:**

1. **RE datasets with NER as secondary task** (e.g., SciERCNER)
   - Current: Handled by allowing NER in RE auto-detection (`is_re && !is_coref && !is_event`)
   - Note: Comment says "Also handle RE datasets that might have NER as secondary task"
   - Status: ✅ Working correctly

2. **CoNLL format with CoNLLCoref annotation scheme**
   - Current: Special handling for `format="CoNLL"` + `annotation_scheme="CoNLLCoref"` + `is_coref && !is_ner`
   - Examples: GICoref, qxoRef
   - Status: ✅ Handled explicitly

3. **Coref datasets with NER annotations**
   - Current: Some coref datasets (QxoRef, GICoref, WNUT16) are in explicit CoNLL list
   - Reason: They have NER annotations alongside coref, so CoNLL parser works
   - Status: ✅ Handled correctly

**Potential Improvements:**

1. **Format metadata completeness**: Some datasets with valid URLs might be missing `format:` metadata
   - Impact: Can't use auto-detection, must be in explicit matches
   - Action: Audit datasets with valid URLs but missing format metadata

2. **Task inference**: `tasks_or_inferred()` helps, but some datasets might have incorrect category flags
   - Impact: Auto-detection might fail due to incorrect task inference
   - Action: Verify category flags match actual dataset capabilities

### 2. Multi-Task Dataset Handling

**Current Approach:**
- Auto-detection is conservative: requires single primary task
- Multi-task datasets go to explicit matches
- Examples: SciERCNER (NER+RE), GICoref (NER+coref)

**Architecture Notes:**
- `tasks_or_inferred()` infers tasks from categories if not explicitly set
- Auto-detection checks: `is_ner && !is_coref && !is_re && !is_event` (single task)
- RE datasets allow NER as secondary: `is_re && !is_coref && !is_event` (NER allowed)

**Recommendation:**
- Current approach is correct: multi-task datasets need explicit handling
- Consider adding more examples to registry for multi-task datasets
- Document which parsers can handle multi-task (e.g., DocredJson can handle NER+RE)

### 3. URL Health Analysis

**Current Status:**
- 251 valid URLs (56%)
- 118 broken URLs (26%)
- 34 paper-only URLs (8%)
- 47 no URL (10%)

**Broken URL Categories:**
1. **GitHub repos moved/deleted** (44 datasets identified)
   - Examples: ChineseNestedNER, AgCNER, LongDocNER, MultiBioNERLong
   - Potential: HuggingFace alternatives might exist
   - Action: Research HF alternatives for these datasets

2. **HuggingFace authentication required** (401/403)
   - Examples: BookCorefBamman, MultiCoNER, MultiCoNERv2
   - Current: Marked as `requires_hf_token()`
   - Status: ✅ Handled correctly

3. **SSL certificate errors**
   - Older sites with expired certificates
   - Action: Add mirror URLs or mark as `ContactAuthors`

4. **Timeouts**
   - Slow servers or large files
   - Action: Add retry logic or S3 cache

### 4. Expected F1 Score Research

**Current Coverage:**
- ~31/451 datasets (7%) have expected F1 scores
- Target: 225+ (50%)

**Research Findings:**
- BioBERT performance on biomedical NER: 87.6% F1 (medical domain)
- General BERT: 82.5% precision, 81.0% F1 (medical domain)
- Limited comprehensive benchmark data available in search results

**Recommendation:**
- Focus on major benchmarks first (CoNLL-2003, OntoNotes, GENIA, BC5CDR)
- Use published paper baselines (BERT, BioBERT, mBERT)
- Add scores incrementally as research progresses

### 5. Format Detection Edge Cases

**Known Edge Cases:**

1. **CADEC/ShARe (discontinuous NER)**
   - Current: Explicit match to `CadecHybrid` parser
   - Reason: Hybrid format (standoff + JSON)
   - Status: ✅ Handled correctly

2. **GoogleRE**
   - Current: Explicit match to `GoogleReCorpus` parser
   - Reason: Custom JSONL format (not DocRED-style)
   - Status: ✅ Handled correctly

3. **TweetNER7**
   - Current: Explicit match to `TweetNer7` parser
   - Reason: JSON format with integer tags (not standard BIO)
   - Status: ✅ Handled correctly

4. **HF API Response datasets**
   - Current: Explicit list in `registry_hint_plan()`
   - Examples: GENIA, AnatEM, BC2GM, FewNERD, CrossNER
   - Reason: Use HuggingFace datasets-server API, not direct downloads
   - Status: ✅ Handled correctly

### 6. Architecture Strengths

1. **Two-tier detection system**
   - Auto-detection for common cases (reduces maintenance)
   - Explicit matches for special cases (ensures correctness)
   - Clear separation of concerns

2. **Task inference**
   - `tasks_or_inferred()` bridges explicit tasks and category flags
   - Helps with datasets that don't have explicit `tasks:` field
   - Conservative approach prevents false positives

3. **Format metadata**
   - `format:` field enables auto-detection
   - `annotation_scheme:` field provides additional signal
   - Clear documentation in registry macro

4. **Provenance tracking**
   - `DataSource` enum tracks where data came from
   - S3 cache, local cache, original URL, embedded
   - Useful for debugging and audit trails

### 7. Opportunities for Improvement

**High Priority:**

1. **Add format metadata to datasets with valid URLs**
   - Many datasets have URLs but missing `format:` field
   - Enables auto-detection, reduces explicit matches
   - Estimated: 50-100 datasets could benefit

2. **Research HuggingFace alternatives for broken GitHub URLs**
   - 44 datasets with broken GitHub URLs identified
   - Many might be available on HuggingFace
   - Action: Systematic search and update registry

3. **Add expected F1 scores from published papers**
   - Focus on major benchmarks first
   - Use BERT/BioBERT/mBERT baselines
   - Incremental addition as research progresses

**Medium Priority:**

4. **Improve task inference accuracy**
   - Verify category flags match actual dataset capabilities
   - Add explicit `tasks:` field where category inference is ambiguous
   - Document multi-task datasets clearly

5. **Add example snippets to key datasets**
   - Focus on unusual formats first (CADEC, GoogleRE, TweetNER7)
   - Extract from actual data files
   - Helps users understand dataset structure

6. **Expand S3 cache coverage**
   - Currently 40% of loadable datasets cached
   - Prioritize frequently-used datasets
   - Reduces download time and improves reliability

**Low Priority:**

7. **Add SHA256 hashes for integrity verification**
   - Currently missing for most datasets
   - Useful for detecting corruption or tampering
   - Can be added incrementally

8. **Add temporal metadata for historical datasets**
   - Enables temporal stratification in evaluation
   - Useful for diachronic analysis
   - Low priority but valuable for research

## Recommendations

### Immediate Actions

1. **Audit format metadata**: Check all datasets with valid URLs for missing `format:` field
2. **Research HF alternatives**: Systematic search for 44 broken GitHub URLs
3. **Add expected F1 scores**: Start with major benchmarks (CoNLL-2003, OntoNotes, GENIA)

### Architecture Improvements

1. **Document multi-task handling**: Clear guidelines on when to use explicit matches vs auto-detection
2. **Improve task inference**: Verify category flags and add explicit tasks where needed
3. **Add format validation**: Ensure format metadata is accurate and complete

### Long-term Goals

1. **Increase auto-detection coverage**: Target 80%+ of datasets via auto-detection
2. **Reduce explicit matches**: Shrink `parse_plan()` to exception table only
3. **Comprehensive metadata**: All datasets should have format, tasks, examples, expected F1

## Conclusion

The dataset system architecture is solid with a clear two-tier detection system. The main opportunities are:
- Completing format metadata for datasets with valid URLs
- Researching alternatives for broken URLs
- Adding expected F1 scores incrementally
- Improving documentation for multi-task datasets

The conservative auto-detection approach is correct and prevents incorrect parser selection. The explicit match fallback ensures correctness for special cases.

