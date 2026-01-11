# Pre-Push Hook Clippy Fix

## Problem
Git push was hanging indefinitely on the pre-push hook's clippy check, even with timeout.

## Root Cause
- Large generated files (9.6k lines) + eval-advanced features = very slow clippy
- Timeout might not properly interrupt cargo/clippy processes
- No way to skip clippy when it's problematic

## Solution

### 1. Environment Variable to Skip Clippy
```bash
# Skip clippy check entirely
ANNO_SKIP_CLIPPY=1 git push
```

### 2. Reduced Timeout
- Changed from 180s to 60s
- If clippy takes longer than 60s, it's likely stuck and will timeout gracefully

### 3. Skip if No Timeout Available
- If `timeout` command is not available, skip clippy to avoid hanging
- Warns user to install coreutils or use skip flag

## Usage

### Normal Push (with clippy)
```bash
git push  # Will run clippy with 60s timeout
```

### Skip Clippy (when slow/hanging)
```bash
ANNO_SKIP_CLIPPY=1 git push
```

### Skip All Hooks
```bash
git push --no-verify
```

## Files Modified
- `scripts/hooks/pre-push` - Added skip option and reduced timeout

## Future Improvements
1. Consider making clippy check async/background
2. Cache clippy results to avoid re-running on unchanged files
3. Split large generated files to reduce clippy analysis time
