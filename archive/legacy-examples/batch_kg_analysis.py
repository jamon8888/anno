#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "httpx",
#     "networkx>=3.0",
#     "matplotlib",
#     "beautifulsoup4",
#     "lxml",
# ]
# ///
"""
Batch Knowledge Graph Analysis

Build a combined knowledge graph from multiple Wikipedia articles,
then analyze cross-article entity connections.

Usage:
    uv run examples/batch_kg_analysis.py
"""

import subprocess
import json
import sys
import tempfile
from pathlib import Path
from collections import Counter, defaultdict
from typing import List, Dict, Any

import httpx
import networkx as nx


TOPICS = [
    "Steve Jobs",
    "Tim Cook",
    "Apple Inc.",
    "History of Apple Inc.",
    "Steve Wozniak",
    "Pixar",
    "NeXT",
]

USER_AGENT = "AnnoNLP/0.2.0 (https://github.com/arclabs561/anno; educational research)"


def fetch_wikipedia(topic: str, sentences: int = 20) -> str:
    """Fetch plain text from Wikipedia API."""
    url = "https://en.wikipedia.org/w/api.php"
    params = {
        "action": "query",
        "format": "json",
        "titles": topic,
        "prop": "extracts",
        "exintro": False,
        "explaintext": True,
        "exsectionformat": "plain",
        "exsentences": sentences,
    }
    
    resp = httpx.get(url, params=params, headers={"User-Agent": USER_AGENT}, timeout=30)
    resp.raise_for_status()
    
    data = resp.json()
    pages = data.get("query", {}).get("pages", {})
    
    for page_id, page in pages.items():
        if page_id != "-1":
            return page.get("extract", "")
    
    return ""


def extract_with_anno(text: str, model: str = "gliner") -> dict:
    """Run anno extraction and return JSON results."""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
        f.write(text)
        input_file = f.name
    
    result = subprocess.run(
        ["./target/release/anno", "extract", 
         "--model", model,
         "--file", input_file,
         "--format", "json"],
        capture_output=True,
        text=True,
        cwd=Path(__file__).parent.parent
    )
    
    Path(input_file).unlink()
    
    if result.returncode != 0:
        return {"entities": [], "relations": []}
    
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return {"entities": [], "relations": []}


def normalize_entity_name(name: str) -> str:
    """Normalize entity names for matching across documents."""
    return name.lower().strip().rstrip('.,;:').replace("'s", "")


def main():
    print(f"\n{'='*70}")
    print("🌐 Batch Knowledge Graph Analysis - Apple Ecosystem")
    print(f"{'='*70}\n")
    
    combined_graph = nx.DiGraph()
    all_entities: Dict[str, Dict] = {}  # normalized_name -> entity info
    entity_doc_freq: Dict[str, set] = defaultdict(set)  # entity -> set of docs
    
    for topic in TOPICS:
        print(f"[fetch] Processing: {topic}")
        
        # Fetch
        text = fetch_wikipedia(topic)
        if not text:
            print(f"   WARN No content found")
            continue
        print(f"   • Fetched {len(text.split())} words")
        
        # Extract (use gliner for better entity detection)
        extraction = extract_with_anno(text, model="gliner")
        entities = extraction.get("entities", [])
        print(f"   • Found {len(entities)} entities")
        
        # Add to combined graph
        for e in entities:
            name = normalize_entity_name(e["text"])
            if len(name) < 2:
                continue
                
            # Track document frequency
            entity_doc_freq[name].add(topic)
            
            # Add node or update if higher confidence
            if name not in all_entities or e.get("confidence", 0) > all_entities[name].get("confidence", 0):
                all_entities[name] = {
                    "label": e["text"],
                    "type": e["type"],
                    "confidence": e.get("confidence", 0),
                }
    
    # Build combined graph with nodes
    for name, info in all_entities.items():
        doc_count = len(entity_doc_freq[name])
        combined_graph.add_node(
            name,
            label=info["label"],
            type=info["type"],
            confidence=info["confidence"],
            doc_frequency=doc_count,
        )
    
    # Add edges based on co-occurrence in same document (simple heuristic)
    for topic in TOPICS:
        text = fetch_wikipedia(topic, sentences=10)
        extraction = extract_with_anno(text, model="gliner")
        entities = [normalize_entity_name(e["text"]) for e in extraction.get("entities", [])]
        entities = [e for e in entities if e in combined_graph]
        
        # Connect entities that appear in same document
        for i, e1 in enumerate(entities):
            for e2 in entities[i+1:]:
                if e1 != e2 and combined_graph.has_node(e1) and combined_graph.has_node(e2):
                    if combined_graph.has_edge(e1, e2):
                        combined_graph[e1][e2]["weight"] += 1
                    else:
                        combined_graph.add_edge(e1, e2, relation="CO_OCCURS", weight=1)
    
    print(f"\n{'='*70}")
    print("📊 Combined Knowledge Graph Analysis")
    print(f"{'='*70}\n")
    
    print(f"Graph Stats:")
    print(f"  • Total nodes: {combined_graph.number_of_nodes()}")
    print(f"  • Total edges: {combined_graph.number_of_edges()}")
    print(f"  • Density: {nx.density(combined_graph):.4f}")
    
    # Type distribution
    type_counts = Counter(d.get("type", "UNK") for _, d in combined_graph.nodes(data=True))
    print(f"\nEntity Type Distribution:")
    for etype, count in sorted(type_counts.items(), key=lambda x: -x[1]):
        print(f"  • {etype}: {count}")
    
    # Cross-document entities (entities appearing in multiple articles)
    multi_doc_entities = {k: v for k, v in entity_doc_freq.items() if len(v) > 1}
    print(f"\nCross-Document Entities ({len(multi_doc_entities)} entities appear in multiple articles):")
    for name, docs in sorted(multi_doc_entities.items(), key=lambda x: -len(x[1]))[:10]:
        print(f"  • {name}: appears in {len(docs)} articles")
    
    # Centrality
    if combined_graph.number_of_edges() > 0:
        try:
            pagerank = nx.pagerank(combined_graph.to_undirected(), alpha=0.85)
            top = sorted(pagerank.items(), key=lambda x: -x[1])[:10]
            print(f"\nTop Entities by PageRank (importance):")
            for name, score in top:
                label = combined_graph.nodes[name].get("label", name)
                etype = combined_graph.nodes[name].get("type", "?")
                print(f"  • {label} [{etype}]: {score:.4f}")
        except Exception as e:
            print(f"\n(PageRank failed: {e})")
    
    # Export
    output_file = "/tmp/apple_ecosystem_kg.json"
    export_data = {
        "topics": TOPICS,
        "nodes": [
            {
                "id": n,
                "label": d.get("label", n),
                "type": d.get("type"),
                "doc_frequency": d.get("doc_frequency", 1),
            }
            for n, d in combined_graph.nodes(data=True)
        ],
        "edges": [
            {"source": u, "target": v, "relation": d.get("relation"), "weight": d.get("weight", 1)}
            for u, v, d in combined_graph.edges(data=True)
        ],
        "stats": {
            "nodes": combined_graph.number_of_nodes(),
            "edges": combined_graph.number_of_edges(),
            "cross_doc_entities": len(multi_doc_entities),
        }
    }
    
    with open(output_file, 'w') as f:
        json.dump(export_data, f, indent=2)
    print(f"\n💾 Exported to: {output_file}")
    
    print(f"\n{'='*70}")
    print("✅ Batch analysis complete!")


if __name__ == "__main__":
    main()

