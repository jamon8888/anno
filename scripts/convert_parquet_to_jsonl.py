#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "pyarrow>=14.0.0",
#     "pandas>=2.0.0",
# ]
# ///
"""Convert parquet files to JSONL for anno dataset loading.

This script converts HuggingFace parquet dataset files to JSONL format
that can be parsed by anno's Rust-based loaders.

Usage:
    uv run scripts/convert_parquet_to_jsonl.py
"""

import json
from pathlib import Path
import pyarrow.parquet as pq

# Platform-aware cache directory
import sys
if sys.platform == "darwin":
    CACHE_DIR = Path.home() / "Library" / "Caches" / "anno" / "datasets"
else:
    CACHE_DIR = Path.home() / ".cache" / "anno" / "datasets"

CONVERSIONS = [
    ("agnews.parquet", "agnews.jsonl"),
    ("dbpedia14.parquet", "dbpedia14.jsonl"),
    ("yahoo_answers.parquet", "yahoo_answers.jsonl"),
]


def convert_parquet_to_jsonl(parquet_path: Path, jsonl_path: Path) -> int:
    """Convert a parquet file to JSONL format."""
    if not parquet_path.exists():
        print(f"  Skipping {parquet_path.name}: not found")
        return 0

    table = pq.read_table(parquet_path)
    df = table.to_pandas()

    count = 0
    with open(jsonl_path, "w") as f:
        for _, row in df.iterrows():
            record = row.to_dict()
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
            count += 1

    print(f"  Converted {parquet_path.name} -> {jsonl_path.name} ({count} records)")
    return count


def main():
    print("Converting parquet files to JSONL for anno...")
    print(f"Cache directory: {CACHE_DIR}\n")

    total = 0
    for parquet_name, jsonl_name in CONVERSIONS:
        parquet_path = CACHE_DIR / parquet_name
        jsonl_path = CACHE_DIR / jsonl_name
        total += convert_parquet_to_jsonl(parquet_path, jsonl_path)

    print(f"\nTotal: {total} records converted")


if __name__ == "__main__":
    main()
