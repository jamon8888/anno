# Hacienda Tauri Vault Unlock and Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit vault lifecycle controls to the Tauri workbench: locked/unlocked state, passphrase/keyring initialization, rehydration audit, and tamper-evident audit chaining.

**Architecture:** Keep the PII vault physically separate from SQLite metadata. Add explicit locked/unlocked vault state owned by `WorkbenchEngine`, expose Tauri commands for status/unlock/lock/rehydrate, and upgrade audit rows to include previous hash and event hash.

**Tech Stack:** Rust 1.95, `anno-rag::vault`, `rusqlite`, `sha2`, Tauri commands, React state.

---

## Lean Validation

The vault lifecycle is valid, but do not reimplement the vault abstraction. Existing `anno_rag::vault::Vault` already wraps the cloakpipe vault behind an async mutex and existing `VaultKeySource` already handles passphrase/keyring/KMS-stub key derivation.

Apply these reductions before implementing:

- Start with a small locked/unlocked state wrapper owned by `WorkbenchEngine`. Create `vault_manager.rs` only if `engine.rs` becomes hard to read.
- Reuse `Vault::lookup`, `Vault::pseudonymize`, and `VaultKeySource::derive`; do not create a parallel crypto/key API.
- Keep audit persistence in the existing workbench SQLite store for now. The hash-chain logic can live in `store.rs`; do not introduce a generic audit crate.
- Mirror the existing `anno-privacy-gateway` JSONL hash-chain pattern conceptually, but avoid adding a dependency from the workbench core to the gateway crate.
- Never persist rehydrated plaintext in SQLite, logs, React state beyond the transient modal, or audit payloads.

## Scope

In scope:

- Vault status: `Uninitialized`, `Locked`, `Unlocked`.
- Explicit unlock with passphrase or OS keyring.
- Manual lock command.
- Rehydrate one token at a time with audit event.
- Hash-chained audit events.

Out of scope:

- Multi-user permissions.
- Hardware security module/KMS.
- Remote audit export.

## Files

- Modify `crates/hacienda-workbench-core/src/engine.rs`
- Modify `crates/hacienda-workbench-core/src/store.rs`
- Modify `crates/hacienda-workbench-core/src/model.rs`
- Modify `apps/hacienda-workbench/src-tauri/src/commands.rs`
- Modify `apps/hacienda-workbench/src/api.ts`
- Modify `apps/hacienda-workbench/src/App.tsx`
- Optionally create `crates/hacienda-workbench-core/src/vault_manager.rs` only if vault state makes `engine.rs` hard to read.

## Tasks

### Task 1: Vault State Model

- [ ] Add model types:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum VaultStatus {
    Uninitialized,
    Locked,
    Unlocked,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RehydrateRequest {
    pub token: String,
    pub reason: String,
}
```

- [ ] Test serialization uses stable enum strings.
- [ ] Run `cargo test -p hacienda-workbench-core model --lib`.

### Task 2: Engine-Owned Vault State

- [ ] Implement a small private engine-owned state, initially in `engine.rs`, equivalent to `{ path, vault: Mutex<Option<Vault>> }`.
- [ ] Methods:

```rust
pub fn status(&self) -> VaultStatus;
pub fn unlock_with_passphrase(&self, passphrase: &str) -> Result<()>;
pub fn lock(&self);
pub async fn rehydrate_token(&self, token: &str) -> Result<Option<String>>;
```

- [ ] Unit tests use temp vault and verify lock removes access.
- [ ] Run `cargo test -p hacienda-workbench-core vault --lib`.

### Task 3: Audit Hash Chain

- [ ] Change `audit_events` schema to include `previous_hash` and `event_hash`.
- [ ] Compute:

```text
event_hash = sha256(previous_hash || event_type || payload_json || created_at)
```

- [ ] Add `verify_audit_chain(matter_id)` returning first broken event if any.
- [ ] Test tampering by editing one row in SQLite and expecting verification failure.
- [ ] Run `cargo test -p hacienda-workbench-core store::audit --lib`.

### Task 4: Engine and Commands

- [ ] Add engine methods:

```rust
pub fn vault_status(&self) -> VaultStatus;
pub fn unlock_vault_with_passphrase(&self, passphrase: String) -> Result<()>;
pub fn lock_vault(&self);
pub async fn rehydrate_token(&self, token: String, reason: String) -> Result<Option<String>>;
```

- [ ] Add Tauri commands with the same names.
- [ ] Ensure rehydration logs `vault.rehydrate` with token and reason, never plaintext.
- [ ] Run `cargo check -p hacienda-workbench-tauri`.

### Task 5: UI Controls

- [ ] Add vault status pill to topbar.
- [ ] Add unlock dialog with passphrase field.
- [ ] Add lock button.
- [ ] Add PII row action to rehydrate a selected token, requiring a reason.
- [ ] Display plaintext only in a transient modal; do not write it into editor state.
- [ ] Run `npm run build`.

## Verification

Run:

```powershell
cargo test -p hacienda-workbench-core
cargo check -p hacienda-workbench-tauri
Set-Location apps\hacienda-workbench; npm run build; Set-Location ..\..
```

Manual smoke:

```text
Start app with locked vault.
Unlock with passphrase.
Open a PII token and rehydrate with reason.
Close modal and confirm plaintext is not inserted into document text.
Lock vault and confirm rehydration is blocked.
Verify audit chain succeeds.
Tamper audit row in test only and confirm verification fails.
```
