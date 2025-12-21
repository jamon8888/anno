# Static Analysis

Static analysis tools integrated into the `anno` project and how to use them.

## Quick Start

Run all static analysis tools:
```bash
just static-analysis
```

Docs hygiene (links, anchors, stale paths):
```bash
just docs-audit
```

Generate comprehensive safety report:
```bash
just safety-report-full
```

## Installed Tools

### 1. cargo-deny
**Purpose:** Comprehensive dependency linting (licenses, security, duplicates)

**Configuration:** `deny.toml`

**Usage:**
```bash
just deny
# or
cargo deny check
```

**What it checks:**
- Security vulnerabilities (advisories)
- License compatibility
- Duplicate dependencies
- Wildcard version requirements
- Unknown sources

### 2. cargo-machete
**Purpose:** Fast unused dependency detection

**Usage:**
```bash
just machete
# or
cargo machete
```

**Note:** Fast but may have false positives. For accurate results, use `cargo-udeps` (slower, requires nightly).

### 3. cargo-geiger
**Purpose:** Count unsafe code in dependency tree

**Usage:**
```bash
just geiger
# or
cargo geiger
```

**Creative Use:** Generate safety report with statistics:
```bash
just safety-report
```

### 4. OpenGrep
**Purpose:** Security pattern detection with custom rules

**Installation:**
```bash
curl -fsSL https://raw.githubusercontent.com/opengrep/opengrep/main/install.sh | bash
```

**Usage:**
```bash
# Default security rules
just opengrep

# Custom rules (project-specific)
just opengrep-custom
```

**Custom Rules:**
- `.opengrep/rules/rust-security.yaml` - General Rust reliability/safety patterns
- `.opengrep/rules/rust-error-handling.yaml` - Mutex poison handling + error context patterns
- `.opengrep/rules/rust-evaluation-framework.yaml` - Eval correctness/perf patterns (variance, loops, edge cases)
- `.opengrep/rules/rust-memory-patterns.yaml` - Cloning/resource-management patterns (perf)
- `.opengrep/rules/rust-nlp-ml-patterns.yaml` - NLP/ML backend + eval patterns (auth hints, max-len checks, etc.)
- `.opengrep/rules/rust-anno-specific.yaml` - Anno-specific structural checks

**AST-grep Rules (optional, local):**
- `.opengrep/rules/rust-unicode-offsets.yaml` - Unicode-unsafe string slicing patterns
- `.opengrep/rules/rust-candle-metal.yaml` - Metal/Candle contiguous-before-matmul patterns

Run them with:
```bash
just ast-grep-unicode
just ast-grep-unicode-all
just ast-grep-metal
just ast-grep-metal-all
```

### 5. Miri
**Purpose:** Undefined behavior detection in unsafe code

**Usage:**
```bash
just miri-unsafe
# or
cargo miri test --lib --features onnx
```

**Note:** Runs selectively on unsafe code paths. Slow but catches memory safety issues.

### 6. cargo-nextest
**Purpose:** Faster test runner with better output

**Usage:**
```bash
just test-nextest
# or
cargo nextest run --all-features
```

### 7. cargo-llvm-cov
**Purpose:** Code coverage via LLVM instrumentation

**Usage:**
```bash
just coverage
# or
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

## Installation

To install all static analysis tools locally:

```bash
cargo install --locked cargo-deny
cargo install cargo-machete
cargo install cargo-geiger
cargo install cargo-nextest
cargo install cargo-llvm-cov
rustup component add miri
curl -fsSL https://raw.githubusercontent.com/opengrep/opengrep/main/install.sh | bash
```

## CI/CD Integration

All tools are integrated into GitHub Actions (`.github/workflows/ci.yml`):

1. **cargo-deny** - Runs on every PR/push
2. **unused-deps** - Fast check for unused dependencies
3. **safety-report** - Generates unsafe code statistics
4. **miri-unsafe** - Validates unsafe code (selective, on-demand)
5. **opengrep** - Security pattern detection with SARIF upload
6. **coverage** - Code coverage (runs on schedule/manual trigger)

### Workspace Updates

All CI commands have been updated to use `--workspace` flag:
- `cargo check --workspace --all-targets`
- `cargo fmt --workspace --all -- --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --lib`
- `cargo build --workspace`

## Creative Integrations

### Comprehensive Safety Report

The `generate-safety-report.sh` script combines multiple tools:

```bash
just safety-report-full
```

This generates `safety-report.md` with:
- Unsafe code statistics (cargo-geiger)
- Security pattern findings (OpenGrep)
- Unused dependencies (cargo-machete)
- Dependency security status (cargo-deny)

### Selective Miri Testing

Miri runs selectively on unsafe code files to avoid long CI times:
- Only runs when unsafe code changes
- Can be triggered with label `test-unsafe` on PRs
- Focuses on files with `unsafe` blocks

### OpenGrep Custom Rules

Rule sets:
1. **rust-security.yaml** - General Rust reliability/safety patterns
2. **rust-error-handling.yaml** - Mutex poison handling + error context patterns
3. **rust-evaluation-framework.yaml** - Eval correctness/perf patterns
4. **rust-memory-patterns.yaml** - Cloning/resource-management patterns
5. **rust-nlp-ml-patterns.yaml** - NLP/ML backend + eval patterns
6. **rust-anno-specific.yaml** - Anno-specific structural checks

Optional local rules (not OpenGrep; run via `ast-grep`):
7. **rust-unicode-offsets.yaml** - Unicode-unsafe string slicing patterns
8. **rust-candle-metal.yaml** - Metal contiguous-before-matmul patterns

## Workflow Recommendations

### Daily Development
```bash
# Quick check before commit
just check          # fmt + clippy + tests
just machete        # Fast unused deps check
```

### Pre-Push
```bash
# Comprehensive check
just static-analysis
```

### Weekly Review
```bash
# Full safety audit
just safety-report-full
just coverage
```

### Before Release
```bash
# Everything
just ci-eval        # Full CI simulation
just static-analysis
just safety-report-full
just coverage
```

## Tool Comparison

| Tool | Speed | Accuracy | CI-Friendly | Purpose |
|------|-------|----------|-------------|---------|
| cargo-deny | Medium | High | ✅ | Dependency security |
| cargo-machete | Very Fast | Good | ✅ | Unused deps (fast) |
| cargo-udeps | Slow | Excellent | ⚠️ | Unused deps (accurate) |
| cargo-geiger | Medium | High | ✅ | Unsafe code stats |
| OpenGrep | Medium | High | ✅ | Security patterns |
| Miri | Slow | Excellent | ⚠️ | Undefined behavior |
| cargo-nextest | Fast | High | ✅ | Test runner |
| cargo-llvm-cov | Medium | High | ✅ | Coverage |

## Current Findings

- **unwrap()**: No matches found (good!)
- **expect()**: No matches found (good!)
- **unsafe**: 5 instances (all in Candle backends - expected for FFI)
- **panic!**: 20 instances (mostly in tests - acceptable)

## Troubleshooting

### Tool Not Found
Most tools need to be installed (see Installation section above).

### CI Failures
All static analysis jobs use `continue-on-error: true` so they won't block CI. Check artifacts for details.

## References

- [cargo-deny docs](https://github.com/EmbarkStudios/cargo-deny)
- [cargo-machete](https://github.com/bnjbvr/cargo-machete)
- [cargo-geiger](https://github.com/rust-secure-code/cargo-geiger)
- [OpenGrep](https://github.com/opengrep/opengrep)
- [Miri](https://github.com/rust-lang/miri)
- [cargo-nextest](https://nexte.st/)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)

