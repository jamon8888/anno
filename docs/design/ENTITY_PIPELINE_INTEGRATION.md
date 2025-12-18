# Entity Pipeline Integration

## Overview

The anno entity extraction pipeline consists of three main phases:

```
Extract → Coalesce → Stratify
```

This document explains how **parentheticals**, **reference resolution**, **coalesce**, and **strata** work together.

## The Pipeline

### 1. Extract (Level 0)

Basic entity extraction from source documents:

```rust
use anno::{Model, RegexNER};
use anno::preprocess::{ParentheticalExtractor, ReferenceExtractor};

let text = "Apple Inc. (AAPL) CEO Tim Cook announced the partnership with Microsoft Corp. (MSFT). See https://en.wikipedia.org/wiki/Tim_Cook for more info.";

// Extract entities
let model = RegexNER::new();
let entities = model.extract_entities(text, None);

// Extract parentheticals (provides aliases)
let paren_extractor = ParentheticalExtractor::new();
let parentheticals = paren_extractor.extract(text);
// → "Apple Inc." ↔ "AAPL" (Ticker)
// → "Microsoft Corp." ↔ "MSFT" (Ticker)

// Extract references (provides KB links and external content)
let ref_extractor = ReferenceExtractor::new();
let references = ref_extractor.extract(text);
// → Wikipedia URL for Tim Cook (entity_id: "Tim_Cook")
```

### 2. Coalesce (Cross-Document Entity Linking)

The `anno-coalesce` crate clusters entities across documents:

```rust
use anno_coalesce::Resolver;
use anno_core::Corpus;

// Parenthetical aliases feed into coalescing
// Document 1: "The WHO announced guidelines..."
// Document 2: "World Health Organization (WHO) released data..."
// 
// Parenthetical in Doc 2 establishes:
//   "World Health Organization" ↔ "WHO" alias
//
// Coalesce can now link Doc 1's "WHO" to Doc 2's "World Health Organization"

let resolver = Resolver::new()
    .with_threshold(0.7);

let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
```

#### How Parentheticals Help Coalescing

| Source Document | Text | Alias Extracted |
|-----------------|------|-----------------|
| Doc 1 | "The WHO issued a warning" | None |
| Doc 2 | "World Health Organization (WHO) data" | WHO ↔ World Health Organization |
| Doc 3 | "W.H.O. headquarters in Geneva" | None |

The coalesce module uses these aliases to link:
- Doc 1 "WHO" → Doc 2 "WHO" (exact match)
- Doc 2 "World Health Organization" → identity
- Doc 3 "W.H.O." → fuzzy match to "WHO"

### 3. Stratify (Hierarchical Clustering)

The `anno-strata` crate creates hierarchical community structures:

```rust
use anno_strata::HierarchicalLeiden;
use anno::preprocess::reference::ReferenceGraph;

// Reference graph creates hierarchy
// Level 0: Source documents
// Level 1: Directly referenced documents (Wikipedia, etc.)
// Level 2+: Transitively referenced documents

let mut ref_graph = ReferenceGraph::new();
ref_graph.add_reference("doc1", "wiki_einstein", ReferenceType::WikipediaUrl, 1.0);
ref_graph.add_reference("doc2", "wiki_einstein", ReferenceType::WikipediaUrl, 1.0);
ref_graph.add_reference("wiki_einstein", "wiki_physics", ReferenceType::WikipediaUrl, 0.5);

// Convert to graph for Leiden clustering
let clusterer = HierarchicalLeiden::new()
    .with_resolution(1.0)
    .with_levels(3);

let graph_doc = GraphDocument::from_edges(ref_graph.to_graph_edges());
let clustered = clusterer.cluster(&graph_doc)?;
```

## Integration Points

### Parentheticals → Coalesce

```
Parenthetical          Coalesce Effect
─────────────────────────────────────────────────────
Abbreviation (WHO)     Creates alias for string matching
Ticker (AAPL)          Links company name to stock ticker
Translation (北京)      Links transliterated names
Role (CEO)             Provides entity context
Temporal (1769-1821)   Adds entity temporal bounds
```

### References → Strata

```
Reference Type         Strata Effect
─────────────────────────────────────────────────────
WikipediaUrl           Creates KB-grounded node
WikidataUrl            Adds canonical entity ID (Q-number)
CrossReference         Links within-document sections
Citation               Links to academic sources
WebUrl                 Adds external evidence
```

### Reference Depth in Strata

The `ReferenceGraph` tracks document depth:

```
Level 0 (Root): Source document
   │
   ├── Level 1: https://en.wikipedia.org/wiki/Einstein
   │      │
   │      └── Level 2: https://en.wikipedia.org/wiki/Relativity
   │
   └── Level 1: https://arxiv.org/abs/2301.00001
          │
          └── Level 2: DOI references from paper
```

Strata uses this depth for:
- Confidence weighting (closer = more confident)
- Community detection (co-cited entities cluster together)
- Transitive entity linking

## Complete Example

```rust
use anno::preprocess::{extract_aliases, ReferenceExtractor, ReferenceGraph};
use anno_coalesce::Resolver;
use anno_strata::HierarchicalLeiden;

// 1. Extract from multiple documents
let docs = vec![
    ("doc1", "Apple Inc. (AAPL) CEO Tim Cook spoke. See https://en.wikipedia.org/wiki/Tim_Cook"),
    ("doc2", "Tim Cook (Apple CEO) announced new products."),
    ("doc3", "The tech giant AAPL reported earnings."),
];

// 2. Extract aliases from parentheticals
let mut all_aliases = Vec::new();
for (doc_id, text) in &docs {
    let aliases = extract_aliases(text, Some(doc_id));
    all_aliases.extend(aliases);
}
// → ("Apple Inc.", "AAPL"), ("Tim Cook", "Apple CEO")

// 3. Build reference graph
let mut ref_graph = ReferenceGraph::new();
let ref_extractor = ReferenceExtractor::new();
for (doc_id, text) in &docs {
    let refs = ref_extractor.extract(text);
    for reference in refs {
        if let Some(entity_id) = &reference.entity_id {
            ref_graph.add_reference(doc_id, entity_id, reference.reference_type, 1.0);
        }
    }
}

// 4. Coalesce entities across documents
// The coalescer uses:
// - Exact string matches
// - Alias pairs from parentheticals
// - Reference graph for KB-grounded entities
let resolver = Resolver::new();
// ... add documents to corpus, extract entities ...
let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

// 5. Stratify with Leiden clustering
let clusterer = HierarchicalLeiden::new()
    .with_resolution(1.0)
    .with_levels(3);

// The reference graph provides edges for clustering
let graph_doc = GraphDocument::from_reference_graph(&ref_graph);
let stratified = clusterer.cluster(&graph_doc)?;
```

## Data Flow Diagram

```
                    ┌─────────────────┐
                    │  Source Docs    │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
              ▼              ▼              ▼
    ┌─────────────────┐ ┌─────────┐ ┌─────────────────┐
    │ Entity Extract  │ │ Parens  │ │ Reference Ext   │
    └────────┬────────┘ └────┬────┘ └────────┬────────┘
             │               │               │
             │    Aliases    │   Ref Graph   │
             │      ┌────────┘               │
             │      │                        │
             ▼      ▼                        │
    ┌─────────────────┐                      │
    │    Coalesce     │◄─────────────────────┘
    │ (Cross-Doc EL)  │      KB Links
    └────────┬────────┘
             │
             │  Identities
             │
             ▼
    ┌─────────────────┐
    │    Strata       │
    │ (Hierarchical)  │
    └────────┬────────┘
             │
             ▼
    ┌─────────────────┐
    │  Knowledge Graph│
    │  with Levels    │
    └─────────────────┘
```

## Summary

| Component | Input | Output | Purpose |
|-----------|-------|--------|---------|
| **Parentheticals** | Raw text | Aliases, temporal bounds | Surface-form linking |
| **References** | Raw text | URLs, KB IDs, citations | External grounding |
| **Coalesce** | Entities + Aliases | Identities | Cross-doc clustering |
| **Strata** | Ref graph + Identities | Hierarchical communities | Abstraction levels |

The pipeline transforms raw text into a hierarchically structured knowledge graph with:
- Entity mentions linked to canonical identities
- Cross-document coreference chains
- KB grounding (Wikidata, Wikipedia)
- Multi-level community structure

