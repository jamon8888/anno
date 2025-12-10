#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["datasets>=2.0", "huggingface_hub>=0.20"]
# ///
"""
Download HuggingFace datasets and convert to local cache format.

Usage:
    uv run scripts/download_hf_datasets.py [--all] [--dataset DATASET] [--list]
"""

import json
import os
import platform
from pathlib import Path

from datasets import load_dataset


def get_cache_dir() -> Path:
    """Get the anno cache directory, matching Rust's env.rs logic.
    
    Priority:
    1. ANNO_CACHE_DIR environment variable
    2. Platform-specific default:
       - macOS: ~/Library/Caches/anno
       - Linux/other: $XDG_CACHE_HOME/anno or ~/.cache/anno
    """
    if custom := os.environ.get("ANNO_CACHE_DIR"):
        return Path(custom)
    
    if platform.system() == "Darwin":  # macOS
        return Path.home() / "Library/Caches/anno"
    else:  # Linux, Windows, etc.
        xdg_cache = os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))
        return Path(xdg_cache) / "anno"


CACHE_DIR = get_cache_dir() / "datasets"
CACHE_DIR.mkdir(parents=True, exist_ok=True)

# Dataset configurations
DATASETS = {
    # Works with standard loading
    "jnlpba": {
        "hf_id": "chufangao/GENIA-NER",
        "config": None,
        "split": "test",
        "output": "jnlpba.conll",
    },
    "bc2gm_full": {
        "hf_id": "disi-unibo-nlp/bc2gm",
        "config": None, 
        "split": "test",
        "output": "bc2gm_full.conll",
    },
    # For deprecated script datasets, use alternatives
    "uner": {
        # Use WikiANN as proxy (similar multilingual NER with standard types)
        "hf_id": "unimelb-nlp/wikiann",
        "config": "en",
        "split": "test",
        "output": "uner.json",
        "note": "WikiANN en as proxy for UNER (similar types: PER, LOC, ORG)",
    },
    "biomner": {
        # BioNLP 2004 via TNER hub
        "hf_id": "tner/bionlp2004",
        "config": "bionlp2004",
        "split": "test",
        "output": "biomner.json",
    },
    "craft": {
        # Use AnatEM as proxy for biomedical NER
        "hf_id": "bigbio/anat_em",
        "config": "anat_em_bigbio_kb",
        "split": "test",
        "output": "craft.conll",
        "note": "AnatEM as proxy for CRAFT (biomedical entity types)",
    },
    "msner": {
        # Generate synthetic placeholder
        "synthetic": True,
        "output": "msner.json",
        "note": "Synthetic placeholder - VoxPopuli speech NER unavailable",
    },
}


def to_conll(examples, output_path: Path, token_col="tokens", tag_col="ner_tags"):
    """Convert to CoNLL format."""
    # Get label names if available
    label_names = None
    if hasattr(examples, "features") and tag_col in examples.features:
        feat = examples.features[tag_col]
        if hasattr(feat, "feature") and hasattr(feat.feature, "names"):
            label_names = feat.feature.names
    
    with open(output_path, "w") as f:
        for ex in examples:
            tokens = ex.get(token_col, [])
            tags = ex.get(tag_col, [])
            
            # Map numeric to string
            if tags and isinstance(tags[0], int) and label_names:
                tags = [label_names[t] if t < len(label_names) else "O" for t in tags]
            elif tags and isinstance(tags[0], int):
                tags = [f"TAG-{t}" for t in tags]
            
            for tok, tag in zip(tokens, tags):
                f.write(f"{tok}\t{tag}\n")
            f.write("\n")


def to_json(examples, output_path: Path):
    """Convert to JSON format."""
    label_names = None
    if hasattr(examples, "features") and "ner_tags" in examples.features:
        feat = examples.features["ner_tags"]
        if hasattr(feat, "feature") and hasattr(feat.feature, "names"):
            label_names = feat.feature.names
    
    data = []
    for ex in examples:
        tokens = ex.get("tokens", [])
        tags = ex.get("ner_tags", [])
        
        if tags and isinstance(tags[0], int) and label_names:
            tags = [label_names[t] if t < len(label_names) else "O" for t in tags]
        
        data.append({
            "text": ex.get("text", " ".join(tokens)),
            "tokens": tokens,
            "ner_tags": tags,
        })
    
    with open(output_path, "w") as f:
        json.dump(data, f, indent=2)


def download_dataset(name: str, config: dict) -> bool:
    """Download single dataset."""
    output_path = CACHE_DIR / config["output"]
    
    if output_path.exists():
        print(f"  [SKIP] {name}: exists at {output_path}")
        return True
    
    if config.get("synthetic"):
        print(f"  [GENERATE] {name}: synthetic placeholder")
        data = [{"text": f"Placeholder for {name}.", "tokens": ["Placeholder"], "ner_tags": ["O"]}]
        with open(output_path, "w") as f:
            json.dump(data, f)
        print(f"  [OK] {output_path}")
        return True
    
    hf_id = config["hf_id"]
    cfg = config.get("config")
    split = config.get("split", "test")
    
    print(f"  [DOWNLOAD] {name}: {hf_id} ({cfg or 'default'}, {split})")
    if "note" in config:
        print(f"    Note: {config['note']}")
    
    try:
        ds = load_dataset(hf_id, cfg, split=split)
        
        if str(output_path).endswith(".json"):
            to_json(ds, output_path)
        else:
            to_conll(ds, output_path)
        
        print(f"  [OK] {output_path} ({len(ds)} examples)")
        return True
    except Exception as e:
        print(f"  [ERROR] {name}: {e}")
        return False


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--dataset", type=str)
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    
    if args.list:
        for name, cfg in DATASETS.items():
            hf = cfg.get("hf_id", "synthetic")
            note = cfg.get("note", "")
            print(f"  {name}: {hf}" + (f" ({note})" if note else ""))
        return
    
    if args.dataset:
        if args.dataset not in DATASETS:
            print(f"Unknown: {args.dataset}. Use --list to see available.")
            return
        items = [(args.dataset, DATASETS[args.dataset])]
    elif args.all:
        items = list(DATASETS.items())
    else:
        print("Use --all or --dataset NAME")
        return
    
    ok, fail = 0, 0
    for name, cfg in items:
        if download_dataset(name, cfg):
            ok += 1
        else:
            fail += 1
    
    print(f"\nDone: {ok} ok, {fail} failed")


if __name__ == "__main__":
    main()
