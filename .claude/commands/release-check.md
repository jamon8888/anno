# /release-check -- Pre-publish validation audit

Comprehensive check before bumping versions or publishing crates. This is the gate between "code works on my machine" and "code is ready for crates.io."

## Procedure

### 1. Full CI simulation

```bash
cd <repo-root>
just ci
```

Read the **full output**. This runs: fmt check, clippy, tests (with and without features), doc compilation. Every failure here is a blocker.

### 2. Feature matrix

```bash
just check-feature-matrix
```

This compiles each optional feature in isolation. Catches: feature-gated code that only compiles when combined with other features, missing `cfg` guards, broken feature algebra.

### 3. Property tests

```bash
just proptest
```

Runs 1000 cases. Check for: span invariant violations, determinism failures, round-trip breakage.

### 4. Publish validation

```bash
just validate-publish
```

Checks: Cargo.toml metadata (license, description, repository, documentation), dependency versions, no path dependencies leaking into published crates, no `publish = false` crates in the dependency chain of published crates.

### 5. Doc compilation

```bash
RUSTDOCFLAGS='-D warnings' cargo doc -p anno -p anno-core -p anno-metrics --no-deps --features "eval discourse"
just docs-audit
```

Checks: no broken intra-doc links, no rustdoc warnings, doc paths are current. Read the full output of `docs-audit` -- it catches stale file references in docs.

### 6. MSRV check

```bash
just msrv
```

Verifies the workspace compiles on the declared minimum supported Rust version (1.85). This requires `rustup toolchain install 1.85.0` if not already present.

### 7. Static analysis (optional but recommended)

```bash
just static-analysis
```

Runs cargo-deny (dependency license/advisory), cargo-machete (unused deps), cargo-geiger (unsafe stats), OpenGrep (custom patterns). Non-blocking but worth reviewing before publish.

### 8. Quick eval sanity

```bash
just eval-sanity
```

Runs a small random-sample evaluation. Catches: model loading regressions, scoring pipeline breakage, dataset loading failures. This is the "does the evaluation framework still work at all" check.

### 9. README accuracy

Read `README.md` and check:
- Do the documented CLI examples still work? Run 2-3 of them.
- Are the listed backends current? Compare against `anno models list`.
- Are version numbers in the README correct?
- Does `just readme-test` pass (if Playwright is available)?

### 10. Changelog / version diff

```bash
git log --oneline $(git describe --tags --abbrev=0 2>/dev/null || echo HEAD~20)..HEAD
```

Review all commits since the last tag. Check:
- Are there breaking API changes? If so, version bump must be major (or minor if pre-1.0).
- Are there new public types/functions that need documentation?
- Are there changes that warrant a CHANGELOG entry?

### 11. Write the verdict

Structure:

1. **Version proposed**: which crates, what version bump (patch/minor/major)
2. **Blockers**: anything that failed and must be fixed before publish
3. **Warnings**: non-blocking issues worth noting (unsafe stats, large dependency tree, etc.)
4. **Changelog draft**: summary of changes for each crate being published
5. **Publish order**: crates must be published in dependency order (anno-core -> anno-metrics -> anno-lib -> anno-cli)

Do **not** bump versions or publish. Report the findings and let the user decide.

## What this is NOT

- Not a code quality check (that's `/check-fix`)
- Not a real-world quality audit (that's `/qa`)
- Not an evaluation benchmark (that's `/eval-review`)

This answers: "is this code ready to publish on crates.io?"
