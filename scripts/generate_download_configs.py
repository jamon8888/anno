#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""
Generate download configurations from datasets_generated.json.

This script reads the dataset registry (JSON format) and generates
download configurations for the download_extended_datasets.py script.

Usage:
    uv run scripts/generate_download_configs.py [--output FILE] [--dry-run]
"""

import argparse
import json
import sys
from pathlib import Path


def infer_format(url: str, name: str) -> str:
    """Infer output format from URL."""
    url_lower = url.lower()
    if ".conllu" in url_lower:
        return "conllu"
    elif ".conll" in url_lower:
        return "conll"
    elif ".jsonl" in url_lower:
        return "jsonl"
    elif ".json" in url_lower:
        return "json"
    elif ".txt" in url_lower or ".bio" in url_lower:
        return "txt"
    elif ".csv" in url_lower:
        return "csv"
    elif ".tsv" in url_lower:
        return "tsv"
    elif ".xml" in url_lower:
        return "xml"
    elif ".zip" in url_lower:
        return "zip"
    else:
        return "txt"


def infer_group(dataset: dict) -> str:
    """Infer dataset group from categories and domain."""
    categories = dataset.get("categories", [])
    domain = dataset.get("domain", "").lower()
    
    # Priority mapping
    if "biomedical" in categories or domain in ["biomedical", "medical", "clinical"]:
        return "biomedical"
    elif "historical" in categories or "ancient" in categories:
        return "ancient"
    elif "african" in categories or domain == "african":
        return "african"
    elif "indigenous" in categories:
        return "indigenous"
    elif "coref" in categories:
        return "coreference"
    elif "relation_extraction" in categories:
        return "relations"
    elif "bias" in categories:
        return "bias"
    elif "multilingual" in categories:
        return "multilingual"
    elif "nested" in categories:
        return "nested"
    elif "event" in categories:
        return "event"
    elif domain in ["legal", "scientific", "food", "music", "aviation", "maritime"]:
        return domain
    else:
        return "general"


def generate_configs(json_path: Path) -> tuple[dict, dict]:
    """Generate download configs from JSON registry."""
    with open(json_path, "r") as f:
        data = json.load(f)
    
    configs = {}
    skipped = {"no_url": [], "paper_only": [], "huggingface_api": []}
    
    datasets = data.get("datasets", [])
    for ds in datasets:
        name = ds.get("id", ds.get("name", "unknown"))
        url = ds.get("url", "")
        
        # Skip if no URL
        if not url:
            skipped["no_url"].append(name)
            continue
        
        # Skip paper-only URLs
        if "doi.org" in url or "aclanthology" in url or "arxiv.org" in url:
            skipped["paper_only"].append(name)
            continue
        
        # Skip HuggingFace API URLs (need special handling)
        if "datasets-server.huggingface.co" in url:
            skipped["huggingface_api"].append(name)
            continue
        
        # Generate config
        fmt = infer_format(url, name)
        group = infer_group(ds)
        
        # Generate output filename
        clean_name = name.lower().replace("-", "_")
        output = f"{clean_name}.{fmt}"
        
        configs[clean_name] = {
            "group": group,
            "url": url,
            "output": output,
            "description": ds.get("description", name),
            "entity_types": ds.get("entity_types", []),
            "language": ds.get("language", "en"),
        }
        
        # Add optional fields
        if ds.get("sha256"):
            configs[clean_name]["sha256"] = ds["sha256"]
        if ds.get("expected_docs"):
            configs[clean_name]["expected_docs"] = ds["expected_docs"]
    
    return configs, skipped


def main():
    parser = argparse.ArgumentParser(description="Generate download configs from registry")
    parser.add_argument("--input", default="datasets_generated.json", help="Input JSON file")
    parser.add_argument("--output", help="Output JSON file (default: stdout)")
    parser.add_argument("--dry-run", action="store_true", help="Show summary only")
    parser.add_argument("--stats", action="store_true", help="Show statistics")
    args = parser.parse_args()
    
    json_path = Path(args.input)
    if not json_path.exists():
        print(f"Error: {json_path} not found", file=sys.stderr)
        sys.exit(1)
    
    configs, skipped = generate_configs(json_path)
    
    if args.stats or args.dry_run:
        print(f"=== Download Config Generation Stats ===")
        print(f"Total downloadable: {len(configs)}")
        print(f"Skipped (no URL): {len(skipped['no_url'])}")
        print(f"Skipped (paper only): {len(skipped['paper_only'])}")
        print(f"Skipped (HF API): {len(skipped['huggingface_api'])}")
        print()
        
        # Group summary
        groups = {}
        for name, cfg in configs.items():
            g = cfg["group"]
            groups[g] = groups.get(g, 0) + 1
        
        print("By group:")
        for g, count in sorted(groups.items(), key=lambda x: -x[1]):
            print(f"  {g}: {count}")
    
    if args.dry_run:
        return
    
    output_data = {
        "generated_from": str(json_path),
        "total_datasets": len(configs),
        "configs": configs,
    }
    
    if args.output:
        with open(args.output, "w") as f:
            json.dump(output_data, f, indent=2)
        print(f"Wrote {len(configs)} configs to {args.output}")
    else:
        json.dump(output_data, sys.stdout, indent=2)
        print()


if __name__ == "__main__":
    main()
