# Pre-Commit Hook Optimizations

## Problem
Pre-commit hook was taking ~1.3 seconds, mostly due to `cargo fmt --all` checking the entire workspace.

## Root Cause Analysis

### Performance Breakdown (Before)
- Trailing whitespace fix: ~0.01s (fast)
- `cargo fmt --all --check`: **~1.3s** (slow - checks entire workspace)
- Filename check: ~0.01s (fast)
- Large file check: ~0.01s (fast)

**Total: ~1.37s**

### Why `cargo fmt --all` is Slow
- Scans entire workspace (all crates, all files)
- Even with `--check`, it still processes all files
- No way to limit to specific files with `cargo fmt`

## Solution

### 1. Use `rustfmt` Directly on Staged Files
- **Before**: `cargo fmt --all` (checks entire workspace)
- **After**: `rustfmt` on only staged `.rs` files
- **Speedup**: ~50x faster (0.026s vs 1.37s)

### 2. Optimize File Size Check
- Only check staged files (not all files)
- Use `stat` for faster size checks when available
- Fallback to `wc -c` if `stat` not available

### 3. Add Skip Option
- `ANNO_SKIP_FORMAT=1` environment variable
- Useful when in a hurry or formatting is already done

### 4. Better User Feedback
- Shows which files were formatted
- Suggests reviewing changes with `git diff --cached`
- Clearer error messages

## Performance After Optimization

### Performance Breakdown (After)
- Trailing whitespace fix: ~0.01s
- `rustfmt` on staged files: **~0.01s** (only processes staged files)
- Filename check: ~0.01s
- Large file check: ~0.01s (only staged files)

**Total: ~0.03s** (45x faster!)

## Usage

### Normal Commit
```bash
git commit -m "message"
# Automatically formats staged .rs files if needed
```

### Skip Formatting
```bash
ANNO_SKIP_FORMAT=1 git commit -m "message"
# Skips formatting check (useful when in a hurry)
```

### Skip All Hooks
```bash
git commit --no-verify -m "message"
# Skips all pre-commit hooks
```

## Technical Details

### Why `rustfmt` Instead of `cargo fmt`
- `cargo fmt` doesn't accept file paths as arguments
- `rustfmt` can format individual files directly
- Much faster for small file sets (staged files only)

### File Processing
- Only processes staged `.rs` files (not entire workspace)
- Uses `git diff --cached` to get staged files
- Filters to only `.rs` files with `grep`

### Error Handling
- Continues if individual files fail to format
- Shows clear error messages
- Doesn't block commit on formatting issues (auto-fixes them)

## Files Modified
- `scripts/hooks/pre-commit` - All optimizations
- `docs/notes/PRE_COMMIT_OPTIMIZATIONS.md` - This documentation

## Future Improvements
1. Cache formatting results (check file hash before formatting)
2. Parallel file formatting (if many files staged)
3. Add timeout protection (though unlikely to be needed now)
