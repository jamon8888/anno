# Pre-Push Hook Comprehensive Improvements

## Overview
The pre-push hook has been significantly improved to handle slow operations gracefully and prevent hanging.

## Problems Addressed

### 1. Clippy Hanging
- **Issue**: Clippy with `eval-advanced discourse` features could hang indefinitely
- **Root Cause**: Large generated files (9.6k lines) + many modules = very slow analysis
- **Solution**: 
  - 60s timeout (non-fatal)
  - `ANNO_SKIP_CLIPPY=1` environment variable
  - `--no-deps` flag for faster checking
  - Clippy allow attributes on generated code

### 2. Tests Hanging
- **Issue**: Tests could also hang, especially on macOS with Gatekeeper delays
- **Solution**:
  - 120s timeout (non-fatal)
  - `ANNO_SKIP_TESTS=1` environment variable
  - Graceful fallback if timeout not available

## Environment Variables

### Skip Clippy
```bash
ANNO_SKIP_CLIPPY=1 git push
```
Useful when:
- Clippy is taking too long (>60s)
- You've already run clippy manually
- You're in a hurry and clippy is slow

### Skip Tests
```bash
ANNO_SKIP_TESTS=1 git push
```
Useful when:
- Tests are taking too long (>120s)
- You've already run tests manually
- You're pushing a documentation-only change

### Skip Both
```bash
ANNO_SKIP_CLIPPY=1 ANNO_SKIP_TESTS=1 git push
```

## Timeout Behavior

### Clippy Timeout (60s)
- **Non-fatal**: Push continues with warning
- **Exit code**: 124 (timeout)
- **Action**: Shows helpful message with skip option

### Test Timeout (120s)
- **Non-fatal**: Push continues with warning
- **Exit code**: 124 (timeout)
- **Action**: Shows helpful message with skip option

## Error Handling

### Clippy Errors
- Truncates output to first 50 lines (prevents spam)
- Shows actionable error messages
- Uses `mktemp` for safe temp file handling

### Test Errors
- Shows last 30 lines of output
- Shows actionable error messages
- Uses `mktemp` for safe temp file handling

## Performance Optimizations

1. **Clippy**:
   - `--no-deps` flag (skips dependency checking)
   - `#[allow]` attributes on generated code
   - Only checks library code (not tests/benchmarks)

2. **Tests**:
   - Uses `quick` profile for nextest (excludes slow tests)
   - Timeout prevents indefinite hanging
   - Graceful fallback to `cargo test` if nextest unavailable

## Usage Examples

### Normal Push
```bash
git push
# Runs all checks with timeouts
```

### Skip Slow Checks
```bash
ANNO_SKIP_CLIPPY=1 git push
# Skips clippy, runs tests
```

### Skip All Checks
```bash
git push --no-verify
# Skips all hooks entirely
```

### Development Workflow
```bash
# Run checks manually first
cargo clippy --workspace --lib --features "eval-advanced discourse" -- -D warnings
cargo nextest run --profile quick --workspace --features "eval-advanced discourse"

# Then push (checks will be fast or can skip)
git push
```

## Files Modified
- `scripts/hooks/pre-push` - All improvements
- `anno/src/eval/dataset_registry.rs` - Clippy allow attributes
- Documentation files in `docs/notes/`

## Future Improvements
1. Cache clippy/test results based on file hashes
2. Run checks in parallel where possible
3. Add progress indicators for long-running checks
4. Consider making checks async/background
