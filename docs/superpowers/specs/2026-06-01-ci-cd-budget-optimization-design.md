# CI/CD Budget Optimization — Design

**Date:** 2026-06-01
**Status:** Implemented for GitHub public-fork release routing
**Approach:** A — surgical optimization of the existing cargo-dist + `ci.yml` pipeline (no rewrite)
**Related:** [`2026-05-27-rapid-build-architecture-design.md`](2026-05-27-rapid-build-architecture-design.md), [`2026-05-26-release-plan-design.md`](2026-05-26-release-plan-design.md), [`2026-05-22-anno-installer-phase2-cargo-dist-design.md`](2026-05-22-anno-installer-phase2-cargo-dist-design.md)

## 1. Problem

The repo `jamon8888/anno` was originally treated as **private**, where GitHub
Actions minutes are metered against the included monthly tier and billed beyond
it, with OS multipliers. The repo is now **public**, so standard GitHub-hosted
CPU runners are the default path for CI and releases. The optimization goal
shifts from avoiding standard-runner minutes to avoiding duplicate tag-triggered
workflows, cache/storage churn, and accidental crates.io publication. Three
budgets still matter:

1. **Actions runtime/noise** — duplicate tag-triggered release workflows make
   failures harder to triage and rebuild the same artifacts more than once.
2. **Local CPU** — the dev machine is an i7-7500U (2 cores / 4 threads, 2016). Heavy local builds are the daily bottleneck.
3. **Release cost** — releases run **weekly or more**, and the release pipeline rebuilds the full heavy dependency tree (candle, lancedb/datafusion, arrow, ort, kreuzberg) **from scratch on every tag**, across 4 targets including 2 macOS targets.

### Root-cause finding

The codebase had two CPU release paths capable of reacting to the same `v*`
tag: the cargo-dist generated `.github/workflows/release.yml` and the
hand-rolled `.github/workflows/release-binaries.yml`. It also had
`.github/workflows/publish.yml` firing on `v*` tags, which made frequent RC tags
unsafe because release packaging and crates.io publication shared the same
trigger.

## 2. Locked decisions

| Decision | Value |
|----------|-------|
| Release platforms | **Windows** (`x86_64-pc-windows-msvc`) + **macOS Intel** (`x86_64-apple-darwin`) + **macOS Apple Silicon** (`aarch64-apple-darwin`) |
| Dropped release targets | `x86_64-unknown-linux-gnu` (no Linux artifact needed). `cargo-dist 0.32.0` does not accept `universal2-apple-darwin` (`Unrecognized architecture: universal2`), so separate native macOS targets are retained to preserve coverage. |
| CI test platform | **Linux ×1** for daily check/test/clippy; Windows touched only in `full-ci`; macOS only at release |
| Release cadence | Weekly+ → per-release cost is central |
| Local dev | Already fixed: `lld-link` linker, `sccache` engaged, `CARGO_TARGET_DIR=E:\cargo-target` (SSD) |

## 3. Local dev (Tier 0) — already shipped

Confirmed working this session (not part of the CI change, recorded for completeness):

- `~/.cargo/config.toml`: `linker = "lld-link"` for `x86_64-pc-windows-msvc` (LLD 22.1.2 matches rustc's LLVM 22.1.2), `target-dir = "E:/cargo-target"` (SSD; was `D:` HDD).
- `sccache` server reset; confirmed engaged on a warm-up `cargo check -p anno-rag` (compile requests climbing, `-C linker=lld-link` present in every rustc invocation).
- Iterate via `scripts/dev-fast.ps1` (concurrency guard + targeted `-p`). **Invoke with `pwsh` (PowerShell 7), not `powershell` (5.1)** — the script uses pwsh-7-only syntax and fails to parse under 5.1.

## 4. CI/CD design — 4 tiers

### Tier 1 — PR gate (Linux ×1, minimal minutes)
Runs on every PR. Unchanged jobs: `docs-audit`, `check`, `fmt`, `clippy`, `test-minimal`, `minimal-build`, `typecheck-python`. `cancel-in-progress` already supersedes stale runs.

Changes:
- **`cargo-deny`**: replace `cargo install --locked cargo-deny` (slow, recompiles every run) with the prebuilt **`EmbarkStudios/cargo-deny-action`**. Keep on the PR gate (now cheap).

### Tier 2 — Full validation (`push` to main / `full-ci` label / `workflow_dispatch`)
- **Linux (×1):** backends (`test-onnx`, `test-candle`), `test-eval`, `examples`, `docs`, `proptest`, `matrix-test`, `regression`, feature-combo jobs.
- **Windows (×2, gated):** replace the 3-OS `test-cross-platform` matrix with **one lean Windows job** — `cargo build` + `cargo test --lib` (no-default-features + `extractor-html2text`) with the existing MSVC CRT alignment step. Windows is the release target, so Windows-specific link regressions must be caught; macOS is not.
- **macOS: removed from CI.** Delete `test-cross-platform` macOS leg and `test-metal` (each ×10). Mac compilation is validated at release-build time (Tier 3). *(Optional, documented, not default: a `schedule`d weekly macOS smoke build.)*
- **Static analysis split by cost:**
  - Keep **fast** checks in CI: `cargo-deny` (Tier 1, prebuilt action).
  - Move **heavy** checks to the existing weekly schedule (`static-analysis-weekly.yml`): `safety-report` (geiger), `opengrep`, `miri-unsafe`, `coverage`, `nlp-ml-patterns`, `unused-deps` (machete). These currently run per main-push and each `cargo install` their tooling.

### Tier 3 — Release (on tag, optimized)
- **`[workspace.metadata.dist].targets`** → `["x86_64-pc-windows-msvc", "x86_64-apple-darwin", "aarch64-apple-darwin"]`. Regenerate with `dist generate`, then **re-apply all `ANNO-PATCH` sections** in `release.yml` (protoc install, build caching, MSVC CRT alignment, release binary validation, gateway smoke, `.mcpb` packaging/validation/upload).
- 🔥 **Add build caching** to `build-local-artifacts` as a new `ANNO-PATCH` block, inserted **before** the `Build artifacts` (`dist build`) step:
  - `mozilla-actions/sccache-action@v0.0.10` (pinned `v0.15.0`, same as CI),
  - env `SCCACHE_GHA_ENABLED: "true"`, `RUSTC_WRAPPER: sccache`, `CARGO_INCREMENTAL: "0"`,
  - `Swatinem/rust-cache@v2` keyed per target.
  - Effect: heavy deps are reused across weekly tags; only changed workspace crates + the final optimized codegen/link recompile.
- **Installers** aligned to the two OS families: `msi` + `powershell` (Windows), `shell` + `homebrew` (macOS). `.mcpb` packaging retained.
- `release.yml` no longer subscribes to `pull_request` events because the
  generated PR run was skipped in this repository and only added check noise.
- `release-binaries.yml` is retained as `Release Binaries (Manual Fallback)` and
  no longer runs on `v*` tags.
- `publish.yml` is manual-only with `confirm=publish`; crates.io publication is
  no longer coupled to GitHub Release tags.

## 5. Budget impact

On the public `jamon8888/anno` fork, standard GitHub-hosted CPU runners are not
the main billing constraint. The practical impact is operational:

- one CPU release workflow runs on each `v*` tag instead of two;
- RC tags no longer attempt crates.io publication;
- cargo-dist release caching still reduces wall time and avoids repeatedly
  rebuilding the heavy dependency tree;
- manual fallback release builds remain available without becoming the default.

If the repo becomes private again, the old multiplier math matters again:
Windows and macOS release jobs should stay cached, macOS should stay out of
daily CI, and fallback workflows should remain manual-only.

## 6. Risks & mitigations

- **Release sccache cache lineage is separate from CI.** Release uses `+crt-static` (Windows) and the dist profile; CI uses `-crt-static`. The release cache warms independently over 1–2 releases. Acceptable.
- **macOS release cost remains high.** `cargo-dist 0.32.0` does not support a `universal2-apple-darwin` target, so preserving Intel + Apple Silicon coverage requires two macOS builds. If the budget still needs more headroom, the next product decision is whether to drop one macOS architecture or build a custom universal artifact outside cargo-dist.
- **Re-applying `ANNO-PATCH` after `dist generate`.** This is an existing, documented hazard (the patches carry `ANNO-PATCH` markers). The new caching block must be added to the patch checklist so it survives future regenerations.
- **Dropping Linux artifact.** If a Linux consumer appears later, re-add `x86_64-unknown-linux-gnu` (×1, cheap) to dist targets.

## 7. Open decision (resolve in review)

Default chosen, flagged for confirmation:
- **Static analysis:** keep fast `cargo-deny` in CI, move heavy tools (geiger/opengrep/miri/coverage) to weekly. *(Alternative: keep more in CI at higher minute cost.)*
- **Weekly macOS smoke:** not included by default (mac validated at release). *(Alternative: add a `schedule`d ×10 mac compile smoke for earlier signal.)*

## 8. Out of scope

- Local-build speed beyond the already-shipped lld/sccache/SSD fixes (covered by the rapid-build spec).
- Migrating off GitHub-hosted runners (self-hosted/external) — rejected (Approach C): mac cross-compile off-Mac is impractical, and the dev laptop is too weak to host.
- Replacing cargo-dist (Approach B) — rejected: loses installer/`.mcpb` generation for a large rewrite.

## 9. Files touched

- `Cargo.toml` — `[workspace.metadata.dist].targets`, `installers`.
- `.github/workflows/release.yml` — regenerate + re-apply ANNO-PATCH + new caching ANNO-PATCH block.
- `.github/workflows/ci.yml` — drop macOS legs, collapse `test-cross-platform` to one Windows job, swap `cargo-deny` to the action, move heavy static analysis out.
- `.github/workflows/static-analysis-weekly.yml` — absorb the heavy static-analysis jobs.
- Docs — a short "CI/CD tiers & budget" section describing when each tier runs and how to trigger `full-ci`.
