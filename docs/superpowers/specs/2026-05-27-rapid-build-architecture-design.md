# Rapid Build Architecture — Design Spec

**Date:** 2026-05-27  
**Status:** Approved  
**Scope:** Local Windows development loop for the `anno` workspace

---

## Problem

Five compounding pain points make the current dev loop slow:

| # | Pain point | Root cause |
|---|-----------|-----------|
| A | Cold builds take too long | sccache present but not wired into every invocation |
| B | Incremental builds are slow | No PDB skip, low `codegen-units`, no dep pre-bake |
| C | Build lock contention | rust-analyzer and manual `cargo` share `target/` |
| D | Unclear what to build | No auto-detection of affected crate |
| E | check→test→run loop is manual | No unified script, no automation |

---

## Architecture

Four layers, each independently runnable:

```
┌─────────────────────────────────────────────────────┐
│  Layer 4 · Automation                               │
│  PostToolUse hook  ·  VS Code tasks.json            │
├─────────────────────────────────────────────────────┤
│  Layer 3 · The D-loop script (scripts/loop.ps1)     │
│  check → nextest → smoke-run  (early-exit)          │
├─────────────────────────────────────────────────────┤
│  Layer 2 · Fast build primitives                    │
│  dev-fast.ps1  ·  cargo-nextest  ·  dev-fast profile│
├─────────────────────────────────────────────────────┤
│  Layer 1 · Compiler infrastructure                  │
│  sccache (disk cache)  ·  RA isolated target dir    │
└─────────────────────────────────────────────────────┘
```

---

## Layer 1 — Compiler Infrastructure

### 1a — Rust-analyzer isolated target directory

**Problem:** VS Code rust-analyzer and manual `cargo` commands share `target/`, causing
`Blocking waiting for file lock on build directory` and mutual cache invalidation.

**Fix:** configure RA to write exclusively to `target/ra` via `.vscode/settings.json`:

```json
{
  "rust-analyzer.cargo.targetDir": "target/ra",
  "rust-analyzer.checkOnSave.extraArgs": ["--profile", "dev-fast"]
}
```

RA and manual builds now run truly in parallel. The `.cargo/config.toml` flags
(`--cap-lints allow`, MSVC CRT workaround) apply to both automatically.

### 1b — sccache

**Problem:** sccache is installed but not consistently activated, so cold builds after
branch switches still recompile all ML dependencies (candle, ort, tokenizers, lancedb,
kreuzberg — typically 15–25 min cold).

**Fix — global env in `~/.claude/settings.json`:**

```json
"env": {
  "SCCACHE_DIR": "C:\\Users\\NMarchitecte\\.sccache",
  "RUSTC_WRAPPER": "sccache",
  "SCCACHE_CACHE_SIZE": "30G"
}
```

**Fix — `.cargo/config.toml` (project-level):**

```toml
[build]
rustc-wrapper = "sccache"
# existing rustflags and target.x86_64-pc-windows-msvc stay unchanged
```

**Expected impact:** cold builds on previously-seen dep graphs: ~20 min → ~2–4 min.
The 30 GB cache comfortably holds candle + ort + tokenizers + lancedb + kreuzberg
artifacts without premature eviction.

---

## Layer 2 — Fast Build Primitives

### 2a — `[profile.dev-fast]` tuning

Current state (already in `Cargo.toml`):
```toml
[profile.dev-fast]
inherits = "dev"
debug = 0
incremental = true
codegen-units = 256
```

Add `split-debuginfo = "off"` and `opt-level = 0`:

```toml
[profile.dev-fast]
inherits = "dev"
debug = 0
incremental = true
codegen-units = 256
opt-level = 0
split-debuginfo = "off"   # skip PDB write on Windows — significant linker speedup
overflow-checks = false   # skip runtime overflow traps in dev loop
strip = "none"            # explicit; no surprises

[profile.dev-fast.build-override]
debug = 0
opt-level = 0
codegen-units = 256
```

`split-debuginfo = "off"` is the key addition: on MSVC targets it skips `.pdb`
generation during linking, which accounts for a significant fraction of link time
on large crates (`anno`, `anno-rag`).

### 2b — cargo-nextest

Install:
```powershell
cargo install cargo-nextest --locked
```

Replaces `cargo test` with parallel per-process test execution (3–5× faster).
Structured failure output: failing tests show the exact panic with file+line.

Project config at `.config/nextest.toml`:

```toml
[profile.default]
test-threads = "num-cpus"
failure-output = "immediate"
status-level = "fail"

[profile.ci]
test-threads = 4
retries = 1
```

Usage:
```powershell
cargo nextest run --profile dev-fast -p <crate>
```

---

## Layer 3 — The D-Loop Script (`scripts/loop.ps1`)

Single entry-point for the check → test → run loop.

### Interface

```
loop.ps1 [-Package <crate>] [-Smoke] [-Since <ref>] [-AllAffected]

  -Package      target a specific crate (default: auto-detect from git diff)
  -Smoke        run anno-rag-bin --smoke-check after tests pass
  -Since        git ref for change detection (default: HEAD)
  -AllAffected  also check all crates that depend on the changed crate
```

### Flow

```
1. Detect changed crate(s) via dev-fast.ps1 -PrintOnly
         │
         ▼
2. cargo check --profile dev-fast -p <crate>
   ── FAIL → print errors, exit 1
         │ OK
         ▼
3. cargo nextest run --profile dev-fast -p <crate>
   ── FAIL → print test output, exit 2
         │ OK
         ▼
4. [if -Smoke] anno-rag-bin --smoke-check
   ── FAIL → exit 3
         │ OK
         ▼
5. Print "✓ check  ✓ tests  [✓ smoke]  <elapsed>"
   exit 0
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | All steps passed |
| 1 | Compile error |
| 2 | Test failure |
| 3 | Smoke-run failure |

### Output format

Each step prints its own elapsed time:
```
[1/3] check anno-rag-mcp ...  0.8s  ✓
[2/3] nextest anno-rag-mcp ... 3.2s  ✓  (12 tests)
[3/3] smoke skipped (no -Smoke flag)
Total: 4.0s
```

### Multi-crate behaviour

When `-AllAffected` is set (or a shared crate like `anno` is changed), `cargo check`
runs on all dependents in dependency order. `cargo nextest` runs only on the
directly-changed crate to keep the loop fast.

---

## Layer 4 — Automation

### 4a — PostToolUse hook

Added to `.claude/settings.json` (project-level, committed):

```json
{
  "hooks": {
    "PostToolUse": [{
      "matcher": "Edit|Write",
      "hooks": [{
        "type": "command",
        "shell": "powershell",
        "command": "jq -r '.tool_input.file_path // .tool_response.filePath // \"\"' | ForEach-Object { if ($_ -match 'crates[\\\\/]([^\\\\/]+)[\\\\/]') { powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1 -Package $matches[1] -Mode check } }",
        "timeout": 60,
        "statusMessage": "cargo check ...",
        "async": true
      }]
    }]
  }
}
```

The hook is **async** — it does not block the next edit. Errors surface as a
system notification when the background check finishes.

Fires only for `.rs` files inside `crates/`. Cargo.toml edits and non-Rust
files are ignored.

### 4b — VS Code `tasks.json`

Three tasks at `.vscode/tasks.json`:

| Shortcut | Task | Command |
|----------|------|---------|
| `Ctrl+Shift+B` | **Dev loop** | `scripts/loop.ps1` (crate auto-detected from git diff) |
| `Ctrl+Shift+T` | **Nextest** | `scripts/loop.ps1` with check step skipped (nextest only) |
| `Ctrl+Shift+R` | **Smoke run** | `scripts/loop.ps1 -Smoke` |

All tasks use `"presentation": {"reveal": "always", "panel": "shared"}` so output
goes to the same terminal panel. The default build task (`Ctrl+Shift+B`) includes
a Rust problem matcher so compile errors appear inline in the editor gutter.

---

## Files Changed / Created

| File | Action |
|------|--------|
| `.vscode/settings.json` | Create — RA target dir isolation |
| `.vscode/tasks.json` | Create — keyboard-bound build tasks |
| `.cargo/config.toml` | Modify — add `rustc-wrapper = "sccache"` |
| `Cargo.toml` | Modify — extend `[profile.dev-fast]` |
| `.config/nextest.toml` | Create — nextest defaults |
| `scripts/loop.ps1` | Create — D-loop script |
| `.claude/settings.json` | Modify — PostToolUse hook |
| `~/.claude/settings.json` | Modify — add `SCCACHE_DIR`, `RUSTC_WRAPPER`, `SCCACHE_CACHE_SIZE` env vars |

---

## Expected Outcomes

| Metric | Before | After |
|--------|--------|-------|
| Cold build (fresh branch) | ~20 min | ~2–4 min (sccache hit) |
| Incremental `cargo check` (1 file) | 8–15s | 1–3s |
| Build lock contention errors | Frequent | Zero (separate target dirs) |
| Test run (`anno-rag-mcp`) | ~12s | ~3s (nextest) |
| Full D-loop (check+test) | Manual, 3+ commands | `Ctrl+Shift+B`, ~5–8s |

---

## Out of Scope

- **cargo-hakari** — worth adding if incremental is still slow after B ships; excluded to avoid maintenance overhead now
- **Cloud sccache** (S3/GCS) — excluded; local 30 GB disk cache covers 95% of the benefit
- **lld-link** — excluded; risky with the existing ort/esaxx-rs CRT workaround
- **Cross-compilation** — excluded; Windows MSVC target only for local dev
