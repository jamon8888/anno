# GitHub Release Binaries and MCP Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a repeatable GitHub Release pipeline that publishes Windows/macOS `anno-rag` MCP and `anno-privacy-gateway` binaries with Claude Desktop config examples and checksums.

**Architecture:** Keep crates.io publishing separate from binary release publishing. Add small release packaging scripts under `scripts/release/`, static Claude Desktop examples under `docs/release/examples/`, and one GitHub Actions workflow that builds OS/arch matrix artifacts and uploads them to GitHub Releases.

**Tech Stack:** GitHub Actions, Rust/Cargo, PowerShell, POSIX shell, `tar`, `Compress-Archive`, SHA-256 checksums.

---

## File Structure

- Create `docs/release/examples/claude_desktop_config.windows.json`
  - Windows Claude Desktop MCP config example using escaped backslashes.
- Create `docs/release/examples/claude_desktop_config.macos.json`
  - macOS Claude Desktop MCP config example using POSIX absolute paths.
- Create `docs/release/README-release.md`
  - User-facing install instructions for GitHub Release archives.
- Create `scripts/release/package-windows.ps1`
  - Packages Windows release files into `dist/hacienda-<tag>-x86_64-pc-windows-msvc.zip`.
- Create `scripts/release/package-unix.sh`
  - Packages macOS release files into `dist/hacienda-<tag>-<target>.tar.gz`.
- Create `scripts/release/checksums.sh`
  - Writes `dist/SHA256SUMS.txt` for all release archives.
- Create `.github/workflows/release-binaries.yml`
  - Builds the release binaries for Windows x64, macOS Intel, and macOS Apple Silicon; packages and uploads assets.
- Modify `README.md`
  - Add a short pointer to `docs/release/README-release.md` from the installation/release area.

---

### Task 1: Add Claude Desktop Release Examples

**Files:**
- Create: `docs/release/examples/claude_desktop_config.windows.json`
- Create: `docs/release/examples/claude_desktop_config.macos.json`

- [ ] **Step 1: Create the examples directory**

Run:

```powershell
New-Item -ItemType Directory -Force -Path docs\release\examples
```

Expected: directory exists at `docs/release/examples`.

- [ ] **Step 2: Add Windows config example**

Create `docs/release/examples/claude_desktop_config.windows.json` with exactly:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "C:\\\\path\\\\to\\\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_VAULT_PASSPHRASE": "change-me",
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

- [ ] **Step 3: Add macOS config example**

Create `docs/release/examples/claude_desktop_config.macos.json` with exactly:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_VAULT_PASSPHRASE": "change-me",
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

- [ ] **Step 4: Verify JSON parses**

Run:

```powershell
Get-Content docs\release\examples\claude_desktop_config.windows.json | ConvertFrom-Json | Out-Null
Get-Content docs\release\examples\claude_desktop_config.macos.json | ConvertFrom-Json | Out-Null
```

Expected: both commands exit successfully with no output.

- [ ] **Step 5: Commit**

Run:

```bash
git add docs/release/examples/claude_desktop_config.windows.json docs/release/examples/claude_desktop_config.macos.json
git commit -m "docs: add claude desktop release examples"
```

Expected: commit contains only the two JSON files.

---

### Task 2: Add Release README

**Files:**
- Create: `docs/release/README-release.md`
- Modify: `README.md`

- [ ] **Step 1: Create release README**

Create `docs/release/README-release.md` with exactly:

```markdown
# Hacienda Release Install

This document explains how to install the GitHub Release binary archives for Claude Desktop and local HTTP gateway use.

## Pick the Right Asset

| Platform | Asset |
|---|---|
| Windows 11 x64 | `hacienda-<tag>-x86_64-pc-windows-msvc.zip` |
| macOS Intel | `hacienda-<tag>-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `hacienda-<tag>-aarch64-apple-darwin.tar.gz` |

Each archive contains:

- `anno-rag` or `anno-rag.exe`
- `anno-privacy-gateway` or `anno-privacy-gateway.exe`
- `README.md`
- `LICENSE-MIT`
- `LICENSE-APACHE`
- `env.example`
- `examples/claude_desktop_config.windows.json`
- `examples/claude_desktop_config.macos.json`

## Claude Desktop

Claude Desktop uses the `anno-rag` binary through stdio MCP:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/absolute/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_VAULT_PASSPHRASE": "change-me",
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

Config file locations:

- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`

Rules:

- Use an absolute path to the extracted `anno-rag` binary.
- Escape Windows backslashes in JSON.
- Restart Claude Desktop after editing the config.
- Verify `anno-rag` appears under Claude Desktop Connectors.
- Keep real passphrases out of git.

## First Run and Offline Mode

The release archives do not contain model weights.

For best first-run behavior, run the warmup command from a source checkout or development build before setting `ANNO_NO_DOWNLOADS=1`:

```sh
cargo run --release --example warmup_model -p anno-rag
```

If models are already in the HuggingFace cache, `ANNO_NO_DOWNLOADS=1` keeps runtime operation offline.

## OCR

Tesseract is optional and is not bundled.

- Windows: `winget install --id UB-Mannheim.TesseractOCR`
- macOS: `brew install tesseract tesseract-lang`

If `tesseract` is not on `PATH`, set `tesseract_path` in the `anno-rag` config.

## Checksums

Download `SHA256SUMS.txt` from the release and verify the archive before extracting it.

Windows PowerShell:

```powershell
Get-FileHash .\hacienda-<tag>-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

macOS:

```sh
shasum -a 256 hacienda-<tag>-aarch64-apple-darwin.tar.gz
```
```

- [ ] **Step 2: Add README pointer**

In `README.md`, in the `### 2.0 Installation Windows / macOS` section, add this paragraph after the opening paragraph about Rust 1.95:

```markdown
Pour une installation depuis les binaires GitHub Releases, voir aussi [docs/release/README-release.md](docs/release/README-release.md). Cette section décrit l'installation depuis le repo source ; les releases fournissent les mêmes binaires déjà compilés.
```

- [ ] **Step 3: Verify markdown diff**

Run:

```powershell
git diff --check -- README.md docs\release\README-release.md
```

Expected: exit code 0.

- [ ] **Step 4: Commit**

Run:

```bash
git add README.md docs/release/README-release.md
git commit -m "docs: add release install guide"
```

Expected: commit contains release README plus one README pointer.

---

### Task 3: Add Windows Packaging Script

**Files:**
- Create: `scripts/release/package-windows.ps1`

- [ ] **Step 1: Create scripts directory**

Run:

```powershell
New-Item -ItemType Directory -Force -Path scripts\release
```

Expected: directory exists at `scripts/release`.

- [ ] **Step 2: Add packaging script**

Create `scripts/release/package-windows.ps1` with exactly:

```powershell
param(
    [Parameter(Mandatory = $true)]
    [string] $Tag,

    [Parameter(Mandatory = $false)]
    [string] $Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$dist = Join-Path $root "dist"
$assetName = "hacienda-$Tag-$Target"
$staging = Join-Path $dist $assetName
$releaseDir = Join-Path $root "target\$Target\release"

New-Item -ItemType Directory -Force -Path $dist | Out-Null
if (Test-Path $staging) {
    Remove-Item -Recurse -Force -LiteralPath $staging
}
New-Item -ItemType Directory -Force -Path $staging | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging "examples") | Out-Null

$annoRag = Join-Path $releaseDir "anno-rag.exe"
$gateway = Join-Path $releaseDir "anno-privacy-gateway.exe"

if (!(Test-Path $annoRag)) {
    throw "Missing binary: $annoRag"
}
if (!(Test-Path $gateway)) {
    throw "Missing binary: $gateway"
}

Copy-Item -LiteralPath $annoRag -Destination $staging
Copy-Item -LiteralPath $gateway -Destination $staging
Copy-Item -LiteralPath (Join-Path $root "README.md") -Destination $staging
Copy-Item -LiteralPath (Join-Path $root "LICENSE-MIT") -Destination $staging
Copy-Item -LiteralPath (Join-Path $root "LICENSE-APACHE") -Destination $staging
Copy-Item -LiteralPath (Join-Path $root "env.example") -Destination $staging
Copy-Item -LiteralPath (Join-Path $root "docs\release\examples\claude_desktop_config.windows.json") -Destination (Join-Path $staging "examples")
Copy-Item -LiteralPath (Join-Path $root "docs\release\examples\claude_desktop_config.macos.json") -Destination (Join-Path $staging "examples")

$zip = Join-Path $dist "$assetName.zip"
if (Test-Path $zip) {
    Remove-Item -Force -LiteralPath $zip
}

Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $zip
Write-Output $zip
```

- [ ] **Step 3: Verify script syntax**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\package-windows.ps1 -Tag test -Target x86_64-pc-windows-msvc
```

Expected before binaries exist locally: FAIL with `Missing binary: ...\target\x86_64-pc-windows-msvc\release\anno-rag.exe`. This verifies parsing and path logic without requiring a build.

- [ ] **Step 4: Commit**

Run:

```bash
git add scripts/release/package-windows.ps1
git commit -m "ci: add windows release packaging script"
```

Expected: commit contains only `scripts/release/package-windows.ps1`.

---

### Task 4: Add Unix Packaging and Checksum Scripts

**Files:**
- Create: `scripts/release/package-unix.sh`
- Create: `scripts/release/checksums.sh`

- [ ] **Step 1: Add Unix packaging script**

Create `scripts/release/package-unix.sh` with exactly:

```bash
#!/usr/bin/env bash
set -euo pipefail

tag="${1:?usage: package-unix.sh TAG TARGET}"
target="${2:?usage: package-unix.sh TAG TARGET}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dist="$root/dist"
asset_name="hacienda-${tag}-${target}"
staging="$dist/$asset_name"
release_dir="$root/target/$target/release"

mkdir -p "$dist"
rm -rf "$staging"
mkdir -p "$staging/examples"

anno_rag="$release_dir/anno-rag"
gateway="$release_dir/anno-privacy-gateway"

if [[ ! -x "$anno_rag" ]]; then
  echo "missing executable: $anno_rag" >&2
  exit 1
fi

if [[ ! -x "$gateway" ]]; then
  echo "missing executable: $gateway" >&2
  exit 1
fi

cp "$anno_rag" "$staging/"
cp "$gateway" "$staging/"
cp "$root/README.md" "$staging/"
cp "$root/LICENSE-MIT" "$staging/"
cp "$root/LICENSE-APACHE" "$staging/"
cp "$root/env.example" "$staging/"
cp "$root/docs/release/examples/claude_desktop_config.windows.json" "$staging/examples/"
cp "$root/docs/release/examples/claude_desktop_config.macos.json" "$staging/examples/"

tarball="$dist/$asset_name.tar.gz"
rm -f "$tarball"
tar -C "$dist" -czf "$tarball" "$asset_name"
echo "$tarball"
```

- [ ] **Step 2: Add checksum script**

Create `scripts/release/checksums.sh` with exactly:

```bash
#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dist="$root/dist"
output="$dist/SHA256SUMS.txt"

if [[ ! -d "$dist" ]]; then
  echo "dist directory does not exist: $dist" >&2
  exit 1
fi

archives=()
while IFS= read -r -d '' file; do
  archives+=("$file")
done < <(find "$dist" -maxdepth 1 -type f \( -name "*.zip" -o -name "*.tar.gz" \) -print0 | sort -z)

if [[ "${#archives[@]}" -eq 0 ]]; then
  echo "no release archives found in $dist" >&2
  exit 1
fi

: > "$output"
for file in "${archives[@]}"; do
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | sed "s#  $dist/#  #"
  else
    shasum -a 256 "$file" | sed "s#  $dist/#  #"
  fi >> "$output"
done

cat "$output"
```

- [ ] **Step 3: Mark scripts executable**

Run:

```bash
chmod +x scripts/release/package-unix.sh scripts/release/checksums.sh
```

Expected: executable bits set in git diff.

- [ ] **Step 4: Verify scripts fail safely without binaries**

Run:

```bash
scripts/release/package-unix.sh test x86_64-apple-darwin
```

Expected before binaries exist locally: FAIL with `missing executable: .../target/x86_64-apple-darwin/release/anno-rag`.

Run:

```bash
scripts/release/checksums.sh
```

Expected before archives exist locally: FAIL with `no release archives found in .../dist` or `dist directory does not exist`.

- [ ] **Step 5: Commit**

Run:

```bash
git add scripts/release/package-unix.sh scripts/release/checksums.sh
git commit -m "ci: add unix release packaging scripts"
```

Expected: commit contains only the two shell scripts.

---

### Task 5: Add Release Binary Workflow

**Files:**
- Create: `.github/workflows/release-binaries.yml`

- [ ] **Step 1: Add workflow**

Create `.github/workflows/release-binaries.yml` with exactly:

```yaml
name: release-binaries

on:
  push:
    tags:
      - "v*"
  workflow_dispatch:

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            archive: zip
          - os: macos-13
            target: x86_64-apple-darwin
            archive: tar.gz
          - os: macos-14
            target: aarch64-apple-darwin
            archive: tar.gz

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: arduino/setup-protoc@v3
        with:
          repo-token: ${{ secrets.GITHUB_TOKEN }}

      - uses: Swatinem/rust-cache@v2
        with:
          cache-all-crates: true
          key: release-${{ matrix.target }}

      - name: Align MSVC CRT (Windows)
        if: matrix.os == 'windows-latest'
        shell: bash
        run: |
          echo "RUSTFLAGS=-C target-feature=-crt-static" >> "$GITHUB_ENV"
          echo "CFLAGS_x86_64-pc-windows-msvc=-MD" >> "$GITHUB_ENV"
          echo "CXXFLAGS_x86_64-pc-windows-msvc=-MD" >> "$GITHUB_ENV"

      - name: Build release binaries
        run: cargo build --release -p anno-rag -p anno-privacy-gateway --target ${{ matrix.target }}

      - name: Smoke test anno-rag
        run: target/${{ matrix.target }}/release/anno-rag${{ matrix.os == 'windows-latest' && '.exe' || '' }} --help

      - name: Smoke test anno-privacy-gateway
        run: target/${{ matrix.target }}/release/anno-privacy-gateway${{ matrix.os == 'windows-latest' && '.exe' || '' }} --help

      - name: Package Windows archive
        if: matrix.os == 'windows-latest'
        shell: pwsh
        run: scripts/release/package-windows.ps1 -Tag "${{ github.ref_name }}" -Target "${{ matrix.target }}"

      - name: Package Unix archive
        if: matrix.os != 'windows-latest'
        shell: bash
        run: scripts/release/package-unix.sh "${{ github.ref_name }}" "${{ matrix.target }}"

      - name: Upload packaged archive
        uses: actions/upload-artifact@v4
        with:
          name: release-${{ matrix.target }}
          path: |
            dist/*.zip
            dist/*.tar.gz
          if-no-files-found: error

  release:
    name: Publish GitHub Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download packaged archives
        uses: actions/download-artifact@v4
        with:
          pattern: release-*
          path: dist
          merge-multiple: true

      - name: Generate checksums
        run: scripts/release/checksums.sh

      - name: Show release assets
        run: ls -la dist

      - name: Publish release assets
        uses: softprops/action-gh-release@v2
        with:
          files: |
            dist/*.zip
            dist/*.tar.gz
            dist/SHA256SUMS.txt
          generate_release_notes: true
```

- [ ] **Step 2: Validate workflow YAML parses**

Run:

```powershell
python - <<'PY'
import sys
from pathlib import Path
try:
    import yaml
except ImportError:
    print("PyYAML not installed; skipping local YAML parse")
    sys.exit(0)
yaml.safe_load(Path(".github/workflows/release-binaries.yml").read_text())
print("workflow yaml parsed")
PY
```

Expected: either `workflow yaml parsed` or `PyYAML not installed; skipping local YAML parse`.

- [ ] **Step 3: Inspect GitHub expression risk**

Open `.github/workflows/release-binaries.yml` and verify the two smoke-test `run:` lines contain the executable suffix expression:

```yaml
${{ matrix.os == 'windows-latest' && '.exe' || '' }}
```

If GitHub Actions rejects expression interpolation inside `run`, split smoke tests into OS-specific steps:

```yaml
- name: Smoke test anno-rag (Windows)
  if: matrix.os == 'windows-latest'
  run: target/${{ matrix.target }}/release/anno-rag.exe --help

- name: Smoke test anno-rag (Unix)
  if: matrix.os != 'windows-latest'
  run: target/${{ matrix.target }}/release/anno-rag --help
```

Apply the same split to `anno-privacy-gateway`. Prefer the split if there is any uncertainty.

- [ ] **Step 4: Commit**

Run:

```bash
git add .github/workflows/release-binaries.yml
git commit -m "ci: publish github release binaries"
```

Expected: commit contains only `.github/workflows/release-binaries.yml`.

---

### Task 6: Add Archive Content Validation

**Files:**
- Modify: `.github/workflows/release-binaries.yml`

- [ ] **Step 1: Add Windows archive inspection step**

In `.github/workflows/release-binaries.yml`, after `Package Windows archive`, add:

```yaml
      - name: Inspect Windows archive
        if: matrix.os == 'windows-latest'
        shell: pwsh
        run: |
          $archive = Get-ChildItem dist -Filter "*.zip" | Select-Object -First 1
          if (-not $archive) { throw "missing zip archive" }
          $tmp = Join-Path $env:RUNNER_TEMP "release-check"
          if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
          Expand-Archive -Path $archive.FullName -DestinationPath $tmp
          foreach ($name in @("anno-rag.exe", "anno-privacy-gateway.exe", "README.md", "LICENSE-MIT", "LICENSE-APACHE", "env.example")) {
            if (-not (Get-ChildItem -Path $tmp -Recurse -File -Filter $name)) { throw "archive missing $name" }
          }
          if (-not (Get-ChildItem -Path $tmp -Recurse -File -Filter "claude_desktop_config.windows.json")) { throw "archive missing windows Claude config" }
          if (-not (Get-ChildItem -Path $tmp -Recurse -File -Filter "claude_desktop_config.macos.json")) { throw "archive missing macOS Claude config" }
```

- [ ] **Step 2: Add Unix archive inspection step**

In `.github/workflows/release-binaries.yml`, after `Package Unix archive`, add:

```yaml
      - name: Inspect Unix archive
        if: matrix.os != 'windows-latest'
        shell: bash
        run: |
          archive="$(find dist -maxdepth 1 -type f -name '*.tar.gz' | head -n 1)"
          test -n "$archive"
          tmp="$(mktemp -d)"
          tar -C "$tmp" -xzf "$archive"
          find "$tmp" -type f -name anno-rag | grep -q .
          find "$tmp" -type f -name anno-privacy-gateway | grep -q .
          find "$tmp" -type f -name README.md | grep -q .
          find "$tmp" -type f -name LICENSE-MIT | grep -q .
          find "$tmp" -type f -name LICENSE-APACHE | grep -q .
          find "$tmp" -type f -name env.example | grep -q .
          find "$tmp" -type f -name claude_desktop_config.windows.json | grep -q .
          find "$tmp" -type f -name claude_desktop_config.macos.json | grep -q .
```

- [ ] **Step 3: Validate diff**

Run:

```powershell
git diff --check -- .github\workflows\release-binaries.yml
```

Expected: exit code 0.

- [ ] **Step 4: Commit**

Run:

```bash
git add .github/workflows/release-binaries.yml
git commit -m "ci: validate release archive contents"
```

Expected: commit contains only workflow validation changes.

---

### Task 7: Add Local Release Metadata Validation

**Files:**
- Modify: `justfile`

- [ ] **Step 1: Add just recipe**

Append this recipe near the existing `pre-tag` recipe in `justfile`:

```make
# Validate release metadata files without building release binaries.
release-validate:
    #!/usr/bin/env bash
    set -euo pipefail
    python -m json.tool docs/release/examples/claude_desktop_config.windows.json >/dev/null
    python -m json.tool docs/release/examples/claude_desktop_config.macos.json >/dev/null
    test -f scripts/release/package-unix.sh
    test -f scripts/release/checksums.sh
    test -f scripts/release/package-windows.ps1
    test -f .github/workflows/release-binaries.yml
    cargo metadata --no-deps --format-version 1 >/dev/null
    git diff --check -- README.md docs/release .github/workflows/release-binaries.yml scripts/release
```

- [ ] **Step 2: Run validation**

Run:

```bash
just release-validate
```

Expected: exit code 0.

- [ ] **Step 3: Commit**

Run:

```bash
git add justfile
git commit -m "ci: add release metadata validation"
```

Expected: commit contains only `justfile`.

---

### Task 8: Final Verification

**Files:**
- Read-only verification across all release files.

- [ ] **Step 1: Run non-build checks**

Run:

```bash
just release-validate
git diff --check
```

Expected: both commands exit 0.

- [ ] **Step 2: Verify GitHub workflow assets are named correctly**

Run:

```bash
rg -n "hacienda-|release-binaries|package-windows|package-unix|SHA256SUMS|softprops/action-gh-release" .github/workflows/release-binaries.yml scripts/release docs/release
```

Expected: output references:

- `hacienda-` in both package scripts
- `SHA256SUMS` in checksum script and workflow
- `softprops/action-gh-release@v2` in workflow
- both Claude Desktop JSON examples

- [ ] **Step 3: Check working tree scope**

Run:

```bash
git status --short
```

Expected: only intended files are modified or untracked before final commit. Do not stage unrelated existing untracked directories such as `.venv-anno-demo/`, `adapter_A/`, `adapter_B/`, or `claude-for-legal/`.

- [ ] **Step 4: Create final integration commit if needed**

If any intended release files remain uncommitted, run:

```bash
git add docs/release .github/workflows/release-binaries.yml scripts/release justfile README.md
git commit -m "ci: add github release binary distribution"
```

Expected: commit contains only release distribution files.

---

## Self-Review

Spec coverage:

- Level 1 GitHub binaries: Tasks 3, 4, 5, 6.
- Claude Desktop examples: Tasks 1, 2.
- Checksums: Task 4 and Task 5.
- Separate crates.io workflow: Task 5 creates a new workflow and does not modify `publish.yml`.
- `.mcpb` deferral: Task 2 documents release scope; no implementation task creates `.mcpb`.
- Validation: Tasks 6, 7, 8.

Open decisions carried from the spec:

- `anno-cli` remains excluded from phase 1 assets.
- Workflow publishes immediately on tag push through `softprops/action-gh-release`; draft releases are not used in phase 1.
- Tags remain global `v*`.

