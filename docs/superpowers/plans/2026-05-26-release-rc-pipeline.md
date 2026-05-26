# Release RC Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a GitHub prerelease RC that builds optimized OS-specific `anno-rag` artifacts and a Windows `.mcpb` package for Cowork testing, without relying on local debug binaries.

**Architecture:** First align only the dist-able package versions so `cargo-dist` can accept a `v0.11.0-rc.1` tag without forcing non-distributed crates into a prerelease dependency graph. Then add cheap release verification scripts that validate `.mcpb` packaging and release-mode artifact paths inside the existing `release.yml` matrix. Finally run `dist plan`, push the version commit, create the RC tag, monitor the release workflow, and validate Cowork uses the release artifact instead of `D:\cargo-shared-target\debug\anno-rag.exe`.

**Tech Stack:** Rust/Cargo metadata, cargo-dist 0.32.0 standalone `dist` CLI, GitHub Actions, Python 3, PowerShell, gh CLI, Claude Desktop MCP config.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | Inspect | Keep workspace version stable unless every workspace package and internal version constraint is released in lockstep. |
| `Cargo.lock` | Modify | Record the dist-able package version bump after Cargo metadata refreshes the lockfile. |
| `crates/anno-rag-bin/Cargo.toml` | Modify | Set the distributed `anno-rag` package to the RC version. |
| `crates/anno-privacy-gateway/Cargo.toml` | Modify | Set the distributed gateway package to the same RC version so one tag can release all dist-able packages. |
| `scripts/release/verify-mcpb.py` | Create | Validate a packaged `.mcpb` zip contains a coherent manifest and embedded binary. |
| `scripts/release/verify-release-binary.ps1` | Create | Validate the Windows release binary exists outside debug paths and starts far enough to print CLI help/version metadata. |
| `.github/workflows/release.yml` | Modify | Run `.mcpb` validation and Windows binary validation during release builds. |
| `docs/release/README-release.md` | Modify | Document the RC tag, release monitoring, and Cowork install flow. |

---

## Pre-flight Notes

- Use the standalone `dist` executable, not `cargo dist`. On this machine `dist --version` returned `cargo-dist 0.32.0`, while `cargo dist` was not installed as a cargo subcommand.
- Do not push `v0.11.0-rc.1` until `dist plan --tag=v0.11.0-rc.1` succeeds.
- If `dist plan` fails with `cargo metadata` access denied, stop build processes and rerun from a fresh shell before changing release logic. That is an environment failure, not a release design failure.
- The first RC is a GitHub prerelease. Do not create or rewrite stable tags.

---

## Task 1: Align Dist-Able Package Versions

**Files:**
- Modify: `Cargo.lock`
- Modify: `crates/anno-rag-bin/Cargo.toml`
- Modify: `crates/anno-privacy-gateway/Cargo.toml`

- [ ] **Step 1: Confirm the workspace version stays stable**

In `Cargo.toml`, leave the workspace version as:

```toml
version = "0.10.0"
```

Do not change the workspace version for this RC. `anno-cli` and `anno-eval` inherit the workspace version and contain internal path dependencies pinned to `0.10.0`; bumping the workspace alone breaks `cargo metadata` for non-distributed crates. This RC only releases the packages marked `dist = true`.

- [ ] **Step 2: Update `anno-rag-bin` version**

In `crates/anno-rag-bin/Cargo.toml`, change:

```toml
version           = "0.2.0"
```

to:

```toml
version           = "0.11.0-rc.1"
```

- [ ] **Step 3: Update `anno-privacy-gateway` version**

In `crates/anno-privacy-gateway/Cargo.toml`, change:

```toml
version = "0.3.0"
```

to:

```toml
version = "0.11.0-rc.1"
```

- [ ] **Step 4: Verify package metadata contains the RC versions**

Run:

```powershell
cargo metadata --no-deps --format-version 1 |
  ConvertFrom-Json |
  Select-Object -ExpandProperty packages |
  Where-Object { $_.name -in @('anno','anno-cli','anno-eval','anno-rag-bin','anno-privacy-gateway') } |
  Select-Object name,version
```

Expected output:

```text
name                 version
----                 -------
anno                 0.10.0
anno-cli             0.10.0
anno-eval            0.10.0
anno-rag-bin         0.11.0-rc.1
anno-privacy-gateway 0.11.0-rc.1
```

- [ ] **Step 5: Run a release planning dry-run**

Run:

```powershell
dist plan --tag=v0.11.0-rc.1 --output-format=json > target\release-plan-v0.11.0-rc.1.json
```

Expected: exit code `0`, and the JSON includes local artifact tasks for `x86_64-pc-windows-msvc`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, and `x86_64-unknown-linux-gnu`.

- [ ] **Step 6: Verify the planned targets**

Run:

```powershell
Select-String -Path target\release-plan-v0.11.0-rc.1.json -Pattern 'x86_64-pc-windows-msvc|aarch64-apple-darwin|x86_64-apple-darwin|x86_64-unknown-linux-gnu'
```

Expected: at least one match for each target triple.

- [ ] **Step 7: Commit**

Run:

```powershell
git add Cargo.lock crates/anno-rag-bin/Cargo.toml crates/anno-privacy-gateway/Cargo.toml
git commit -m "chore(release): prepare v0.11.0-rc.1"
```

Expected: one commit containing only the two dist-able package version changes and their matching `Cargo.lock` entries.

---

## Task 2: Add `.mcpb` Verification Script

**Files:**
- Create: `scripts/release/verify-mcpb.py`

- [ ] **Step 1: Create the verifier**

Create `scripts/release/verify-mcpb.py` with this exact content:

```python
#!/usr/bin/env python3
"""Validate an anno .mcpb package produced by the release workflow."""

from __future__ import annotations

import argparse
import json
import sys
import zipfile
from pathlib import PurePosixPath


def fail(message: str) -> None:
    print(f"verify-mcpb: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("package", help="Path to .mcpb zip package")
    parser.add_argument("--binary", required=True, help="Expected binary filename")
    parser.add_argument("--platform", required=True, help="Expected MCPB platform")
    args = parser.parse_args()

    with zipfile.ZipFile(args.package) as archive:
        names = set(archive.namelist())
        if "manifest.json" not in names:
            fail("manifest.json missing")

        manifest = json.loads(archive.read("manifest.json").decode("utf-8"))
        binary_path = PurePosixPath("server") / args.binary
        binary_entry = str(binary_path)

        if binary_entry not in names:
            fail(f"{binary_entry} missing")

        if manifest.get("server", {}).get("entry_point") != binary_entry:
            fail("server.entry_point does not point to embedded binary")

        expected_command = "${__dirname}/" + binary_entry
        if manifest.get("mcp_config", {}).get("command") != expected_command:
            fail("mcp_config.command does not point to embedded binary")

        if manifest.get("mcp_config", {}).get("args") != ["mcp"]:
            fail("mcp_config.args must be ['mcp']")

        platforms = manifest.get("compatibility", {}).get("platforms", [])
        if platforms != [args.platform]:
            fail(f"compatibility.platforms mismatch: {platforms!r}")

    print(f"verify-mcpb: OK {args.package}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Verify syntax**

Run:

```powershell
python scripts\release\verify-mcpb.py --help
```

Expected: argparse help output includes `--binary` and `--platform`.

- [ ] **Step 3: Verify failure mode on a missing file**

Run:

```powershell
python scripts\release\verify-mcpb.py missing.mcpb --binary anno-rag.exe --platform win32
```

Expected: non-zero exit with a Python `FileNotFoundError`. This confirms the script starts and validates inputs before CI uses it.

- [ ] **Step 4: Commit**

Run:

```powershell
git add scripts/release/verify-mcpb.py
git commit -m "ci(release): add mcpb package verifier"
```

Expected: one commit containing only `scripts/release/verify-mcpb.py`.

---

## Task 3: Add Windows Release Binary Verification

**Files:**
- Create: `scripts/release/verify-release-binary.ps1`

- [ ] **Step 1: Create the verifier**

Create `scripts/release/verify-release-binary.ps1` with this exact content:

```powershell
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$BinaryPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Resolved = Resolve-Path -LiteralPath $BinaryPath
$PathText = $Resolved.Path

if ($PathText -match "\\debug\\") {
    throw "Release verification rejected debug binary path: $PathText"
}

if ($PathText -notmatch "\\release\\") {
    throw "Release verification expected a release path, got: $PathText"
}

$Item = Get-Item -LiteralPath $PathText
if ($Item.Length -lt 1MB) {
    throw "Release binary is unexpectedly small: $($Item.Length) bytes"
}

$Output = & $PathText --help 2>&1
$ExitCode = $LASTEXITCODE

if ($ExitCode -ne 0) {
    throw "Binary --help failed with exit code $ExitCode. Output: $Output"
}

if (($Output -join "`n") -notmatch "mcp") {
    throw "Binary help output does not mention the mcp command"
}

Write-Output "verify-release-binary: OK $PathText"
```

- [ ] **Step 2: Verify syntax with a missing path**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\verify-release-binary.ps1 -BinaryPath C:\not-real\anno-rag.exe
```

Expected: non-zero exit with a `Resolve-Path` error.

- [ ] **Step 3: Commit**

Run:

```powershell
git add scripts/release/verify-release-binary.ps1
git commit -m "ci(release): add windows release binary verifier"
```

Expected: one commit containing only `scripts/release/verify-release-binary.ps1`.

---

## Task 4: Wire Verification Into `release.yml`

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Add `.mcpb` validation after the package step**

In `.github/workflows/release.yml`, find the existing step named `Package .mcpb extension`. Immediately after it and before `Upload .mcpb artifact`, add:

```yaml
      - name: Validate .mcpb extension
        shell: bash
        run: |
          TARGET="${{ join(matrix.targets, '-') }}"
          if [ "${{ runner.os }}" = "Windows" ]; then
            BIN="anno-rag.exe"
            PLATFORM="win32"
          else
            BIN="anno-rag"
            PLATFORM="darwin"
          fi
          python3 scripts/release/verify-mcpb.py "${MCPB_NAME}" --binary "${BIN}" --platform "${PLATFORM}"
```

- [ ] **Step 2: Add Windows release binary validation after artifact build**

In the same file, find the existing `Build artifacts` step. Immediately after it and before `Gateway boot smoke (Windows)`, add:

```yaml
      - name: Validate anno-rag release binary (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: .\scripts\release\verify-release-binary.ps1 -BinaryPath ".\target\${{ join(matrix.targets, '-') }}\release\anno-rag.exe"
```

- [ ] **Step 3: Validate workflow syntax by parsing YAML**

Run:

```powershell
python -c "import pathlib, yaml; yaml.safe_load(pathlib.Path('.github/workflows/release.yml').read_text()); print('release.yml YAML OK')"
```

Expected output:

```text
release.yml YAML OK
```

If Python cannot import `yaml`, run this fallback instead:

```powershell
rg -n "Validate \\.mcpb extension|Validate anno-rag release binary" .github\workflows\release.yml
```

Expected: both step names are present.

- [ ] **Step 4: Run release plan again**

Run:

```powershell
dist plan --tag=v0.11.0-rc.1 --output-format=json > target\release-plan-after-verifiers.json
```

Expected: exit code `0`.

- [ ] **Step 5: Commit**

Run:

```powershell
git add .github/workflows/release.yml
git commit -m "ci(release): verify mcpb and windows release binary"
```

Expected: one commit containing only `.github/workflows/release.yml`.

---

## Task 5: Document RC Release and Cowork Install Flow

**Files:**
- Modify: `docs/release/README-release.md`

- [ ] **Step 1: Add the RC section**

Append this section to `docs/release/README-release.md`:

```markdown

## RC release flow for Cowork performance testing

Use this flow to create an optimized GitHub prerelease for Claude Desktop/Cowork testing.

### Preconditions

- `origin/main` equals local `main`.
- Latest GitHub Actions CI on `main` is successful.
- `dist plan --tag=v0.11.0-rc.1 --output-format=json` exits with code `0`.
- No local Claude Desktop MCP process is still expected to run from `D:\cargo-shared-target\debug\anno-rag.exe` after install.

### Create the RC

```powershell
git tag v0.11.0-rc.1
git push origin main
git push origin v0.11.0-rc.1
gh run list --repo jamon8888/anno --workflow Release --limit 5
```

Monitor the selected release run:

```powershell
$Run = gh run list --repo jamon8888/anno --workflow Release --limit 5 --json databaseId,displayTitle |
  ConvertFrom-Json |
  Where-Object { $_.displayTitle -match 'v0\.11\.0-rc\.1' } |
  Select-Object -First 1
gh run view $Run.databaseId --repo jamon8888/anno --json status,conclusion,url,jobs
```

### Install in Cowork

Prefer the Windows `.mcpb` asset from the GitHub prerelease. If `.mcpb` installation is unavailable, extract the Windows release archive and point Claude Desktop at the extracted `anno-rag.exe`.

After installing, restart Claude Desktop/Cowork and verify:

```powershell
Get-Process anno-rag -ErrorAction SilentlyContinue |
  Select-Object Id,Path,StartTime,CPU,WorkingSet64
```

The `Path` must not contain `\debug\`.

### Capture performance evidence

Record these values before promoting the RC:

- MCP process path.
- `anno_health` response.
- First tool-call latency after restart.
- Representative ingest latency.
- Representative search latency.
- Peak working set for `anno-rag.exe`.
- Relevant lines from `C:\Users\NMarchitecte\AppData\Roaming\Claude\logs\mcp-server-anno-rag.log`.
```

- [ ] **Step 2: Verify markdown contains the new section**

Run:

```powershell
rg -n "RC release flow for Cowork performance testing|git tag v0.11.0-rc.1|Capture performance evidence" docs\release\README-release.md
```

Expected: three matches.

- [ ] **Step 3: Commit**

Run:

```powershell
git add docs/release/README-release.md
git commit -m "docs(release): document rc cowork test flow"
```

Expected: one commit containing only `docs/release/README-release.md`.

---

## Task 6: Final Pre-Tag Verification

**Files:**
- No file edits.

- [ ] **Step 1: Verify working tree is clean**

Run:

```powershell
git status --short --branch
```

Expected: no modified or untracked files.

- [ ] **Step 2: Verify `origin/main` is current**

Run:

```powershell
git fetch origin --prune
git rev-parse HEAD origin/main
```

Expected: both SHAs match after pushing the plan and implementation commits.

- [ ] **Step 3: Verify CI on `main`**

Run:

```powershell
gh run list --repo jamon8888/anno --branch main --workflow CI --limit 1 --json databaseId,status,conclusion,headSha,url
```

Expected: latest run for the implementation commit has `status` `completed` and `conclusion` `success`.

- [ ] **Step 4: Verify release planning one last time**

Run:

```powershell
dist plan --tag=v0.11.0-rc.1 --output-format=json > target\release-plan-final.json
```

Expected: exit code `0`.

- [ ] **Step 5: Create and push the RC tag**

Run:

```powershell
git tag v0.11.0-rc.1
git push origin v0.11.0-rc.1
```

Expected: GitHub starts the `Release` workflow.

---

## Task 7: Monitor Release Workflow and Validate Assets

**Files:**
- No file edits unless a release failure requires a follow-up fix.

- [ ] **Step 1: Find the release run**

Run:

```powershell
gh run list --repo jamon8888/anno --workflow Release --limit 5 --json databaseId,status,conclusion,headSha,url,displayTitle
```

Expected: a run for `v0.11.0-rc.1` is present.

- [ ] **Step 2: Monitor until complete**

Run:

```powershell
$Run = gh run list --repo jamon8888/anno --workflow Release --limit 5 --json databaseId,displayTitle |
  ConvertFrom-Json |
  Where-Object { $_.displayTitle -match 'v0\.11\.0-rc\.1' } |
  Select-Object -First 1
gh run view $Run.databaseId --repo jamon8888/anno --json status,conclusion,url,jobs
```

Expected: `status` eventually becomes `completed`.

- [ ] **Step 3: Inspect failed jobs if needed**

If any job fails, run:

```powershell
$Run = gh run list --repo jamon8888/anno --workflow Release --limit 5 --json databaseId,displayTitle |
  ConvertFrom-Json |
  Where-Object { $_.displayTitle -match 'v0\.11\.0-rc\.1' } |
  Select-Object -First 1
$Jobs = gh run view $Run.databaseId --repo jamon8888/anno --json jobs |
  ConvertFrom-Json |
  Select-Object -ExpandProperty jobs
$FailedJobs = $Jobs | Where-Object { $_.conclusion -and $_.conclusion -ne 'success' -and $_.conclusion -ne 'skipped' }
$FailedJobs | Select-Object name,databaseId,conclusion,url
gh api "repos/jamon8888/anno/actions/jobs/$($FailedJobs[0].databaseId)/logs"
```

Expected: logs identify the failing command. Fix only the failing release issue, then create a new RC tag such as `v0.11.0-rc.2`; do not rewrite `v0.11.0-rc.1`.

- [ ] **Step 4: Validate GitHub prerelease exists**

Run:

```powershell
gh release view v0.11.0-rc.1 --repo jamon8888/anno --json tagName,isPrerelease,assets,url
```

Expected: `isPrerelease` is `true`, and assets include a Windows `.mcpb` plus optimized OS artifacts.

---

## Task 8: Cowork Runtime Validation

**Files:**
- No repository file edits.

- [ ] **Step 1: Stop existing debug MCP processes**

Run:

```powershell
Get-Process anno-rag -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -like '*\debug\anno-rag.exe' } |
  Stop-Process -Force
```

Expected: no `anno-rag.exe` process remains from a `\debug\` path.

- [ ] **Step 2: Install the Windows `.mcpb` or configure the release binary**

Use the Windows `.mcpb` from the GitHub prerelease. If Claude Desktop cannot install the `.mcpb`, extract the Windows release archive and update `C:\Users\NMarchitecte\AppData\Roaming\Claude\claude_desktop_config.json` so the `anno-rag` MCP server command points to the extracted release `anno-rag.exe`.

- [ ] **Step 3: Restart Claude Desktop/Cowork**

Close and reopen Claude Desktop/Cowork so it starts the MCP server from the new release path.

- [ ] **Step 4: Verify process path**

Run:

```powershell
Get-Process anno-rag -ErrorAction SilentlyContinue |
  Select-Object Id,Path,StartTime,CPU,WorkingSet64
```

Expected: every `Path` points to the release install location and none contains `\debug\`.

- [ ] **Step 5: Capture log evidence**

Run:

```powershell
Get-Content C:\Users\NMarchitecte\AppData\Roaming\Claude\logs\mcp-server-anno-rag.log -Tail 80
```

Expected: log lines show the release binary command path, not `D:\cargo-shared-target\debug\anno-rag.exe`.

- [ ] **Step 6: Record performance evidence**

During the slow Cowork interaction, record:

```text
MCP binary path:
First anno tool call latency:
Representative ingest latency:
Representative search latency:
Peak WorkingSet64:
Notable log errors:
```

Expected: release-mode timings are materially better than the previous debug-binary run, or the logs reveal a separate bottleneck to investigate.
