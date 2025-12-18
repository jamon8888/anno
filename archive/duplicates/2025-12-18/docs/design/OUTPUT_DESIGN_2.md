# Output Design

Design philosophy, implementation status, and guidelines for CLI output formatting.

## Philosophy

### What's Actually Useful vs. What's Noise

#### ❌ BAD: Character Spans
**Why:** `[0:11)` tells me nothing useful. I don't care where in the text the entity is - I care what it is.

**Verdict:** Spans should ONLY appear in JSON/TSV formats for programmatic use. Never in human-readable output.

#### ❌ BAD: Signal/Track IDs
**Why:** `S0`, `T1`, `I2` are implementation details. Users don't think in terms of "signal 0" or "track 1".

**Verdict:** Show entity text and relationships, not internal IDs.

#### ✅ GOOD: Entity Text First
**Why:** This is what users actually want to see. Everything else is metadata.

**Verdict:** Entity text is primary. Types and counts are secondary.

#### ✅ GOOD: Confidence When Useful
**Why:** Low confidence (< 0.5) tells me the extraction might be wrong. High confidence (> 0.8) is noise.

**Verdict:** Progressive disclosure - only show what's needed at each level.

#### ✅ GOOD: Context Snippets
**Why:** Helps verify correctness. "Is 'Jobs' really a person or is it 'Apple Jobs'?"

**Verdict:** Context is valuable for debugging, but only at verbose levels.

## Output Levels

### Level 0 (Default): Entity List Only

**What to show:**
- Entity types and counts
- Entity text (the actual mentions)

**What NOT to show:**
- Spans
- Confidence
- Context
- Statistics
- Coreference

**Rationale:** Users want to quickly scan "what did we find?" Not "how confident are we?" or "where in the text?"

**Example:**
```
ORG:1 "Microsoft Corporation"
PER:2 "Bill Gates" "Paul Allen"
LOC:1 "Redmond, Washington"
```

**Status:** ✅ Implemented

### Level 1 (-v): Debugging

**What to show:**
- Everything from Level 0
- Confidence scores (to filter low-confidence)
- Context snippets (to verify correctness)
- Negation/quantifiers if present

**What NOT to show:**
- Coreference (too early)
- Statistics (not needed yet)

**Rationale:** When debugging, I want to know "is this extraction correct?" Confidence and context answer that.

**Example:**
```
ORG:1
  "Microsoft Corporation" (0.85)
    Microsoft Corporation was founded by...
PER:2
  "Bill Gates" (0.75)
    ...was founded by Bill Gates and Paul...
  "Paul Allen" (0.75)
    ...Bill Gates and Paul Allen in 1975...
```

**Status:** ✅ Implemented (context snippets use 30 chars)

### Level 2 (-vv): Analysis

**What to show:**
- Everything from Level 1
- Coreference chains (which mentions refer to same entity)
- Statistics (quality metrics)

**What NOT to show:**
- KB links (too detailed)
- Full metadata (too verbose)

**Rationale:** When analyzing, I want to understand relationships and quality.

**Example:**
```
ORG:1
  "Microsoft Corporation" (0.85)
    Microsoft Corporation was founded by...
PER:2
  "Bill Gates" (0.75)
    ...was founded by Bill Gates and Paul...
  "Paul Allen" (0.75)
    ...Bill Gates and Paul Allen in 1975...

Coreference:
  "microsoft corporation" [ORG] → "Microsoft Corporation"

stats: 3 entities, 3 tracks, 0 identities, avg confidence 0.78
```

**Status:** ✅ Implemented (single-mention tracks are hidden)

### Level 3 (-vvv): Deep Dive

**What to show:**
- Everything from Level 2
- KB links (identities)
- Full metadata (timing, model, document ID)
- Annotated text (for verification)

**Rationale:** For programmatic use or deep analysis.

**Status:** ✅ Implemented

## Implementation Status

### ✅ Completed Fixes

1. **Removed Spans from Default Output**
   - Level 0 output now shows only entity types, counts, and text
   - Spans available in JSON/TSV formats only

2. **Simplified Coreference Display**
   - Removed track IDs (T0, T1) and signal IDs (S0, S1)
   - Shows only entity text and relationships
   - Single mentions (no coreference) are not shown

3. **Improved Statistics Display**
   - Changed from cryptic `sig=4 trk=3` to readable format
   - Format: `stats: 4 entities, 3 tracks, 0 identities, avg confidence 0.70`

4. **Context Snippets**
   - Increased from 15 to 30 characters for better context
   - Shows surrounding text to verify correctness

### ⚠️ Known Issues

1. **Verbose Flag Order**
   - `-v` flag must come **before** positional text arguments
   - Working: `anno extract -v "text"` ✅
   - Not working: `anno extract "text" -v` ❌
   - Solution: Use `--verbose` or `--text` flag when using positional args

2. **Documentation**
   - Flag ordering needs to be documented in help text

## Batch Output

### Design Principles

**Batch processing should default to Level 0**, even if `-v` is passed. Verbose should be per-document opt-in.

**Good batch output:**
```
Document: doc1
PER:2 "Barack Obama" "Obama"

Document: doc2
PER:1 "Steve Jobs"
```

**Summary statistics** should be added:
```
Processed 100 documents:
  Total entities: 450
  Entity types: PER:200, ORG:150, LOC:100
  Avg confidence: 0.72
  Documents with entities: 95/100
```

## Empty Results

**Bad:** Silent failure
```
(no entities)
```

**Good:** Actionable message
```
(no entities found - try -v for debugging or --model gliner for zero-shot NER)
```

## Error Messages

**Bad:** Cryptic errors
```
Failed to parse
```

**Good:** Actionable errors
```
Error: No entities found in text "The quick brown fox"
  Suggestion: Try --model gliner for zero-shot NER, or check if text contains named entities
```

## Testing Results

✅ **Small documents:** Clean, scannable output  
✅ **Large directories:** Batch processing works well  
✅ **URLs:** Handles web content extraction  
✅ **Coreference:** Shows relationships clearly  
✅ **Real datasets:** Tested on WNUT-17, WikiGold

## Key Principles

1. **Entity text is primary** - spans are secondary/optional
2. **Group by type** - easier to scan
3. **Show confidence when useful** - not always needed
4. **Context is valuable** - helps verify correctness
5. **Progressive disclosure** - each level adds value
6. **No redundant information** - coreference should add value, not repeat

## Future Improvements

- Confidence thresholds (only show if < 0.5 or > 0.9)
- Entity type distribution in batch output
- Better error messages with suggestions
- Summary stats for batch processing

