# Expected F1 Refactoring Summary

**Date**: 2025-01-27  
**Status**: Complete - All methods updated, no call sites found

## Problem Statement

Expected F1 scores are meaningless without context. A score of 92.5% could be:
- BERT-base on CoNLL-2003 test set (NER)
- BioBERT on BC5CDR dev set (biomedical NER)
- mBERT on MultiCoNER test set (multilingual NER)

The user correctly identified that expected F1 scores are **contingent on**:
1. **Model architecture** (BERT, BioBERT, mBERT, etc.)
2. **Task type** (NER, coreference, relation extraction)
3. **Dataset split** (train/dev/test)
4. **Evaluation metric** (F1, CoNLL F1, Macro F1, etc.)
5. **Citation/reference** (which paper reported this)

## Solution

Created `BaselinePerformance` struct that captures all necessary context:

```rust
pub struct BaselinePerformance {
    pub f1: f32,
    pub precision: Option<f32>,
    pub recall: Option<f32>,
    pub model: String,        // e.g., "BERT-base", "BioBERT"
    pub task: String,         // e.g., "ner", "coref", "re"
    pub split: String,         // e.g., "test", "dev"
    pub metric: String,        // e.g., "F1", "CoNLL F1"
    pub citation: Option<String>,
    pub notes: Option<String>,
}
```

## Implementation

### New Methods

1. **`expected_baseline()`** - Returns `Option<BaselinePerformance>` with full context
2. **`expected_zero_shot_baseline()`** - Returns adjusted baseline for zero-shot models

### Deprecated Methods (Backwards Compatible)

1. **`expected_f1()`** - Now delegates to `expected_baseline().map(|b| b.f1)`
2. **`expected_zero_shot_f1()`** - Now delegates to `expected_zero_shot_baseline().map(|b| b.f1)`

Both deprecated methods are marked with `#[deprecated]` attribute to guide migration.

### Helper Functions

Created `common` module with helper functions for common baseline patterns:

- `bert_base_ner(split, f1)` - BERT-base NER baseline
- `biobert_ner(split, f1)` - BioBERT NER baseline
- `mbert_ner(split, f1)` - mBERT NER baseline
- `bert_coref(split, f1)` - BERT coreference baseline
- `bert_re(split, f1)` - BERT relation extraction baseline

## Migration Status

**Call Sites**: No active call sites found for `expected_f1()` or `expected_zero_shot_f1()` in the codebase. The methods are only used internally within `dataset_registry.rs` for backwards compatibility.

**Usage Pattern**: New code should use:
```rust
if let Some(baseline) = dataset_id.expected_baseline() {
    println!("Expected: {} ({}, {}, {})", 
        baseline.f1, baseline.model, baseline.task, baseline.split);
    if let Some(citation) = &baseline.citation {
        println!("Source: {}", citation);
    }
}
```

## Examples

### Before (Context Lost)
```rust
let expected_f1 = dataset_id.expected_f1(); // Just 92.5, no context
```

### After (Full Context)
```rust
if let Some(baseline) = dataset_id.expected_baseline() {
    // baseline.f1 = 92.5
    // baseline.model = "BERT-base"
    // baseline.task = "ner"
    // baseline.split = "test"
    // baseline.citation = Some("Devlin et al. 2018")
    // baseline.metric = "F1"
}
```

## Benefits

1. **Meaningful Comparisons**: Can now compare "BERT-base on CoNLL-2003 test" vs "BioBERT on BC5CDR test"
2. **Reproducibility**: Citations allow verification of baseline claims
3. **Task Awareness**: Distinguishes NER F1 from coreference CoNLL F1
4. **Split Awareness**: Test set vs dev set performance clearly marked
5. **Future Extensibility**: Easy to add precision/recall, confidence intervals, etc.

## Research Basis

Based on best practices from:
- Perplexity research on F1 score contextualization
- Academic paper reporting standards (model, task, split, metric)
- Evaluation framework requirements (reproducibility, citations)

## Next Steps

1. ✅ Refactoring complete
2. ✅ Deprecated methods maintained for backwards compatibility
3. ⏳ Update evaluation reports to use `expected_baseline()` when displaying baselines
4. ⏳ Add precision/recall to baseline data where available
5. ⏳ Add confidence intervals for baseline scores
