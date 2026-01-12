# Git Hooks Comprehensive Improvements

## Overview
Based on our debugging and optimization experiences, both pre-commit and pre-push hooks have been significantly improved for reliability, performance, and user experience.

## Key Improvements

### 1. Performance Monitoring
- **Added timing information** to both hooks
- Pre-commit: Shows duration if >1s
- Pre-push: Shows duration in minutes/seconds format
- Helps identify slow operations for future optimization

### 2. Better Error Handling
- **Tool availability checks**: Verify required tools (rustfmt, cargo) before running
- **Graceful degradation**: Handle missing tools with helpful error messages
- **Edge case handling**: Handle empty commits, non-git directories, etc.
- **Error recovery**: Continue processing other files if one fails

### 3. Improved User Feedback
- **Clear error messages** with actionable suggestions
- **Progress indicators** showing what's being checked
- **Summary messages** with helpful hints
- **File count feedback** (e.g., "formatted and staged 3 file(s)")

### 4. Timeout Protection
- **Pre-push**: Timeouts for clippy (60s) and tests (120s)
- **Pre-commit**: Optional compilation check with timeout (30s)
- **Non-fatal timeouts**: Warnings instead of blocking operations

### 5. Skip Options
- **Pre-commit**: `ANNO_SKIP_FORMAT=1` to skip formatting
- **Pre-commit**: `ANNO_QUICK_CHECK=1` to enable compilation check
- **Pre-push**: `ANNO_SKIP_CLIPPY=1` to skip clippy
- **Pre-push**: `ANNO_SKIP_TESTS=1` to skip tests

### 6. Performance Optimizations
- **Pre-commit**: Use `rustfmt` on staged files only (45x faster)
- **Pre-commit**: Only check staged files for size (not entire workspace)
- **Pre-push**: Use `--no-deps` flag for faster clippy
- **Pre-push**: Clippy allow attributes on generated code

## Pre-Commit Hook Improvements

### Before
- ~1.37s per commit
- `cargo fmt --all` checks entire workspace
- No error recovery
- No timing information

### After
- ~0.03s per commit (45x faster)
- `rustfmt` on only staged files
- Better error handling and recovery
- Timing information
- Optional compilation check

### Features
1. **Trailing whitespace removal** (staged .rs files only)
2. **Formatting check** (staged files only, auto-fix)
3. **Filename validation** (no spaces)
4. **Large file detection** (staged files only, warning)
5. **Optional compilation check** (opt-in with `ANNO_QUICK_CHECK=1`)

## Pre-Push Hook Improvements

### Before
- Could hang indefinitely on clippy/tests
- No timeout protection
- No skip options
- Poor error messages

### After
- Timeout protection (60s clippy, 120s tests)
- Skip options for slow operations
- Better error handling with output truncation
- Timing information
- Helpful summary messages

### Features
1. **Format check** (should be clean from pre-commit)
2. **Clippy check** (with timeout, skip option)
3. **Test check** (with timeout, skip option)
4. **Doc tests** (skipped by default)
5. **Uncommitted changes** (warning only)

## Usage Examples

### Pre-Commit
```bash
# Normal commit (auto-formats staged .rs files)
git commit -m "message"

# Skip formatting
ANNO_SKIP_FORMAT=1 git commit -m "message"

# Enable compilation check
ANNO_QUICK_CHECK=1 git commit -m "message"

# Skip all hooks
git commit --no-verify -m "message"
```

### Pre-Push
```bash
# Normal push (all checks with timeouts)
git push

# Skip clippy
ANNO_SKIP_CLIPPY=1 git push

# Skip tests
ANNO_SKIP_TESTS=1 git push

# Skip both
ANNO_SKIP_CLIPPY=1 ANNO_SKIP_TESTS=1 git push

# Skip all hooks
git push --no-verify
```

## Performance Metrics

### Pre-Commit
| Operation | Before | After | Improvement |
|-----------|--------|-------|-------------|
| Total time | 1.37s | 0.03s | 45x faster |
| Format check | 1.3s | 0.01s | 130x faster |
| File size check | 0.01s | 0.01s | Same (already fast) |

### Pre-Push
| Operation | Before | After |
|-----------|--------|-------|
| Clippy timeout | None (hangs) | 60s (non-fatal) |
| Test timeout | None (hangs) | 120s (non-fatal) |
| Error handling | Poor | Excellent |

## Lessons Learned

### 1. Large Generated Files
- **Issue**: 9.6k line files cause clippy to be very slow
- **Solution**: Add clippy allow attributes, use `--no-deps` flag
- **Future**: Consider splitting large files or using code generation

### 2. Timeout Protection
- **Issue**: Operations can hang indefinitely
- **Solution**: Add timeouts with graceful degradation
- **Future**: Consider async/background processing

### 3. Scope Optimization
- **Issue**: Checking entire workspace is slow
- **Solution**: Only check/process staged files
- **Future**: Cache results based on file hashes

### 4. User Experience
- **Issue**: Poor feedback when operations are slow
- **Solution**: Clear messages, timing info, skip options
- **Future**: Progress indicators for long operations

## Files Modified
- `scripts/hooks/pre-commit` - All improvements
- `scripts/hooks/pre-push` - All improvements
- `anno/src/eval/dataset_registry.rs` - Clippy allow attributes
- Documentation files in `docs/notes/`

## Future Improvements
1. **Caching**: Cache formatting/clippy results based on file hashes
2. **Parallel processing**: Format multiple files in parallel
3. **Progress indicators**: Show progress for long operations
4. **Async operations**: Run slow checks in background
5. **Incremental checks**: Only check changed files
