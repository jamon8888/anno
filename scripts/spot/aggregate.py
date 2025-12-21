#!/usr/bin/env python3
"""
Aggregate spot evaluation results into JSONL and generate HTML dashboard.

Usage:
    python aggregate.py [--download] [--output reports/eval-results.jsonl]
    
This script:
1. Downloads .md results from S3 (if --download)
2. Parses the markdown tables to extract metrics
3. Appends new results to the JSONL file (deduped by timestamp+backend+dataset)
4. Generates an HTML dashboard for visualization
"""

import argparse
import json
import re
import subprocess
import sys
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
    
    # Find NER results table
    # | Dataset | Backend | F1 | P | R | N | ms |
    ner_pattern = r'\|\s*(\w+)\s*\|\s*(\w+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|'
    
    for match in re.finditer(ner_pattern, content):
        dataset, backend, f1, precision, recall, n, ms = match.groups()
        if dataset.lower() not in ('dataset', '---', '-'):  # Skip headers
            results.append({
                'dataset': dataset,
                'backend': backend,
                'metrics': {
                    'f1': float(f1),
                    'precision': float(precision),
                    'recall': float(recall),
                },
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
    
    # Extract timestamp from filename: seed_42_20251221_025204.md
    filename = path.stem
    ts_match = re.search(r'(\d{8})_(\d{6})', filename)
    if ts_match:
        date_str, time_str = ts_match.groups()
        timestamp = datetime.strptime(f'{date_str}{time_str}', '%Y%m%d%H%M%S')
        meta['timestamp'] = timestamp.isoformat() + 'Z'
    
    # Parse metrics from table
    table_results = parse_markdown_table(content)
    
    if table_results:
        # Use table data
        result = table_results[0]
        result['timestamp'] = meta.get('timestamp', datetime.now(timezone.utc).isoformat() + 'Z')
        result['backend'] = meta.get('backend', result.get('backend', 'unknown'))
        result['dataset'] = meta.get('dataset', result.get('dataset', 'unknown'))
        result['seed'] = int(meta.get('seed', 42))
        result['task'] = meta.get('task', 'ner')
        result['config'] = {}
        result['source'] = 'spot'
        result['instance'] = meta.get('instance', '')
        return result
    else:
        # No table data (empty result)
        return {
            'timestamp': meta.get('timestamp', datetime.now(timezone.utc).isoformat() + 'Z'),
            'backend': meta.get('backend', 'unknown'),
            'dataset': meta.get('dataset', 'unknown'),
            'task': meta.get('task', 'ner'),
            'seed': int(meta.get('seed', 42)),
            'config': {},
            'metrics': {'f1': 0, 'precision': 0, 'recall': 0},
            'n': 0,
            'duration_ms': int(meta.get('duration', 0).rstrip('s')) if meta.get('duration') else 0,
            'source': 'spot',
            'instance': meta.get('instance', ''),
            'error': 'no_results',
        }


# ============================================================================
# JSONL Management
# ============================================================================

def load_existing_results(jsonl_path: Path) -> list[dict]:
    """Load existing results from JSONL file."""
    if not jsonl_path.exists():
        return []
    
    results = []
    for line in jsonl_path.read_text().strip().split('\n'):
        if line.strip():
            try:
                results.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return results


def result_key(r: dict) -> str:
    """Generate a unique key for deduplication."""
    return f"{r.get('timestamp', '')}|{r.get('backend', '')}|{r.get('dataset', '')}|{r.get('seed', '')}"


def append_results(jsonl_path: Path, new_results: list[dict]) -> int:
    """Append new results to JSONL, deduplicating by key."""
    existing = load_existing_results(jsonl_path)
    existing_keys = {result_key(r) for r in existing}
    
    added = 0
    with jsonl_path.open('a') as f:
        for result in new_results:
            key = result_key(result)
            if key not in existing_keys:
                f.write(json.dumps(result, separators=(',', ':')) + '\n')
                existing_keys.add(key)
                added += 1
    
    return added


# ============================================================================
# HTML Dashboard
# ============================================================================

def generate_html_dashboard(jsonl_path: Path, html_path: Path):
    """Generate an interactive HTML dashboard from JSONL results."""
    results = load_existing_results(jsonl_path)
    
    if not results:
        html_path.write_text("<html><body><h1>No results yet</h1></body></html>")
        return
    
    # Group by backend, then by dataset
    by_backend = {}
    for r in results:
        backend = r.get('backend', 'unknown')
        if backend not in by_backend:
            by_backend[backend] = []
        by_backend[backend].append(r)
    
    # Get all datasets
    all_datasets = sorted({r.get('dataset', '') for r in results})
    all_backends = sorted(by_backend.keys())
    
    # Build pivot table: backend × dataset → best F1
    pivot = {}
    for backend in all_backends:
        pivot[backend] = {}
        for r in by_backend[backend]:
            dataset = r.get('dataset', '')
            f1 = r.get('metrics', {}).get('f1', 0)
            if dataset not in pivot[backend] or f1 > pivot[backend][dataset].get('f1', 0):
                pivot[backend][dataset] = {
                    'f1': f1,
                    'precision': r.get('metrics', {}).get('precision', 0),
                    'recall': r.get('metrics', {}).get('recall', 0),
                    'n': r.get('n', 0),
                    'duration_ms': r.get('duration_ms', 0),
                }
    
    # Generate HTML
    html = f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Anno Evaluation Dashboard</title>
    <style>
        :root {{
            --bg: #0d1117;
            --fg: #c9d1d9;
            --accent: #58a6ff;
            --success: #3fb950;
            --warning: #d29922;
            --border: #30363d;
        }}
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, monospace;
            background: var(--bg);
            color: var(--fg);
            padding: 2rem;
            line-height: 1.6;
        }}
        h1 {{
            color: var(--accent);
            margin-bottom: 0.5rem;
            font-size: 1.8rem;
        }}
        .meta {{
            color: #8b949e;
            font-size: 0.9rem;
            margin-bottom: 2rem;
        }}
        table {{
            border-collapse: collapse;
            width: 100%;
            margin: 1rem 0;
            font-size: 0.9rem;
        }}
        th, td {{
            border: 1px solid var(--border);
            padding: 0.6rem 0.8rem;
            text-align: right;
        }}
        th {{
            background: #161b22;
            color: var(--accent);
            font-weight: 600;
        }}
        th:first-child, td:first-child {{
            text-align: left;
            font-weight: 500;
        }}
        tr:hover {{ background: #161b22; }}
        .best {{ color: var(--success); font-weight: bold; }}
        .mid {{ color: var(--warning); }}
        .low {{ color: #f85149; }}
        .zero {{ color: #484f58; }}
        .section {{ margin: 2rem 0; }}
        .section h2 {{
            font-size: 1.2rem;
            color: var(--fg);
            margin-bottom: 1rem;
            padding-bottom: 0.5rem;
            border-bottom: 1px solid var(--border);
        }}
        .stats {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
            gap: 1rem;
            margin-bottom: 2rem;
        }}
        .stat {{
            background: #161b22;
            border: 1px solid var(--border);
            border-radius: 6px;
            padding: 1rem;
        }}
        .stat-value {{ font-size: 1.5rem; color: var(--accent); }}
        .stat-label {{ font-size: 0.8rem; color: #8b949e; }}
        .raw-link {{
            margin-top: 2rem;
            font-size: 0.85rem;
            color: #8b949e;
        }}
        .raw-link a {{ color: var(--accent); }}
        .charts {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(400px, 1fr));
            gap: 2rem;
            margin: 2rem 0;
        }}
        .chart-container {{
            background: #161b22;
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1.5rem;
        }}
        .chart-container h3 {{
            font-size: 1rem;
            color: var(--fg);
            margin-bottom: 1rem;
        }}
        canvas {{ max-width: 100%; }}
    </style>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.min.js"></script>
</head>
<body>
    <h1>📊 Anno Evaluation Dashboard</h1>
    <p class="meta">Generated: {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M:%S')} UTC | {len(results)} results | {len(all_backends)} backends | {len(all_datasets)} datasets</p>
    
    <div class="stats">
        <div class="stat">
            <div class="stat-value">{len(results)}</div>
            <div class="stat-label">Total Evaluations</div>
        </div>
        <div class="stat">
            <div class="stat-value">{len(all_backends)}</div>
            <div class="stat-label">Backends Tested</div>
        </div>
        <div class="stat">
            <div class="stat-value">{len(all_datasets)}</div>
            <div class="stat-label">Datasets</div>
        </div>
        <div class="stat">
            <div class="stat-value">{max((r.get('metrics', {}).get('f1', 0) for r in results), default=0):.1f}%</div>
            <div class="stat-label">Best F1</div>
        </div>
    </div>
    
    <div class="charts">
        <div class="chart-container">
            <h3>Backend Comparison (Best F1 per Dataset)</h3>
            <canvas id="backendChart"></canvas>
        </div>
        <div class="chart-container">
            <h3>Dataset Coverage</h3>
            <canvas id="datasetChart"></canvas>
        </div>
        <div class="chart-container">
            <h3>Precision vs Recall</h3>
            <canvas id="prChart"></canvas>
        </div>
        <div class="chart-container">
            <h3>Latency vs F1 (Quality-Speed Tradeoff)</h3>
            <canvas id="latencyChart"></canvas>
        </div>
    </div>
    
    <div class="section">
        <h2>Backend × Dataset Matrix (Best F1 %)</h2>
        <table>
            <thead>
                <tr>
                    <th>Backend</th>
                    {''.join(f'<th>{d}</th>' for d in all_datasets)}
                    <th>Avg</th>
                </tr>
            </thead>
            <tbody>
'''
    
    # Find best per dataset for highlighting
    best_per_dataset = {}
    for dataset in all_datasets:
        best = 0
        for backend in all_backends:
            f1 = pivot.get(backend, {}).get(dataset, {}).get('f1', 0)
            if f1 > best:
                best = f1
        best_per_dataset[dataset] = best
    
    for backend in all_backends:
        row_f1s = []
        cells = []
        for dataset in all_datasets:
            data = pivot.get(backend, {}).get(dataset, {})
            f1 = data.get('f1', 0)
            row_f1s.append(f1)
            
            # Color class
            if f1 == 0:
                cls = 'zero'
            elif f1 >= best_per_dataset[dataset] - 0.1:
                cls = 'best'
            elif f1 >= best_per_dataset[dataset] * 0.7:
                cls = 'mid'
            else:
                cls = 'low'
            
            cells.append(f'<td class="{cls}">{f1:.1f}</td>' if f1 > 0 else '<td class="zero">-</td>')
        
        avg = sum(row_f1s) / len(row_f1s) if row_f1s else 0
        html += f'''                <tr>
                    <td>{backend}</td>
                    {''.join(cells)}
                    <td>{avg:.1f}</td>
                </tr>
'''
    
    html += '''            </tbody>
        </table>
    </div>
    
    <div class="section">
        <h2>All Results</h2>
        <table>
            <thead>
                <tr>
                    <th>Backend</th>
                    <th>Dataset</th>
                    <th>F1</th>
                    <th>P</th>
                    <th>R</th>
                    <th>N</th>
                    <th>Time (s)</th>
                    <th>Timestamp</th>
                </tr>
            </thead>
            <tbody>
'''
    
    for r in sorted(results, key=lambda x: (-x.get('metrics', {}).get('f1', 0), x.get('backend', ''))):
        m = r.get('metrics', {})
        ts = r.get('timestamp', '')[:19].replace('T', ' ')
        html += f'''                <tr>
                    <td>{r.get('backend', '-')}</td>
                    <td>{r.get('dataset', '-')}</td>
                    <td>{m.get('f1', 0):.1f}</td>
                    <td>{m.get('precision', 0):.1f}</td>
                    <td>{m.get('recall', 0):.1f}</td>
                    <td>{r.get('n', 0)}</td>
                    <td>{r.get('duration_ms', 0) / 1000:.1f}</td>
                    <td>{ts}</td>
                </tr>
'''
    
    html += '''            </tbody>
        </table>
    </div>
    
    <p class="raw-link">
        Raw data: <a href="eval-results.jsonl">eval-results.jsonl</a>
    </p>
'''
    
    # Prepare chart data
    backend_avgs = {}
    for backend in all_backends:
        f1_values = [pivot.get(backend, {}).get(d, {}).get('f1', 0) for d in all_datasets]
        non_zero = [v for v in f1_values if v > 0]
        backend_avgs[backend] = sum(non_zero) / len(non_zero) if non_zero else 0
    
    # Sort backends by average F1
    sorted_backends = sorted(backend_avgs.keys(), key=lambda b: backend_avgs[b], reverse=True)
    
    # Dataset coverage (how many backends have results)
    dataset_coverage = {}
    for dataset in all_datasets:
        count = sum(1 for b in all_backends if pivot.get(b, {}).get(dataset, {}).get('f1', 0) > 0)
        best_f1 = max((pivot.get(b, {}).get(dataset, {}).get('f1', 0) for b in all_backends), default=0)
        dataset_coverage[dataset] = {'count': count, 'best_f1': best_f1}
    
    # Color palette
    colors = [
        'rgba(88, 166, 255, 0.8)',   # blue
        'rgba(63, 185, 80, 0.8)',    # green  
        'rgba(210, 153, 34, 0.8)',   # yellow
        'rgba(248, 81, 73, 0.8)',    # red
        'rgba(163, 113, 247, 0.8)',  # purple
        'rgba(56, 139, 253, 0.8)',   # light blue
        'rgba(219, 97, 162, 0.8)',   # pink
        'rgba(121, 192, 255, 0.8)',  # cyan
    ]
    
    # Generate grouped bar chart data for backend comparison
    chart_datasets = []
    for i, dataset in enumerate(all_datasets):
        data = [pivot.get(b, {}).get(dataset, {}).get('f1', 0) for b in sorted_backends]
        chart_datasets.append({
            'label': dataset,
            'data': data,
            'backgroundColor': colors[i % len(colors)],
        })
    
    html += f'''
    <script>
        // Backend comparison chart (grouped bars)
        const backendCtx = document.getElementById('backendChart').getContext('2d');
        new Chart(backendCtx, {{
            type: 'bar',
            data: {{
                labels: {json.dumps(sorted_backends)},
                datasets: {json.dumps(chart_datasets)}
            }},
            options: {{
                responsive: true,
                plugins: {{
                    legend: {{
                        position: 'bottom',
                        labels: {{ color: '#c9d1d9' }}
                    }}
                }},
                scales: {{
                    x: {{
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }}
                    }},
                    y: {{
                        beginAtZero: true,
                        max: 100,
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }},
                        title: {{
                            display: true,
                            text: 'F1 Score (%)',
                            color: '#8b949e'
                        }}
                    }}
                }}
            }}
        }});
        
        // Dataset coverage chart (horizontal bars)
        const datasetCtx = document.getElementById('datasetChart').getContext('2d');
        new Chart(datasetCtx, {{
            type: 'bar',
            data: {{
                labels: {json.dumps(list(all_datasets))},
                datasets: [{{
                    label: 'Best F1',
                    data: {json.dumps([dataset_coverage[d]['best_f1'] for d in all_datasets])},
                    backgroundColor: 'rgba(63, 185, 80, 0.8)',
                }}, {{
                    label: 'Backends with Results',
                    data: {json.dumps([dataset_coverage[d]['count'] * 10 for d in all_datasets])},
                    backgroundColor: 'rgba(88, 166, 255, 0.4)',
                }}]
            }},
            options: {{
                indexAxis: 'y',
                responsive: true,
                plugins: {{
                    legend: {{
                        position: 'bottom',
                        labels: {{ color: '#c9d1d9' }}
                    }}
                }},
                scales: {{
                    x: {{
                        beginAtZero: true,
                        max: 100,
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }}
                    }},
                    y: {{
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }}
                    }}
                }}
            }}
        }});
'''
    
    # Prepare precision/recall scatter data
    pr_scatter_data = []
    for i, backend in enumerate(all_backends):
        for r in by_backend.get(backend, []):
            m = r.get('metrics', {})
            p, rec, f1 = m.get('precision', 0), m.get('recall', 0), m.get('f1', 0)
            if f1 > 0:  # Only include successful runs
                pr_scatter_data.append({
                    'x': p,
                    'y': rec,
                    'label': f"{backend}/{r.get('dataset', '')}",
                    'backgroundColor': colors[i % len(colors)],
                })
    
    # Group scatter by backend for legend
    pr_datasets = []
    for i, backend in enumerate(all_backends):
        points = [{'x': r.get('metrics', {}).get('precision', 0), 
                   'y': r.get('metrics', {}).get('recall', 0)}
                  for r in by_backend.get(backend, [])
                  if r.get('metrics', {}).get('f1', 0) > 0]
        if points:
            pr_datasets.append({
                'label': backend,
                'data': points,
                'backgroundColor': colors[i % len(colors)],
            })
    
    # Prepare latency vs F1 data
    latency_datasets = []
    for i, backend in enumerate(all_backends):
        points = [{'x': r.get('duration_ms', 0) / 1000,  # seconds
                   'y': r.get('metrics', {}).get('f1', 0)}
                  for r in by_backend.get(backend, [])
                  if r.get('metrics', {}).get('f1', 0) > 0 and r.get('duration_ms', 0) > 0]
        if points:
            latency_datasets.append({
                'label': backend,
                'data': points,
                'backgroundColor': colors[i % len(colors)],
            })
    
    html += f'''
        // Precision vs Recall scatter
        const prCtx = document.getElementById('prChart').getContext('2d');
        new Chart(prCtx, {{
            type: 'scatter',
            data: {{
                datasets: {json.dumps(pr_datasets)}
            }},
            options: {{
                responsive: true,
                plugins: {{
                    legend: {{
                        position: 'bottom',
                        labels: {{ color: '#c9d1d9' }}
                    }}
                }},
                scales: {{
                    x: {{
                        beginAtZero: true,
                        max: 100,
                        title: {{ display: true, text: 'Precision (%)', color: '#8b949e' }},
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }}
                    }},
                    y: {{
                        beginAtZero: true,
                        max: 100,
                        title: {{ display: true, text: 'Recall (%)', color: '#8b949e' }},
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }}
                    }}
                }}
            }}
        }});
        
        // Latency vs F1 scatter (quality-speed tradeoff)
        const latencyCtx = document.getElementById('latencyChart').getContext('2d');
        new Chart(latencyCtx, {{
            type: 'scatter',
            data: {{
                datasets: {json.dumps(latency_datasets)}
            }},
            options: {{
                responsive: true,
                plugins: {{
                    legend: {{
                        position: 'bottom',
                        labels: {{ color: '#c9d1d9' }}
                    }}
                }},
                scales: {{
                    x: {{
                        beginAtZero: true,
                        type: 'logarithmic',
                        title: {{ display: true, text: 'Latency (seconds, log scale)', color: '#8b949e' }},
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }}
                    }},
                    y: {{
                        beginAtZero: true,
                        max: 100,
                        title: {{ display: true, text: 'F1 Score (%)', color: '#8b949e' }},
                        ticks: {{ color: '#8b949e' }},
                        grid: {{ color: '#30363d' }}
                    }}
                }}
            }}
        }});
    </script>
</body>
</html>
'''
    
    html_path.write_text(html)


# ============================================================================
# S3 Download
# ============================================================================

def download_from_s3(bucket: str, prefix: str, local_dir: Path) -> list[Path]:
    """Download result files from S3."""
    local_dir.mkdir(parents=True, exist_ok=True)
    
    # List objects
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
    parser = argparse.ArgumentParser(description='Aggregate spot evaluation results')
    parser.add_argument('--download', action='store_true', help='Download results from S3')
    parser.add_argument('--bucket', default='arc-anno-data', help='S3 bucket')
    parser.add_argument('--prefix', default='results/', help='S3 prefix')
    parser.add_argument('--local-dir', default='reports/spot/', help='Local results directory')
    parser.add_argument('--output', default='reports/eval-results.jsonl', help='Output JSONL file')
    parser.add_argument('--html', default='reports/eval-dashboard.html', help='Output HTML dashboard')
    
    args = parser.parse_args()
    
    output_path = Path(args.output)
    html_path = Path(args.html)
    local_dir = Path(args.local_dir)
    
    output_path.parent.mkdir(parents=True, exist_ok=True)
    
    # Download from S3 if requested
    if args.download:
        print(f"Downloading results from s3://{args.bucket}/{args.prefix}...")
        files = download_from_s3(args.bucket, args.prefix, local_dir)
        print(f"Downloaded {len(files)} files")
    
    # Parse all result files
    result_files = list(local_dir.glob('**/*.md'))
    print(f"Parsing {len(result_files)} result files...")
    
    new_results = []
    for path in result_files:
        result = parse_result_file(path)
        if result:
            new_results.append(result)
    
    print(f"Parsed {len(new_results)} results")
    
    # Append to JSONL
    added = append_results(output_path, new_results)
    print(f"Added {added} new results to {output_path}")
    
    # Generate HTML dashboard
    generate_html_dashboard(output_path, html_path)
    print(f"Generated dashboard: {html_path}")


if __name__ == '__main__':
    main()
