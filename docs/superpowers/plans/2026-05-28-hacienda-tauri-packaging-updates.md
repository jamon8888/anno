# Hacienda Tauri Packaging and Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Hacienda Workbench as signed Windows/macOS installers with Tauri app updates. Offline resources and signed resource-pack updates are optional follow-ons once profiles/workflows actually consume imported resources.

**Architecture:** Use Tauri v2 bundling for the app and updater first. Add a resource-pack manifest/signature verifier inside `hacienda-workbench-core` only after GLiNER2 model/profile/workflow imports exist.

**Tech Stack:** Tauri v2 bundler/updater, GitHub Actions, code signing/notarization, Ed25519 signatures for resource packs, existing cargo-dist release knowledge.

---

## Lean Validation

Packaging should lean on existing release infrastructure first. The root workspace already has cargo-dist metadata, and the Tauri app can use Tauri v2 bundling/updater without a custom release framework.

Apply these reductions before implementing:

- Configure Tauri bundling/updater first; only add a separate GitHub Actions workflow if the existing release flow cannot cover the app.
- Delay resource-pack signature code until resource packs are actually consumed by profiles/workflows.
- If resource packs are needed, keep verifier code in one `resources.rs` module at first; split `resources/signature.rs` only after tests make the split worthwhile.
- Do not bundle giant model files by default. Prefer local cache detection, explicit resource-pack import, and clear offline warnings.
- Keep signing keys outside the repo and document environment variables rather than adding secret-management code.

## Scope

In scope:

- Windows MSI/NSIS and macOS DMG.
- Tauri app updater config and signing.
- Offline resources included in installer.
- Separate signed resource-pack import/update.
- CI smoke checks.

Out of scope:

- Linux packaging.
- Enterprise MDM deployment profiles.
- Delta binary patching.

## Files

- Modify `apps/hacienda-workbench/src-tauri/tauri.conf.json`
- Modify existing release configuration where possible.
- Optionally create `crates/hacienda-workbench-core/src/resources.rs` only when resource packs are consumed by profiles/workflows.
- Optionally create `.github/workflows/hacienda-workbench-release.yml` only if the existing release flow cannot cover the Tauri app.
- Optionally add resource manifests under `resources/hacienda-workbench/` only after import/verification code exists.

## Tasks

### Task 1: Optional Resource Pack Manifest

Do this task only after profiles/workflows need importable resources.

- [ ] Define manifest:

```rust
pub struct ResourcePackManifest {
    pub id: String,
    pub version: String,
    pub kind: ResourcePackKind,
    pub files: Vec<ResourceFile>,
    pub sha256: String,
    pub signature: String,
}
```

- [ ] Verify file hash before import.
- [ ] Test unsigned and mismatched packs fail closed.

### Task 2: Optional Resource Registry

Do this task only after Task 1 is needed.

- [ ] Store installed packs in SQLite table `resource_packs`.
- [ ] Add engine methods `list_resource_packs` and `import_resource_pack`.
- [ ] Audit `resource_pack.imported`.
- [ ] Test duplicate version import is idempotent.

### Task 3: Tauri Updater

- [ ] Add updater plugin and config.
- [ ] Configure public key and update endpoint.
- [ ] Add commands `check_for_app_update` and `install_app_update`.
- [ ] UI shows update status in settings.

### Task 4: Installer Resources

- [ ] Add bundling step that copies `resources/hacienda-workbench/base` into app resource dir.
- [ ] Include initial workflows and PII rules.
- [ ] Add model pack as separate artifact if installer size exceeds release threshold.

### Task 5: CI Release Workflow

- [ ] Build Windows and macOS app.
- [ ] Sign Windows installer when secrets exist.
- [ ] Sign/notarize macOS DMG when Apple secrets exist.
- [ ] Run smoke command for app launch.
- [ ] Publish update JSON and resource-pack artifacts.

## Verification

Run:

```powershell
cargo test -p hacienda-workbench-core resources --lib  # only if resources.rs exists
Set-Location apps\hacienda-workbench; npm run tauri build; Set-Location ..\..
```

Manual release smoke:

```text
Install signed Windows build.
Launch offline with bundled resources.
Import signed LoRA/workflow resource pack.
Reject tampered resource pack.
Check app update channel without installing when no update exists.
```
