#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "httpx",
#     "networkx>=3.0",
#     "matplotlib",
#     "beautifulsoup4",
#     "lxml",
#     "scipy",
# ]
# ///
"""
Knowledge Graph Analysis with Anno

This script demonstrates extracting entities and relations from Wikipedia,
building a knowledge graph, and running network analysis.

Usage:
    uv run examples/graph_analysis.py "Artificial intelligence"
    
Or with Python directly (if deps installed):
    python examples/graph_analysis.py "Machine learning"
"""

import subprocess
import json
import sys
import tempfile
from pathlib import Path
from collections import Counter

import httpx
import networkx as nx
from bs4 import BeautifulSoup


def fetch_wikipedia(topic: str, sentences: int = 50) -> str:
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
    
    print(f"[fetch] Fetching Wikipedia article: {topic}")
    headers = {
        "User-Agent": "AnnoNLP/0.2.0 (https://github.com/arclabs561/anno; educational research)"
    }
    resp = httpx.get(url, params=params, headers=headers, timeout=30)
    resp.raise_for_status()
    
    data = resp.json()
    pages = data.get("query", {}).get("pages", {})
    
    for page_id, page in pages.items():
        if page_id != "-1":
            text = page.get("extract", "")
            print(f"   OK Got {len(text)} chars, {len(text.split())} words")
            return text
    
    raise ValueError(f"Wikipedia article not found: {topic}")


def extract_entities_relations(text: str, model: str = "tplinker") -> dict:
    """Run anno extraction and return JSON results."""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
        f.write(text)
        input_file = f.name
    
    print(f"[scan] Extracting entities and relations with {model}...")
    
    # Run anno extract
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
        print(f"   WARN Anno error: {result.stderr}")
        # Fallback to heuristic model
        return {"entities": [], "relations": []}
    
    try:
        data = json.loads(result.stdout)
        n_ents = len(data.get("entities", []))
        n_rels = len(data.get("relations", []))
        print(f"   OK Found {n_ents} entities, {n_rels} relations")
        return data
    except json.JSONDecodeError:
        print(f"   WARN Failed to parse JSON output")
        return {"entities": [], "relations": []}


def normalize_entity_name(name: str) -> str:
    """Normalize entity name for matching."""
    name = name.lower().strip()
    # Remove trailing punctuation
    name = name.rstrip('.,;:')
    # Remove possessives
    name = name.rstrip("'s").rstrip("s'")
    return name.replace(" ", "_")


def merge_name_variants(entities: list) -> dict:
    """
    Group entities that are likely the same person/org.
    Returns mapping from normalized_id -> canonical_entity.
    """
    # Known name variant patterns
    variants = {
        "steve": "steve_jobs",
        "steven": "steve_jobs",
        "steven_paul_jobs": "steve_jobs",
        "jobs": "steve_jobs",
        "wozniak": "steve_wozniak",
        "woz": "steve_wozniak",
        "tim": "tim_cook",
        "cook": "tim_cook",
        "apple": "apple_inc",
        "apple_inc": "apple_inc",
        "apple_inc.": "apple_inc",
    }
    
    canonical_map = {}
    canonical_entities = {}
    
    for e in entities:
        node_id = normalize_entity_name(e["text"])
        
        # Check if this is a known variant
        canonical_id = variants.get(node_id, node_id)
        
        if canonical_id not in canonical_entities:
            canonical_entities[canonical_id] = {
                "text": e["text"],
                "type": e["type"],
                "confidence": e.get("confidence", 0.0),
                "mentions": 1,
            }
        else:
            # Update if higher confidence or longer name
            existing = canonical_entities[canonical_id]
            existing["mentions"] += 1
            if len(e["text"]) > len(existing["text"]):
                existing["text"] = e["text"]
            if e.get("confidence", 0) > existing["confidence"]:
                existing["confidence"] = e.get("confidence", 0)
        
        canonical_map[node_id] = canonical_id
    
    return canonical_map, canonical_entities


def build_graph(extraction: dict) -> nx.DiGraph:
    """Build NetworkX graph from extraction results."""
    G = nx.DiGraph()
    
    entities = extraction.get("entities", [])
    canonical_map, canonical_entities = merge_name_variants(entities)
    
    # Add entity nodes
    for node_id, e in canonical_entities.items():
        G.add_node(node_id, 
                   label=e["text"],
                   type=e["type"],
                   confidence=e["confidence"],
                   mentions=e.get("mentions", 1))
    
    # Add relation edges (using canonical IDs)
    for r in extraction.get("relations", []):
        head = normalize_entity_name(r["head"])
        tail = normalize_entity_name(r["tail"])
        
        # Map to canonical IDs
        head = canonical_map.get(head, head)
        tail = canonical_map.get(tail, tail)
        
        if head in G and tail in G and head != tail:
            if G.has_edge(head, tail):
                # Update if higher confidence
                if r.get("confidence", 0) > G[head][tail].get("confidence", 0):
                    G[head][tail]["confidence"] = r.get("confidence", 0)
            else:
                G.add_edge(head, tail,
                           relation=r["relation"],
                           confidence=r.get("confidence", 0.0))
    
    return G


def analyze_graph(G: nx.DiGraph) -> dict:
    """Compute graph analytics."""
    if G.number_of_nodes() == 0:
        return {"error": "Empty graph"}
    
    stats = {
        "nodes": G.number_of_nodes(),
        "edges": G.number_of_edges(),
        "density": nx.density(G),
    }
    
    # Entity type distribution
    type_counts = Counter(d.get("type", "UNK") for _, d in G.nodes(data=True))
    stats["entity_types"] = dict(type_counts)
    
    # Relation type distribution  
    rel_counts = Counter(d.get("relation", "UNK") for _, _, d in G.edges(data=True))
    stats["relation_types"] = dict(rel_counts)
    
    # Centrality metrics (if graph has nodes)
    if G.number_of_nodes() > 1:
        try:
            degree_cent = nx.degree_centrality(G)
            top_by_degree = sorted(degree_cent.items(), key=lambda x: x[1], reverse=True)[:5]
            stats["top_entities_by_degree"] = [
                {"entity": k, "centrality": round(v, 3)} for k, v in top_by_degree
            ]
        except Exception:
            pass
        
        try:
            if G.number_of_edges() > 0:
                pagerank = nx.pagerank(G, alpha=0.85)
                top_by_pr = sorted(pagerank.items(), key=lambda x: x[1], reverse=True)[:5]
                stats["top_entities_by_pagerank"] = [
                    {"entity": k, "score": round(v, 4)} for k, v in top_by_pr
                ]
        except Exception:
            pass
    
    # Connected components (for undirected view)
    undirected = G.to_undirected()
    if undirected.number_of_nodes() > 0:
        components = list(nx.connected_components(undirected))
        stats["connected_components"] = len(components)
        stats["largest_component_size"] = max(len(c) for c in components) if components else 0
    
    return stats


def export_cypher(G: nx.DiGraph) -> str:
    """Export graph as Neo4j Cypher CREATE statements."""
    lines = ["// Neo4j Cypher statements", "// Generated by anno graph_analysis.py", ""]
    
    # Create nodes
    for node_id, data in G.nodes(data=True):
        label = data.get("type", "Entity")
        name = data.get("label", node_id)
        conf = data.get("confidence", 0.0)
        lines.append(f"CREATE (:{label} {{id: '{node_id}', name: '{name}', confidence: {conf:.2f}}});")
    
    lines.append("")
    
    # Create relationships
    for u, v, data in G.edges(data=True):
        rel = data.get("relation", "RELATED_TO")
        conf = data.get("confidence", 0.0)
        lines.append(f"MATCH (a {{id: '{u}'}}), (b {{id: '{v}'}}) CREATE (a)-[:{rel} {{confidence: {conf:.2f}}}]->(b);")
    
    return "\n".join(lines)


def main():
    topic = " ".join(sys.argv[1:]) if len(sys.argv) > 1 else "Artificial intelligence"
    
    print(f"\n{'='*60}")
    print(f"🧠 Anno Knowledge Graph Analysis")
    print(f"{'='*60}")
    print(f"Topic: {topic}\n")
    
    # Step 1: Fetch content
    try:
        text = fetch_wikipedia(topic, sentences=30)
    except Exception as e:
        print(f"❌ Failed to fetch Wikipedia: {e}")
        sys.exit(1)
    
    # Step 2: Extract entities and relations
    extraction = extract_entities_relations(text, model="tplinker")
    
    # Step 3: Build graph
    print("\n📊 Building knowledge graph...")
    G = build_graph(extraction)
    print(f"   OK Graph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges")
    
    # Step 4: Analyze
    print("\n📈 Graph Analytics:")
    stats = analyze_graph(G)
    
    print(f"\n   Basic Stats:")
    print(f"   • Nodes: {stats.get('nodes', 0)}")
    print(f"   • Edges: {stats.get('edges', 0)}")
    print(f"   • Density: {stats.get('density', 0):.4f}")
    print(f"   • Connected components: {stats.get('connected_components', 0)}")
    
    if stats.get("entity_types"):
        print(f"\n   Entity Type Distribution:")
        for etype, count in sorted(stats["entity_types"].items(), key=lambda x: -x[1]):
            print(f"   • {etype}: {count}")
    
    if stats.get("relation_types"):
        print(f"\n   Relation Type Distribution:")
        for rtype, count in sorted(stats["relation_types"].items(), key=lambda x: -x[1]):
            print(f"   • {rtype}: {count}")
    
    if stats.get("top_entities_by_degree"):
        print(f"\n   Top Entities by Degree Centrality:")
        for item in stats["top_entities_by_degree"]:
            print(f"   • {item['entity']}: {item['centrality']}")
    
    if stats.get("top_entities_by_pagerank"):
        print(f"\n   Top Entities by PageRank:")
        for item in stats["top_entities_by_pagerank"]:
            print(f"   • {item['entity']}: {item['score']}")
    
    # Step 5: Export to file
    output_file = f"/tmp/kg_{topic.lower().replace(' ', '_')}.cypher"
    cypher = export_cypher(G)
    Path(output_file).write_text(cypher)
    print(f"\n💾 Exported Neo4j Cypher to: {output_file}")
    
    # Also export JSON
    json_file = output_file.replace(".cypher", ".json")
    with open(json_file, 'w') as f:
        json.dump({
            "topic": topic,
            "stats": stats,
            "entities": extraction.get("entities", []),
            "relations": extraction.get("relations", []),
        }, f, indent=2)
    print(f"   Exported JSON to: {json_file}")
    
    # Step 6: Visualize if we have nodes
    if G.number_of_nodes() >= 2:
        try:
            import matplotlib.pyplot as plt
            
            # Color map by entity type
            type_colors = {
                "PER": "#4A90D9",  # Blue
                "ORG": "#7B68EE",  # Purple
                "LOC": "#2ECC71",  # Green
                "MISC": "#F39C12", # Orange
            }
            
            colors = [type_colors.get(d.get("type", "MISC"), "#95A5A6") 
                      for _, d in G.nodes(data=True)]
            
            fig, ax = plt.subplots(figsize=(12, 8))
            
            # Use spring layout for positioning
            pos = nx.spring_layout(G, k=2, iterations=50, seed=42)
            
            # Draw nodes
            nx.draw_networkx_nodes(G, pos, node_color=colors, node_size=800, alpha=0.8, ax=ax)
            
            # Draw edges with labels
            nx.draw_networkx_edges(G, pos, edge_color="#CCCCCC", arrows=True, 
                                   arrowsize=15, alpha=0.6, ax=ax)
            
            # Draw labels
            labels = {n: d.get("label", n) for n, d in G.nodes(data=True)}
            nx.draw_networkx_labels(G, pos, labels, font_size=9, font_weight="bold", ax=ax)
            
            # Edge labels (relation types)
            edge_labels = {(u, v): d.get("relation", "") for u, v, d in G.edges(data=True)}
            nx.draw_networkx_edge_labels(G, pos, edge_labels, font_size=7, alpha=0.7, ax=ax)
            
            ax.set_title(f"Knowledge Graph: {topic}", fontsize=14, fontweight="bold")
            ax.axis("off")
            
            # Add legend
            legend_handles = [plt.Line2D([0], [0], marker='o', color='w', 
                                         markerfacecolor=c, markersize=10, label=t)
                             for t, c in type_colors.items()]
            ax.legend(handles=legend_handles, loc="upper left", frameon=True)
            
            plt.tight_layout()
            
            png_file = output_file.replace(".cypher", ".png")
            plt.savefig(png_file, dpi=150, bbox_inches="tight", facecolor="white")
            print(f"   Exported visualization to: {png_file}")
            plt.close()
        except Exception as e:
            print(f"   WARN Visualization failed: {e}")
    
    print(f"\n{'='*60}")
    print("✅ Done!")


if __name__ == "__main__":
    main()

