# CLI Dense Output Design

Reference document for `iw`/`ss`/`dmidecode`-style dense output in Anno CLI.

## Current State vs. Research Wisdoms

### Already Implemented

| Wisdom | Status | Location |
|--------|--------|----------|
| Stable content-addressed IDs | ✅ | `e:XXXX` via xxHash in extract.rs |
| Provenance tracking | ✅ | `provenance` in JSON output |
| Span format documented | ✅ | `byte_offsets_exclusive_end` |
| Verbosity levels | ✅ | `-v`, `-vv`, `-vvv` progressive disclosure |
| Context windows | ✅ | `--context-window N`, `--include-sentence` |
| Expected type validation | ✅ | `--expected-types PER,ORG,DATE` |
| Error recovery | ✅ | `--on-error skip|fail|warn` |
| Type mapping/ontology | ✅ | `--type-map schema.tsv` |
| Explain command | ✅ | Feature analysis in explain.rs |
| Exit codes as API | ✅ | Semantic codes 0-6 |
| NIL entity awareness | ⚠️ | Partial in crossdoc |
| Confidence statistics | ✅ | `confidence_stats` in provenance |
| Result hash | ✅ | `xxh3:XXXX` for caching |

### High-Value Gaps

| Wisdom | Priority | Notes |
|--------|----------|-------|
| **Confidence decomposition** | High | Single score hides NER vs NED vs coref uncertainty |
| **Negative space reporting** | High | What wasn't found is often as important |
| **Error cascade analysis** | Medium | How NER errors propagate to coref/NED |
| **Cross-doc merge reasoning** | Medium | Why things merged or didn't |
| **Calibration reporting** | Medium | Confidence vs actual accuracy |
| **Time-aware relations** | Low | Temporal scope on relations |
| **Watch mode delta output** | Low | Changes since last run |

## Dense Output Formats

### Per-Entity Dense Line (Default)

Inspired by `ss -tunlp` and `iw dev info`:

```
PER "Dr. Sarah Chen" @1247:1261 [98%] src:heuristic c:007 doc:paper.txt
```

Components:
- `PER` - entity type (colored)
- `"Dr. Sarah Chen"` - surface form
- `@1247:1261` - span (byte offsets, exclusive end)
- `[98%]` - confidence (omit if ≥95%)
- `src:heuristic` - source backend
- `c:007` - coref cluster (omit for singletons)
- `doc:paper.txt` - source document

### Verbose Entity Block (-vv)

Inspired by `dmidecode -t memory`:

```
Entity e:6c926597 [PERSON]
    surface: "Dr. Sarah Chen"
    span: 1247:1261 (doc:paper.txt, sent:34, tok:3-5)
    confidence: 0.98
    source: heuristic
    
    coref_cluster: c:007 (5 mentions)
        "Dr. Sarah Chen"      sent:34  HEAD
        "she"                 sent:36  PRON
        "Chen"                sent:38  NOMINAL
    
    features:
        capitalization: TitleCase (+0.12)
        context_left: "Dr." (+0.23)
        context_right: "said" (+0.18)
```

### Batch Summary Block

Inspired by `ethtool -S`:

```
Corpus: ./papers/ (247 files, 1.2GB)
    elapsed: 34.2s (7.2 docs/s)
    
Extraction Summary:
    entities: 12,847 (PER:4521 ORG:2341 LOC:1876 MISC:4109)
    coref_clusters: 3,241 (avg:3.9 mentions)
    
Confidence Distribution:
    ≥0.95: 8,234 (64.1%)
    0.90-0.95: 2,341 (18.2%)
    0.80-0.90: 1,567 (12.2%)
    <0.80: 705 (5.5%)
```

### Diagnostic Block (--diagnostics)

Inspired by `dmesg` and `journalctl -o verbose`:

```
Diagnostics (doc:paper.txt)
    warnings: 7 | errors: 0
    
    WARN span_overlap ×3:
        e:0042 "Dr. Sarah Chen" ∩ e:0043 "Sarah"
        resolution: outer (kept e:0042)
        
    WARN low_confidence ×2:
        e:0089 "it" → c:004 (score:0.51) ⚠️ pleonastic?
        e:0234 "bank" → [LOC:0.49 ORG:0.48]
        
    INFO expected_type_missing:
        DATE: 0 found (expected for contracts)
```

## Proposed Enhancements

### 1. Confidence Decomposition (--decompose-confidence)

Expose per-component confidence instead of single score:

```json
{
  "id": "e:6c926597",
  "text": "Chen",
  "confidence": 0.87,
  "confidence_components": {
    "ner": 0.99,
    "type": 0.95,
    "span": 0.98,
    "ned": 0.72,
    "coref": 0.84
  }
}
```

Implementation: Modify `Entity` to carry `Option<ConfidenceBreakdown>`.

### 2. Negative Space Reporting (--gap-analysis)

Report what wasn't found:

```
Gap Analysis (doc:contract.pdf)
    Expected types not found:
        MONEY (unusual for contracts)
        DATE (only 1 found, expected 3+)
    
    Suspicious gaps:
        pages 8-12: 0 entities (OCR failure?)
        section "Payment Terms": 0 MONEY entities
    
    Boilerplate ratio: 34%
```

Implementation: Add `GapAnalysis` struct, compute during extraction.

### 3. Cross-Doc Merge Evidence (--explain-merges)

Show why entities merged or didn't:

```
Cross-Doc Merge: xd:0012 "Sarah Chen"
    merged_from: [doc:001/e:0042, doc:045/e:0089, doc:089/e:0123]
    evidence:
        name_similarity: 0.94
        affiliation_overlap: MIT (docs 001, 045)
        temporal_overlap: 2019-2023
    
    NOT merged with: doc:156/e:0456 "Chen"
        reason: different_affiliation (MIT vs Stanford)
        similarity: 0.34
```

Implementation: Extend `CrossDocCluster` with merge evidence.

### 4. Calibration Report (--calibration)

Compare confidence to actual accuracy:

```
Calibration Report (vs gold.jsonl)
    Threshold 0.90:
        predicted: 4521
        expected_correct: 4069 (0.90 × 4521)
        actual_correct: 3842
        calibration_error: -0.05 (overconfident)
    
    By type:
        PERSON: well-calibrated (±0.02)
        ORG: overconfident by +0.08
        LOC: underconfident by -0.03
```

Implementation: Add to `eval` command.

## Output Format Quick Reference

| Format | Use Case | Provenance |
|--------|----------|------------|
| `human` | Terminal, debugging | Header only |
| `json` | API, single-doc | Full `provenance` object |
| `jsonl` | Streaming, batch | First line `_provenance` |
| `tsv` | Unix tools, pandas | Comment header |
| `grounded` | Pipeline integration | Full metadata |

## Design Principles Recap

1. **Default is boring** - TSV or JSONL, one record per line
2. **Diagnostics to stderr** - Never pollute data stream
3. **Fail loudly on ambiguity** - Don't guess
4. **Version everything** - Model, KB, tool in output
5. **Stable IDs or no IDs** - Content-addressed, not sequential
6. **Support full lifecycle** - Extract → Review → Correct → Improve

## The Meta-Insight

The ideal NER/coref CLI is less like `grep` (stateless text transformation) and more like `git` (stateful, versioned, with porcelain and plumbing layers).

- **Porcelain**: `anno extract`, `anno debug`, `anno cross-doc`
- **Plumbing**: `--format grounded`, `--export`, `--import`

