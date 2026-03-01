# /check-fix -- Run checks, auto-fix, verify

Run the standard check suite, interpret any failures, auto-fix what can be auto-fixed, and re-verify. Repeat until clean or until a failure requires manual intervention.

## Procedure

### 1. Run the fast check suite

```bash
cd <repo-root>
just check
```

Read the **full output** -- do not truncate. Classify each failure:

| Category | Auto-fixable? | Tool |
|----------|---------------|------|
| Format | Yes | `cargo fmt` |
| Clippy (lint) | Usually | `cargo clippy --fix --allow-dirty --allow-staged` |
| Clippy (correctness) | Sometimes | Manual review needed |
| Test failure | No | Investigate root cause |
| Doc audit | Sometimes | Fix stale paths/links directly |

### 2. Auto-fix what you can

Run in order (each may affect the next):

```bash
# Format first
cargo fmt --manifest-path Cargo.toml -p anno

# Then clippy autofix (scoped to avoid broad damage)
cargo clippy --manifest-path Cargo.toml --workspace --all-targets --features "eval discourse" --fix --allow-dirty --allow-staged

# Re-format after clippy changes
cargo fmt --manifest-path Cargo.toml -p anno
```

### 3. Re-run checks

```bash
just check
```

If clean, report what was fixed (show before/after for non-trivial changes).

If still failing:
- For **test failures**: read the test, read the code under test, diagnose root cause. Fix if the fix is obvious and safe. If not, report the failure with diagnosis.
- For **clippy errors that --fix couldn't handle**: fix manually, explaining the change.
- For **doc audit failures**: fix stale paths or links directly.

### 4. Loop

Re-run `just check` after each fix. Stop when clean or when a failure requires user decision.

### 5. Feature matrix (if requested or if changes touch feature-gated code)

```bash
just check-feature-matrix
```

This catches feature-gated code that compiles under `eval discourse` but breaks under other flag combinations.

## What this is NOT

- Not a full CI simulation (that's `just ci`)
- Not a pre-release audit (that's `/release-check`)
- Not a quality audit (that's `/qa`)

This answers: "is the code clean enough to commit?"
