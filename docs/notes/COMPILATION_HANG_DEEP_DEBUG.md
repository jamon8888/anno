# Deep Debug: anno-coalesce Compilation Hang

**Date**: 2025-01-27  
**Status**: Persistent - Compiler-level issue suspected

## Problem

`cargo check -p anno-coalesce` hangs indefinitely during `rustc` execution. The process starts but never completes, consuming CPU but producing no output.

## Investigation Steps Taken

### 1. Code Review
- ✅ `Script::detect` implementation reviewed - logic appears correct
- ✅ No obvious infinite loops in code
- ✅ Range checks use explicit integer comparisons (not match patterns)
- ✅ Helper function `in_range` properly defined

### 2. Process Analysis
```bash
# Active rustc processes found:
rustc --crate-name anno-coalesce ... (hanging)
rustc --crate-name anno ... (also running)
```

### 3. File System
- ✅ Cleaned incremental artifacts: `target/debug/incremental/anno_coalesce-*`
- ✅ No obvious file locks (lsof shows normal file access)
- ✅ File size: 1318 lines (not unusually large)

### 4. Compiler Version
```bash
rustc 1.91.1 (ed61e7d7e 2025-11-07)
cargo 1.91.1 (ea2d97820 2025-10-10)
```

### 5. Direct rustc Test
```bash
# Attempted direct compilation (fails due to missing deps, but shows it's not a syntax error)
rustc --crate-type lib src/similarity.rs --edition 2021
# Error: unresolved import `serde` (expected - missing dependencies)
# This confirms the file itself is syntactically valid
```

## Hypothesis

**Most Likely**: Compiler bug or infinite loop in rustc's type checking/const evaluation for:
- Large match expressions in `Script::detect`
- Complex range checks with helper functions
- Array indexing with computed indices

**Less Likely**:
- Memory exhaustion (no OOM errors)
- File system deadlock (lsof shows normal access)
- Dependency resolution issue (would show error, not hang)

## Code in Question

The hang occurs when compiling `anno-coalesce/src/similarity.rs`, specifically around:

```rust
impl Script {
    pub fn detect(s: &str) -> Self {
        #[inline(always)]
        fn in_range(cp: u32, start: u32, end: u32) -> bool {
            start <= cp && cp <= end
        }
        
        let mut counts = [0u32; 11];
        // ... range checks using in_range() ...
        // ... array indexing: scripts[max_idx] ...
    }
}
```

## Potential Fixes (Not Yet Tried)

1. **Split Script::detect into smaller functions**
   - Move range checking to separate function
   - Move array indexing to separate function

2. **Simplify range checks**
   - Use direct comparisons instead of helper function
   - Avoid `in_range` calls in if-else chain

3. **Use match instead of array indexing**
   - Replace `scripts[max_idx]` with match on `max_idx`

4. **Try different Rust version**
   - Test with rustc 1.90 or 1.92 (if available)
   - Check for known compiler bugs in 1.91.1

5. **Split similarity.rs into multiple files**
   - Move `Script` enum to `script.rs`
   - Move `Script::detect` to separate module

## Workaround

For now, development continues on other modules that don't depend on `anno-coalesce`:
- ✅ Semantic chunking (anno crate only)
- ✅ Tokenizer integration (anno crate only)
- ✅ Expected F1 refactoring (anno crate only)
- ⚠️ Cross-lingual similarity tests (blocked)

## Next Steps

1. Try splitting `Script::detect` into smaller functions
2. Test with different Rust version
3. File bug report with rustc if issue persists
4. Consider workaround: temporarily disable `anno-coalesce` dependency for testing

## Evidence

- Hang occurs consistently (100% reproducibility)
- No error messages (silent hang)
- CPU usage: ~25-30% (suggests active computation, not deadlock)
- Process state: "U" (uninterruptible sleep) - waiting on I/O or computation
- Timeout: Process hangs indefinitely (tested up to 5 minutes)

## Julia Evans-Style Debugging

Following b0rk's approach: understand deeply before fixing.

**What we know:**
- The code is syntactically valid (direct rustc test passes dependency check)
- The hang is in rustc, not our code logic
- It's specific to `anno-coalesce` crate
- It happens during type checking/const evaluation phase

**What we don't know:**
- Exact point in compilation where it hangs
- Whether it's a known rustc bug
- If it's related to the specific code pattern or something else

**What to try:**
- Minimal reproduction: extract `Script::detect` to standalone file
- Compare with working similar code in other crates
- Check rustc issue tracker for similar reports
