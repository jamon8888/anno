# Dataset System Improvements Plan

**Date**: 2025-01-27  
**Status**: In Progress  
**Based on**: Deep Review of Dataset System (2025-01-27)

## Current Status

### URL Health
- **251 valid URLs** (56%)
- **118 broken URLs** (26%) - needs attention
- **34 paper-only URLs** (8%)
- **47 no URL** (10%)

### Format Metadata
- Some datasets with valid URLs missing `format:` metadata
- Impact: Can't use auto-detection, must be in explicit matches
- Action: Audit and add format metadata

### Expected F1 Scores
- **~31/451 datasets** (7%) have expected F1 scores
- Target: **225+ (50%)**
- Focus on major benchmarks first (CoNLL-2003, OntoNotes, GENIA, BC5CDR)

## Improvement Priorities

### 1. Fix Broken URLs (High Priority)

**Categories of broken URLs:**

1. **GitHub repos moved/deleted** (44 datasets)
   - Examples: ChineseNestedNER, AgCNER, LongDocNER, MultiBioNERLong
   - Action: Research HuggingFace alternatives
   - Script: `scripts/find_hf_alternatives.py`

2. **HuggingFace authentication required** (401/403)
   - Examples: BookCorefBamman, MultiCoNER, MultiCoNERv2
   - Status: ✅ Handled correctly (marked as `requires_hf_token()`)
   - Action: Document HF token setup in README

3. **SSL certificate errors**
   - Older sites with expired certificates
   - Action: Add mirror URLs or mark as `ContactAuthors`

4. **Timeouts**
   - Slow servers or large files
   - Status: ✅ Retry logic with exponential backoff implemented
   - Action: Consider S3 cache for large files

**Implementation Plan:**
```rust
// Add to dataset_registry.rs
pub fn find_hf_alternative(&self) -> Option<String> {
    // Search HuggingFace datasets hub for alternative
}

pub fn validate_url(&self) -> UrlStatus {
    // Check URL health (200, 404, timeout, etc.)
}
```

### 2. Enhance Format Metadata

**Missing format metadata prevents auto-detection.**

**Action Items:**
1. Audit datasets with valid URLs but missing `format:` field
2. Add format metadata based on:
   - File extension in URL
   - Known dataset formats (CoNLL, JSONL, etc.)
   - Documentation/paper references

**Script to identify missing formats:**
```bash
# Find datasets with URLs but no format
rg -A 5 "url.*http" anno/src/eval/dataset_registry.rs | \
  grep -B 5 -A 5 "format:" | \
  # Identify missing formats
```

### 3. Improve Task Inference

**Current:** `tasks_or_inferred()` infers tasks from categories if not explicitly set.

**Issues:**
- Some datasets might have incorrect category flags
- Multi-task datasets need explicit handling

**Action Items:**
1. Verify category flags match actual dataset capabilities
2. Add more explicit task declarations for multi-task datasets
3. Document task inference logic

### 4. Add Expected F1 Scores

**Research-based baselines to add:**

| Dataset | Baseline | F1 Score | Source |
|---------|----------|----------|--------|
| CoNLL-2003 | BERT | 92.5 | Devlin et al. 2018 |
| OntoNotes | BERT | 89.5 | Devlin et al. 2018 |
| GENIA | BioBERT | 79.0 | Lee et al. 2019 |
| BC5CDR | BioBERT | 87.0 | Lee et al. 2019 |
| NCBIDisease | BioBERT | 89.0 | Lee et al. 2019 |
| CHEMDNER | Winners | 87.0 | CHEMDNER 2013 |
| JNLPBA | BioNLP | 77.0 | BioNLP |

**Implementation:**
- Add `expected_f1` method to `DatasetId` (already exists)
- Populate with research baselines
- Use for evaluation validation

## Implementation Steps

### Phase 1: URL Health (Week 1)
- [ ] Create script to check all URLs
- [ ] Identify HuggingFace alternatives for broken GitHub URLs
- [ ] Add mirror URLs for SSL/timeout issues
- [ ] Update registry with fixed URLs

### Phase 2: Format Metadata (Week 1-2)
- [ ] Audit datasets with URLs but no format
- [ ] Add format metadata based on URL/file extension
- [ ] Verify auto-detection works for newly-added formats

### Phase 3: Expected F1 Scores (Week 2-3)
- [ ] Research baselines for major benchmarks
- [ ] Add expected_f1 scores to registry
- [ ] Use for evaluation validation

### Phase 4: Task Inference (Week 3)
- [ ] Verify category flags accuracy
- [ ] Add explicit task declarations
- [ ] Document task inference logic

## Tools Needed

1. **URL Health Checker**
   ```rust
   // scripts/check_url_health.rs
   // Check all dataset URLs and report status
   ```

2. **HF Alternative Finder**
   ```python
   # scripts/find_hf_alternatives.py
   # Search HuggingFace for dataset alternatives
   ```

3. **Format Metadata Auditor**
   ```bash
   # scripts/audit_format_metadata.sh
   # Find datasets missing format metadata
   ```

## Success Metrics

- **URL Health**: >80% valid URLs (currently 56%)
- **Format Coverage**: >90% datasets with format metadata
- **Expected F1**: >50% datasets with expected F1 scores
- **Auto-detection**: >80% datasets use auto-detection (currently ~60%)
