# Anno Release Finalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the path from the verified local ingest pipeline to publishable Windows/macOS GitHub Release assets for Claude Desktop.

**Architecture:** Keep runtime correctness gates local and fast, then let GitHub Actions build the three OS archives from an immutable tag. The release workflow must use the same gateway boot-smoke semantics as the local gate, because `anno-privacy-gateway` is a server binary and `--help` starts the server path instead of printing CLI help.

**Tech Stack:** Rust/Cargo, PowerShell, POSIX shell, GitHub Actions, GitHub Releases, Claude Desktop MCP config.

---

## Current Baseline

- Branch: `claude/clever-wescoff-7b6592`
- Last known commit: `4f889e61 ci: add local release pipeline gate`
- Local fresh gate artifact: `target/local-release-gate/run-20260519-185543`
- Fresh gate result:
  - `anno-rag ingest local samples`: passed, `10` documents, `400.95s`
  - `anno-rag reingest idempotency smoke`: passed, `0` documents, `1.23s`
  - four `anno-rag search` smoke queries: passed
  - `anno-privacy-gateway boot smoke`: passed
- Known local host constraint: Windows debug PDB files can exhaust `C:`. Before long builds, keep at least 20 GB free or move `CARGO_TARGET_DIR` to a larger drive.

## File Structure

- Create `scripts/release/smoke-gateway.ps1`
  - Windows and local PowerShell smoke for `anno-privacy-gateway`: start on `ANNO_GATEWAY_LISTEN=127.0.0.1:0`, require it to stay alive for a short window, then stop it.
- Create `scripts/release/smoke-gateway.sh`
  - macOS/Linux CI smoke with the same server-start semantics.
- Modify `.github/workflows/release-binaries.yml`
  - Replace `anno-privacy-gateway --help` smoke steps with the new boot-smoke scripts.
- Modify `justfile`
  - Add the two gateway smoke scripts to `release-validate`.
- Modify `docs/release/README-release.md`
  - Add final pre-tag gate commands and acceptance metrics for `fast`, release archive inspection, and Claude Desktop smoke.

---

### Task 1: Add Reusable Gateway Boot Smoke Scripts

**Files:**
- Create: `scripts/release/smoke-gateway.ps1`
- Create: `scripts/release/smoke-gateway.sh`

- [ ] **Step 1: Write Windows PowerShell smoke**

Create `scripts/release/smoke-gateway.ps1` with this content:

```powershell
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$BinaryPath,

    [Parameter(Mandatory = $false)]
    [ValidateRange(1, 30)]
    [int]$Seconds = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ResolvedBinary = Resolve-Path -LiteralPath $BinaryPath
$StdoutPath = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath ("anno-gateway-smoke-{0}.stdout.log" -f [System.Guid]::NewGuid())
$StderrPath = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath ("anno-gateway-smoke-{0}.stderr.log" -f [System.Guid]::NewGuid())

$Psi = [System.Diagnostics.ProcessStartInfo]::new()
$Psi.FileName = $ResolvedBinary.Path
$Psi.WorkingDirectory = (Get-Location).Path
$Psi.RedirectStandardOutput = $true
$Psi.RedirectStandardError = $true
$Psi.UseShellExecute = $false
$Psi.CreateNoWindow = $true
$Psi.EnvironmentVariables["ANNO_GATEWAY_LISTEN"] = "127.0.0.1:0"

$Process = [System.Diagnostics.Process]::Start($Psi)
$StdoutTask = $Process.StandardOutput.ReadToEndAsync()
$StderrTask = $Process.StandardError.ReadToEndAsync()

try {
    if ($Process.WaitForExit($Seconds * 1000)) {
        $StdoutTask.Result | Set-Content -LiteralPath $StdoutPath -Encoding UTF8
        $StderrTask.Result | Set-Content -LiteralPath $StderrPath -Encoding UTF8
        Write-Error "Gateway exited early with code $($Process.ExitCode). stdout=$StdoutPath stderr=$StderrPath"
        exit 1
    }

    $Process.Kill($true)
    $Process.WaitForExit()
    $StdoutTask.Result | Set-Content -LiteralPath $StdoutPath -Encoding UTF8
    $StderrTask.Result | Set-Content -LiteralPath $StderrPath -Encoding UTF8
    Write-Output "Gateway stayed alive for ${Seconds}s on ANNO_GATEWAY_LISTEN=127.0.0.1:0; smoke passed."
} finally {
    if (-not $Process.HasExited) {
        $Process.Kill($true)
        $Process.WaitForExit()
    }
}
```

- [ ] **Step 2: Write POSIX smoke**

Create `scripts/release/smoke-gateway.sh` with this content:

```bash
#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 BINARY_PATH [SECONDS]" >&2
  exit 2
fi

binary_path="$1"
seconds="${2:-3}"

if [[ ! -x "${binary_path}" ]]; then
  echo "Gateway binary is not executable: ${binary_path}" >&2
  exit 1
fi

stdout_path="$(mktemp "${TMPDIR:-/tmp}/anno-gateway-smoke.XXXXXX.stdout.log")"
stderr_path="$(mktemp "${TMPDIR:-/tmp}/anno-gateway-smoke.XXXXXX.stderr.log")"

ANNO_GATEWAY_LISTEN="127.0.0.1:0" "${binary_path}" >"${stdout_path}" 2>"${stderr_path}" &
pid="$!"

cleanup() {
  if kill -0 "${pid}" >/dev/null 2>&1; then
    kill "${pid}" >/dev/null 2>&1 || true
    wait "${pid}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

sleep "${seconds}"

if kill -0 "${pid}" >/dev/null 2>&1; then
  cleanup
  trap - EXIT
  echo "Gateway stayed alive for ${seconds}s on ANNO_GATEWAY_LISTEN=127.0.0.1:0; smoke passed."
  exit 0
fi

wait "${pid}"
exit_code="$?"
{
  echo "Gateway exited early with code ${exit_code}."
  echo "stdout: ${stdout_path}"
  echo "stderr: ${stderr_path}"
} >&2
exit 1
```

- [ ] **Step 3: Mark POSIX smoke executable**

Run:

```bash
chmod +x scripts/release/smoke-gateway.sh
```

Expected: `git diff --summary -- scripts/release/smoke-gateway.sh` shows executable mode.

- [ ] **Step 4: Verify syntax without running the server**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\smoke-gateway.ps1 -BinaryPath C:\not-real\anno-privacy-gateway.exe
```

Expected: fail with `Cannot find path` or `Resolve-Path` error. This proves the script parses before it tries to spawn the binary.

Run:

```bash
bash -n scripts/release/smoke-gateway.sh
```

Expected: exit code `0`.

- [ ] **Step 5: Verify against an existing debug gateway**

Run on Windows after `cargo build -p anno-privacy-gateway`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\smoke-gateway.ps1 -BinaryPath C:\cargo-target\debug\anno-privacy-gateway.exe
```

Expected: `Gateway stayed alive for 3s ... smoke passed.`

- [ ] **Step 6: Commit**

Run:

```bash
git add scripts/release/smoke-gateway.ps1 scripts/release/smoke-gateway.sh
git commit -m "ci: add privacy gateway boot smoke scripts"
```

Expected: commit contains only the two smoke scripts.

---

### Task 2: Fix GitHub Release Workflow Gateway Smoke

**Files:**
- Modify: `.github/workflows/release-binaries.yml`
- Modify: `justfile`

- [ ] **Step 1: Replace Windows gateway help smoke**

In `.github/workflows/release-binaries.yml`, replace this step:

```yaml
      - name: Smoke test anno-privacy-gateway (Windows)
        if: runner.os == 'Windows'
        run: .\target\${{ matrix.target }}\release\anno-privacy-gateway.exe --help
```

with:

```yaml
      - name: Smoke test anno-privacy-gateway boot (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: .\scripts\release\smoke-gateway.ps1 -BinaryPath ".\target\${{ matrix.target }}\release\anno-privacy-gateway.exe"
```

- [ ] **Step 2: Replace macOS gateway help smoke**

In `.github/workflows/release-binaries.yml`, replace this step:

```yaml
      - name: Smoke test anno-privacy-gateway (macOS)
        if: runner.os == 'macOS'
        run: ./target/${{ matrix.target }}/release/anno-privacy-gateway --help
```

with:

```yaml
      - name: Smoke test anno-privacy-gateway boot (macOS)
        if: runner.os == 'macOS'
        run: ./scripts/release/smoke-gateway.sh "./target/${{ matrix.target }}/release/anno-privacy-gateway"
```

- [ ] **Step 3: Add smoke scripts to release validation**

In `justfile`, add these checks inside `release-validate` beside the other `scripts/release` checks:

```make
    test -f scripts/release/smoke-gateway.ps1
    test -x scripts/release/smoke-gateway.sh
```

- [ ] **Step 4: Run local validation**

Run:

```bash
just release-validate
git diff --check -- .github/workflows/release-binaries.yml justfile scripts/release
```

Expected: both commands exit `0`.

- [ ] **Step 5: Commit**

Run:

```bash
git add .github/workflows/release-binaries.yml justfile
git commit -m "ci: boot-smoke privacy gateway in release workflow"
```

Expected: commit contains workflow and `justfile` only.

---

### Task 3: Lock Pre-Tag Local Gate Acceptance Metrics

**Files:**
- Modify: `docs/release/README-release.md`

- [ ] **Step 1: Add pre-tag gate section**

In `docs/release/README-release.md`, after the existing "Pre-Release Local Pipeline Gate" artifact list, add:

````markdown
### Pre-Tag Acceptance Metrics

Before pushing a release tag, run:

```powershell
cargo build -p anno-rag-bin -p anno-privacy-gateway
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\local-pipeline-gate.ps1 -Profile fast -SkipMcp -SkipBuild
```

Acceptance:

- `reports/metrics.json` has `"summary": { "status": "passed" }`.
- `anno-rag ingest local samples` ingests exactly `10` documents from the built-in `11` sample files. The duplicate sample is expected to deduplicate.
- `anno-rag reingest idempotency smoke` ingests exactly `0` documents.
- Each search smoke exits `0`.
- `anno-privacy-gateway boot smoke` exits `0`.
- No `anno-rag`, `anno_rag`, `anno-privacy-gateway`, `cargo`, or `rustc` process remains after the gate.

The latest known Windows debug baseline was `400.95s` for fresh ingest and `1.23s` for re-ingest. Treat fresh ingest above `900s`, re-ingest above `10s`, or any search above `90s` as a regression to investigate before tagging.
````

- [ ] **Step 2: Verify docs diff**

Run:

```powershell
git diff --check -- docs\release\README-release.md
```

Expected: exit code `0`.

- [ ] **Step 3: Commit**

Run:

```bash
git add docs/release/README-release.md
git commit -m "docs: define pre-tag release gate metrics"
```

Expected: commit contains only the release README.

---

### Task 4: Run Final Local Gates On The Release Branch

**Files:**
- Read-only verification.

- [ ] **Step 1: Check disk before long local builds**

Run:

```powershell
Get-PSDrive -PSProvider FileSystem | Select-Object Name,Free,Root | Format-Table -AutoSize
```

Expected: `C:` has at least `20 GB` free. If not, delete only generated files under `C:\cargo-target\debug\deps\*.pdb` larger than `1 GB` after verifying the resolved absolute path stays inside `C:\cargo-target\debug\deps`.

- [ ] **Step 2: Build current debug binaries**

Run:

```powershell
cargo build -p anno-rag-bin -p anno-privacy-gateway
```

Expected: exit code `0`.

- [ ] **Step 3: Run static local gate validation**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\test-local-pipeline-gate.ps1
```

Expected: `local-pipeline-gate static tests passed`.

- [ ] **Step 4: Run the fresh fast gate**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\local-pipeline-gate.ps1 -Profile fast -SkipMcp -SkipBuild
```

Expected:

- final output includes `status: passed`
- `reports/metrics.json` exists under the printed run directory
- first ingest log says `ingested 10 documents`
- re-ingest log says `ingested 0 documents`

- [ ] **Step 5: Verify no gate process remains**

Run:

```powershell
Get-CimInstance Win32_Process -Filter "name='powershell.exe' OR name='anno-rag.exe' OR name='anno_rag.exe' OR name='anno-privacy-gateway.exe' OR name='cargo.exe' OR name='rustc.exe'" |
  Where-Object {
    $_.CommandLine -like '*local-pipeline-gate.ps1*' -or
    $_.CommandLine -like '*anno*rag*.exe*ingest*local-release-gate*' -or
    $_.CommandLine -like '*anno-privacy-gateway.exe*'
  } |
  Select-Object ProcessId,ParentProcessId,Name,CommandLine |
  Format-List
```

Expected: no output.

- [ ] **Step 6: Commit if docs or scripts changed during gate work**

Run:

```bash
git status --short
```

Expected: clean. If intentional files changed, inspect `git diff`, stage only those files, and commit with a `ci:` or `docs:` message.

---

### Task 5: Build And Inspect The Windows Release Archive Locally

**Files:**
- Read-only verification unless packaging defects are found.

- [ ] **Step 1: Build Windows release binaries**

Run:

```powershell
$env:RUSTFLAGS="-C target-feature=-crt-static"
$env:CFLAGS_x86_64_pc_windows_msvc="-MD"
$env:CXXFLAGS_x86_64_pc_windows_msvc="-MD"
cargo build --release -p anno-rag-bin -p anno-privacy-gateway --target x86_64-pc-windows-msvc
```

Expected: exit code `0`; binaries exist at:

```text
target/x86_64-pc-windows-msvc/release/anno-rag.exe
target/x86_64-pc-windows-msvc/release/anno-privacy-gateway.exe
```

- [ ] **Step 2: Smoke test release binaries**

Run:

```powershell
.\target\x86_64-pc-windows-msvc\release\anno-rag.exe --help
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\smoke-gateway.ps1 -BinaryPath .\target\x86_64-pc-windows-msvc\release\anno-privacy-gateway.exe
```

Expected: `anno-rag --help` exits `0`; gateway smoke says it stayed alive for `3s`.

- [ ] **Step 3: Package Windows archive**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\package-windows.ps1 -Tag v0.0.0-local -Target x86_64-pc-windows-msvc
```

Expected: creates:

```text
dist/hacienda-v0.0.0-local-x86_64-pc-windows-msvc.zip
```

- [ ] **Step 4: Inspect Windows archive contents**

Run:

```powershell
$Archive = "dist\hacienda-v0.0.0-local-x86_64-pc-windows-msvc.zip"
$Out = Join-Path $env:TEMP "anno-release-archive-check"
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
Expand-Archive -LiteralPath $Archive -DestinationPath $Out -Force
Get-ChildItem -Path $Out -Recurse -File | Select-Object -ExpandProperty FullName
```

Expected file names include:

```text
anno-rag.exe
anno-privacy-gateway.exe
README.md
LICENSE-MIT
LICENSE-APACHE
env.example
examples/claude_desktop_config.windows.json
examples/claude_desktop_config.macos.json
```

---

### Task 6: Run GitHub Release Matrix On A Release Candidate Tag

**Files:**
- Remote verification through GitHub Actions.

- [ ] **Step 1: Confirm local branch is pushed**

Run:

```bash
git status --short
git log -1 --oneline
git push -u origin HEAD
```

Expected: clean worktree, latest commit includes this plan and workflow fixes, branch pushed.

- [ ] **Step 2: Create and push an RC tag**

Use the next intended release version. Example:

```bash
git tag -a v0.3.0-rc.1 -m "v0.3.0-rc.1"
git push origin v0.3.0-rc.1
```

Expected: GitHub starts `Release Binaries` on the tag.

- [ ] **Step 3: Monitor workflow**

Run:

```bash
gh run list --workflow release-binaries.yml --limit 5
gh run watch <RUN_ID> --exit-status
```

Expected: Windows x64, macOS Intel, and macOS Apple Silicon build jobs pass; release job uploads three archives plus `SHA256SUMS.txt`.

- [ ] **Step 4: If the RC release is only a test, delete it after inspection**

Run only after assets have been inspected and recorded:

```bash
gh release delete v0.3.0-rc.1 --yes
git push origin :refs/tags/v0.3.0-rc.1
git tag -d v0.3.0-rc.1
```

Expected: RC release and remote/local RC tag are removed.

---

### Task 7: Claude Desktop Archive Install Smoke

**Files:**
- Read-only verification on each OS.

- [ ] **Step 1: Extract the OS archive to a stable path**

Windows example:

```powershell
New-Item -ItemType Directory -Force -Path C:\anno-release-test | Out-Null
Expand-Archive -LiteralPath .\hacienda-v0.3.0-x86_64-pc-windows-msvc.zip -DestinationPath C:\anno-release-test -Force
```

macOS example:

```bash
mkdir -p "$HOME/anno-release-test"
tar -xzf hacienda-v0.3.0-aarch64-apple-darwin.tar.gz -C "$HOME/anno-release-test"
```

Expected: extracted folder contains `anno-rag` and `anno-privacy-gateway`.

- [ ] **Step 2: Run CLI smoke from extracted archive**

Windows:

```powershell
C:\anno-release-test\anno-rag.exe --help
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\smoke-gateway.ps1 -BinaryPath C:\anno-release-test\anno-privacy-gateway.exe
```

macOS:

```bash
"$HOME/anno-release-test/anno-rag" --help
./scripts/release/smoke-gateway.sh "$HOME/anno-release-test/anno-privacy-gateway"
```

Expected: both commands pass.

- [ ] **Step 3: Configure Claude Desktop**

Windows config path:

```text
%APPDATA%\Claude\claude_desktop_config.json
```

Example server entry:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "C:\\anno-release-test\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_DATA_DIR": "C:\\anno-release-test\\data",
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

macOS config path:

```text
~/Library/Application Support/Claude/claude_desktop_config.json
```

Example server entry:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/Users/you/anno-release-test/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_DATA_DIR": "/Users/you/anno-release-test/data",
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

Expected: after restarting Claude Desktop, `anno-rag` appears as a connector. If models have not been warmed locally, unset `ANNO_NO_DOWNLOADS` for first run or run the warmup from a source checkout before returning to offline mode.

---

### Task 8: Publish The Final Release

**Files:**
- GitHub release metadata only.

- [ ] **Step 1: Confirm final preconditions**

Run:

```bash
just release-validate
git status --short
npx gitnexus status
```

Expected:

- `just release-validate` exits `0`
- worktree is clean
- GitNexus status is up-to-date

- [ ] **Step 2: Push final tag**

Use the final approved version. Example:

```bash
git tag -a v0.3.0 -m "v0.3.0"
git push origin v0.3.0
```

Expected: `Release Binaries` workflow starts on the final tag.

- [ ] **Step 3: Monitor and inspect final assets**

Run:

```bash
gh run list --workflow release-binaries.yml --limit 5
gh run watch <RUN_ID> --exit-status
gh release view v0.3.0 --json assets,tagName,url
```

Expected assets:

```text
hacienda-v0.3.0-x86_64-pc-windows-msvc.zip
hacienda-v0.3.0-x86_64-apple-darwin.tar.gz
hacienda-v0.3.0-aarch64-apple-darwin.tar.gz
SHA256SUMS.txt
```

- [ ] **Step 4: Add release notes top block**

Edit the GitHub release body to begin with:

```markdown
## Install for Claude Desktop

1. Download the archive for your OS.
2. Extract it somewhere stable.
3. Copy the matching Claude Desktop config example.
4. Replace the binary path.
5. Restart Claude Desktop.

## Checksums

Verify the downloaded archive with `SHA256SUMS.txt`.

## Known limitations

- Model weights are not bundled in this release.
- First run needs a warmed HuggingFace cache or network access unless `ANNO_NO_DOWNLOADS=1` is unset.
- Embedded OCR is build/runtime gated and still needs the OCR release profile validated before treating scanned-PDF ingestion as the default customer path.
- `.mcpb` Claude Desktop Extension packaging is deferred.
```

Expected: release page clearly tells users how to install, verify, and understand limits.

---

## Self-Review

Spec coverage:

- GitHub Release binaries: Tasks 2, 5, 6, 8.
- Local end-to-end pipeline metrics before `.exe`/`.dmg`: Tasks 3 and 4.
- Claude Desktop installation validation: Task 7.
- Gateway `--help` issue discovered during monitoring: Tasks 1 and 2 replace it with boot smoke.
- OCR distribution caveat: Task 8 release notes keep scanned-PDF default claims constrained until OCR release profile is validated.
- Model bundling remains out of this finalization plan because the approved 2026-05-18 release spec explicitly excluded model weights from phase 1 assets.

Placeholder scan:

- No placeholder markers remain.
- Every code-changing task includes exact file paths, snippets, commands, and expected output.

Execution handoff:

- Recommended execution mode: **Inline Execution** for Tasks 1-4 in this session, because the write set is small and tightly coupled.
- Recommended execution mode: **Subagent-Driven** for Tasks 5-8 only if running GitHub/macOS validation in parallel with local Windows archive inspection.
