#!/usr/bin/env python3
"""
Aggregate spot evaluation results into hierarchical summary.

Usage:
    python aggregate.py [--download]
    
Outputs:
    reports/eval-results.jsonl  - Raw append-only log (source of truth)
    reports/eval-summary.json   - Hierarchical aggregation
    reports/RESULTS.md          - Human-readable summary (GitHub-renderable)
"""

import argparse
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

# ============================================================================
# Parsing
# ============================================================================

def parse_meta_comment(content: str) -> dict:
    """Parse the HTML comment metadata at the top of result files."""
    match = re.search(r'<!--\s*_meta:\s*(.+?)\s*-->', content)
    if not match:
        return {}
    
    meta = {}
    for pair in match.group(1).split():
        if '=' in pair:
            key, value = pair.split('=', 1)
            meta[key] = value
    return meta


def parse_markdown_table(content: str) -> list[dict]:
    """Parse markdown tables to extract metrics."""
    results = []
    # | Dataset | Backend | F1 | P | R | N | ms |
    pattern = r'\|\s*(\w+)\s*\|\s*(\w+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|'
    
    for match in re.finditer(pattern, content):
        dataset, backend, f1, precision, recall, n, ms = match.groups()
        if dataset.lower() not in ('dataset', '---', '-'):
            results.append({
                'dataset': dataset,
                'backend': backend,
                'f1': float(f1),
                'precision': float(precision),
                'recall': float(recall),
                'n': int(n),
                'duration_ms': int(ms),
            })
    return results


def parse_result_file(path: Path) -> dict | None:
    """Parse a single result markdown file."""
    content = path.read_text()
    meta = parse_meta_comment(content)
    if not meta:
        return None
    
    # Extract timestamp from filename
    ts_match = re.search(r'(\d{8})_(\d{6})', path.stem)
    if ts_match:
        date_str, time_str = ts_match.groups()
        timestamp = datetime.strptime(f'{date_str}{time_str}', '%Y%m%d%H%M%S')
        meta['timestamp'] = timestamp.isoformat() + 'Z'
    
    table_results = parse_markdown_table(content)
    
    if table_results:
        r = table_results[0]
        return {
            'timestamp': meta.get('timestamp', datetime.now(timezone.utc).isoformat()),
            'backend': meta.get('backend', r.get('backend', 'unknown')),
            'dataset': meta.get('dataset', r.get('dataset', 'unknown')),
            'seed': int(meta.get('seed', 42)),
            'f1': r['f1'],
            'precision': r['precision'],
            'recall': r['recall'],
            'n': r['n'],
            'duration_ms': r['duration_ms'],
        }
    else:
        return {
            'timestamp': meta.get('timestamp', datetime.now(timezone.utc).isoformat()),
            'backend': meta.get('backend', 'unknown'),
            'dataset': meta.get('dataset', 'unknown'),
            'seed': int(meta.get('seed', 42)),
            'f1': 0, 'precision': 0, 'recall': 0, 'n': 0, 'duration_ms': 0,
            'error': 'no_results',
        }


# ============================================================================
# JSONL (raw log)
# ============================================================================

def load_jsonl(path: Path) -> list[dict]:
    """Load results from JSONL file."""
    if not path.exists():
        return []
    results = []
    for line in path.read_text().strip().split('\n'):
        if line.strip():
            try:
                results.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return results


def append_jsonl(path: Path, new_results: list[dict]) -> int:
    """Append new results to JSONL, deduplicating."""
    existing = load_jsonl(path)
    existing_keys = {f"{r.get('timestamp')}|{r.get('backend')}|{r.get('dataset')}" for r in existing}
    
    added = 0
    with path.open('a') as f:
        for r in new_results:
            key = f"{r.get('timestamp')}|{r.get('backend')}|{r.get('dataset')}"
            if key not in existing_keys:
                f.write(json.dumps(r, separators=(',', ':')) + '\n')
                existing_keys.add(key)
                added += 1
    return added


# ============================================================================
# Hierarchical Summary
# ============================================================================

def get_f1(r: dict) -> float:
    """Extract F1 from result (handles both flat and nested metrics)."""
    if 'metrics' in r:
        return r['metrics'].get('f1', 0)
    return r.get('f1', 0)


def get_precision(r: dict) -> float:
    """Extract precision from result."""
    if 'metrics' in r:
        return r['metrics'].get('precision', 0)
    return r.get('precision', 0)


def get_recall(r: dict) -> float:
    """Extract recall from result."""
    if 'metrics' in r:
        return r['metrics'].get('recall', 0)
    return r.get('recall', 0)


def build_summary(results: list[dict]) -> dict:
    """Build hierarchical summary from flat results."""
    if not results:
        return {'generated': datetime.now(timezone.utc).isoformat(), 'backends': {}, 'datasets': {}}
    
    # Group by backend
    by_backend = {}
    for r in results:
        b = r.get('backend', 'unknown')
        if b not in by_backend:
            by_backend[b] = []
        by_backend[b].append(r)
    
    # Group by dataset
    by_dataset = {}
    for r in results:
        d = r.get('dataset', 'unknown')
        if d not in by_dataset:
            by_dataset[d] = []
        by_dataset[d].append(r)
    
    # Build backend summaries
    backends = {}
    for backend, runs in sorted(by_backend.items()):
        successful = [r for r in runs if get_f1(r) > 0]
        if successful:
            avg_f1 = sum(get_f1(r) for r in successful) / len(successful)
            best = max(successful, key=get_f1)
        else:
            avg_f1 = 0
            best = None
        
        datasets = {}
        for r in runs:
            d = r.get('dataset')
            if d not in datasets or get_f1(r) > datasets[d].get('f1', 0):
                datasets[d] = {'f1': get_f1(r), 'p': get_precision(r), 'r': get_recall(r)}
        
        backends[backend] = {
            'runs': len(runs),
            'avg_f1': round(avg_f1, 1),
            'best_f1': round(get_f1(best), 1) if best else 0,
            'best_dataset': best.get('dataset') if best else None,
            'datasets': datasets,
        }
    
    # Build dataset summaries
    datasets = {}
    for dataset, runs in sorted(by_dataset.items()):
        successful = [r for r in runs if get_f1(r) > 0]
        if successful:
            best = max(successful, key=get_f1)
        else:
            best = None
        
        datasets[dataset] = {
            'runs': len(runs),
            'best_f1': round(get_f1(best), 1) if best else 0,
            'best_backend': best.get('backend') if best else None,
            'backends_tested': len(set(r.get('backend') for r in runs)),
        }
    
    # Overall stats
    all_f1 = [get_f1(r) for r in results if get_f1(r) > 0]
    overall_best = max(results, key=get_f1) if results else None
    
    return {
        'generated': datetime.now(timezone.utc).isoformat(),
        'total_runs': len(results),
        'successful_runs': len(all_f1),
        'best_f1': round(get_f1(overall_best), 1) if overall_best else 0,
        'best': f"{overall_best.get('backend')}/{overall_best.get('dataset')}" if overall_best and get_f1(overall_best) > 0 else None,
        'avg_f1': round(sum(all_f1) / len(all_f1), 1) if all_f1 else 0,
        'backends': backends,
        'datasets': datasets,
    }


# ============================================================================
# Markdown Output
# ============================================================================

def generate_markdown(summary: dict) -> str:
    """Generate clean markdown summary."""
    lines = [
        "# Evaluation Results",
        "",
        f"Generated: {summary['generated'][:19].replace('T', ' ')} UTC",
        "",
        "## Summary",
        "",
        f"| Metric | Value |",
        f"|--------|-------|",
        f"| Total runs | {summary['total_runs']} |",
        f"| Successful | {summary['successful_runs']} |",
        f"| Best F1 | {summary['best_f1']}% |",
        f"| Best | {summary['best']} |",
        f"| Avg F1 | {summary['avg_f1']}% |",
        "",
        "## By Backend",
        "",
        "| Backend | Avg F1 | Best F1 | Best Dataset | Runs |",
        "|---------|--------|---------|--------------|------|",
    ]
    
    for backend, data in sorted(summary['backends'].items(), key=lambda x: -x[1]['avg_f1']):
        lines.append(f"| {backend} | {data['avg_f1']}% | {data['best_f1']}% | {data['best_dataset'] or '-'} | {data['runs']} |")
    
    lines.extend([
        "",
        "## By Dataset",
        "",
        "| Dataset | Best F1 | Best Backend | Backends Tested |",
        "|---------|---------|--------------|-----------------|",
    ])
    
    for dataset, data in sorted(summary['datasets'].items(), key=lambda x: -x[1]['best_f1']):
        lines.append(f"| {dataset} | {data['best_f1']}% | {data['best_backend'] or '-'} | {data['backends_tested']} |")
    
    lines.extend([
        "",
        "## Backend × Dataset Matrix",
        "",
    ])
    
    # Build matrix
    all_datasets = sorted(summary['datasets'].keys())
    all_backends = sorted(summary['backends'].keys(), key=lambda b: -summary['backends'][b]['avg_f1'])
    
    header = "| Backend | " + " | ".join(all_datasets) + " |"
    sep = "|---------|" + "|".join(["------" for _ in all_datasets]) + "|"
    lines.append(header)
    lines.append(sep)
    
    for backend in all_backends:
        cells = []
        for dataset in all_datasets:
            f1 = summary['backends'][backend]['datasets'].get(dataset, {}).get('f1', 0)
            cells.append(f"{f1:.0f}" if f1 > 0 else "-")
        lines.append(f"| {backend} | " + " | ".join(cells) + " |")
    
    lines.extend([
        "",
        "---",
        f"*Raw data: [eval-results.jsonl](eval-results.jsonl)*",
    ])
    
    return "\n".join(lines)


# ============================================================================
# S3 Download
# ============================================================================

def download_from_s3(bucket: str, prefix: str, local_dir: Path) -> list[Path]:
    """Download result files from S3."""
    local_dir.mkdir(parents=True, exist_ok=True)
    
    result = subprocess.run(
        ['aws', 's3', 'ls', f's3://{bucket}/{prefix}', '--recursive'],
        capture_output=True, text=True
    )
    
    files = []
    for line in result.stdout.strip().split('\n'):
        if not line.strip():
            continue
        parts = line.split()
        if len(parts) >= 4:
            key = parts[-1]
            if key.endswith('.md'):
                local_path = local_dir / key.replace('/', '_')
                subprocess.run(
                    ['aws', 's3', 'cp', f's3://{bucket}/{key}', str(local_path)],
                    capture_output=True
                )
                files.append(local_path)
    return files


# ============================================================================
# Main
# ============================================================================

def main():
    parser = argparse.ArgumentParser(description='Aggregate evaluation results')
    parser.add_argument('--download', action='store_true', help='Download from S3')
    parser.add_argument('--bucket', default='arc-anno-data')
    parser.add_argument('--prefix', default='results/')
    parser.add_argument('--local-dir', default='reports/spot/')
    parser.add_argument('--output', default='reports/eval-results.jsonl')
    parser.add_argument('--summary', default='reports/eval-summary.json')
    parser.add_argument('--markdown', default='reports/RESULTS.md')
    
    args = parser.parse_args()
    
    output_path = Path(args.output)
    summary_path = Path(args.summary)
    markdown_path = Path(args.markdown)
    local_dir = Path(args.local_dir)
    
    output_path.parent.mkdir(parents=True, exist_ok=True)
    
    # Download if requested
    if args.download:
        print(f"Downloading from s3://{args.bucket}/{args.prefix}...")
        files = download_from_s3(args.bucket, args.prefix, local_dir)
        print(f"Downloaded {len(files)} files")
    
    # Parse result files
    result_files = list(local_dir.glob('**/*.md'))
    print(f"Parsing {len(result_files)} files...")
    
    new_results = []
    for path in result_files:
        result = parse_result_file(path)
        if result:
            new_results.append(result)
    
    # Append to JSONL
    added = append_jsonl(output_path, new_results)
    print(f"Added {added} new results to {output_path}")
    
    # Load all results and build summary
    all_results = load_jsonl(output_path)
    summary = build_summary(all_results)
    
    # Write summary JSON
    summary_path.write_text(json.dumps(summary, indent=2))
    print(f"Summary: {summary_path}")
    
    # Write markdown
    markdown = generate_markdown(summary)
    markdown_path.write_text(markdown)
    print(f"Markdown: {markdown_path}")
    
    # Print quick summary
    print(f"\n{summary['total_runs']} runs | Best: {summary['best']} @ {summary['best_f1']}% F1")


if __name__ == '__main__':
    main()
