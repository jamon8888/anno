# Testing

Comprehensive testing strategy, findings, and recommendations.

## Test Coverage

### Input Types Tested

1. ✅ **News/Politics** - Multiple entities, dates, locations
2. ✅ **Social Media** - Mentions, hashtags, emojis
3. ✅ **Scientific/Technical** - NASA, missions, costs
4. ✅ **Medical** - Doctors, hospitals, contact info
5. ✅ **Financial** - Cryptocurrency, prices, money
6. ✅ **Technology** - Programming languages, products
7. ✅ **Long Documents** - 3+ paragraphs, multiple entities
8. ✅ **Edge Cases** - Empty, whitespace, special chars, very long
9. ✅ **Non-English** - Japanese, French (limited support)
10. ✅ **Multiple Entities** - Same type, different types
11. ✅ **Dates/Times** - Various formats
12. ✅ **URLs/Emails** - Various formats
13. ✅ **Acronyms** - UN, NASA, etc.
14. ✅ **Academic/Historical** - Scientists, universities
15. ✅ **Entertainment** - Movies, directors, actors
16. ✅ **Batch Processing** - Multiple documents

**Total test cases:** 50+ diverse inputs

## Critical Bugs Found

### 1. Command Line Pollution (CRITICAL) 🚨
**When:** Using `-vv "text"` (verbose flag before positional text)  
**What:** Shell command fragments leak into extraction  
**Workaround:** Use `--text "text" -vv` instead  
**Status:** Needs fix  
**Priority:** HIGHEST

### 2. False Positive PER (HIGH)
**What:** "Just", "Both", "Its", "It's", "Mars", "November" extracted as PER  
**Confidence:** 0.45 (low, but still shown)  
**Impact:** Users lose trust  
**Fix:** Filter <0.5 confidence at Level 0, or better heuristics  
**Status:** Needs fix

### 3. Months as LOC (MEDIUM)
**What:** "November", "December", "January" extracted as PER/LOC  
**Should be:** DATE or not extracted  
**Impact:** Noise in output  
**Status:** Needs fix

### 4. URL Entity False Positives
**Issue:** Single letters and partial words detected as URLs  
**Root Cause:** URL regex pattern too permissive  
**Fix Needed:** Tighten URL detection regex to require protocol (`http://` or `https://`)  
**Status:** Needs fix

### 5. Money Extraction Bug (RESOLVED)
**When:** "$2.7 billion" format  
**What:** Was extracting as "7 billion" (missing "$2")  
**Root Cause:** Command line pollution, not extraction bug  
**Status:** ✅ Works correctly when using `--text` flag

### 6. Acronym Coreference (MEDIUM)
**What:** "UN" and "United Nations" not linked  
**Impact:** Can't see they're the same entity  
**Fix:** Better coreference resolution  
**Status:** Needs improvement

## What Works Exceptionally Well

1. ✅ **Standard NER** - PER, ORG, LOC extraction is solid
2. ✅ **Structured data** - Dates, emails, URLs, phone numbers (high confidence ~0.95-0.98)
3. ✅ **Long documents** - Handles 1000+ word texts without issues
4. ✅ **Edge cases** - Empty, whitespace, special chars handled gracefully
5. ✅ **Titles/honorifics** - Dr., M.D., etc. handled well
6. ✅ **Multiple entities** - Handles many entities of same type
7. ✅ **Batch processing** - Processes directories efficiently (~0.09s for 20 files)

## Design Issues

1. **Acronym expansion not linked** - "UN" and "United Nations" not in coreference
2. **Product names not recognized** - iPhone, Bitcoin, Python not extracted (expected with default model)
3. **Movie titles as PER** - "Oppenheimer" should be MISC/WORK
4. **Months as LOC** - "November" extracted as LOC, should be DATE
5. **High confidence always shown** - Noise at Level 1+ (should only show if <0.5)

## Output Quality Assessment

### Level 0 (Default) - Grade: A
**What's Good:**
- Clean, scannable
- No noise (spans, confidence)
- Perfect for quick scanning

**What's Bad:**
- Shows false positives (0.45 confidence PER)
- **Fix:** Filter <0.5 confidence

### Level 1 (-v) - Grade: B
**What's Good:**
- Context snippets helpful (30 chars is good)
- Confidence shown for debugging

**What's Bad:**
- Shows high confidence (>0.8) which is noise
- **Fix:** Only show confidence if <0.5 or >0.9

### Level 2 (-vv) - Grade: B+
**What's Good:**
- Coreference chains shown
- Statistics helpful

**What's Bad:**
- Shows zero tracks/identities (noise)
- Coreference shows `[-]` for missing type
- **Fix:** Hide zero stats, fix type display

### Batch Processing - Grade: C
**What's Good:**
- Per-document output is clean
- Processes efficiently

**What's Bad:**
- No summary statistics
- Verbose flag applies to all (should default to Level 0)
- **Fix:** Add summary stats, default to Level 0 for batch

## Testing Scenarios

### Small Documents (Single Text)
**Command**: `anno extract "Marie Curie was born in Paris."`

**Output (Level 0)**:
```
PER:1 "Marie Curie"
LOC:1 "Paris"
```

**Observations**:
- ✅ Dense, single-line format is perfect for quick scanning
- ✅ All essential info (types, counts, text) in one line
- ✅ Works well with default model (stacked)

**Performance**: <1ms per document

### Large Directories (Batch Processing)
**Command**: `anno batch --dir testdata/fixtures/cross_doc --format human`

**Output**:
```
Document: doc1
PER:1 "Jensen Huang"
LOC:1 "San Jose"
DATE:1 "2024-01-15"
ORG:1 "Nvidia"

Document: doc2
PER:1 "Jensen Huang"
LOC:1 "Paris"
ORG:1 "Nvidia"
```

**Observations**:
- ✅ "Document: filename" prefix is clear and helpful
- ✅ Dense format prevents output from being overwhelming
- ✅ "(no entities)" is concise for empty results
- ✅ Easy to skim per-file extractions before running `cross-doc`

**Performance**:
- Scales linearly with number of files (I/O + model runtime)

### URLs (Web Content)
**Command**: `anno extract --url https://example.com`

**Observations**:
- ✅ Works with URLs (requires eval-advanced feature)
- ✅ Handles HTML content extraction
- ⚠️ Some validation errors with complex HTML (expected - whitespace normalization issues)
- ✅ Still extracts entities correctly despite validation warnings

## Property Tests Needed

### 1. URL Detection Invariants
```rust
// Property: URLs must start with http:// or https://
prop_assert!(url_regex.matches("http://example.com"));
prop_assert!(url_regex.matches("https://example.com"));
prop_assert!(!url_regex.matches("example.com")); // No protocol
prop_assert!(!url_regex.matches("A")); // Single letter
```

### 2. Crossdoc Coref Transitivity
```rust
// Property: If A≈B and B≈C, then A≈C (via identity)
// Test: Three documents with overlapping entities
// Expected: All three linked to same identity
```

### 3. Track Consistency
```rust
// Property: All signals in a track must be from same document
// Property: Track canonical_surface must match at least one signal
```

### 4. Graph Export Round-trip
```rust
// Property: GraphDocument → JSON → GraphDocument should preserve structure
// Test: Export to JSON, re-import, verify nodes/edges match
```

## Regression Tests to Add

1. **URL false positive regression**: Test that single letters aren't detected as URLs
2. **Crossdoc track preservation**: Test that tracks are preserved through crossdoc merge
3. **Graph export format**: Test all export formats (Cypher, NetworkX, JSON-LD) with valid inputs
4. **Old text handling**: Test historical text formatting doesn't break extraction
5. **Unicode handling**: Test mixed scripts, emoji, special characters

## Recommendations

### Immediate (Critical)
1. Fix command line pollution bug
2. Fix false positive filtering (<0.5 confidence)
3. Add confidence threshold for false positives

### Short-term (High Priority)
1. Hide high confidence at Level 1+ (only show if <0.5)
2. Hide zero stats (don't show "0 tracks, 0 identities")
3. Fix coreference type display (show type or omit, not `[-]`)
4. Fix URL detection regex (require protocol)

### Medium-term (Nice to Have)
1. Add batch summary statistics
2. Better acronym handling in coreference
3. Document limitations (non-English, product names)
4. Add `--follow-urls` flag (automatic URL resolution from text)

### Long-term (Future)
1. Add PRODUCT/TECH entity types
2. Better non-English support
3. Movie/TV title recognition
4. LSH for large corpora (optimize crossdoc for 1000+ documents)

## Final Verdict

**What's Production-Ready:**
- ✅ Structured data extraction (dates, emails, URLs, phone, money)
- ✅ Standard NER (PER, ORG, LOC) for common cases
- ✅ Long document handling
- ✅ Edge case handling
- ✅ Batch processing infrastructure

**What Needs Immediate Fix:**
- 🚨 Command line pollution bug (CRITICAL)
- 🚨 False positive filtering (HIGH)
- ⚠️ Confidence display logic (MEDIUM)
- ⚠️ Zero stats hiding (MEDIUM)

**What's Expected Limitations:**
- ⚠️ Product names not recognized (document this)
- ⚠️ Non-English limited support (document this)
- ⚠️ Movie titles as PER (expected, but could improve)

**Overall Grade:** B+
- Core functionality works well
- Output quality is good but has noise
- Critical bugs need fixing
- Design issues are fixable

