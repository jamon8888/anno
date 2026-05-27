# Rapid Build Architecture — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire up the four-layer rapid build architecture from the approved spec, cutting cold builds from ~20 min to ~2–4 min and collapsing the check+test cycle to a single `Ctrl+Shift+B` keypress.

**Architecture:** Layer 1 isolates rust-analyzer into `target/ra` and activates sccache for all cargo invocations. Layer 2 hardens the `dev-fast` Cargo profile (skip PDB, opt-level 0) and adds a `dev-fast` nextest profile. Layer 3 adds `scripts/loop.ps1` as the single entry-point check→nextest→smoke script. Layer 4 automates Layer 3 via an async Claude Code PostToolUse hook and three bound VS Code tasks.

**Tech Stack:** Rust 1.95/MSVC, sccache, cargo-nextest, PowerShell 7+, VS Code, jq (for the hook event parser)

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `.vscode/settings.json` | **Create** | Point RA's target dir to `target/ra`; set checkOnSave to `dev-fast` profile |
| `.cargo/config.toml` | **Modify** | Add `rustc-wrapper = "sccache"` under `[build]` |
| `~/.claude/settings.json` | **Modify** | Add `SCCACHE_DIR`, `RUSTC_WRAPPER`, `SCCACHE_CACHE_SIZE` to global `"env"` |
| `Cargo.toml` | **Modify** | Extend `[profile.dev-fast]` with `split-debuginfo`, `opt-level 0`, `overflow-checks false` |
| `.config/nextest.toml` | **Modify** | Add `failure-output = "immediate"` to default; add `[profile.dev-fast]` section |
| `scripts/loop.ps1` | **Create** | D-loop: detect crate → cargo check → nextest → optional smoke, with per-step timing |
| `.claude/settings.json` | **Create** | Project-level PostToolUse hook: async cargo check on every `.rs` edit |
| `.vscode/tasks.json` | **Create** | Ctrl+Shift+B (loop), Ctrl+Shift+T (nextest only), Ctrl+Shift+R (smoke) |

---

## Task 1: Layer 1a — rust-analyzer target directory isolation

**Files:**
- Create: `.vscode/settings.json`

**Why this matters:** VS Code rust-analyzer and manual `cargo` default to sharing `target/`. This causes `Blocking waiting for file lock on build directory` and mutual cache busting. Pointing RA exclusively at `target/ra` lets both processes run truly in parallel.

- [ ] **Step 1: Record current state**

Check whether a lock wait appears when running alongside RA:
```powershell
cargo check -p anno-rag-mcp --profile dev-fast 2>&1 | Select-String "Blocking|lock"
```
Expected: no output (no current lock). If you see "Blocking waiting for file lock" here, RA is still using the shared dir.

- [ ] **Step 2: Create `.vscode/settings.json`**

```json
{
  "rust-analyzer.cargo.targetDir": "target/ra",
  "rust-analyzer.checkOnSave.extraArgs": ["--profile", "dev-fast"]
}
```

- [ ] **Step 3: Reload VS Code and verify RA uses the isolated dir**

Press `Ctrl+Shift+P` → "Developer: Reload Window". Open any `.rs` file to trigger a background RA check (watch the status bar for "rust-analyzer: checking…"). After a few seconds:
```powershell
Test-Path "target\ra"
```
Expected: `True`

- [ ] **Step 4: Verify parallel execution (no lock contention)**

While the RA status bar shows "checking…", run a targeted check simultaneously:
```powershell
cargo check -p anno-rag-mcp --profile dev-fast 2>&1 | Select-String "Blocking|lock"
```
Expected: no output — both processes run in parallel without contention.

- [ ] **Step 5: Commit**

```powershell
git add .vscode/settings.json
git commit -m "feat(build): isolate rust-analyzer to target/ra, checkOnSave uses dev-fast profile"
```

---

## Task 2: Layer 1b — sccache activation

**Files:**
- Modify: `.cargo/config.toml`
- Modify: `~/.claude/settings.json` (global, outside repo — not committed)

**Why this matters:** sccache is installed but ignored. Without `rustc-wrapper`, every branch switch recompiles candle, ort, tokenizers, lancedb from scratch (~20 min). With sccache active, cache-hitting builds drop to ~2–4 min.

**Prerequisite check:**
```powershell
sccache --version
```
Expected: `sccache 0.x.y`. If missing: `cargo install sccache --locked`

- [ ] **Step 1: Record baseline sccache stats**

```powershell
sccache --show-stats
```
Note the "Compile requests" and "Cache hits" values. You'll compare after.

- [ ] **Step 2: Add `rustc-wrapper` to `.cargo/config.toml`**

The current `[build]` section in `.cargo/config.toml` is:
```toml
[build]
rustflags = ["--cap-lints", "allow"]
rustdocflags = ["--cap-lints", "allow"]
```

Replace it with:
```toml
[build]
rustc-wrapper = "sccache"
rustflags = ["--cap-lints", "allow"]
rustdocflags = ["--cap-lints", "allow"]
```

Leave the `[target.x86_64-pc-windows-msvc]` and `[env]` sections below unchanged.

- [ ] **Step 3: Add sccache env vars to `~/.claude/settings.json`**

Open `C:\Users\NMarchitecte\.claude\settings.json`. The existing `"env"` key is:
```json
"env": {
  "DISABLE_NON_ESSENTIAL_MODEL_CALLS": "1"
}
```

Add three new keys (keep the existing one):
```json
"env": {
  "DISABLE_NON_ESSENTIAL_MODEL_CALLS": "1",
  "SCCACHE_DIR": "C:\\Users\\NMarchitecte\\.sccache",
  "RUSTC_WRAPPER": "sccache",
  "SCCACHE_CACHE_SIZE": "30G"
}
```

- [ ] **Step 4: Verify sccache is invoked**

Run a targeted check (this will be a cold miss — that's expected on first run):
```powershell
cargo check -p anno-rag-mcp --profile dev-fast 2>&1 | Select-Object -Last 3
sccache --show-stats
```
Expected: "Compile requests" count increased. The first run adds entries to the cache.

- [ ] **Step 5: Verify cache hits on second run**

```powershell
cargo clean -p anno-rag-mcp
cargo check -p anno-rag-mcp --profile dev-fast 2>&1 | Select-Object -Last 3
sccache --show-stats
```
Expected: "Cache hits" count increased — the second check after a clean is now fast because sccache serves the artifacts from disk.

- [ ] **Step 6: Commit**

```powershell
git add .cargo/config.toml
git commit -m "feat(build): wire sccache as rustc-wrapper in .cargo/config.toml"
```

---

## Task 3: Layer 2a — dev-fast profile hardening

**Files:**
- Modify: `Cargo.toml` (the `[profile.dev-fast]` and `[profile.dev-fast.build-override]` blocks)

**Why this matters:** `split-debuginfo = "off"` is the key addition — it skips `.pdb` file generation during linking on MSVC, which accounts for a large fraction of link time on `anno-rag` and `anno-rag-mcp`. `opt-level = 0` and `overflow-checks = false` remove the remaining per-function overhead during the dev loop.

- [ ] **Step 1: Record incremental check timing (before)**

Touch a file to force a re-check, then measure:
```powershell
(Get-Item crates\anno-rag-mcp\src\lib.rs).LastWriteTime = Get-Date
$t = Measure-Command { cargo check -p anno-rag-mcp --profile dev-fast 2>&1 | Out-Null }
Write-Host ("Before: {0:F1}s" -f $t.TotalSeconds)
```

- [ ] **Step 2: Replace the dev-fast profile blocks in `Cargo.toml`**

Find and replace the current blocks (currently at lines 128–137):

**Before:**
```toml
[profile.dev-fast]
inherits = "dev"
debug = 0
incremental = true
codegen-units = 256

[profile.dev-fast.build-override]
debug = 0
opt-level = 0
codegen-units = 256
```

**After:**
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

- [ ] **Step 3: Verify clean compilation**

```powershell
cargo check -p anno-rag-mcp --profile dev-fast 2>&1
```
Expected: exit 0, no errors.

Also check with a broader crate to confirm `split-debuginfo` doesn't conflict with the MSVC CRT workaround:
```powershell
cargo check -p anno-rag-bin --profile dev-fast 2>&1 | Select-Object -Last 5
```
Expected: exit 0.

- [ ] **Step 4: Record incremental timing (after)**

```powershell
(Get-Item crates\anno-rag-mcp\src\lib.rs).LastWriteTime = Get-Date
$t = Measure-Command { cargo check -p anno-rag-mcp --profile dev-fast 2>&1 | Out-Null }
Write-Host ("After: {0:F1}s" -f $t.TotalSeconds)
```
Expected: lower than Task 3 Step 1 — the link phase is shorter without PDB generation.

- [ ] **Step 5: Commit**

```powershell
git add Cargo.toml
git commit -m "perf(build): harden dev-fast profile — split-debuginfo off, overflow-checks false, strip none"
```

---

## Task 4: Layer 2b — cargo-nextest dev-fast profile

**Files:**
- Modify: `.config/nextest.toml`

**Why this matters:** `.config/nextest.toml` already has ci/quick/ml/property profiles but is missing a `dev-fast` nextest profile used by `loop.ps1`. It also lacks `failure-output = "immediate"` on the default profile, which means failing test output only appears after the full run completes.

**Prerequisite check:**
```powershell
cargo nextest --version
```
Expected: `cargo-nextest nextest 0.x.y`. If missing: `cargo install cargo-nextest --locked`

- [ ] **Step 1: Run baseline test timing**

```powershell
$t = Measure-Command { cargo nextest run -p anno-rag-mcp 2>&1 | Out-Null }
Write-Host ("Baseline: {0:F1}s" -f $t.TotalSeconds)
```

- [ ] **Step 2: Add `failure-output = "immediate"` to `[profile.default]`**

In `.config/nextest.toml`, add one line to `[profile.default]`:

```toml
[profile.default]
# Retry flaky tests once (not timeouts -- those are real failures)
retries = 1
failure-output = "immediate"
```

- [ ] **Step 3: Add `[profile.dev-fast]` section**

Insert this block after `[profile.default]` (before the `# CI Profile` comment):

```toml
# =============================================================================
# Dev-Fast Profile — used by scripts/loop.ps1
# =============================================================================

[profile.dev-fast]
# No retries in the dev loop — fail fast, fix, retry manually
retries = 0
# Show failure output immediately; don't wait for the full run to finish
failure-output = "immediate"
# Only show failing tests — keep the loop output clean
status-level = "fail"
# Use all available CPU threads for maximum parallelism
test-threads = "num-cpus"
```

- [ ] **Step 4: Verify the dev-fast profile works**

```powershell
cargo nextest run --profile dev-fast -p anno-rag-mcp 2>&1
```
Expected: tests run in parallel, only failures (if any) appear in output, exit 0 on a clean codebase.

- [ ] **Step 5: Verify baseline profile still works**

```powershell
cargo nextest run -p anno-rag-mcp 2>&1 | Select-Object -Last 3
```
Expected: exit 0 — the change to `[profile.default]` didn't break anything.

- [ ] **Step 6: Commit**

```powershell
git add .config/nextest.toml
git commit -m "feat(build): add nextest dev-fast profile, add failure-output=immediate to default profile"
```

---

## Task 5: Layer 3 — loop.ps1 D-loop script

**Files:**
- Create: `scripts/loop.ps1`

**Why this matters:** This is the single-entry-point that replaces the manual 3-command sequence. It auto-detects the changed crate via `dev-fast.ps1 -PrintOnly`, runs each step with elapsed timing printed inline, and exits with a meaningful code so automation can react to failure type.

- [ ] **Step 1: Create `scripts/loop.ps1`**

```powershell
<#
.SYNOPSIS
    D-loop: check → nextest → smoke-run for the anno workspace.

.PARAMETER Package
    Target a specific crate (default: auto-detect from git diff via dev-fast.ps1).

.PARAMETER Smoke
    Run anno-rag-bin --smoke-check after tests pass.

.PARAMETER Since
    Git ref for change detection passed to dev-fast.ps1 (default: HEAD).

.PARAMETER AllAffected
    Run cargo check on all crates that depend on the changed crate.
    cargo nextest still runs only the primary (directly-changed) crate.

.PARAMETER SkipCheck
    Skip the cargo check step and go straight to nextest.
    Used by the VS Code Nextest task (Ctrl+Shift+T).

.EXAMPLE
    scripts\loop.ps1
    scripts\loop.ps1 -Package anno-rag-mcp
    scripts\loop.ps1 -Package anno-rag -AllAffected -Smoke
    scripts\loop.ps1 -SkipCheck -Package anno-rag-mcp

.NOTES
    Exit codes:
      0 — all steps passed
      1 — cargo check failed
      2 — nextest failed
      3 — smoke-run failed
#>
param(
    [string]$Package  = "",
    [switch]$Smoke,
    [string]$Since    = "HEAD",
    [switch]$AllAffected,
    [switch]$SkipCheck
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# ─────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────

function Format-Elapsed {
    param([datetime]$Start)
    return "{0:F1}s" -f ((Get-Date) - $Start).TotalSeconds
}

# ─────────────────────────────────────────────────
# Package detection
# ─────────────────────────────────────────────────

$primary_package = $Package
$check_packages  = @()

if (-not $primary_package) {
    $raw = & powershell -NoProfile -ExecutionPolicy Bypass `
        -File "$ScriptDir\dev-fast.ps1" -PrintOnly -Since $Since 2>&1
    $pkg_line = $raw | Where-Object { $_ -match '^Packages: ' } | Select-Object -First 1
    if (-not $pkg_line) {
        Write-Host "No changed crates detected. Use -Package <crate> to target manually."
        exit 0
    }
    $names = ($pkg_line -replace '^Packages: ', '') -split ', ' |
        ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }
    $primary_package = $names[0]
}

$check_packages = @($primary_package)

if ($AllAffected) {
    $raw_all = & powershell -NoProfile -ExecutionPolicy Bypass `
        -File "$ScriptDir\dev-fast.ps1" -AllAffected -PrintOnly -Since $Since 2>&1
    $pkg_line_all = $raw_all | Where-Object { $_ -match '^Packages: ' } | Select-Object -First 1
    if ($pkg_line_all) {
        $check_packages = ($pkg_line_all -replace '^Packages: ', '') -split ', ' |
            ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }
    }
}

$total_start = Get-Date

# ─────────────────────────────────────────────────
# Step 1 — cargo check
# ─────────────────────────────────────────────────

if (-not $SkipCheck) {
    $check_label = $check_packages -join ', '
    Write-Host "[1/3] check $check_label ..." -NoNewline
    $step_start = Get-Date

    $check_ok  = $true
    $check_out = @()
    foreach ($pkg in $check_packages) {
        $out = cargo check --profile dev-fast -p $pkg 2>&1
        if ($LASTEXITCODE -ne 0) {
            $check_ok  = $false
            $check_out += $out
            break
        }
    }

    $elapsed = Format-Elapsed $step_start
    if (-not $check_ok) {
        Write-Host "  $elapsed  ✗"
        $check_out | ForEach-Object { Write-Host $_ }
        exit 1
    }
    Write-Host "  $elapsed  ✓"
} else {
    Write-Host "[1/3] check skipped (-SkipCheck)"
}

# ─────────────────────────────────────────────────
# Step 2 — cargo nextest
# ─────────────────────────────────────────────────

Write-Host "[2/3] nextest $primary_package ..." -NoNewline
$step_start  = Get-Date

$nextest_out  = cargo nextest run --profile dev-fast -p $primary_package 2>&1
$nextest_exit = $LASTEXITCODE
$elapsed      = Format-Elapsed $step_start

if ($nextest_exit -ne 0) {
    Write-Host "  $elapsed  ✗"
    $nextest_out | ForEach-Object { Write-Host $_ }
    exit 2
}

# Parse test count from summary line: "N tests run: N passed, 0 skipped"
$summary = $nextest_out | Where-Object { $_ -match '(\d+) tests? run:' } | Select-Object -Last 1
$count_label = ""
if ($summary -match '(\d+) tests? run:') {
    $count_label = "  ($($Matches[1]) tests)"
}

Write-Host "  $elapsed  ✓$count_label"

# ─────────────────────────────────────────────────
# Step 3 — smoke run (optional)
# ─────────────────────────────────────────────────

if ($Smoke) {
    Write-Host "[3/3] smoke ..." -NoNewline
    $step_start = Get-Date

    $smoke_out = & anno-rag-bin --smoke-check 2>&1
    $elapsed   = Format-Elapsed $step_start

    if ($LASTEXITCODE -ne 0) {
        Write-Host "  $elapsed  ✗"
        $smoke_out | ForEach-Object { Write-Host $_ }
        exit 3
    }

    Write-Host "  $elapsed  ✓"
} else {
    Write-Host "[3/3] smoke skipped (no -Smoke flag)"
}

# ─────────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────────

$total_elapsed = Format-Elapsed $total_start
Write-Host "Total: $total_elapsed"
exit 0
```

- [ ] **Step 2: Stage a change so auto-detection works, then run**

```powershell
(Get-Item crates\anno-rag-mcp\src\lib.rs).LastWriteTime = Get-Date
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\loop.ps1
```
Expected output format:
```
[1/3] check anno-rag-mcp ...  0.9s  ✓
[2/3] nextest anno-rag-mcp ...  3.1s  ✓  (12 tests)
[3/3] smoke skipped (no -Smoke flag)
Total: 4.0s
```

- [ ] **Step 3: Verify exit code 0**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\loop.ps1 -Package anno-rag-mcp
Write-Host "Exit: $LASTEXITCODE"
```
Expected: `Exit: 0`

- [ ] **Step 4: Verify exit code 1 (compile failure)**

```powershell
# Introduce a temporary syntax error
Add-Content crates\anno-rag-mcp\src\lib.rs "`n// BREAK fn bad_syntax( {"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\loop.ps1 -Package anno-rag-mcp
Write-Host "Exit: $LASTEXITCODE"
# Revert
git checkout -- crates\anno-rag-mcp\src\lib.rs
```
Expected: `Exit: 1` with cargo error output shown, no nextest step reached.

- [ ] **Step 5: Verify -SkipCheck**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\loop.ps1 -Package anno-rag-mcp -SkipCheck
```
Expected first line: `[1/3] check skipped (-SkipCheck)`

- [ ] **Step 6: Verify -AllAffected expands dependents**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\loop.ps1 -Package anno-rag -AllAffected
```
Expected: `[1/3] check anno-rag, anno-rag-mcp, anno-rag-bin ...  ✓` (3 crates checked in Step 1; nextest runs only `anno-rag` in Step 2)

- [ ] **Step 7: Commit**

```powershell
git add scripts\loop.ps1
git commit -m "feat(build): add loop.ps1 D-loop (auto-detect, check -> nextest -> smoke, per-step timing)"
```

---

## Task 6: Layer 4a — PostToolUse hook

**Files:**
- Create: `.claude/settings.json` (project-level, committed to the repo)

**Why this matters:** Every time Claude Code edits a `.rs` file in `crates/`, a background `cargo check` fires automatically. Compile errors surface as notifications without requiring a manual loop run. The `async: true` flag means the hook never blocks the next edit.

**Prerequisite check:**
```powershell
jq --version
```
Expected: `jq-1.x`. If missing: `winget install jqlang.jq`

- [ ] **Step 1: Create `.claude/settings.json`**

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "shell": "powershell",
            "command": "jq -r '.tool_input.file_path // .tool_response.filePath // \"\"' | ForEach-Object { if ($_ -match 'crates[\\\\/]([^\\\\/]+)[\\\\/].*\\.rs$') { powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1 -Package $Matches[1] -Mode check } }",
            "timeout": 60,
            "statusMessage": "cargo check ...",
            "async": true
          }
        ]
      }
    ]
  }
}
```

**How it works:** Claude Code passes the tool event as JSON on stdin. `jq` extracts the `file_path`. The PowerShell `if` guard fires only when the path matches `crates/<name>/...*.rs` — Cargo.toml edits and non-Rust files are silently ignored.

- [ ] **Step 2: Verify hook fires after a Rust file edit**

Ask Claude Code to make a trivial edit (e.g., add a blank comment) to `crates/anno-rag-mcp/src/lib.rs`. Within 5–10 seconds, you should see a "cargo check ..." status message appear in the Claude Code status bar. The hook runs in the background (`async: true`) and does not block.

- [ ] **Step 3: Verify hook does NOT fire for non-Rust files**

Ask Claude Code to edit `Cargo.toml` or `.cargo/config.toml`. Verify no "cargo check" notification appears — the `.rs` regex guard prevents it.

- [ ] **Step 4: Commit**

```powershell
git add .claude/settings.json
git commit -m "feat(build): add PostToolUse hook to auto cargo-check on .rs file edits"
```

---

## Task 7: Layer 4b — VS Code tasks.json

**Files:**
- Create: `.vscode/tasks.json`

**Why this matters:** Three keyboard shortcuts replace the manual terminal workflow. `Ctrl+Shift+B` runs the full check+nextest loop with the Rust problem matcher so compile errors appear inline in the editor gutter. `Ctrl+Shift+T` runs nextest only (skips check, fastest feedback on logic bugs). `Ctrl+Shift+R` runs the full loop including smoke.

- [ ] **Step 1: Create `.vscode/tasks.json`**

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "Dev loop (check + nextest)",
      "type": "shell",
      "command": "powershell",
      "args": [
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", "${workspaceFolder}\\scripts\\loop.ps1"
      ],
      "group": {
        "kind": "build",
        "isDefault": true
      },
      "presentation": {
        "reveal": "always",
        "panel": "shared",
        "clear": true
      },
      "problemMatcher": {
        "owner": "rust",
        "fileLocation": ["relative", "${workspaceFolder}"],
        "pattern": [
          {
            "regexp": "^error(?:\\[([^\\]]+)\\])?: (.+)$",
            "message": 2,
            "code": 1
          },
          {
            "regexp": "^\\s*-->\\s+(.+):(\\d+):(\\d+)$",
            "file": 1,
            "line": 2,
            "column": 3
          }
        ]
      }
    },
    {
      "label": "Nextest only (skip check)",
      "type": "shell",
      "command": "powershell",
      "args": [
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", "${workspaceFolder}\\scripts\\loop.ps1",
        "-SkipCheck"
      ],
      "group": "test",
      "presentation": {
        "reveal": "always",
        "panel": "shared",
        "clear": true
      },
      "problemMatcher": []
    },
    {
      "label": "Smoke run (check + nextest + smoke)",
      "type": "shell",
      "command": "powershell",
      "args": [
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", "${workspaceFolder}\\scripts\\loop.ps1",
        "-Smoke"
      ],
      "group": "build",
      "presentation": {
        "reveal": "always",
        "panel": "shared",
        "clear": true
      },
      "problemMatcher": []
    }
  ]
}
```

- [ ] **Step 2: Bind Ctrl+Shift+T and Ctrl+Shift+R to the named tasks**

Open VS Code keyboard shortcuts JSON (`Ctrl+Shift+P` → "Open Keyboard Shortcuts (JSON)") and add:

```json
[
  {
    "key": "ctrl+shift+t",
    "command": "workbench.action.tasks.runTask",
    "args": "Nextest only (skip check)"
  },
  {
    "key": "ctrl+shift+r",
    "command": "workbench.action.tasks.runTask",
    "args": "Smoke run (check + nextest + smoke)"
  }
]
```

> `Ctrl+Shift+B` is auto-bound to the default build task by VS Code — no extra keybinding needed.

- [ ] **Step 3: Verify Ctrl+Shift+B triggers the dev loop**

Press `Ctrl+Shift+B`. Expected: a terminal panel labelled "Dev loop (check + nextest)" opens and runs `loop.ps1`. You should see:
```
[1/3] check <crate> ...  X.Xs  ✓
[2/3] nextest <crate> ...  X.Xs  ✓  (N tests)
[3/3] smoke skipped (no -Smoke flag)
Total: X.Xs
```

- [ ] **Step 4: Verify Ctrl+Shift+T skips check**

Press `Ctrl+Shift+T`. Expected first line in terminal: `[1/3] check skipped (-SkipCheck)`

- [ ] **Step 5: Verify Rust problem matcher picks up a compile error**

Introduce a temporary syntax error in `crates/anno-rag-mcp/src/lib.rs`, then press `Ctrl+Shift+B`. After the task runs, open the Problems panel (`Ctrl+Shift+M`). Expected: the error appears with a clickable file link pointing to the line with the syntax error. Revert the error after verifying.

- [ ] **Step 6: Commit**

```powershell
git add .vscode/tasks.json
git commit -m "feat(build): add VS Code tasks for dev loop (Ctrl+Shift+B/T/R)"
```

---

## Self-Review Checklist

Before starting execution, verify against the spec:

| Spec requirement | Covered by |
|-----------------|------------|
| RA isolated to `target/ra` | Task 1 |
| RA checkOnSave uses `dev-fast` profile | Task 1 |
| `rustc-wrapper = "sccache"` in `.cargo/config.toml` | Task 2 |
| `SCCACHE_DIR`, `RUSTC_WRAPPER`, `SCCACHE_CACHE_SIZE` in global env | Task 2 |
| `split-debuginfo = "off"` in dev-fast profile | Task 3 |
| `opt-level = 0`, `overflow-checks = false`, `strip = "none"` | Task 3 |
| `[profile.dev-fast.build-override]` unchanged | Task 3 |
| nextest installed | Task 4 prerequisite |
| `[profile.dev-fast]` in nextest config | Task 4 |
| `failure-output = "immediate"` in nextest default | Task 4 |
| `loop.ps1` with `-Package`, `-Smoke`, `-Since`, `-AllAffected`, `-SkipCheck` | Task 5 |
| Per-step elapsed timing in loop output | Task 5 |
| Exit codes 0/1/2/3 | Task 5 |
| Multi-crate check (AllAffected), nextest on primary only | Task 5 |
| PostToolUse hook (async, `.rs` files only) | Task 6 |
| VS Code tasks at Ctrl+Shift+B/T/R | Task 7 |
| Shared terminal panel | Task 7 |
| Rust problem matcher on Ctrl+Shift+B | Task 7 |
