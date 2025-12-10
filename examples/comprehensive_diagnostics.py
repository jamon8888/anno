#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "httpx",
#     "networkx>=3.0",
#     "matplotlib",
#     "pandas",
#     "tabulate",
#     "scipy",
# ]
# ///
"""
Comprehensive Entity Extraction Diagnostics

This example runs the full anno pipeline and generates extensive diagnostics:
- Console output with entity tables
- CSV exports for analysis
- Visualization plots
- JSON reports with statistics

Usage:
    uv run examples/comprehensive_diagnostics.py

Outputs to /tmp/anno_diagnostics/:
    - entities.csv          Entity extraction results
    - relations.csv         Relation extraction results
    - diagnostics.json      Full statistics
    - entity_types.png      Type distribution chart
    - confidence_hist.png   Confidence histogram
    - graph.png             Knowledge graph visualization
    - report.html           HTML summary report
"""

import subprocess
import json
import sys
import os
from pathlib import Path
from collections import Counter
from datetime import datetime
from typing import List, Dict, Any

import matplotlib.pyplot as plt
import pandas as pd
import networkx as nx

# Test inputs covering various scenarios
TEST_TEXTS = [
    # Tech - multiple entities, relations
    ("tech", "Steve Jobs co-founded Apple with Steve Wozniak in 1976. Tim Cook became CEO in 2011 after Jobs resigned due to illness. Apple is headquartered in Cupertino, California."),
    
    # Politics - complex names, titles
    ("politics", "President Biden met with Chancellor Scholz in Berlin to discuss NATO expansion. Ukraine's President Zelenskyy joined via video call from Kyiv."),
    
    # Science - technical terms, organizations
    ("science", "Researchers at MIT and Stanford published findings on CRISPR gene editing. Dr. Jennifer Doudna shared the 2020 Nobel Prize in Chemistry with Emmanuelle Charpentier."),
    
    # Finance - companies, numbers
    ("finance", "JPMorgan Chase reported Q3 earnings of $12.7 billion. CEO Jamie Dimon warned about inflation concerns. Goldman Sachs and Morgan Stanley also beat expectations."),
    
    # Unicode - multilingual
    ("unicode", "習近平 met with Putin in Moscow. Angela Merkel, former Bundeskanzlerin, was also mentioned. François Müller attended from France."),
    
    # Coreference - pronouns
    ("coref", "Barack Obama was born in Hawaii. He later moved to Chicago where he began his political career. His wife Michelle Obama grew up there."),
    
    # Nested - organization/person overlap
    ("nested", "The Microsoft CEO Satya Nadella announced new AI features. OpenAI's Sam Altman praised the partnership. Google's Sundar Pichai responded."),
]

OUTPUT_DIR = Path("/tmp/anno_diagnostics")


def run_anno(text: str, model: str = "gliner") -> dict:
    """Run anno extraction and return results."""
    result = subprocess.run(
        ["./target/release/anno", "extract",
         "--model", model,
         "--format", "json",
         text],
        capture_output=True,
        text=True,
        cwd=Path(__file__).parent.parent
    )
    
    if result.returncode != 0:
        return {"entities": [], "relations": [], "error": result.stderr}
    
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return {"entities": [], "relations": [], "error": "JSON parse failed"}


def run_coref(text: str) -> dict:
    """Run anno with coreference resolution."""
    result = subprocess.run(
        ["./target/release/anno", "debug",
         "--coref", "--link-kb",
         "-t", text,
         "--export", "/tmp/coref_temp.json",
         "--export-format", "full"],
        capture_output=True,
        text=True,
        cwd=Path(__file__).parent.parent
    )
    
    try:
        with open("/tmp/coref_temp.json") as f:
            return json.load(f)
    except:
        return {"tracks": [], "identities": {}}


def main():
    print(f"\n{'='*70}")
    print(" Comprehensive Entity Extraction Diagnostics")
    print(f"{'='*70}")
    print(f"Timestamp: {datetime.now().isoformat()}")
    print(f"Output: {OUTPUT_DIR}/")
    print()
    
    # Create output directory
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    
    all_entities = []
    all_relations = []
    results_by_category = {}
    
    # Run extraction on all test texts
    for category, text in TEST_TEXTS:
        print(f"[proc] Processing: {category}")
        print(f"   Text: {text[:60]}...")
        
        # Extract with GLiNER
        result = run_anno(text, "gliner")
        entities = result.get("entities", [])
        
        # Extract relations with TPLinker
        rel_result = run_anno(text, "tplinker")
        relations = rel_result.get("relations", [])
        
        # Run coreference
        coref_result = run_coref(text)
        
        print(f"   OK Entities: {len(entities)}, Relations: {len(relations)}")
        
        # Add category and text to each entity
        for e in entities:
            e["category"] = category
            e["source_text"] = text[:50]
            all_entities.append(e)
        
        for r in relations:
            r["category"] = category
            all_relations.append(r)
        
        results_by_category[category] = {
            "text": text,
            "entities": entities,
            "relations": relations,
            "coref": {
                "tracks": len(coref_result.get("tracks", {})),
                "identities": len(coref_result.get("identities", {})),
            }
        }
    
    # Create DataFrames
    df_entities = pd.DataFrame(all_entities)
    df_relations = pd.DataFrame(all_relations)
    
    # =====================================================
    # Console Output - Entity Table
    # =====================================================
    print(f"\n{'='*70}")
    print("📊 Entity Extraction Summary")
    print(f"{'='*70}")
    
    if not df_entities.empty:
        # Summary by type
        type_counts = df_entities['type'].value_counts()
        print(f"\nEntity Type Distribution:")
        for t, c in type_counts.items():
            print(f"  • {t}: {c}")
        
        # Summary by category
        cat_counts = df_entities['category'].value_counts()
        print(f"\nEntities by Category:")
        for cat, c in cat_counts.items():
            print(f"  • {cat}: {c}")
        
        # Top entities by frequency
        entity_counts = df_entities['text'].value_counts().head(10)
        print(f"\nMost Frequent Entities:")
        for e, c in entity_counts.items():
            print(f"  • {e}: {c}")
        
        # Confidence statistics
        if 'confidence' in df_entities.columns:
            conf = df_entities['confidence'].dropna()
            if len(conf) > 0:
                print(f"\nConfidence Statistics:")
                print(f"  • Mean: {conf.mean():.3f}")
                print(f"  • Std:  {conf.std():.3f}")
                print(f"  • Min:  {conf.min():.3f}")
                print(f"  • Max:  {conf.max():.3f}")
    
    # =====================================================
    # Relation Summary
    # =====================================================
    print(f"\n{'='*70}")
    print("🔗 Relation Extraction Summary")
    print(f"{'='*70}")
    
    if not df_relations.empty:
        rel_counts = df_relations['relation'].value_counts()
        print(f"\nRelation Type Distribution:")
        for r, c in rel_counts.items():
            print(f"  • {r}: {c}")
        
        print(f"\nSample Relations:")
        for _, row in df_relations.head(5).iterrows():
            print(f"  • {row.get('head', '?')} --[{row.get('relation', '?')}]--> {row.get('tail', '?')}")
    else:
        print("No relations extracted.")
    
    # =====================================================
    # CSV Exports
    # =====================================================
    print(f"\n{'='*70}")
    print("💾 Exporting Data Files")
    print(f"{'='*70}")
    
    # Entities CSV
    entities_csv = OUTPUT_DIR / "entities.csv"
    if not df_entities.empty:
        df_entities.to_csv(entities_csv, index=False)
        print(f"  OK {entities_csv} ({len(df_entities)} rows)")
    
    # Relations CSV
    relations_csv = OUTPUT_DIR / "relations.csv"
    if not df_relations.empty:
        df_relations.to_csv(relations_csv, index=False)
        print(f"  OK {relations_csv} ({len(df_relations)} rows)")
    
    # =====================================================
    # Visualizations
    # =====================================================
    print(f"\n{'='*70}")
    print("📈 Generating Visualizations")
    print(f"{'='*70}")
    
    # Entity Type Distribution
    if not df_entities.empty:
        fig, ax = plt.subplots(figsize=(10, 6))
        type_colors = {'PER': '#4A90D9', 'ORG': '#7B68EE', 'LOC': '#2ECC71', 'MISC': '#F39C12'}
        colors = [type_colors.get(t, '#95A5A6') for t in type_counts.index]
        type_counts.plot(kind='bar', ax=ax, color=colors)
        ax.set_title('Entity Type Distribution', fontsize=14, fontweight='bold')
        ax.set_xlabel('Entity Type')
        ax.set_ylabel('Count')
        ax.tick_params(axis='x', rotation=45)
        plt.tight_layout()
        type_png = OUTPUT_DIR / "entity_types.png"
        plt.savefig(type_png, dpi=150)
        plt.close()
        print(f"  OK {type_png}")
        
        # Confidence Histogram
        if 'confidence' in df_entities.columns and df_entities['confidence'].notna().any():
            fig, ax = plt.subplots(figsize=(10, 6))
            df_entities['confidence'].dropna().hist(bins=20, ax=ax, color='#3498DB', edgecolor='white')
            ax.set_title('Entity Confidence Distribution', fontsize=14, fontweight='bold')
            ax.set_xlabel('Confidence')
            ax.set_ylabel('Count')
            ax.axvline(df_entities['confidence'].mean(), color='red', linestyle='--', label=f'Mean: {df_entities["confidence"].mean():.2f}')
            ax.legend()
            plt.tight_layout()
            conf_png = OUTPUT_DIR / "confidence_hist.png"
            plt.savefig(conf_png, dpi=150)
            plt.close()
            print(f"  OK {conf_png}")
    
    # Knowledge Graph
    if not df_relations.empty:
        G = nx.DiGraph()
        
        for _, row in df_entities.iterrows():
            node_id = row['text'].lower().replace(' ', '_')
            G.add_node(node_id, label=row['text'], type=row.get('type', 'UNK'))
        
        for _, row in df_relations.iterrows():
            head = row['head'].lower().replace(' ', '_')
            tail = row['tail'].lower().replace(' ', '_')
            if head in G and tail in G:
                G.add_edge(head, tail, relation=row.get('relation', 'RELATED'))
        
        if G.number_of_nodes() > 0:
            fig, ax = plt.subplots(figsize=(14, 10))
            
            type_colors = {'PER': '#4A90D9', 'ORG': '#7B68EE', 'LOC': '#2ECC71'}
            colors = [type_colors.get(G.nodes[n].get('type', ''), '#95A5A6') for n in G.nodes()]
            
            pos = nx.spring_layout(G, k=2, iterations=50, seed=42)
            nx.draw_networkx_nodes(G, pos, node_color=colors, node_size=600, alpha=0.8, ax=ax)
            nx.draw_networkx_edges(G, pos, edge_color='#CCCCCC', arrows=True, arrowsize=12, alpha=0.6, ax=ax)
            
            labels = {n: G.nodes[n].get('label', n)[:15] for n in G.nodes()}
            nx.draw_networkx_labels(G, pos, labels, font_size=8, ax=ax)
            
            ax.set_title('Combined Knowledge Graph', fontsize=14, fontweight='bold')
            ax.axis('off')
            plt.tight_layout()
            
            graph_png = OUTPUT_DIR / "graph.png"
            plt.savefig(graph_png, dpi=150)
            plt.close()
            print(f"  OK {graph_png}")
    
    # =====================================================
    # JSON Diagnostics Report
    # =====================================================
    diagnostics = {
        "timestamp": datetime.now().isoformat(),
        "summary": {
            "total_entities": len(all_entities),
            "total_relations": len(all_relations),
            "categories_tested": len(TEST_TEXTS),
            "unique_entity_texts": df_entities['text'].nunique() if not df_entities.empty else 0,
        },
        "type_distribution": dict(type_counts) if not df_entities.empty else {},
        "category_distribution": dict(cat_counts) if not df_entities.empty else {},
        "confidence_stats": {
            "mean": float(df_entities['confidence'].mean()) if 'confidence' in df_entities.columns else None,
            "std": float(df_entities['confidence'].std()) if 'confidence' in df_entities.columns else None,
            "min": float(df_entities['confidence'].min()) if 'confidence' in df_entities.columns else None,
            "max": float(df_entities['confidence'].max()) if 'confidence' in df_entities.columns else None,
        } if not df_entities.empty else {},
        "relation_types": dict(rel_counts) if not df_relations.empty else {},
        "results_by_category": results_by_category,
    }
    
    diag_json = OUTPUT_DIR / "diagnostics.json"
    with open(diag_json, 'w') as f:
        json.dump(diagnostics, f, indent=2, default=str)
    print(f"  OK {diag_json}")
    
    # =====================================================
    # HTML Report
    # =====================================================
    html_content = f"""<!DOCTYPE html>
<html>
<head>
    <title>Anno Entity Extraction Diagnostics</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 40px; background: #f5f5f5; }}
        h1 {{ color: #2c3e50; }}
        h2 {{ color: #34495e; border-bottom: 2px solid #3498db; padding-bottom: 10px; }}
        .stats {{ display: flex; gap: 20px; flex-wrap: wrap; }}
        .stat-card {{ background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); min-width: 150px; }}
        .stat-card h3 {{ margin: 0; color: #7f8c8d; font-size: 14px; }}
        .stat-card .value {{ font-size: 32px; font-weight: bold; color: #2c3e50; }}
        table {{ border-collapse: collapse; width: 100%; background: white; margin: 20px 0; }}
        th, td {{ border: 1px solid #ddd; padding: 12px; text-align: left; }}
        th {{ background: #3498db; color: white; }}
        tr:nth-child(even) {{ background: #f9f9f9; }}
        img {{ max-width: 100%; height: auto; margin: 20px 0; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .timestamp {{ color: #7f8c8d; font-size: 12px; }}
    </style>
</head>
<body>
    <h1>Anno Entity Extraction Diagnostics</h1>
    <p class="timestamp">Generated: {datetime.now().isoformat()}</p>
    
    <h2>Summary Statistics</h2>
    <div class="stats">
        <div class="stat-card">
            <h3>Total Entities</h3>
            <div class="value">{len(all_entities)}</div>
        </div>
        <div class="stat-card">
            <h3>Total Relations</h3>
            <div class="value">{len(all_relations)}</div>
        </div>
        <div class="stat-card">
            <h3>Categories Tested</h3>
            <div class="value">{len(TEST_TEXTS)}</div>
        </div>
        <div class="stat-card">
            <h3>Unique Entities</h3>
            <div class="value">{df_entities['text'].nunique() if not df_entities.empty else 0}</div>
        </div>
    </div>
    
    <h2>Entity Type Distribution</h2>
    <img src="entity_types.png" alt="Entity Types">
    
    <h2>Confidence Distribution</h2>
    <img src="confidence_hist.png" alt="Confidence Histogram">
    
    <h2>Knowledge Graph</h2>
    <img src="graph.png" alt="Knowledge Graph">
    
    <h2>Entities by Category</h2>
    <table>
        <tr><th>Category</th><th>Entity Count</th><th>Relation Count</th></tr>
        {''.join(f'<tr><td>{cat}</td><td>{len(data["entities"])}</td><td>{len(data["relations"])}</td></tr>' for cat, data in results_by_category.items())}
    </table>
    
    <h2>Sample Entities</h2>
    <table>
        <tr><th>Text</th><th>Type</th><th>Confidence</th><th>Category</th></tr>
        {''.join(f'<tr><td>{e["text"]}</td><td>{e.get("type", "?")}</td><td>{e.get("confidence", 0):.2f}</td><td>{e.get("category", "?")}</td></tr>' for e in all_entities[:20])}
    </table>
    
</body>
</html>
"""
    
    html_file = OUTPUT_DIR / "report.html"
    with open(html_file, 'w') as f:
        f.write(html_content)
    print(f"  OK {html_file}")
    
    # =====================================================
    # Final Summary
    # =====================================================
    print(f"\n{'='*70}")
    print("✅ Diagnostics Complete!")
    print(f"{'='*70}")
    print(f"\nOutputs:")
    for f in sorted(OUTPUT_DIR.iterdir()):
        size = f.stat().st_size
        print(f"  • {f.name} ({size:,} bytes)")
    
    print(f"\nOpen report: file://{html_file}")
    print()
    
    # Return exit code based on results
    if len(all_entities) == 0:
        print("WARN️ Warning: No entities extracted!")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())


