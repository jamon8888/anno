#!/usr/bin/env python3
"""Fetch a sample from the KiloGram dataset.

Downloads the dense10.json (smallest file) to inspect the SND metric format.

Usage:
    uv run scripts/fetch_kilogram_sample.py
"""

import json
from pathlib import Path
from urllib.request import urlopen

# KiloGram dataset URLs
BASE_URL = "https://raw.githubusercontent.com/lil-lab/kilogram/main/dataset"
DENSE10_URL = f"{BASE_URL}/dense10.json"

OUTPUT_DIR = Path("testdata/kilogram")


def main():
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    
    print(f"Downloading dense10.json from {DENSE10_URL}...")
    
    with urlopen(DENSE10_URL) as response:
        data = json.loads(response.read().decode())
    
    # Inspect structure
    print(f"\nDataset contains {len(data)} tangrams")
    
    # Sample first tangram
    if data:
        sample_key = list(data.keys())[0]
        sample = data[sample_key]
        
        print(f"\nSample tangram: {sample_key}")
        print(f"  SND (Shape Naming Divergence): {sample.get('snd', 'N/A')}")
        print(f"  PND (Part Naming Divergence): {sample.get('pnd', 'N/A')}")
        print(f"  PSA (Part Segment Agreement): {sample.get('psa', 'N/A')}")
        print(f"  Number of annotations: {len(sample.get('annotations', []))}")
        
        if sample.get('annotations'):
            ann = sample['annotations'][0]
            print(f"\n  Sample annotation:")
            print(f"    Whole: {ann.get('whole', {}).get('wholeAnnotation', 'N/A')}")
            
        # Compute nameability from SND
        snd = sample.get('snd', 0.5)
        nameability = 1.0 - snd
        print(f"\n  Computed Nameability: {nameability:.3f}")
        print(f"  Classification: {'high' if nameability >= 0.7 else 'medium' if nameability >= 0.4 else 'low'}")
    
    # Save SND distribution for analysis
    snd_values = []
    for key, tangram in data.items():
        if 'snd' in tangram:
            snd_values.append({
                'tangram_id': key,
                'snd': tangram['snd'],
                'nameability': 1.0 - tangram['snd'],
                'num_annotations': len(tangram.get('annotations', [])),
            })
    
    output_file = OUTPUT_DIR / "snd_distribution.json"
    with open(output_file, 'w') as f:
        json.dump(snd_values, f, indent=2)
    
    print(f"\nSaved SND distribution to {output_file}")
    
    # Summary statistics
    if snd_values:
        snds = [v['snd'] for v in snd_values]
        print(f"\nSND Statistics (n={len(snds)}):")
        print(f"  Min: {min(snds):.3f}")
        print(f"  Max: {max(snds):.3f}")
        print(f"  Mean: {sum(snds)/len(snds):.3f}")
        
        # Count by nameability class
        high = sum(1 for s in snds if (1-s) >= 0.7)
        low = sum(1 for s in snds if (1-s) < 0.4)
        medium = len(snds) - high - low
        print(f"\n  High nameability: {high}")
        print(f"  Medium nameability: {medium}")
        print(f"  Low nameability: {low}")


if __name__ == "__main__":
    main()
