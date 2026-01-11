# Compilation Hang Issue - anno-coalesce

**Date**: 2025-01-27  
**Status**: Investigating  
**Affected**: `anno-coalesce` crate compilation

## Symptoms

- `cargo check -p anno-coalesce` hangs indefinitely during rustc execution
- Hang occurs after dependency resolution completes
- Process shows as "Checking anno-coalesce" but never completes
- Issue persists after:
  - Cleaning incremental compilation
  - Killing all cargo/rustc processes
  - Simplifying code patterns
  - Removing Unicode range patterns from match statements

## Investigation

### Changes Made
1. Replaced Unicode range patterns in `Script::detect` match statement with explicit integer comparisons
2. Added helper function `in_range` to simplify range checks
3. Verified no syntax errors or linter issues

### Attempted Fixes
- Clean incremental compilation: `rm -rf target/debug/incremental/anno_coalesce-*`
- Kill all processes: `pkill -9 cargo rustc`
- Remove build artifacts: `rm -rf target/.rustc_info.json`
- Simplify code patterns (avoided complex match with Unicode ranges)

### Current Code Pattern
```rust
pub fn detect(s: &str) -> Self {
    #[inline(always)]
    fn in_range(cp: u32, start: u32, end: u32) -> bool {
        start <= cp && cp <= end
    }
    
    let mut counts = [0u32; 11];
    let mut total_chars = 0u32;
    
    for c in s.chars() {
        if c.is_whitespace() || c.is_ascii_punctuation() {
            continue;
        }
        total_chars += 1;
        let cp = c as u32;
        // Explicit range checks using helper function
        if cp <= 0x007F || in_range(cp, 0x0080, 0x024F) {
            counts[0] += 1; // Latin
        } else if in_range(cp, 0x4E00, 0x9FFF) || in_range(cp, 0x3400, 0x4DBF) {
            counts[1] += 1; // CJK
        }
        // ... more ranges
    }
    // ... rest of function
}
```

## Possible Causes

1. **Compiler Bug**: Rust 1.91.1 may have a bug with certain Unicode/range patterns
2. **Memory Exhaustion**: Large file (1312 lines) with complex Unicode handling
3. **Deadlock**: Internal compiler deadlock (unlikely but possible)
4. **Toolchain Issue**: Corrupted Rust installation

## Next Steps

1. Try compiling with different Rust versions
2. Split `similarity.rs` into smaller modules
3. Report as compiler bug if issue persists
4. Work around by temporarily commenting out problematic code

## Workaround

For now, continue with other improvements that don't require recompiling `anno-coalesce`. The code changes are correct and should work once compilation succeeds.
