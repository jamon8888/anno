# Test Profiling Guide

This guide explains how to profile test execution using nextest and Rust tooling.

## Quick Start

```bash
# Profile quick tests
just profile-tests quick

# Analyze results
just profile-analyze

# Show slowest tests
just profile-slowest
```

## Methods

### Method 1: Nextest JSON Output (Recommended)

Nextest can output test results in JSON format with timing information:

```bash
NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run \
    --profile quick \
    --workspace \
    --features "eval-advanced discourse" \
    --message-format libtest-json-plus \
    --status-level all \
    > target/test-profiles/nextest_output.json
```

The JSON output includes:
- Test execution times (`exec_time` field)
- Test status (ok, failed, ignored)
- Binary and module information
- Nextest metadata

### Method 2: Rust Native Profiling

For deeper profiling, use Rust's native tools:

```bash
# Build with debug symbols
RUSTFLAGS="-g -C debuginfo=1" cargo build --tests

# Run with profiling
just profile-tests-rust quick
```

On macOS, you can use Instruments.app:
```bash
instruments -t "Time Profiler" cargo nextest run --profile quick
```

On Linux, use perf:
```bash
perf record cargo nextest run --profile quick
perf report
```

## Analysis

### Automatic Analysis

The `profile-analyze` command processes JSON output and shows:
- Total tests and execution time
- Slowest tests (top 20)
- Time breakdown by binary
- Time breakdown by module

```bash
just profile-analyze [path/to/nextest_output.json]
```

### Manual Analysis

Use `jq` to query the JSON output:

```bash
# Count total tests
jq -r 'select(.type == "test" and .event == "ok")' nextest_output.json | wc -l

# Show slowest tests
jq -r 'select(.type == "test" and .event == "ok") | "\(.exec_time)s  \(.name)"' \
    nextest_output.json | sort -rn | head -20

# Group by binary
jq -r 'select(.type == "test" and .event == "ok") | 
    "\(.nextest.binary_name // "unknown")\t\(.exec_time)"' \
    nextest_output.json | awk '{sum[$1]+=$2; count[$1]++} 
    END {for (bin in sum) print bin, sum[bin], count[bin]}' | sort -k2 -rn
```

## Available Commands

| Command | Description |
|---------|-------------|
| `just profile-tests [profile]` | Run tests with profiling (default: quick) |
| `just profile-tests-rust [profile]` | Use Rust native profiling tools |
| `just profile-quick` | Profile quick tests |
| `just profile-ci` | Profile CI tests |
| `just profile-ml` | Profile ML tests (slow) |
| `just profile-filter FILTER` | Profile specific test filter |
| `just profile-analyze [file]` | Analyze profiling results |
| `just profile-slowest` | Show slowest tests from last run |

## Output Location

All profiling output is saved to `target/test-profiles/`:
- `nextest_*.json` - Nextest JSON output with timing
- `analysis_*.txt` - Analysis reports
- `rust_profile_*.log` - Rust profiling logs

## Tips

1. **Start with quick profile**: Use `--profile quick` to focus on fast tests first
2. **Filter slow tests**: Use `--status-level slow` to see which tests are marked as slow
3. **Compare runs**: Save JSON outputs with timestamps to track performance over time
4. **Focus on outliers**: Look for tests taking >1s that could be optimized

## Integration with CI

To profile tests in CI, add to your workflow:

```yaml
- name: Profile tests
  run: |
    NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run \
      --profile ci \
      --message-format libtest-json-plus \
      > test-profile.json
    # Upload or analyze test-profile.json
```

## Related Documentation

- [Testing Guidelines](../TESTING.md) - General testing practices
- [Performance Analysis](../../docs/notes/design/performance/PERFORMANCE_ANALYSIS.md) - Performance optimization
- [Nextest Documentation](https://nexte.st/docs/running/) - Nextest features

