# CI/CD Budget Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut GitHub Actions billed minutes by ~3× on a private repo by adding build caching to the release pipeline, dropping macOS from daily CI, consolidating to Windows + macOS universal2 release targets, and removing heavy static analysis jobs already covered by the weekly workflow.

**Architecture:** Four surgical edits to existing YAML files — no new files, no rewrites. `Cargo.toml` dist targets → `release.yml` regeneration + cache patch → `ci.yml` slimming. The release pipeline is the dominant budget driver (no sccache, 4 targets, weekly cadence); the CI changes prevent macOS ×10 cost on every push to main.

**Tech Stack:** GitHub Actions, cargo-dist 0.32.0, `mozilla-actions/sccache-action@v0.0.10`, `Swatinem/rust-cache@v2`, `EmbarkStudios/cargo-deny-action@v2`, PowerShell 7 / bash.

**Spec:** `docs/superpowers/specs/2026-06-01-ci-cd-budget-optimization-design.md`

---

## Files touched

| File | Change |
|------|--------|
| `Cargo.toml` | `[workspace.metadata.dist].targets` → `["x86_64-pc-windows-msvc", "universal2-apple-darwin"]` |
| `.github/workflows/release.yml` | Regenerate with `cargo dist generate`; re-apply 7 existing ANNO-PATCHes; add 1 new caching ANNO-PATCH |
| `.github/workflows/ci.yml` | Swap `cargo-deny` to prebuilt action; remove `macos-latest` from matrix + delete `test-metal`; delete 8 heavy static-analysis jobs already in weekly |

---

## Task 1 — Update dist targets in Cargo.toml

**Files:**
- Modify: `Cargo.toml` (lines 191-196, `[workspace.metadata.dist]`)

- [ ] **Step 1: Verify current targets**

```powershell
Select-String -Path Cargo.toml -Pattern "targets|installers" | Format-Table -Auto
```

Expected output shows `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`.

- [ ] **Step 2: Replace the targets array**

In `Cargo.toml`, find the `[workspace.metadata.dist]` section (around line 183) and change:

```toml
targets             = [
  "x86_64-pc-windows-msvc",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-unknown-linux-gnu",
]
```

to:

```toml
targets             = [
  "x86_64-pc-windows-msvc",
  "universal2-apple-darwin",
]
```

Leave `installers = ["msi", "shell", "powershell", "homebrew"]` unchanged — all four are valid for the two remaining platforms (msi/powershell → Windows, shell/homebrew → macOS).

- [ ] **Step 3: Dry-run with cargo dist plan**

```powershell
cargo dist plan --output-format=json 2>&1 | Select-String -Pattern "target|error" | Select-Object -First 20
```

Expected: JSON output listing only `x86_64-pc-windows-msvc` and `universal2-apple-darwin` targets. No errors.

- [ ] **Step 4: Commit Cargo.toml only**

```powershell
git add Cargo.toml
git commit -m "ci: target windows + universal2-apple-darwin for releases

Drop linux artifact (no external Linux consumers) and both separate
mac targets in favour of a single universal2 binary. Halves macOS
runner cost at release time.
"
```

---

## Task 2 — Replace cargo-deny with prebuilt action in ci.yml

**Files:**
- Modify: `.github/workflows/ci.yml` (the `cargo-deny` job, around line 943)

**Why:** The current job runs `cargo install --locked cargo-deny` on every execution (no cache), which takes 1–3 min itself. `EmbarkStudios/cargo-deny-action@v2` is a prebuilt action that runs in ~15 s. The job stays on the PR gate (already cheap now).

- [ ] **Step 1: Verify deny.toml exists at workspace root**

```powershell
Test-Path deny.toml
```

Expected: `True`. The EmbarkStudios action looks for `deny.toml` at workspace root and exits non-zero if missing.

- [ ] **Step 2: Replace the cargo-deny job**

Find the `cargo-deny:` job block in `.github/workflows/ci.yml` (currently ~12 lines with toolchain/sccache/rust-cache/install steps) and replace the entire block with:

```yaml
  cargo-deny:
    name: Cargo Deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

The action automatically uses `deny.toml` at repo root, runs all checks, and needs no Rust toolchain. All 9 other steps in the old job are deleted.

- [ ] **Step 3: Validate YAML syntax**

```powershell
# If actionlint is installed:
actionlint .github/workflows/ci.yml
# Otherwise a quick Python check:
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))" && Write-Host "YAML OK"
```

Expected: no errors.

- [ ] **Step 4: Commit**

```powershell
git add .github/workflows/ci.yml
git commit -m "ci: replace cargo-deny cargo install with EmbarkStudios/cargo-deny-action

Cuts the PR gate by ~2 min on every run: the job went from
toolchain + sccache + rust-cache + cargo install + deny check
to checkout + prebuilt action (~15 s total).
"
```

---

## Task 3 — Drop macOS from CI test matrix and remove test-metal

**Files:**
- Modify: `.github/workflows/ci.yml` (the `test-cross-platform` job matrix and the `test-metal` job)

**Why:** `macos-latest` costs ×10 billed minutes. Mac compilation is validated at release-build time. `test-metal` is macOS-only (×10) and covers a feature already exercised by the release runner.

- [ ] **Step 1: Remove macos-latest from the test-cross-platform matrix**

Find the `test-cross-platform` job in `ci.yml` (around line 227). Change:

```yaml
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
```

to:

```yaml
      matrix:
        os: [ubuntu-latest, windows-latest]
```

Leave the rest of the job intact — the macOS-specific `Free disk space` step already has `if: runner.os == 'Linux'` and `if: matrix.os == 'windows-latest'` guards, so they remain correct for the two remaining runners.

- [ ] **Step 2: Delete the test-metal job entirely**

Find the `test-metal:` job block in `ci.yml` (around line 441) and delete the entire block — from the `test-metal:` line through the closing `run: cargo test --workspace --lib --features "metal"` line (~25 lines). The job is a full macOS runner just for Metal backend tests.

- [ ] **Step 3: Validate YAML syntax**

```powershell
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))" && Write-Host "YAML OK"
```

Expected: no errors.

- [ ] **Step 4: Commit**

```powershell
git add .github/workflows/ci.yml
git commit -m "ci: remove macOS from daily CI (×10 minutes, validated at release)

Drop macos-latest from test-cross-platform matrix and delete test-metal.
Mac builds are already validated by the release pipeline on every tag.
Saves ~350 billed minutes per full-ci run on a private repo.
"
```

---

## Task 4 — Remove heavy static analysis from ci.yml (already in weekly workflow)

**Files:**
- Modify: `.github/workflows/ci.yml`

**Why:** `static-analysis-weekly.yml` already runs every Monday with geiger, opengrep, machete, llvm-cov, and cargo bin caching. Having the same tools in `ci.yml` (which `cargo install`s them fresh on every push to main) is pure duplicated cost.

**Jobs to remove entirely from ci.yml:**
1. `safety-report` — geiger, ~5 min (cargo install)
2. `unused-deps` — machete, ~3 min (cargo install)
3. `opengrep` — curl install + 6 scan steps, ~8 min
4. `nlp-ml-patterns` — apt + 5 scripts, ~4 min
5. `unified-static-analysis` — reinstalls all 4 tools above + downloads artifacts, ~15 min
6. `static-analysis-summary` — depends on above, ~2 min
7. `miri-unsafe` — nightly toolchain + Miri, ~10–15 min
8. `coverage` — cargo-llvm-cov install + full workspace coverage, ~20 min

- [ ] **Step 1: Delete the 8 job blocks from ci.yml**

Delete these complete job blocks from `.github/workflows/ci.yml`. Each block starts with the job key (`safety-report:`, `unused-deps:`, etc.) and ends before the next job key. Be careful to also remove blank lines between jobs so the YAML stays clean.

Jobs in order as they appear in the file:
- `safety-report:` (~30 lines, around line 984)
- `miri-unsafe:` (~50 lines, around line 1016)
- `opengrep:` (~95 lines, around line 1050)
- `nlp-ml-patterns:` (~40 lines, around line 1151)
- `unified-static-analysis:` (~35 lines, around line 1188)
- `static-analysis-summary:` (~50 lines, around line 1222)
- `coverage:` (~30 lines, around line 1263)
- `unused-deps:` (~30 lines, around line 964)

- [ ] **Step 2: Validate YAML syntax**

```powershell
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))" && Write-Host "YAML OK"
```

Expected: no errors.

- [ ] **Step 3: Verify no jobs still reference deleted jobs as `needs`**

```powershell
Select-String -Path .github/workflows/ci.yml -Pattern "safety-report|unused-deps|opengrep|nlp-ml-patterns|miri-unsafe|coverage" | Format-Table -Auto
```

Expected: zero matches. If any remain, they are `needs:` references in jobs that depended on the deleted ones — update or remove those `needs:` entries too.

- [ ] **Step 4: Commit**

```powershell
git add .github/workflows/ci.yml
git commit -m "ci: remove 8 heavy static analysis jobs from push gate

safety-report, unused-deps, opengrep, nlp-ml-patterns,
unified-static-analysis, static-analysis-summary, miri-unsafe,
coverage — all already run every Monday in static-analysis-weekly.yml
with binary caching. Removes ~60+ minutes from every push-to-main
CI run on a private repo.
"
```

---

## Task 5 — Regenerate release.yml and re-apply all ANNO-PATCHes + add caching

**Files:**
- Modify: `.github/workflows/release.yml`

**Why:** `cargo dist generate` rewrites `release.yml` from scratch based on `Cargo.toml`. It removes all manually-added steps. Every project-specific step lives in clearly-marked `ANNO-PATCH` blocks that must be re-inserted after each regeneration. This task adds **one new** ANNO-PATCH block for build caching — the biggest single budget fix.

- [ ] **Step 1: Regenerate release.yml**

```powershell
cargo dist generate
```

Expected: `release.yml` rewritten. The `build-local-artifacts` matrix will now compute two targets: `x86_64-pc-windows-msvc` and `universal2-apple-darwin`.

- [ ] **Step 2: Verify the new target matrix is correct**

```powershell
Select-String -Path .github/workflows/release.yml -Pattern "targets|universal2|windows-msvc" | Select-Object -First 10
```

Expected: references to `universal2-apple-darwin` and `x86_64-pc-windows-msvc` in the generated matrix expression.

- [ ] **Step 3: Add CARGO_INCREMENTAL to build-local-artifacts job env**

Find the `build-local-artifacts:` job's `env:` block (it contains `GH_TOKEN` and `BUILD_MANIFEST_NAME`). Add one line:

```yaml
    env:
      GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      BUILD_MANIFEST_NAME: target/distrib/${{ join(matrix.targets, '-') }}-dist-manifest.json
      # ANNO-PATCH: sccache and incremental compilation are mutually exclusive.
      CARGO_INCREMENTAL: "0"
```

- [ ] **Step 4: Re-apply ANNO-PATCH — protoc (Linux/macOS/Windows)**

Find the `Install dependencies` step in `build-local-artifacts` (runs `${{ matrix.packages_install }}`). Insert these three steps **immediately after** it:

```yaml
      # ANNO-PATCH: protoc is required by lance-encoding/prost during anno-rag builds.
      # Use OS package managers instead of arduino/setup-protoc to avoid GitHub
      # API rate limits in tag release jobs.
      - name: Install protoc (Linux)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y protobuf-compiler
      - name: Install protoc (macOS)
        if: runner.os == 'macOS'
        run: brew list protobuf >/dev/null 2>&1 || brew install protobuf
      - name: Install protoc (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: choco install protoc -y --no-progress
```

- [ ] **Step 5: Re-apply ANNO-PATCH — Build caching (NEW)**

Insert these two steps **immediately after** the three protoc steps (before the CRT alignment step):

```yaml
      # ANNO-PATCH: Build caching — re-apply after every `dist generate`.
      # sccache + rust-cache warm the heavy dep tree (candle, lancedb, datafusion,
      # arrow, ort, kreuzberg) so weekly releases don't rebuild all deps from scratch
      # on every tag. First 1-2 runs after a toolchain version change will be cold;
      # subsequent runs use the GHA cache.
      - name: Set up sccache
        uses: mozilla-actions/sccache-action@v0.0.10
        with:
          version: "v0.15.0"
      - uses: Swatinem/rust-cache@v2
        with:
          key: release-${{ join(matrix.targets, '-') }}
```

- [ ] **Step 6: Re-apply ANNO-PATCH — MSVC CRT alignment (Windows)**

Insert this step **after** the caching steps, before `Build artifacts`:

```yaml
      # ANNO-PATCH: MSVC CRT alignment — re-apply after every `dist generate`.
      # cargo-dist statically links the MSVC CRT for Windows release artifacts.
      # Keep cc-rs/CMake dependencies on /MT too, otherwise aws-lc-sys and
      # libsqlite3-sys compile for /MD and the final link fails with __imp_*.
      # See docs/superpowers/plans/2026-05-04-gliner2-fastino-finalization.md F1.2.
      - name: Align MSVC CRT (Windows)
        if: runner.os == 'Windows'
        shell: bash
        run: |
          echo "RUSTFLAGS=-C target-feature=+crt-static" >> "$GITHUB_ENV"
          echo "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS=-C target-feature=+crt-static" >> "$GITHUB_ENV"
          echo "CFLAGS_x86_64-pc-windows-msvc=-MT" >> "$GITHUB_ENV"
          echo "CFLAGS_x86_64_pc_windows_msvc=-MT" >> "$GITHUB_ENV"
          echo "CXXFLAGS_x86_64-pc-windows-msvc=-MT" >> "$GITHUB_ENV"
          echo "CXXFLAGS_x86_64_pc_windows_msvc=-MT" >> "$GITHUB_ENV"
```

- [ ] **Step 7: Re-apply ANNO-PATCH — release binary validation (Windows)**

Insert **after** the `Build artifacts` step (the `dist build` step):

```yaml
      - name: Validate anno-rag release binary (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: .\scripts\release\verify-release-binary.ps1 -BinaryPath ".\target\${{ join(matrix.targets, '-') }}\dist\anno-rag.exe"
```

- [ ] **Step 8: Re-apply ANNO-PATCH — Gateway boot smoke**

Insert after the release binary validation step:

```yaml
      # ANNO-PATCH: Gateway boot smoke — re-apply after every `dist generate`.
      # anno-privacy-gateway is a server binary; --help starts the server (does not print and exit).
      # These scripts start on ANNO_GATEWAY_LISTEN=127.0.0.1:0, wait 3s, verify alive, terminate.
      - name: Gateway boot smoke (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: .\scripts\release\smoke-gateway.ps1 -BinaryPath ".\target\${{ join(matrix.targets, '-') }}\dist\anno-privacy-gateway.exe"
      - name: Gateway boot smoke (Unix)
        if: runner.os != 'Windows'
        shell: bash
        run: ./scripts/release/smoke-gateway.sh "./target/${{ join(matrix.targets, '-') }}/dist/anno-privacy-gateway"
```

- [ ] **Step 9: Re-apply ANNO-PATCH — .mcpb packaging**

Insert after the gateway smoke steps:

```yaml
      # ANNO-PATCH: .mcpb extension packaging — re-apply after every `dist generate`.
      - name: Package .mcpb extension
        shell: bash
        run: |
          TARGET="${{ join(matrix.targets, '-') }}"
          RELEASE_TAG="${{ needs.plan.outputs.tag }}"
          MCPB_NAME="hacienda-${RELEASE_TAG}-${TARGET}.mcpb"
          VERSION="${RELEASE_TAG#v}"
          if [ "${{ runner.os }}" = "Windows" ]; then
            BIN="anno-rag.exe"
            PLATFORM="win32"
          elif [ "${{ runner.os }}" = "macOS" ]; then
            BIN="anno-rag"
            PLATFORM="darwin"
          else
            BIN="anno-rag"
            PLATFORM="linux"
          fi
          mkdir -p mcpb-staging/server
          cp "target/${TARGET}/dist/${BIN}" mcpb-staging/server/
          python3 -c "
          import json, sys
          m = json.load(open('scripts/release/mcpb-manifest-template.json'))
          m['version'] = sys.argv[1]
          m['compatibility']['platforms'] = [sys.argv[2]]
          m['server']['entry_point'] = 'server/' + sys.argv[3]
          m['mcp_config']['command'] = '\${__dirname}/server/' + sys.argv[3]
          json.dump(m, open('mcpb-staging/manifest.json', 'w'), indent=2)
          print('manifest OK')
          " "$VERSION" "$PLATFORM" "$BIN"
          cd mcpb-staging && zip -r "../${MCPB_NAME}" . && cd ..
          echo "MCPB_NAME=${MCPB_NAME}" >> "$GITHUB_ENV"
```

- [ ] **Step 10: Re-apply ANNO-PATCH — .mcpb validation + upload**

Insert after the packaging step:

```yaml
      - name: Validate .mcpb extension
        shell: bash
        run: |
          TARGET="${{ join(matrix.targets, '-') }}"
          if [ "${{ runner.os }}" = "Windows" ]; then
            BIN="anno-rag.exe"
            PLATFORM="win32"
          elif [ "${{ runner.os }}" = "macOS" ]; then
            BIN="anno-rag"
            PLATFORM="darwin"
          else
            BIN="anno-rag"
            PLATFORM="linux"
          fi
          python3 scripts/release/verify-mcpb.py "${MCPB_NAME}" --binary "${BIN}" --platform "${PLATFORM}"

      - name: Upload .mcpb artifact
        uses: actions/upload-artifact@v7
        with:
          name: artifacts-mcpb-${{ join(matrix.targets, '_') }}
          path: ${{ env.MCPB_NAME }}
```

- [ ] **Step 11: Validate YAML syntax**

```powershell
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))" && Write-Host "YAML OK"
```

Expected: no errors.

- [ ] **Step 12: Verify patch positions are correct**

```powershell
Select-String -Path .github/workflows/release.yml -Pattern "ANNO-PATCH" | Format-Table LineNumber,Line -Auto
```

Expected: 8 ANNO-PATCH comment lines in this order:
1. `ANNO-PATCH: protoc is required`
2. `ANNO-PATCH: Build caching`
3. `ANNO-PATCH: MSVC CRT alignment`
4. Validate anno-rag release binary (no ANNO-PATCH comment but between CRT and smoke)
5. `ANNO-PATCH: Gateway boot smoke`
6. `ANNO-PATCH: .mcpb extension packaging`

- [ ] **Step 13: Dry-run cargo dist plan to confirm the new matrix**

```powershell
cargo dist plan 2>&1 | Select-String -Pattern "target|artifact|platform" | Select-Object -First 20
```

Expected: plan output shows `x86_64-pc-windows-msvc` and `universal2-apple-darwin` targets, `.mcpb` and installer artifacts for each.

- [ ] **Step 14: Commit**

```powershell
git add .github/workflows/release.yml
git commit -m "ci(release): add build caching + drop to 2 targets (windows + universal2 mac)

The release pipeline had no sccache/rust-cache; every tag rebuilt
candle/lancedb/datafusion/arrow/ort from scratch (~35 min per target,
×2 Windows ×10 mac = ~800 billed min/tag on a private repo).

Changes:
- Add sccache-action + rust-cache as ANNO-PATCH before dist build
- CARGO_INCREMENTAL=0 added to job env (sccache mutual exclusion)
- Drop x86_64-unknown-linux-gnu + separate aarch64/x86_64 mac targets
- Add universal2-apple-darwin (one runner covers Intel + Silicon)
- Re-apply all 7 existing ANNO-PATCHes (protoc, CRT, smoke, mcpb)

Steady-state estimate: ~270 billed min/tag (was ~830). Weekly cadence
=> ~1100 min/month on releases (was ~3300, over the 2000 free tier).
"
```

---

## Task 6 — Validate end-to-end

- [ ] **Step 1: Check that the three touched workflows are individually valid**

```powershell
foreach ($f in @("ci.yml","release.yml","static-analysis-weekly.yml")) {
  python3 -c "import yaml; yaml.safe_load(open('.github/workflows/$f'))"
  Write-Host "$f OK"
}
```

Expected: three `OK` lines, no exceptions.

- [ ] **Step 2: Confirm no macOS runners remain in ci.yml**

```powershell
Select-String -Path .github/workflows/ci.yml -Pattern "macos" | Format-Table LineNumber,Line -Auto
```

Expected: zero matches. If any remain, they were missed in Task 3.

- [ ] **Step 3: Confirm release.yml targets match Cargo.toml**

```powershell
Write-Host "=== Cargo.toml targets ==="
Select-String -Path Cargo.toml -Pattern "windows-msvc|universal2" | Format-Table -Auto
Write-Host "=== release.yml references ==="
Select-String -Path .github/workflows/release.yml -Pattern "windows-msvc|universal2" | Select-Object -First 10 | Format-Table LineNumber,Line -Auto
```

Expected: both files reference `x86_64-pc-windows-msvc` and `universal2-apple-darwin`, nothing else.

- [ ] **Step 4: Push branch and verify PR gate runs (Linux only, no macOS)**

```powershell
git push -u origin HEAD
```

Open the Actions tab on GitHub. Verify:
- The PR gate jobs run (check, fmt, clippy, test-minimal, cargo-deny).
- No macOS runners appear in the run.
- `cargo-deny` completes in under 30 s (prebuilt action).
- None of the deleted heavy analysis jobs appear.

- [ ] **Step 5: Update ANNO-PATCH checklist comment in release.yml**

At the top of the `build-local-artifacts` job, update or add a comment listing all ANNO-PATCHes that must survive a `dist generate`. This prevents future regenerations from silently losing the caching block:

```yaml
      # ── ANNO-PATCH checklist (re-apply after every `cargo dist generate`) ────
      # 1. CARGO_INCREMENTAL=0 in job env
      # 2. Install protoc (Linux/macOS/Windows) — after "Install dependencies"
      # 3. Build caching (sccache-action + rust-cache) — after protoc, before CRT
      # 4. Align MSVC CRT (Windows) — before dist build
      # 5. Validate release binary (Windows) — after dist build
      # 6. Gateway boot smoke (Windows + Unix) — after binary validation
      # 7. Package .mcpb extension — after smoke
      # 8. Validate + upload .mcpb — after packaging
      # ─────────────────────────────────────────────────────────────────────────
```

Place this comment block immediately after the `steps:` line of `build-local-artifacts`, before the first actual step.

- [ ] **Step 6: Final commit**

```powershell
git add .github/workflows/release.yml
git commit -m "ci(release): add ANNO-PATCH checklist comment

Prevents future cargo dist generate from silently dropping the
caching block or other patches. Lists all 8 items in order.
"
```

---

## Budget summary (reference)

| Scenario | Billed min/tag | Billed min/month (weekly) |
|----------|---------------|--------------------------|
| Before (4 targets, no cache) | ~830 | ~3 300 |
| After (2 targets, cache warm) | ~270 | ~1 100 |
| First 1–2 tags (cache cold) | ~500 | — |

Free tier: 2 000 min/month. Before: **over budget**. After: **~45% of budget used**, with headroom for CI on feature branches.

---

## ANNO-PATCH survivor guide

After any future `cargo dist generate`, immediately run:

```powershell
Select-String -Path .github/workflows/release.yml -Pattern "ANNO-PATCH" | Measure-Object | Select-Object Count
```

Expected count: **8** (including the checklist header). If the count is lower, the missing patches were wiped by regeneration — re-apply from this plan.
