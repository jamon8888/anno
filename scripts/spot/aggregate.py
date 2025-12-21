#!/usr/bin/env python3
"""Aggregate spot evaluation results from S3 into local reports."""

import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

BUCKET = os.environ.get("ANNO_SPOT_BUCKET", "arc-anno-data")
REGION = os.environ.get("ANNO_SPOT_REGION", "us-east-1")
LOCAL_DIR = Path("reports/spot")


def s3_ls():
    """List results in S3."""
    cmd = ["aws", "s3", "ls", f"s3://{BUCKET}/results/", "--region", REGION]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        return []
    
    files = []
    for line in result.stdout.strip().split("\n"):
        if line:
            parts = line.split()
            if len(parts) >= 4:
                files.append(parts[-1])
    return files


def s3_download(filename):
    """Download a single file from S3."""
    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    local_path = LOCAL_DIR / filename
    if local_path.exists():
        return local_path
    
    cmd = [
        "aws", "s3", "cp",
        f"s3://{BUCKET}/results/{filename}",
        str(local_path),
        "--region", REGION
    ]
    subprocess.run(cmd, capture_output=True)
    return local_path


def parse_result_file(path):
    """Parse a benchmark result file."""
    content = path.read_text()
    
    # Extract metrics from markdown table
    results = []
    in_table = False
    for line in content.split("\n"):
        if "| Dataset | Backend | F1 |" in line:
            in_table = True
            continue
        if in_table and line.startswith("|"):
            if "---" in line:
                continue
            parts = [p.strip() for p in line.split("|")[1:-1]]
            if len(parts) >= 6 and parts[2] not in ("✗", "F1"):
                try:
                    results.append({
                        "dataset": parts[0],
                        "backend": parts[1],
                        "f1": float(parts[2]),
                        "precision": float(parts[3]),
                        "recall": float(parts[4]),
                        "n": int(parts[5]),
                    })
                except ValueError:
                    pass
        elif in_table and not line.startswith("|"):
            in_table = False
    
    return results


def aggregate():
    """Aggregate all results."""
    print("=== Aggregating Spot Results ===\n")
    
    # List and download
    files = s3_ls()
    print(f"Found {len(files)} result files in S3")
    
    all_results = []
    for f in files:
        path = s3_download(f)
        results = parse_result_file(path)
        all_results.extend(results)
        print(f"  {f}: {len(results)} results")
    
    if not all_results:
        print("\nNo results to aggregate")
        return
    
    # Aggregate by backend
    by_backend = defaultdict(list)
    for r in all_results:
        by_backend[r["backend"]].append(r["f1"])
    
    print("\n=== Backend Summary ===")
    for backend, f1s in sorted(by_backend.items()):
        avg = sum(f1s) / len(f1s) if f1s else 0
        print(f"  {backend}: {len(f1s)} runs, avg F1={avg:.1f}%")
    
    # Aggregate by dataset
    by_dataset = defaultdict(list)
    for r in all_results:
        by_dataset[r["dataset"]].append(r["f1"])
    
    print("\n=== Dataset Summary ===")
    for dataset, f1s in sorted(by_dataset.items()):
        avg = sum(f1s) / len(f1s) if f1s else 0
        print(f"  {dataset}: {len(f1s)} runs, avg F1={avg:.1f}%")
    
    # Save aggregated JSON
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    agg_path = LOCAL_DIR / f"aggregated-{timestamp}.json"
    agg_data = {
        "timestamp": timestamp,
        "total_results": len(all_results),
        "by_backend": {k: {"count": len(v), "avg_f1": sum(v)/len(v) if v else 0} 
                       for k, v in by_backend.items()},
        "by_dataset": {k: {"count": len(v), "avg_f1": sum(v)/len(v) if v else 0}
                       for k, v in by_dataset.items()},
        "results": all_results,
    }
    agg_path.write_text(json.dumps(agg_data, indent=2))
    print(f"\nAggregated results saved to: {agg_path}")
    
    # Export badness for CI
    badness_path = LOCAL_DIR / "badness-history.csv"
    with open(badness_path, "w") as f:
        for r in all_results:
            # Badness = 100 - F1 (simple metric)
            badness = int(100 - r["f1"])
            f.write(f"{r['backend']},{r['dataset']},{badness}\n")
    print(f"Badness history saved to: {badness_path}")


if __name__ == "__main__":
    aggregate()

