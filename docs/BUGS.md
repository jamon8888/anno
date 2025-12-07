# Bugs and Issues

Known bugs, fixes applied, and issues requiring attention.

## Fixed Bugs

### 1. Deadlock Bug (CRITICAL) ✅
**Location**: `src/eval/task_evaluator.rs:553-558`

**Issue**: Potential deadlock when handling mutex poisoning. The code tried to lock the mutex again in the else branch.

**Fix**: Simplified to use `unwrap_or_else` directly without nested if/else:
```rust
// Before (buggy):
if let Ok(mut cache) = self.per_example_scores_cache.lock() {
    *cache = None;
} else {
    drop(self.per_example_scores_cache.lock().unwrap_or_else(|e| e.into_inner()));
}

// After (fixed):
let mut cache = self.per_example_scores_cache.lock().unwrap_or_else(|e| e.into_inner());
*cache = None;
```

### 2. Variance Calculation Bug (STATISTICAL ERROR) ✅
**Location**: Multiple locations in `src/eval/task_evaluator.rs`

**Issue**: Using population variance (dividing by n) instead of sample variance (dividing by n-1, Bessel's correction). This causes biased standard deviation estimates.

**Fixed Locations**:
1. `compute_confidence_intervals` (line ~2312)
2. `compute_confidence_intervals_from_scores` (line ~2591)
3. `compute_temporal_stratification` - pre_cutoff (line ~2539)
4. `compute_temporal_stratification` - post_cutoff (line ~2555)
5. `compute_stratified_metrics_from_scores` - per entity type (line ~2456)

**Fix**: Changed from:
```rust
let variance = scores.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / scores.len() as f64;
```

To:
```rust
let n = scores.len() as f64;
let variance = if n > 1.0 {
    scores.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)
} else {
    0.0
};
```

**Impact**: Confidence intervals are now correctly computed using unbiased sample variance.

### 3. Heuristic NER False Positives ✅
**Location**: `anno/src/backends/heuristic.rs` line 747-750

**Issue**: Single letters (A, B, C) detected as Person entities

**Fix**: Added filter in `classify_minimal` to skip single-letter words

**Impact**: Reduces false positives in heuristic NER output

### 4. Auto-Track Creation in Crossdoc Directory Mode ✅
**Location**: `anno/src/cli/commands/crossdoc.rs` lines 581-600

**Issue**: `anno crossdoc --directory` didn't automatically create tracks (Level 2)

**Fix**: Added call to `resolve_coreference` after signal creation

**Impact**: Improves crossdoc clustering quality by using tracks instead of raw signals

## Critical Issues (Unfixed)

### 1. Command Line Pollution in Output 🚨
**Severity:** HIGH - Breaks output completely

**Problem:** When using `-vv` with quoted text, shell command fragments leak into extraction:
```
PER:10 "Pro Max" "Users/arc/Documents/dev/anno" "Apple's" "Pro Max" "Both" ...
```

**Root Cause:** The text argument includes the shell command itself when passed with `-vv`.

**Workaround:** Use `--text "text" -vv` instead of `-vv "text"`

**Fix Needed:** Sanitize input or fix argument parsing to not include command fragments.

### 2. False Positive Person Entities
**Severity:** MEDIUM - Reduces trust

**Problem:** Common words extracted as PER with low confidence:
- "Just" (0.45)
- "Both" (0.45)
- "Its" (0.45)
- "It's" (0.45)
- "Mars" (0.45) - should be LOC or MISC
- "November" (0.70) - should be DATE, not LOC

**Impact:** Users lose trust when seeing obvious false positives.

**Recommendation:** Filter out low-confidence (<0.5) entities at Level 0, or add better heuristics.

### 3. Duplicate Entity Extraction
**Severity:** LOW - Noise

**Problem:** Same entity extracted multiple times in verbose output:
```
EMAIL:2 "info@example.com" "info@example.com"
DATE:2 "2024-12-25" "2024-12-25"
```

**Root Cause:** Likely command line pollution or text being processed twice.

## Design Issues

### 1. Acronym Expansion Not Linked
**Problem:** "United Nations (UN)" and "UN" are separate entities, not linked in coreference.

**Expected:** Should show coreference: `"united nations" [ORG] → "United Nations (UN)" "UN"`

**Impact:** Users can't see that acronyms refer to the same entity.

### 2. Product Names Not Recognized
**Problem:** Product names not extracted as entities:
- "iPhone 15 Pro Max" → not recognized
- "Bitcoin" → not recognized
- "Python" → not recognized
- "Rust" → extracted as PER (wrong)

**Impact:** Can't extract product/technology mentions.

**Note:** Expected with default model, but should be documented.

### 3. Movie/TV Titles Not Recognized
**Problem:** "Oppenheimer" extracted as PER, not as work title.

**Expected:** Should be MISC or WORK, not PER.

### 4. Low Confidence Entities Always Shown
**Problem:** Level 1+ shows ALL confidence scores, even high ones (>0.8) which are noise.

**Recommendation:** Only show confidence if <0.5 (suspiciously low) or if explicitly requested.

### 5. Non-English Text Limited Support
**Problem:** Japanese and French text have limited extraction.

**Note:** This is expected with default model, but should be documented.

## Medium Priority Issues

### 1. Crossdoc Directory Mode Doesn't Create Tracks
**Status:** ✅ Fixed (see Fixed Bugs #4)

### 2. Graph Export Format Documentation
**Issue:** `GraphDocument` expects string IDs, but users might use integers
- Error: `invalid type: integer `1`, expected a string`

**Fix Needed:** Document correct format, add validation/auto-conversion

**Location:** `anno-core/src/graph.rs`

### 3. Abstract Anaphora Not Accessible
**Issue:** Abstract anaphora requires `discourse` feature, but no clear CLI flag
- `--coref` flag exists but unclear if it uses `DiscourseAwareResolver`
- No `--abstract` flag

**Fix Needed:** Add `--abstract` flag or clarify `--coref` behavior

**Location:** `anno/src/cli/commands/pipeline.rs`, `anno/src/cli/commands/extract.rs`

### 4. Relation Extraction Not Exposed
**Issue:** GLiNER2 supports `RelationExtractor` trait, but no CLI flag found
- Relations might be extracted but not shown in output
- No `--relation` or `--relations` flag

**Fix Needed:** Add relation extraction flag, or document how to access relations

**Location:** `anno/src/cli/commands/extract.rs`, `anno/src/cli/commands/pipeline.rs`

## Low Priority Issues

### 1. URL Following Not Automatic
**Issue:** URLs mentioned in text are NOT automatically resolved
- Must manually: extract → resolve URLs → crossdoc

**Fix:** Add `--follow-urls` flag (see `docs/URL_REFERENCE_SUPPORT.md`)

### 2. Crossdoc Performance for Large Corpora
**Issue:** O(n²) track comparisons for large document sets
- No LSH (Locality-Sensitive Hashing) optimization

**Fix:** Add LSH for 100+ document sets (future enhancement)

### 3. Old Text Entity Confusion
**Issue:** Historical text (1920s) produces low-quality extractions
- "Peace Conference" → PER (should be ORG/EVENT)
- "M" → PER (should be ignored or context-aware)
- "League" → PER (should be ORG)

**Root Cause:** Heuristic NER relies on capitalization patterns that don't match historical formatting

**Fix:** Use GLiNER for historical text, or add historical text preprocessing

## Property Test Issues

### property_crossdoc_transitivity
**Problem:** Tests are failing with `unwrap()` on `None` when trying to access tracks after `resolve_inter_doc_coref`.

**Root Cause:** The tests use hardcoded track IDs (1, 2) but `add_track()` auto-increments from 0. Fixed by using actual track IDs returned from `add_track()`.

However, tests still fail because `get_track()` returns `None` after resolution. Possible causes:
1. Tracks are being removed or modified during `resolve_inter_doc_coref`
2. Track IDs are changing during resolution
3. Documents are being modified in a way that removes tracks

**Investigation Needed:**
- Check if `resolve_inter_doc_coref` modifies track IDs
- Verify that tracks still exist after resolution by iterating through all tracks
- Check if `get_document()` returns the correct document reference
- Consider using `doc.tracks()` iterator instead of `get_track(id)`

**Status:** In progress - need to investigate corpus structure after resolution.

## Verification

All fixes verified:
- ✅ Deadlock warning eliminated (clippy clean)
- ✅ All variance calculations use sample variance (n-1)
- ✅ Edge case handling (n=0, n=1) added
- ✅ Tests pass
- ✅ Examples run successfully

## Remaining Warnings

Some `unwrap()` calls remain but are intentional (e.g., in test code, or where we've already checked for None/Err). These are acceptable for now but could be improved with better error handling in the future.

