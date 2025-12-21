#!/usr/bin/env python3
"""
Merge prediction cache shards from S3.

Usage:
    python3 merge_cache.py [--bucket BUCKET] [--prefix cache/]

1. Downloads all predictions-*.jsonl from S3.
2. Merges them into a single file, deduplicating by cache key.
3. Uploads predictions-merged.jsonl back to S3.
"""

import argparse
import json
import logging
import boto3
import tempfile
import os
import glob
from pathlib import Path

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger(__name__)

def merge_caches(input_files: list[str], output_file: str) -> dict:
    """Merge JSONL cache files, deduplicating by key."""
    seen_keys = set()
    stats = {"read": 0, "written": 0, "files": 0, "duplicates": 0}
    
    with open(output_file, "w") as out_f:
        for input_file in input_files:
            stats["files"] += 1
            logger.info(f"Processing {input_file}...")
            try:
                with open(input_file, "r") as in_f:
                    for line in in_f:
                        line = line.strip()
                        if not line:
                            continue
                        
                        stats["read"] += 1
                        try:
                            data = json.loads(line)
                            # Key logic: text_hash is the unique identifier in PredictionCache
                            key = data.get("text_hash")
                            
                            if key and key not in seen_keys:
                                out_f.write(line + "\n")
                                seen_keys.add(key)
                                stats["written"] += 1
                            else:
                                stats["duplicates"] += 1
                        except json.JSONDecodeError:
                            logger.warning(f"Invalid JSON in {input_file}: {line[:100]}...")
            except Exception as e:
                logger.error(f"Error reading {input_file}: {e}")
                
    return stats

def main():
    parser = argparse.ArgumentParser(description="Merge prediction cache shards")
    parser.add_argument("--bucket", default=os.environ.get("ANNO_SPOT_BUCKET", "arc-anno-data"))
    parser.add_argument("--prefix", default="cache/")
    parser.add_argument("--dry-run", action="store_true", help="Don't upload result")
    args = parser.parse_args()
    
    s3 = boto3.client("s3")
    bucket = args.bucket
    prefix = args.prefix
    
    with tempfile.TemporaryDirectory() as temp_dir:
        logger.info(f"Listing objects in s3://{bucket}/{prefix}predictions-*")
        
        paginator = s3.get_paginator("list_objects_v2")
        pages = paginator.paginate(Bucket=bucket, Prefix=prefix + "predictions-")
        
        downloaded_files = []
        for page in pages:
            for obj in page.get("Contents", []):
                key = obj["Key"]
                if key.endswith("predictions-merged.jsonl"):
                    # Optionally include the old merged file to preserve history
                    pass
                elif not key.endswith(".jsonl"):
                    continue
                    
                local_path = os.path.join(temp_dir, os.path.basename(key))
                logger.info(f"Downloading {key}...")
                s3.download_file(bucket, key, local_path)
                downloaded_files.append(local_path)
        
        if not downloaded_files:
            logger.info("No cache shards found.")
            return
            
        merged_path = os.path.join(temp_dir, "predictions-merged.jsonl")
        stats = merge_caches(downloaded_files, merged_path)
        
        logger.info(f"Merge complete: {stats}")
        
        if not args.dry_run and stats["written"] > 0:
            target_key = f"{prefix}predictions-merged.jsonl"
            logger.info(f"Uploading merged cache to s3://{bucket}/{target_key}")
            s3.upload_file(merged_path, bucket, target_key)
            
            # Optional: Clean up shards?
            # logger.info("Cleaning up shards...")
            # for f in downloaded_files:
            #     # extract key from filename? requires mapping back
            #     pass
        else:
            logger.info("Dry run or empty result, skipping upload")

if __name__ == "__main__":
    main()

