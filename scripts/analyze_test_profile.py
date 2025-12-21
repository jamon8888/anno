#!/usr/bin/env python3
"""Analyze test profiling results from nextest timing JSON files."""

import json
import sys
from pathlib import Path
from collections import defaultdict
from typing import Dict, List, Tuple

def analyze_timing_file(timing_file: Path) -> Dict:
    """Analyze a nextest JSON output file (libtest-json-plus format)."""
    executions = []
    
    # Nextest outputs newline-delimited JSON
    with open(timing_file) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
                # Look for test completion events
                if event.get("type") == "test":
                    test_event = event.get("event")
                    if test_event in ("ok", "failed", "ignored"):
                        test_name = event.get("name", "unknown")
                        elapsed = event.get("elapsed", 0)
                        # Extract binary from test name or use default
                        binary = event.get("binary_name", "unknown")
                        if "nextest" in event:
                            binary = event["nextest"].get("binary_name", binary)
                        
                        executions.append({
                            "test_name": test_name,
                            "duration_secs": elapsed,
                            "binary_name": binary,
                            "status": test_event,
                        })
            except json.JSONDecodeError:
                continue
    
    if not executions:
        return {"error": "No test executions found in JSON output"}
    
    # Aggregate statistics
    total_tests = len(executions)
    total_time = sum(e["duration_secs"] for e in executions)
    
    # Group by binary/module
    by_binary = defaultdict(lambda: {"count": 0, "time": 0.0, "tests": []})
    by_module = defaultdict(lambda: {"count": 0, "time": 0.0, "tests": []})
    
    slow_tests = []
    
    for exec in executions:
        duration = exec["duration_secs"]
        test_name = exec["test_name"]
        binary = exec["binary_name"]
        
        # Extract module from test name (e.g., "anno::entity::tests::test_span" -> "anno::entity")
        parts = test_name.split("::")
        module = "::".join(parts[:-2]) if len(parts) > 2 else "unknown"
        
        by_binary[binary]["count"] += 1
        by_binary[binary]["time"] += duration
        by_binary[binary]["tests"].append((test_name, duration))
        
        by_module[module]["count"] += 1
        by_module[module]["time"] += duration
        by_module[module]["tests"].append((test_name, duration))
        
        if duration > 0.1:  # Tests taking >100ms
            slow_tests.append((test_name, duration, binary))
    
    # Sort slow tests
    slow_tests.sort(key=lambda x: x[1], reverse=True)
    
    # Sort by time
    by_binary_sorted = sorted(
        by_binary.items(),
        key=lambda x: x[1]["time"],
        reverse=True
    )
    by_module_sorted = sorted(
        by_module.items(),
        key=lambda x: x[1]["time"],
        reverse=True
    )
    
    return {
        "summary": {
            "total_tests": total_tests,
            "total_time_secs": total_time,
            "avg_time_secs": total_time / total_tests if total_tests > 0 else 0,
            "slow_tests_count": len(slow_tests),
        },
        "slow_tests": slow_tests[:50],  # Top 50
        "by_binary": [
            {
                "binary": name,
                "count": stats["count"],
                "time_secs": stats["time"],
                "avg_secs": stats["time"] / stats["count"] if stats["count"] > 0 else 0,
            }
            for name, stats in by_binary_sorted
        ],
        "by_module": [
            {
                "module": name,
                "count": stats["count"],
                "time_secs": stats["time"],
                "avg_secs": stats["time"] / stats["count"] if stats["count"] > 0 else 0,
            }
            for name, stats in by_module_sorted[:30]  # Top 30 modules
        ],
    }


def print_analysis(analysis: Dict):
    """Print analysis results in a readable format."""
    if "error" in analysis:
        print(f"Error: {analysis['error']}")
        return
    
    summary = analysis["summary"]
    print("=" * 70)
    print("TEST PROFILING ANALYSIS")
    print("=" * 70)
    print()
    print(f"Total Tests:     {summary['total_tests']}")
    print(f"Total Time:      {summary['total_time_secs']:.2f}s")
    print(f"Average Time:   {summary['avg_time_secs']:.3f}s")
    print(f"Slow Tests (>100ms): {summary['slow_tests_count']}")
    print()
    
    print("=" * 70)
    print("SLOWEST TESTS (Top 20)")
    print("=" * 70)
    for i, (test_name, duration, binary) in enumerate(analysis["slow_tests"][:20], 1):
        print(f"{i:2d}. {duration:6.3f}s  [{binary:30s}]  {test_name}")
    print()
    
    print("=" * 70)
    print("TIME BY BINARY (Top 15)")
    print("=" * 70)
    for i, bin_stats in enumerate(analysis["by_binary"][:15], 1):
        print(
            f"{i:2d}. {bin_stats['time_secs']:7.2f}s  "
            f"({bin_stats['count']:3d} tests, "
            f"avg {bin_stats['avg_secs']:.3f}s)  {bin_stats['binary']}"
        )
    print()
    
    print("=" * 70)
    print("TIME BY MODULE (Top 15)")
    print("=" * 70)
    for i, mod_stats in enumerate(analysis["by_module"][:15], 1):
        print(
            f"{i:2d}. {mod_stats['time_secs']:7.2f}s  "
            f"({mod_stats['count']:3d} tests, "
            f"avg {mod_stats['avg_secs']:.3f}s)  {mod_stats['module']}"
        )


def main():
    """Main entry point."""
    if len(sys.argv) < 2:
        # Find latest JSON output file
        profile_dir = Path("target/test-profiles")
        if not profile_dir.exists():
            print("Error: No profiling directory found. Run 'just profile-tests' first.")
            sys.exit(1)
        
        json_files = sorted(
            profile_dir.glob("nextest_*.json"),
            key=lambda p: p.stat().st_mtime,
            reverse=True
        )
        
        if not json_files:
            print("Error: No nextest JSON files found. Run 'just profile-tests' first.")
            sys.exit(1)
        
        timing_file = json_files[0]
        print(f"Using latest JSON file: {timing_file.name}")
        print()
    else:
        timing_file = Path(sys.argv[1])
        if not timing_file.exists():
            print(f"Error: File not found: {timing_file}")
            sys.exit(1)
    
    analysis = analyze_timing_file(timing_file)
    print_analysis(analysis)


if __name__ == "__main__":
    main()

