# GitLab CI Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native GitLab CI pipeline for the imported `rubio.jamin/anno` project while preserving the existing GitHub release workflows until release publishing is migrated deliberately.

**Architecture:** Start with a GitLab-native fast gate that mirrors the portable parts of `.github/workflows/ci.yml`: docs audit, Rust check/fmt/clippy, minimal tests, Python helper type checks, and cargo-deny. Keep GitHub-only release and crates.io trusted publishing workflows out of phase 1 because they depend on GitHub Actions OIDC, GitHub Releases, and Actions artifact APIs.

**Tech Stack:** GitLab CI YAML, Docker executor images, Rust/Cargo, protoc, Python/uv, cargo-deny.

---

## File Structure

- Create: `.gitlab-ci.yml`
  - Owns the GitLab pipeline entrypoint.
  - Uses root-level jobs instead of GitHub `uses:` actions.
  - Uses GitLab `cache:` and `artifacts:` rather than Actions cache/artifact actions.
- Keep: `.github/workflows/ci.yml`
  - Remains the GitHub Actions source of truth until GitLab has parity.
- Keep: `.github/workflows/release.yml`, `.github/workflows/release-binaries.yml`, `.github/workflows/release-accelerated.yml`, `.github/workflows/publish.yml`
  - Remain GitHub-only in phase 1.
- Create: `docs/superpowers/plans/2026-06-03-gitlab-ci-migration.md`
  - Tracks the phased migration plan and commands.

## Task 1: Fast GitLab CI Gate

**Files:**
- Create: `.gitlab-ci.yml`
- Create: `docs/superpowers/plans/2026-06-03-gitlab-ci-migration.md`

- [x] **Step 1: Add GitLab workflow rules**

Create root pipeline rules that run on merge requests, branches, tags, web-triggered pipelines, and schedules:

```yaml
workflow:
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
    - if: '$CI_COMMIT_BRANCH'
    - if: '$CI_COMMIT_TAG'
    - if: '$CI_PIPELINE_SOURCE == "web"'
    - if: '$CI_PIPELINE_SOURCE == "schedule"'
```

- [x] **Step 2: Add portable fast jobs**

Create jobs that directly run the same underlying commands as the GitHub fast gate:

```bash
python scripts/docs_audit.py
uvx ty check scripts/docs_audit.py scripts/apply_registry_enrichment.py
cargo check --workspace --all-targets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features "eval discourse" -- -D warnings
cargo test --package anno --no-default-features --lib
cargo test --package anno --lib
cargo check --workspace --no-default-features --features anno-cli/extractor-html2text
cargo deny --all-features check
```

- [x] **Step 3: Validate YAML locally**

Run:

```powershell
npx --yes yaml-lint .gitlab-ci.yml
```

Expected: no YAML syntax errors.

- [x] **Step 4: Review the diff**

Run:

```powershell
git diff -- .gitlab-ci.yml docs/superpowers/plans/2026-06-03-gitlab-ci-migration.md
```

Expected: only the GitLab CI entrypoint and migration plan are changed.

## Task 2: Full CI Parity

**Files:**
- Modify: `.gitlab-ci.yml`

- [ ] **Step 1: Add full-CI rules**

Add a hidden rules block for expensive jobs:

```yaml
.full_ci_rules:
  rules:
    - if: '$FULL_CI == "1"'
    - if: '$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH'
    - if: '$CI_MERGE_REQUEST_LABELS =~ /(?:^|,)full-ci(?:,|$)/'
    - if: '$CI_PIPELINE_SOURCE == "schedule"'
    - when: never
```

- [ ] **Step 2: Port backend and eval jobs**

Add GitLab jobs for these commands under `.full_ci_rules`:

```bash
cargo build --workspace --features eval
cargo test --workspace --lib --features "eval discourse" -- --skip test_randomized_matrix_sample
cargo test --package anno --tests --features "discourse bundled-crf-weights bundled-hmm-params"
cargo test -p anno --doc --features discourse
cargo build --workspace --features onnx
cargo test --workspace --lib --features onnx
cargo build --workspace --features candle
cargo test --workspace --lib --features candle
./scripts/eval-sanity.sh
```

- [ ] **Step 3: Upload reports with GitLab artifacts**

Use:

```yaml
artifacts:
  when: always
  paths:
    - reports/eval-sanity-report.md
```

## Task 3: Release And Publish Migration

**Files:**
- Modify: `.gitlab-ci.yml`
- Keep or later remove: `.github/workflows/release.yml`
- Keep or later remove: `.github/workflows/publish.yml`

- [x] **Step 1: Replace GitHub Releases with GitLab Releases**

Translate release asset upload from `gh release create` / `softprops/action-gh-release` to GitLab release CLI or API using protected tag pipelines.

- [ ] **Step 2: Replace GitHub Actions Trusted Publishing**

For crates.io publishing, use a masked and protected `CARGO_REGISTRY_TOKEN` GitLab CI variable unless crates.io adds GitLab OIDC trusted publishing support for this project.

- [x] **Step 3: Add release tag rules**

Release and publish jobs run only for release-shaped tags:

```yaml
rules:
  - if: '$CI_COMMIT_TAG =~ /^v[0-9]+\.[0-9]+\.[0-9]+.*$/'
  - when: never
```

Implemented in phase 1.1 with `v*` release tag rules. Tighten to `CI_COMMIT_REF_PROTECTED == "true"` after GitLab protected tags are configured for this project.

## Task 4: Documentation And Cleanup

**Files:**
- Modify: `README.md`
- Modify: `docs/admins/release-management.md`

- [ ] **Step 1: Document GitLab fast CI**

Add a short note that GitLab runs the fast CI gate and release archive pipeline from `.gitlab-ci.yml`, while crates.io publishing still stays in GitHub Actions until the publishing authentication model is migrated.

- [ ] **Step 2: Update badges after GitLab pipeline is green**

Replace or add a GitLab pipeline badge once the first GitLab pipeline succeeds.

## Verification

- Run `npx --yes yaml-lint .gitlab-ci.yml`.
- Run `git diff --check`.
- Push a branch to GitLab and confirm the fast jobs start.
- Trigger a manual pipeline with `FULL_CI=1` only after Task 2 is implemented.
