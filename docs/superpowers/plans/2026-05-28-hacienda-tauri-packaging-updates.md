# Hacienda Tauri Packaging and Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Hacienda Workbench as signed Windows/macOS installers with offline resources, Tauri app updates, and separate signed resource-pack updates.

**Architecture:** Use Tauri v2 bundling for the app and updater. Add a resource-pack manifest/signature verifier inside `hacienda-workbench-core` so GLiNER2 base model, LoRA adapters, workflows, PII rules, and export templates can update separately from the app binary.

**Tech Stack:** Tauri v2 bundler/updater, GitHub Actions, code signing/notarization, Ed25519 signatures for resource packs, existing cargo-dist release knowledge.

---

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
- Create `crates/hacienda-workbench-core/src/resources/mod.rs`
- Create `crates/hacienda-workbench-core/src/resources/signature.rs`
- Create `docs/release/hacienda-workbench.md`
- Create `.github/workflows/hacienda-workbench-release.yml`
- Add resource manifests under `resources/hacienda-workbench/`

## Tasks

### Task 1: Resource Pack Manifest

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

### Task 2: Resource Registry

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
cargo test -p hacienda-workbench-core resources --lib
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
