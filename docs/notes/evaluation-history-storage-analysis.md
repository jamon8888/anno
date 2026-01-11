# Evaluation History Storage: SQLite vs JSONL Analysis

## Current State

### Evaluation Results Storage
- **Format**: JSONL (JSON Lines) - append-only text file
- **Location**: `reports/eval-results.jsonl`
- **Size**: ~440 lines currently
- **Structure**: One JSON object per line with:
  - `timestamp`, `backend`, `dataset`, `seed`
  - `f1`, `precision`, `recall`, `n`, `duration_ms`
  - Optional `error` field

### Prediction Cache
- **Format**: JSONL with in-memory index (HashMap)
- **Location**: `~/.cache/anno/predictions.jsonl`
- **Purpose**: Cache model predictions to avoid re-inference
- **Key**: `sha256(text_hash + backend + version + sorted_labels)`

### Test History (Badness Tracking)
- **Format**: CSV (simple comma-separated)
- **Location**: Configurable via `ANNO_HISTORY_FILE` env var
- **Purpose**: Track "badness" scores for backend/dataset combinations
- **Usage**: Only in `randomized_matrix_ci.rs` test

## SQLite Proposal

### Benefits

1. **Query Capabilities**
   - Time-series queries: "Show F1 trends for backend X over last 30 days"
   - Comparison queries: "Compare all runs of backend A vs B"
   - Aggregation: "Average F1 per dataset", "Best F1 per backend"
   - Filtering: "All runs where F1 > 0.9"

2. **Performance**
   - Indexed lookups (much faster than scanning JSONL)
   - Efficient joins for cross-referencing
   - Better for large datasets (1000s+ of evaluation runs)

3. **Data Integrity**
   - ACID transactions
   - Schema enforcement
   - Foreign key constraints (if we add relationships)

4. **Structured Metadata**
   - Can store relationships (evaluation run → multiple results)
   - Can track evaluation configuration
   - Can link to prediction cache entries

5. **Concurrency**
   - Better handling of concurrent writes
   - SQLite handles locking automatically

### Drawbacks

1. **Complexity**
   - Requires schema design and migrations
   - More complex than append-only JSONL
   - Need to handle database initialization/upgrades

2. **Version Control**
   - Binary format (harder to diff/merge)
   - Can't easily review changes in git
   - JSONL is human-readable and git-friendly

3. **Dependency**
   - Adds `rusqlite` dependency (already transitive, but would become direct)
   - ~500KB binary size increase

4. **Portability**
   - SQLite files are less portable than JSONL
   - JSONL can be processed with standard Unix tools (`jq`, `grep`, `awk`)

5. **Overkill for Current Scale**
   - ~440 evaluation results is small
   - JSONL is perfectly adequate for this scale
   - Prediction cache already has in-memory index

## Recommendation

### For Evaluation Results History: **Hybrid Approach**

**Keep JSONL as primary storage** (append-only, git-friendly, human-readable)
**Add optional SQLite index** for querying (can be regenerated from JSONL)

Benefits:
- Best of both worlds: simple storage + powerful queries
- JSONL remains source of truth
- SQLite can be regenerated if corrupted
- Can query without loading entire JSONL into memory

Implementation:
```rust
// Primary: Append to JSONL (current behavior)
append_to_jsonl(result);

// Optional: Update SQLite index (if enabled)
if sqlite_enabled {
    insert_into_sqlite(result);
}
```

### For Prediction Cache: **Keep JSONL**

**Rationale:**
- Already has in-memory index (O(1) lookups)
- Append-only is perfect for cache (no updates needed)
- Simple, fast, and working well
- SQLite would add complexity without significant benefit

### For Test History (Badness Tracking): **Consider SQLite**

**Rationale:**
- Currently simple CSV (limited querying)
- Would benefit from time-series queries
- Small dataset, but queries are valuable
- Could track trends: "Is backend X getting worse over time?"

## Implementation Plan (If Proceeding)

### Phase 1: Evaluation Results SQLite Index (Optional)

1. Add `rusqlite` as optional dependency (`eval-history` feature)
2. Create schema:
   ```sql
   CREATE TABLE eval_results (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       timestamp TEXT NOT NULL,
       backend TEXT NOT NULL,
       dataset TEXT NOT NULL,
       seed INTEGER NOT NULL,
       f1 REAL,
       precision REAL,
       recall REAL,
       n INTEGER,
       duration_ms REAL,
       error TEXT,
       metadata TEXT,  -- JSON for flexible fields
       created_at TEXT DEFAULT CURRENT_TIMESTAMP
   );
   
   CREATE INDEX idx_backend_dataset ON eval_results(backend, dataset);
   CREATE INDEX idx_timestamp ON eval_results(timestamp);
   CREATE INDEX idx_f1 ON eval_results(f1);
   ```

3. Add CLI commands:
   - `anno history query "SELECT * FROM eval_results WHERE backend='gliner' ORDER BY timestamp DESC LIMIT 10"`
   - `anno history trends --backend gliner --days 30`
   - `anno history compare --backend gliner --vs stacked`

4. Keep JSONL as source of truth, SQLite as queryable index

### Phase 2: Test History SQLite (If Needed)

1. Replace CSV with SQLite for badness tracking
2. Add time-series analysis
3. Track trends and regressions

## Alternative: Enhanced JSONL Tooling

Instead of SQLite, could add better JSONL querying tools:

```rust
// Add to CLI
anno history query --backend gliner --dataset CoNLL2003 --min-f1 0.8
anno history trends --backend gliner --days 30
anno history compare --backend gliner --vs stacked
```

Uses `jq`-like filtering on JSONL files. Simpler, but less powerful than SQL.

## Decision Matrix

| Use Case | Current | SQLite | Enhanced JSONL |
|----------|---------|--------|----------------|
| Evaluation Results | JSONL | ✅ Recommended (optional) | ⚠️ Possible |
| Prediction Cache | JSONL + in-memory | ❌ Not needed | ❌ Not needed |
| Test History | CSV | ✅ Recommended | ⚠️ Possible |
| Query Performance | Slow (scan) | ✅ Fast (indexed) | ⚠️ Medium (scan) |
| Git-friendliness | ✅ Excellent | ❌ Binary | ✅ Excellent |
| Complexity | ✅ Low | ❌ Medium | ✅ Low |

## Final Recommendation

**For now: Keep JSONL, add optional SQLite index for evaluation results**

- JSONL remains source of truth (simple, git-friendly)
- SQLite index enables powerful queries (optional feature)
- Prediction cache stays as-is (already optimal)
- Test history can migrate to SQLite if needed

This gives us:
- ✅ Simple storage (JSONL)
- ✅ Powerful queries (SQLite when needed)
- ✅ No breaking changes
- ✅ Can enable/disable SQLite via feature flag

