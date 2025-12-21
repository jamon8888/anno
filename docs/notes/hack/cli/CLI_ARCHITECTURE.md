# CLI Architecture: Signal → Track → Identity Hierarchy

## Command Relationships

### Level 1: Signal (Raw Entity Extraction)
**Command**: `extract` (alias: `x`)
- **Purpose**: Extract raw entities from a single document
- **Output**: List of entities with spans, types, confidence
- **Use case**: Quick entity extraction, building blocks for other commands

```bash
anno extract "Marie Curie won the Nobel Prize."
```

### Level 2: Track (Within-Document Coreference)
**Command**: `debug --coref` (alias: `d --coref`)
- **Purpose**: Extract entities + resolve coreference within a single document
- **Output**: Entities grouped into tracks (coreference chains)
- **Use case**: Understanding entity mentions within one document

```bash
anno debug --coref "Barack Obama met Angela Merkel. He praised her."
```

### Level 3: Identity (KB-Linked Entities)
**Command**: `debug --coref --link-kb` (alias: `d --coref --link-kb`)
- **Purpose**: Extract + coreference + link to knowledge base
- **Output**: Entities → Tracks → Identities (with Wikidata IDs)
- **Use case**: Full document understanding with external knowledge

```bash
anno debug --coref --link-kb "Barack Obama met Angela Merkel."
```

### Cross-Document: Clustering Across Multiple Documents
**Command**: `cross-doc` (alias: `cd`)
- **Purpose**: Cluster entities across multiple documents
- **Current implementation**: Directory mode now creates within-document tracks before cross-doc clustering.
- **Output**: Clusters of entities that refer to the same real-world entity
- **Use case**: Finding entity mentions across a document collection

```bash
anno cross-doc /path/to/documents --format tree
```

## Current Limitations

### cross-doc Command
- **Pipeline integration varies by subcommand**: prefer `docs/PIPELINE.md` for current workflows.
- **Identities (KB linking) are not a prerequisite**: cross-doc clustering can run without KB linking.

## Proposed Enhancements

### 1. Pipeline Integration
```bash
# Extract and save single document results
anno extract --file doc1.txt --format grounded --output doc1.json

# Use pre-processed results in cross-doc
anno cross-doc --input-format grounded /path/to/json/files
```

**Benefits**:
- Reuse expensive model runs
- Incremental processing
- Use Level 2/3 data in clustering

### 2. Hierarchy-Aware Clustering
```bash
# Use tracks (Level 2) for better cross-doc clustering
anno cross-doc /path/to/documents --use-tracks

# Use identities (Level 3) for KB-aware clustering
anno cross-doc /path/to/documents --use-identities
```

**Benefits**:
- Better clustering quality (tracks already group mentions)
- KB-aware clustering (same Wikidata ID = same entity)
- Reduced false positives

### 3. Export/Import Format
```bash
# Export extract/debug results
anno extract --export doc.json "text..."
anno debug --coref --export doc.json "text..."

# Import into cross-doc
anno cross-doc --import doc1.json doc2.json doc3.json
```

**Benefits**:
- Workflow flexibility
- Incremental processing
- Combine different processing levels

### 4. Query/Filter Capabilities
```bash
# Filter clusters by entity type
anno cross-doc /path/to/documents --entity-types PER ORG

# Filter by document set
anno cross-doc /path/to/documents --documents doc1.txt doc2.txt

# Filter by confidence
anno cross-doc /path/to/documents --min-confidence 0.7
```

### 5. Comparison Mode
```bash
# Show entity differences across documents
anno cross-doc /path/to/documents --compare doc1.txt doc2.txt

# Output: Entities in doc1 but not doc2, entities in doc2 but not doc1
```

### 6. Visualization
```bash
# Generate HTML visualization of cross-doc clusters
anno cross-doc /path/to/documents --format html --output clusters.html
```

## Implementation Priority

1. **High**: Export/import format (enables all other enhancements)
2. **High**: Hierarchy-aware clustering (better quality)
3. **Medium**: Query/filter capabilities (usability)
4. **Medium**: HTML visualization (usability)
5. **Low**: Comparison mode (nice-to-have)

## Code Locations

- **Single document extraction**: `cmd_extract()` in `src/bin/anno.rs`
- **Single document with coreference**: `cmd_debug()` in `src/bin/anno.rs`
- **Cross-document clustering**: `cmd_crossdoc()` in `src/bin/anno.rs`
- **GroundedDocument structure**: `src/grounded.rs`
- **CDCR clustering logic**: `src/eval/cdcr.rs`

