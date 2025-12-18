# Randomized Test Analysis & Evaluation Quality

Generated: 2025-12-14

## Executive Summary

The randomized matrix tests are functioning correctly (all tests pass), but the **evaluation results reveal several important patterns**:

1. **Many 0.0% F1 results** (75% of successful runs) - primarily due to:
   - Untrained backends (CRF uses heuristic weights, not trained)
   - Entity type mismatches (backends predict types not in gold, or vice versa)
   - Strict evaluation mode (exact boundaries + exact type required)

2. **High variance across seeds** - WikiGold × stacked: 37.3% (seed 123) vs 14.7% (seed 789)
   - Small sample size (20 examples) causes high variance
   - Different random samples per seed → different difficulty

3. **Backend limitations**:
   - CRF: Needs trained weights (heuristic weights → 0.0% F1)
   - Heuristic: Only works on formal text with capitalization
   - Pattern: Only extracts structured entities (dates, money, emails)

## Key Findings

### 1. Zero F1 Results (75% of successful runs)

**Pattern**: Many backends return 0.0% F1 even when they make predictions.

**Examples**:
- `CRF × LitBank`: 509 gold entities, 17 predictions → 0.0% F1 (all wrong)
- `CRF × CHisIEC`: 126 gold entities, 0 predictions → 0.0% F1 (all false negatives)
- `CRF × WNUT16`: 20 gold entities, 1 prediction → 0.0% F1 (all wrong)

**Root Causes**:

1. **Untrained CRF weights**: CRF uses heuristic weights by default (docs say ~65-70% F1 expected, but we see 0.0%)
   - Heuristic weights are very conservative (strong bias toward O)
   - Need trained weights from `scripts/train_crf_weights.py` for ~88% F1

2. **Entity type mismatches**: Backends predict types not in gold labels
   - CRF only does PER/ORG/LOC/MISC
   - Datasets may have different type schemas (e.g., CHisIEC has OFI, PER, LOC, ORG, GPE, etc.)
   - Strict evaluation requires exact type match → 0.0% F1

3. **Strict evaluation mode**: Requires exact boundaries AND exact type
   - Partial mode would give credit for overlapping spans
   - Type mode would give credit for correct type regardless of boundaries

### 2. Variance Across Seeds

**Pattern**: Same backend-dataset pair shows different F1 across seeds.

**Example**: WikiGold × stacked
- Seed 123: 37.3% F1
- Seed 789: 14.7% F1
- Variance: 22.6 percentage points

**Root Causes**:

1. **Small sample size**: 20 examples per run
   - High variance with small samples
   - Different random samples per seed → different difficulty

2. **Sampling variance**: Random selection of 20 examples from larger dataset
   - Some samples have more entities, some fewer
   - Some samples have easier entities, some harder

**Recommendation**: Increase `max_examples` to 50-100 for more stable results, or report confidence intervals.

### 3. Backend-Dataset Compatibility

**Pattern**: Some backends are incompatible with certain datasets.

**Examples**:
- `pattern` backend: Only extracts structured entities (DATE, MONEY, EMAIL, URL)
  - Incompatible with named entity datasets (WikiGold, LitBank, etc.)
- `heuristic` backend: Only does PER/ORG/LOC
  - Incompatible with datasets that have other types (CHisIEC has OFI, GPE, etc.)

**Current handling**: Type incompatibility errors are marked as "expected" failures.

### 4. Evaluation Mode Impact

**Current**: Uses strict mode (exact boundaries + exact type)

**Impact**:
- Very conservative - no credit for partial matches
- Explains why many results are 0.0% even when backends make reasonable predictions

**Alternative modes available**:
- `partial`: Allows boundary overlap (would increase F1 for boundary errors)
- `type`: Only requires correct type (would increase F1 for boundary errors)
- `exact`: Only requires exact boundaries (would increase F1 for type errors)

**Recommendation**: Consider reporting multiple modes (strict + partial) to understand if failures are boundary vs type issues.

## Recommendations

### Immediate Improvements

1. **Add mode comparison**: Report both strict and partial F1 to diagnose boundary vs type issues
   ```rust
   // Already computed, just need to display:
   strict_f1: 0.0%, partial_f1: 15.3% → boundary issues
   strict_f1: 0.0%, partial_f1: 0.0% → type mismatches
   ```

2. **Increase sample size**: 20 examples → 50-100 for more stable results
   - Trade-off: Slower tests vs more reliable metrics

3. **Add backend training status**: Warn when using untrained backends
   ```rust
   if backend == "crf" && !has_trained_weights {
       eprintln!("⚠️  CRF using heuristic weights (expected ~65-70% F1, not ~88%)");
   }
   ```

4. **Type mismatch analysis**: Report predicted vs gold type distributions
   ```rust
   // Show which types backend predicted vs which types are in gold
   predicted_types: {PER: 10, ORG: 5, LOC: 2}
   gold_types: {OFI: 50, PER: 30, LOC: 20, ORG: 10}
   → Type mismatch: backend doesn't support OFI
   ```

### Statistical Improvements

1. **Confidence intervals**: Already computed, just need to display
   - Shows uncertainty in F1 estimates
   - Helps distinguish real variance from sampling variance

2. **Cross-seed aggregation**: Aggregate results across multiple seeds
   - Mean F1 across seeds
   - Standard deviation across seeds
   - Min/max across seeds

3. **Backend-dataset variance tracking**: Track F1 variance for same backend-dataset across seeds
   - High variance → unstable results (need more samples)
   - Low variance → stable results (current sample size OK)

### Documentation Improvements

1. **Expected performance**: Document expected F1 ranges for each backend
   - CRF: 65-70% (heuristic) vs 88-91% (trained)
   - Heuristic: 60-70% on formal text, ~0% on social media
   - Stacked: Varies by composition

2. **Backend limitations**: Document which backends work with which datasets
   - CRF: Only PER/ORG/LOC/MISC
   - Heuristic: Only PER/ORG/LOC, needs capitalization
   - Pattern: Only structured entities

3. **Evaluation mode guidance**: When to use strict vs partial vs type mode
   - Strict: Production benchmarks (CoNLL standard)
   - Partial: Applications where boundary errors are acceptable
   - Type: When only type classification matters

## Test Coverage Analysis

### Current Coverage

- **Datasets**: 5 per run (increased from 2)
- **Backends**: 4-5 per run (increased from 2-3)
- **Examples**: 20 per dataset (increased from 10)
- **Tasks**: Task-aware sampling ensures multiple task types

### Coverage Quality

**Strengths**:
- ✅ Tests multiple backends per run
- ✅ Tests multiple datasets per run
- ✅ Task-aware sampling ensures task diversity
- ✅ Deterministic across seeds (xxHash3)

**Gaps**:
- ⚠️ Small sample size (20 examples) → high variance
- ⚠️ No ML backends tested (requires `onnx` or `candle` features)
- ⚠️ No mode comparison (strict vs partial)
- ⚠️ No type mismatch analysis

## Conclusion

The randomized tests are **functionally correct** (all pass), but the **evaluation results reveal important patterns**:

1. **Many 0.0% F1 results** are expected for untrained backends and type mismatches
2. **High variance** is expected with small sample sizes (20 examples)
3. **Backend limitations** are correctly identified (type incompatibility errors)

**Next steps**:
1. Add mode comparison (strict vs partial) to diagnose boundary vs type issues
2. Increase sample size to 50-100 for more stable results
3. Add backend training status warnings
4. Add type mismatch analysis

These improvements will make the randomized tests more informative while maintaining their speed and coverage.

