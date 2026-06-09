# Privacy Safety Hardening — Audit Remediation (P1 + P4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix two confirmed privacy risks: PII span loss on partial overlaps (P1) and silent data corruption on decryption failure with static salt (P4).

**Priority:** P1 — these are the highest-severity findings from the code audit, both affecting the privacy-by-design promise.

**Crates touched:** `anno-rag`, `cloakpipe-core` (vendored)

---

## P1 — Interval Fusion in `dedup_overlaps`

### Problem

`crates/anno-rag/src/detect.rs:757` — the `dedup_overlaps` function drops entities whose `start < last.end` via a simple `continue`. When two PII entities partially overlap (e.g., NER detects `"Jean Dupont"` [0..11] and regex detects `"Dupont SA"` [5..14]), the second entity is silently dropped. Characters [11..14] are never masked.

### Fix

Replace the drop with interval fusion. When a partial overlap is detected, extend `last.end` to `max(last.end, entity.end)`:

```rust
fn dedup_overlaps(entities: &mut Vec<DetectedEntity>) {
    let mut out: Vec<DetectedEntity> = Vec::with_capacity(entities.len());
    for entity in entities.drain(..) {
        if let Some(last) = out.last_mut() {
            if entity.start < last.end {
                // Fusion: extend coverage to the max of both spans
                last.end = last.end.max(entity.end);
                continue;
            }
        }
        out.push(entity);
    }
    *entities = out;
}
```

### Pre-condition

Entities must be sorted by `start` before calling `dedup_overlaps`. The existing code already does this (see `detect_inner` which sorts by start before dedup). Document this invariant with a debug assertion.

### Tests

Three unit tests in `detect.rs`:

1. **No overlap** — `[0..5], [6..10]` → unchanged. Regression guard.
2. **Total containment** — `[0..10], [2..8]` → `[0..10]`. Inner entity absorbed.
3. **Partial overlap** — `[0..8], [5..14]` → `[0..14]`. Span extended. This is the bug fix case.
4. **Adjacent (no overlap)** — `[0..5], [5..10]` → `[0..5], [5..10]`. Boundary entities preserved.

### Files

- Modify: `crates/anno-rag/src/detect.rs:757-768`
- Test: same file, new `#[cfg(test)]` tests

---

## P4a — Decrypt Failure Error Propagation

### Problem

`vendor/cloakpipe/crates/cloakpipe-core/src/vault_sqlite.rs:132-133`:

```rust
.unwrap_or_else(|_| "[decrypt_failed]".to_string())
```

When decryption fails (corrupted DB, wrong passphrase), the literal string `"[decrypt_failed]"` is inserted into the forward cache as if it were real data. Downstream code treats it as a valid original text value. This is silent data corruption.

### Fix

Replace the `unwrap_or_else` with a `match` that:

1. Logs `tracing::warn!("vault: skipping corrupted token {token_id}")` — the token_id only, never the encrypted value or plaintext.
2. Skips the entry entirely — does NOT insert it into either cache.
3. Continues processing remaining entries.

```rust
for row in rows {
    let (enc, token, category_str, token_id) = row?;
    let original = match self.decrypt_value(&enc) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(token_id, "vault: skipping entry with decrypt failure");
            continue;
        }
    };
    // ... rest of cache population
}
```

### Return value

After `load_mappings`, the caller should know if entries were skipped. Add a return value: `fn load_mappings(&mut self) -> Result<usize>` where `usize` is the count of skipped entries. If non-zero, the caller can warn the user.

### Note on vendored code

`cloakpipe-core` is in `vendor/`. This is a local patch. Document it in a `PATCHES.md` or a comment at the top of the modified function.

### Files

- Modify: `vendor/cloakpipe/crates/cloakpipe-core/src/vault_sqlite.rs:128-144`
- Test: add a unit test that creates a vault, corrupts an entry, reloads, and verifies the entry is skipped (not served as `"[decrypt_failed]"`)

---

## P4b — Vault-Path-Derived Salt

### Problem

`crates/anno-rag/src/vault.rs:457`:

```rust
let salt = b"anno-rag-vault-salt-v1";
```

Static salt means two vaults with the same passphrase produce identical encryption keys. Acceptable for single-user today, but a risk for multi-vault scenarios.

### Fix

Derive salt from the canonical vault database path:

```rust
pub(crate) fn derive_via_argon2(passphrase: &str, vault_path: &Path) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};
    use sha2::{Sha256, Digest};

    let params = Params::new(19_456, 2, 1, Some(32))
        .map_err(|e| Error::Vault(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    // Derive salt from vault path — unique per vault, deterministic
    let path_str = vault_path.to_string_lossy();
    let path_hash = Sha256::digest(path_str.as_bytes());
    let salt = &path_hash[..16];

    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Vault(format!("argon2 derive: {e}")))?;
    Ok(key)
}
```

### Migration strategy

Existing vaults are encrypted with the static salt. The migration must be transparent:

1. Try deriving key with new path-based salt first
2. If decryption fails, try the legacy static salt `b"anno-rag-vault-salt-v1"`
3. If legacy salt works, re-encrypt all entries with the new salt (background migration)
4. If both fail, the passphrase is wrong — return error

This logic lives in `VaultStore::open()` or equivalent init path.

### Callers to update

All callers of `derive_via_argon2` must now pass `vault_path`. Grep for call sites:

```
crates/anno-rag/src/vault.rs — VaultStore::open, VaultStore::new
```

### Files

- Modify: `crates/anno-rag/src/vault.rs:450-463` (function signature + body)
- Modify: all callers of `derive_via_argon2` in `vault.rs`
- Test: verify migration path — create vault with old salt, open with new code, confirm transparent re-encryption

---

## Non-goals

- **P3 (std::fs blocking in async):** Audit finding was factually incorrect — the cited `create_dir_all` calls are in `#[test]` functions, not production code. No action.
- **Salt randomization with DB storage:** Deferred — path-derived salt is sufficient for now and avoids schema migration.
- **Vault re-encryption CLI command:** If migration fails, the user creates a new vault. No explicit re-encrypt command in this spec.

## Risk assessment

| Change | Blast radius | Risk |
|--------|-------------|------|
| `dedup_overlaps` | `detect_inner` → all PII detection consumers | LOW — behavior change is strictly safer (more masking, never less) |
| `decrypt_value` error | `load_mappings` → vault init | LOW — corrupted entries were already broken |
| Salt derivation | `VaultStore::open` → all vault users | MEDIUM — migration path must handle both salts |
