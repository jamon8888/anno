# Graph Export

Convert NER/Coref output to graph formats for RAG applications (Neo4j, NetworkX, etc.).

**Related**: See [CROSSDOC_COREF_ARCHITECTURE.md](../coref/CROSSDOC_COREF_ARCHITECTURE.md) for how coreference enables unified graph nodes.

## Overview

`GraphDocument` bridges NER/IE extraction and graph databases. It converts the `Signal → Track → Identity` hierarchy into graph nodes and edges.

## The "Fractured Graph" Problem

Without coreference resolution, the same real-world entity creates multiple disconnected nodes:

```
┌────────────────────────────────────────────────────────────────────┐
│                    FRACTURED GRAPH (BAD)                          │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│    [Elon Musk]          [Musk]          [he]          [The CEO]   │
│         │                  │               │               │      │
│         v                  v               v               v      │
│    "founded Tesla"   "bought Twitter"  "said..."    "announced"  │
│                                                                    │
│    → 4 disconnected nodes, no cross-reference!                    │
└────────────────────────────────────────────────────────────────────┘
```

With coreference resolution, all mentions link to a single canonical node:

```
┌────────────────────────────────────────────────────────────────────┐
│                    UNIFIED GRAPH (GOOD)                           │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│                       [Elon Musk]                                  │
│                     (canonical node)                               │
│                    /      |      \                                │
│                   v       v       v                               │
│           "founded"  "bought"  "announced"                        │
│               |          |          |                             │
│               v          v          v                             │
│           [Tesla]   [Twitter]   [layoffs]                        │
│                                                                    │
│    → 1 node with all relationships connected                      │
└────────────────────────────────────────────────────────────────────┘
```

## Supported Formats

### 1. Neo4j Cypher (`neo4j` or `cypher`)

Cypher CREATE statements for importing into Neo4j:

```bash
anno extract "Apple was founded by Steve Jobs in Cupertino." --export-graph neo4j
```

Output:
```cypher
CREATE (nperson_steve_jobs:PERSON {id: 'person_steve_jobs', name: 'Steve Jobs'});
CREATE (norg_apple:ORG {id: 'org_apple', name: 'Apple'});
CREATE (nloc_cupertino:LOC {id: 'loc_cupertino', name: 'Cupertino'});

MATCH (a {id: 'person_steve_jobs'}), (b {id: 'org_apple'}) CREATE (a)-[:RELATED_TO]->(b);
MATCH (a {id: 'org_apple'}), (b {id: 'loc_cupertino'}) CREATE (a)-[:RELATED_TO]->(b);
```

### 2. NetworkX JSON (`networkx` or `nx`)

NetworkX-compatible JSON format (node_link_graph):

```bash
anno extract "Apple was founded by Steve Jobs in Cupertino." --export-graph networkx
```

Output:
```json
{
  "directed": true,
  "multigraph": false,
  "graph": {},
  "nodes": [
    {
      "id": "person_steve_jobs",
      "type": "PERSON",
      "name": "Steve Jobs",
      "mentions_count": 1,
      "first_seen": 25
    },
    {
      "id": "org_apple",
      "type": "ORG",
      "name": "Apple",
      "mentions_count": 1,
      "first_seen": 0
    },
    {
      "id": "loc_cupertino",
      "type": "LOC",
      "name": "Cupertino",
      "mentions_count": 1,
      "first_seen": 50
    }
  ],
  "links": [
    {
      "source": "person_steve_jobs",
      "target": "org_apple",
      "relation": "RELATED_TO",
      "properties": {}
    },
    {
      "source": "org_apple",
      "target": "loc_cupertino",
      "relation": "RELATED_TO",
      "properties": {}
    }
  ]
}
```

Load in Python:
```python
import networkx as nx
import json

with open('graph.json') as f:
    data = json.load(f)
G = nx.node_link_graph(data)
```

### 3. JSON-LD (`jsonld` or `json-ld`)

Semantic web format with RDF-style structure:

```bash
anno extract "Apple was founded by Steve Jobs in Cupertino." --export-graph jsonld
```

Output:
```json
{
  "@context": {
    "@vocab": "http://schema.org/",
    "name": "http://schema.org/name",
    "type": "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
  },
  "@graph": [
    {
      "@id": "person_steve_jobs",
      "@type": "PERSON",
      "name": "Steve Jobs",
      "relations": [
        {
          "@type": "RELATED_TO",
          "target": "org_apple"
        }
      ]
    },
    {
      "@id": "org_apple",
      "@type": "ORG",
      "name": "Apple",
      "relations": [
        {
          "@type": "RELATED_TO",
          "target": "loc_cupertino"
        }
      ]
    },
    {
      "@id": "loc_cupertino",
      "@type": "LOC",
      "name": "Cupertino",
      "relations": []
    }
  ]
}
```

## CLI Usage

### Extract with Graph Export

```bash
# Export to Neo4j Cypher
anno extract "text" --export-graph neo4j > output.cypher

# Export to NetworkX JSON
anno extract "text" --export-graph networkx > graph.json

# Export to JSON-LD
anno extract "text" --export-graph jsonld > graph.jsonld
```

### Debug with Graph Export

```bash
anno debug "text" --export-graph neo4j
```

### Pipeline with Graph Export

```bash
anno pipeline "text" --export-graph networkx
```

## Node ID Generation

Node IDs are generated with the following priority:

1. **KB ID** (`entity.kb_id`): If entity is linked to a knowledge base (e.g., Wikidata Q-ID)
2. **Canonical ID** (`entity.canonical_id`): If entity is part of a coreference chain
3. **Content-based hash**: Fallback: `{entity_type}:{normalized_text}`

Example:
- Entity with `kb_id: "Q76"` → node ID: `"Q76"`
- Entity with `canonical_id: 42` → node ID: `"coref_42"`
- Entity "Apple Inc." → node ID: `"org:apple_inc"`

## Node Properties

Each node includes:

- `id`: Unique identifier
- `name`: Display name (canonical mention text)
- `type`: Entity type (PERSON, ORG, LOC, etc.)
- `mentions_count`: Number of times this entity appears
- `first_seen`: Character offset of first occurrence
- `valid_from`: Temporal validity start (if present)
- `valid_until`: Temporal validity end (if present)
- `viewport`: Viewport context (business, historical, etc.)

## Edge Properties

Each edge includes:

- `source`: Source node ID
- `target`: Target node ID
- `relation`: Relation type (from relation extraction or co-occurrence)
- `confidence`: Confidence score (0.0-1.0)
- `trigger`: Trigger text span (if available)
- `distance`: Character distance (for co-occurrence edges)

## Relation Extraction

When relations are extracted (via `RelationExtractor`), they become edges:

```rust
use anno::backends::inference::RelationExtractor;

let extractor: &dyn RelationExtractor = ...;
let result = extractor.extract_with_relations(
    text,
    &["PERSON", "ORG"],
    &["FOUNDED", "EMPLOYED_BY"],
    0.7
);

// result.relations contains RelationTriple objects
// These are automatically converted to GraphEdge objects
```

## Co-occurrence Edges

If no explicit relations are extracted, `GraphDocument` can infer co-occurrence edges:

```rust
use anno_core::graph::GraphDocument;

let graph = GraphDocument::from_entities_cooccurrence(
    &entities,
    50  // max_distance: entities within 50 chars are related
);
```

This creates `RELATED_TO` edges between entities that appear close together in the text.

## From GroundedDocument

Convert a `GroundedDocument` (with Signals, Tracks, Identities) to a graph:

```rust
use anno_core::graph::GraphDocument;
use anno_core::grounded::GroundedDocument;

let doc: GroundedDocument = ...;
let graph = GraphDocument::from_grounded_document(&doc);
```

This automatically:
- Deduplicates entities by canonical ID (from Tracks)
- Links nodes via KB IDs (from Identities)
- Preserves mention counts and first occurrence offsets

## Round-trip Testing

Graph documents can be exported and re-imported:

```bash
# Export to JSON
anno extract "text" --export-graph networkx > graph.json

# Import and cluster with tier
anno tier --input graph.json --method leiden --levels 3
```

The `tier` command reads `GraphDocument` JSON and performs hierarchical clustering, adding community assignments to node properties.

## Examples

### Complete Pipeline

```bash
# 1. Extract entities and relations
anno extract "Apple was founded by Steve Jobs in 1976." \
  --model gliner \
  --export-graph networkx > graph.json

# 2. Cluster hierarchically
anno tier --input graph.json --method leiden --levels 3 > clustered.json

# 3. Import into Neo4j
cat clustered.json | jq -r '.nodes[] | "CREATE (n\(.id):\(.type) {id: \"\(.id)\", name: \"\(.name)\"});"' | cypher-shell
```

### Batch Processing

```bash
# Process directory and export all graphs
for file in docs/*.txt; do
  anno extract "$(cat "$file")" \
    --export-graph networkx \
    > "graphs/$(basename "$file" .txt).json"
done
```

## Limitations

1. **No explicit relation extraction by default**: Most backends don't extract relations. Use `--model gliner2` or implement a `RelationExtractor` for explicit relations.

2. **Co-occurrence is a heuristic**: `RELATED_TO` edges from co-occurrence are not semantic relations. They indicate proximity, not meaning.

3. **String IDs required**: Node IDs must be strings. Integer IDs are not supported.

4. **No graph validation**: Invalid graphs (e.g., edges to non-existent nodes) are not detected.

## Research Background

- **GraphRAG** (Microsoft, 2024-2025): Combines knowledge graphs with vector retrieval
- **Entity Linking**: Maps extracted mentions to canonical KB entities
- **Coreference Resolution**: Clusters mentions referring to the same entity

## See Also

- [`GraphDocument` API](https://docs.rs/anno-core/latest/anno_core/graph/struct.GraphDocument.html)
- [`GroundedDocument`](https://docs.rs/anno-core/latest/anno_core/grounded/struct.GroundedDocument.html)
- [Tier Clustering](../../../ARCHITECTURE.md#hierarchical-clustering)

