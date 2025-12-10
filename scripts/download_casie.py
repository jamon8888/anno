#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "requests>=2.31.0",
# ]
# ///
"""Download and combine CASIE cybersecurity event extraction dataset.

CASIE has ~1000 annotation files in data/annotation/*.json.
This script fetches them all and combines into a single JSONL file.

Usage:
    uv run scripts/download_casie.py
"""

import json
import sys
from pathlib import Path
import requests

# Platform-aware cache directory
if sys.platform == "darwin":
    CACHE_DIR = Path.home() / "Library" / "Caches" / "anno" / "datasets"
else:
    CACHE_DIR = Path.home() / ".cache" / "anno" / "datasets"

CASIE_API_URL = "https://api.github.com/repos/Ebiquity/CASIE/contents/data/annotation"
CASIE_RAW_BASE = "https://raw.githubusercontent.com/Ebiquity/CASIE/master/data/annotation"


def get_annotation_files() -> list[str]:
    """Get list of annotation file names from GitHub API."""
    print("Fetching file list from GitHub API...")
    resp = requests.get(CASIE_API_URL)
    resp.raise_for_status()
    files = [f["name"] for f in resp.json() if f["name"].endswith(".json")]
    print(f"  Found {len(files)} annotation files")
    return files


def download_file(filename: str) -> dict | None:
    """Download a single annotation file."""
    url = f"{CASIE_RAW_BASE}/{filename}"
    try:
        resp = requests.get(url, timeout=10)
        resp.raise_for_status()
        return resp.json()
    except Exception as e:
        print(f"  Warning: Failed to download {filename}: {e}")
        return None


def main():
    print("Downloading CASIE dataset...")
    print(f"Cache directory: {CACHE_DIR}\n")

    # Get list of files
    files = get_annotation_files()

    # Download and combine
    output_path = CACHE_DIR / "casie.jsonl"
    total = 0
    events_count = 0

    print(f"Downloading {len(files)} files...")
    with open(output_path, "w") as out:
        for i, filename in enumerate(files):
            if (i + 1) % 100 == 0:
                print(f"  Progress: {i + 1}/{len(files)}")

            data = download_file(filename)
            if data:
                # Count events
                if "cyberevent" in data:
                    events_count += len(data.get("cyberevent", {}).get("hopper", []))
                
                out.write(json.dumps(data, ensure_ascii=False) + "\n")
                total += 1

    print(f"\nDone! Saved {total} documents to {output_path}")
    print(f"  Total events found: {events_count}")
    print(f"  File size: {output_path.stat().st_size / 1024 / 1024:.2f} MB")


if __name__ == "__main__":
    main()
