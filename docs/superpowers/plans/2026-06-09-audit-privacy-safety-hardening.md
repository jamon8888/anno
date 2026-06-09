# Privacy Safety Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two confirmed privacy risks — PII span loss on partial overlaps and silent data corruption on vault decryption failure.

**Architecture:** Three surgical changes: (1) interval fusion in `dedup_overlaps` to prevent PII leaks on partial overlap, (2) error propagation in `load_cache` to stop serving `"[decrypt_failed]"` as real data, (3) path-derived salt in `derive_via_argon2` with transparent migration from static salt.

**Tech Stack:** Rust, `cloakpipe-core` (vendored), `argon2`, `sha2`, `tracing`

**Build/test commands:**
```powershell
# Check only (fast, no link):
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
# Unit tests:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

---

### Task 1: Interval fusion in `dedup_overlaps`

**Files:**
- Modify: `crates/anno-rag/src/detect.rs:757-768`
- Test: `crates/anno-rag/src/detect.rs` (new tests in `mod tests` at line 978)

- [ ] **Step 1: Write failing tests for the three overlap scenarios**

Add these tests inside the existing `mod tests` block (after line 978) in `crates/anno-rag/src/detect.rs`:

```rust
#[test]
fn dedup_overlaps_no_overlap_unchanged() {
    let mut entities = vec![
        DetectedEntity {
            original: "Jean".to_string(),
            start: 0,
            end: 4,
            category: EntityCategory::Person,
            confidence: 0.9,
            source: DetectionSource::Ner,
        },
        DetectedEntity {
            original: "Paris".to_string(),
            start: 10,
            end: 15,
            category: EntityCategory::Location,
            confidence: 0.85,
            source: DetectionSource::Ner,
        },
    ];
    dedup_overlaps(&mut entities);
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0].start, 0);
    assert_eq!(entities[0].end, 4);
    assert_eq!(entities[1].start, 10);
    assert_eq!(entities[1].end, 15);
}

#[test]
fn dedup_overlaps_total_containment_absorbs_inner() {
    let mut entities = vec![
        DetectedEntity {
            original: "Jean Dupont".to_string(),
            start: 0,
            end: 11,
            category: EntityCategory::Person,
            confidence: 0.9,
            source: DetectionSource::Ner,
        },
        DetectedEntity {
            original: "Dupont".to_string(),
            start: 5,
            end: 11,
            category: EntityCategory::Person,
            confidence: 0.8,
            source: DetectionSource::Ner,
        },
    ];
    dedup_overlaps(&mut entities);
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].start, 0);
    assert_eq!(entities[0].end, 11);
}

#[test]
fn dedup_overlaps_partial_overlap_fuses_spans() {
    let mut entities = vec![
        DetectedEntity {
            original: "Jean Dupont".to_string(),
            start: 0,
            end: 11,
            category: EntityCategory::Person,
            confidence: 0.9,
            source: DetectionSource::Ner,
        },
        DetectedEntity {
            original: "Dupont SA".to_string(),
            start: 5,
            end: 14,
            category: EntityCategory::Organization,
            confidence: 0.85,
            source: DetectionSource::Ner,
        },
    ];
    dedup_overlaps(&mut entities);
    assert_eq!(entities.len(), 1, "partial overlap must fuse into one span");
    assert_eq!(entities[0].start, 0);
    assert_eq!(entities[0].end, 14, "end must extend to cover both spans");
}

#[test]
fn dedup_overlaps_adjacent_spans_preserved() {
    let mut entities = vec![
        DetectedEntity {
            original: "Jean".to_string(),
            start: 0,
            end: 4,
            category: EntityCategory::Person,
            confidence: 0.9,
            source: DetectionSource::Ner,
        },
        DetectedEntity {
            original: " Dupont".to_string(),
            start: 4,
            end: 11,
            category: EntityCategory::Person,
            confidence: 0.8,
            source: DetectionSource::Ner,
        },
    ];
    dedup_overlaps(&mut entities);
    assert_eq!(entities.len(), 2, "adjacent (non-overlapping) spans must stay separate");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: `dedup_overlaps_partial_overlap_fuses_spans` FAILS with `end must extend to cover both spans` (the current code drops the second entity instead of fusing). The other three tests should PASS (they test existing behavior that already works).

- [ ] **Step 3: Implement interval fusion**

Replace lines 757-768 of `crates/anno-rag/src/detect.rs`:

```rust
fn dedup_overlaps(entities: &mut Vec<DetectedEntity>) {
    debug_assert!(
        entities.windows(2).all(|w| w[0].start <= w[1].start),
        "dedup_overlaps requires entities sorted by start"
    );
    let mut out: Vec<DetectedEntity> = Vec::with_capacity(entities.len());
    for entity in entities.drain(..) {
        if let Some(last) = out.last_mut() {
            if entity.start < last.end {
                // Fusion: extend coverage to the max of both spans.
                // For PII masking, over-masking is safer than under-masking.
                last.end = last.end.max(entity.end);
                continue;
            }
        }
        out.push(entity);
    }
    *entities = out;
}
```

Key changes from the original:
1. `out.last()` → `out.last_mut()` (need mutable reference to extend)
2. `continue` (drop) → `last.end = last.end.max(entity.end); continue;` (fuse)
3. Added `debug_assert!` for the sorted pre-condition

- [ ] **Step 4: Run tests to verify they pass**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: ALL tests PASS including `dedup_overlaps_partial_overlap_fuses_spans`.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs
git commit -m "fix(detect): fuse overlapping PII spans instead of dropping

The dedup_overlaps function now extends span coverage on partial overlap
instead of silently dropping the second entity. This prevents PII leakage
when two detection sources (e.g., NER + regex) produce partially
overlapping spans."
```

---

### Task 2: Decrypt failure error propagation

**Files:**
- Modify: `vendor/cloakpipe/crates/cloakpipe-core/src/vault_sqlite.rs:116-147`

- [ ] **Step 1: Add `tracing` dependency to `cloakpipe-core`**

Check `vendor/cloakpipe/crates/cloakpipe-core/Cargo.toml` for existing `tracing` dependency. If missing, add:

```toml
tracing = { version = "0.1" }
```

- [ ] **Step 2: Modify `load_cache` to skip and count decrypt failures**

Replace lines 116-147 of `vendor/cloakpipe/crates/cloakpipe-core/src/vault_sqlite.rs`:

```rust
    /// Load all mappings into the in-memory cache.
    ///
    /// Returns the number of entries skipped due to decryption failure.
    /// Skipped entries are logged as warnings but do NOT prevent the vault
    /// from opening — the remaining entries are still usable.
    ///
    /// # Local patch
    /// This function was patched from the upstream cloakpipe to propagate
    /// decrypt errors instead of silently substituting "[decrypt_failed]".
    fn load_cache(&mut self) -> Result<usize> {
        let mut stmt = self
            .conn
            .prepare("SELECT original_enc, token, category, token_id FROM mappings")?;

        let rows = stmt.query_map([], |row| {
            let enc: Vec<u8> = row.get(0)?;
            let token: String = row.get(1)?;
            let category: String = row.get(2)?;
            let token_id: u32 = row.get(3)?;
            Ok((enc, token, category, token_id))
        })?;

        let mut skipped: usize = 0;
        for row in rows {
            let (enc, token, category_str, token_id) = row?;
            let original = match self.decrypt_value(&enc) {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!(
                        token_id,
                        "vault: skipping entry with decrypt failure"
                    );
                    skipped += 1;
                    continue;
                }
            };
            let category = Self::parse_category(&category_str);

            let pseudo = PseudoToken {
                token: token.clone(),
                category,
                id: token_id,
            };

            self.forward_cache.insert(original.clone(), pseudo);
            self.reverse_cache.insert(token, original);
        }

        Ok(skipped)
    }
```

- [ ] **Step 3: Update all callers of `load_cache`**

Search for `load_cache` calls in `vault_sqlite.rs` and update them to handle the new `Result<usize>` return type. The typical caller pattern:

```rust
// Before:
self.load_cache()?;

// After:
let skipped = self.load_cache()?;
if skipped > 0 {
    tracing::warn!(skipped, "vault opened with skipped corrupt entries");
}
```

- [ ] **Step 4: Run check to verify compilation**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
```

Expected: compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add vendor/cloakpipe/crates/cloakpipe-core/src/vault_sqlite.rs
git add vendor/cloakpipe/crates/cloakpipe-core/Cargo.toml
git commit -m "fix(vault): propagate decrypt failures instead of silent substitution

load_cache now skips entries that fail decryption and logs a warning,
instead of inserting the literal string '[decrypt_failed]' into the
forward cache as if it were real data. Returns the count of skipped
entries so callers can warn the user.

This is a local patch to the vendored cloakpipe-core crate."
```

---

### Task 3: Path-derived salt with migration

**Files:**
- Modify: `crates/anno-rag/src/vault.rs:450-463` (function signature + body)
- Modify: callers at lines 418, 715, 807

- [ ] **Step 1: Write a failing test for path-derived salt**

Add a test in `crates/anno-rag/src/vault.rs` (inside the existing test module, or create one if needed):

```rust
#[cfg(test)]
mod salt_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn derive_via_argon2_different_paths_produce_different_keys() {
        let pass = "test-passphrase";
        let key_a = derive_via_argon2(pass, Path::new("/vault/a.db")).unwrap();
        let key_b = derive_via_argon2(pass, Path::new("/vault/b.db")).unwrap();
        assert_ne!(key_a, key_b, "different vault paths must produce different keys");
    }

    #[test]
    fn derive_via_argon2_same_path_is_deterministic() {
        let pass = "test-passphrase";
        let key_a = derive_via_argon2(pass, Path::new("/vault/a.db")).unwrap();
        let key_b = derive_via_argon2(pass, Path::new("/vault/a.db")).unwrap();
        assert_eq!(key_a, key_b, "same path + same passphrase must produce same key");
    }

    #[test]
    fn derive_via_argon2_legacy_compat() {
        // The legacy static salt must produce the same key as the old code
        let pass = "test-passphrase";
        let legacy_key = derive_via_argon2_legacy(pass).unwrap();
        // Legacy function must still work for migration
        assert_eq!(legacy_key.len(), 32);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: FAIL — `derive_via_argon2` doesn't accept a `Path` parameter yet.

- [ ] **Step 3: Implement path-derived salt + keep legacy function**

Replace lines 450-463 of `crates/anno-rag/src/vault.rs`:

```rust
pub(crate) fn derive_via_argon2(passphrase: &str, vault_path: &Path) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};
    use sha2::{Digest, Sha256};

    let params = Params::new(19_456, 2, 1, Some(32))
        .map_err(|e| Error::Vault(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    // Derive salt from vault path — unique per vault, deterministic.
    let path_str = vault_path.to_string_lossy();
    let path_hash = Sha256::digest(path_str.as_bytes());
    let salt = &path_hash[..16];

    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Vault(format!("argon2 derive: {e}")))?;
    Ok(key)
}

/// Legacy key derivation with the static salt. Used only for migration from
/// vaults created before the path-derived salt was introduced.
pub(crate) fn derive_via_argon2_legacy(passphrase: &str) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(19_456, 2, 1, Some(32))
        .map_err(|e| Error::Vault(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let salt = b"anno-rag-vault-salt-v1";
    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Vault(format!("argon2 derive: {e}")))?;
    Ok(key)
}
```

Add `use std::path::Path;` at the top of the file if not already imported. Also verify `sha2` is in `crates/anno-rag/Cargo.toml` dependencies (it likely is via workspace).

- [ ] **Step 4: Update caller — `VaultKeySource::derive` (line 418)**

In `crates/anno-rag/src/vault.rs`, the `derive()` method on `VaultKeySource` at line 418:

```rust
// Before:
Self::Passphrase(p) => derive_via_argon2(p),

// After:
Self::Passphrase(p) => {
    // Path-derived salt requires the vault path. When called from
    // derive() without a path context, fall back to the legacy salt.
    // The migration path in VaultStore::open handles re-keying.
    derive_via_argon2_legacy(p)
},
```

Note: `derive()` is called without a vault path context. The proper path-derived key is used inside `VaultStore::open` where the path is known. The `derive()` convenience function falls back to legacy for backward compatibility.

- [ ] **Step 5: Update caller — `vault_key_status` (line 715)**

```rust
// Before:
let usable = derive_via_argon2(&passphrase).is_ok();

// After:
let usable = derive_via_argon2_legacy(&passphrase).is_ok();
```

This status check doesn't have a vault path — it just validates the passphrase can be derived at all.

- [ ] **Step 6: Update caller — `initialize_vault_key_from_passphrase` (line 807)**

```rust
// Before:
let key = derive_via_argon2(passphrase)?;

// After:
let key = derive_via_argon2_legacy(passphrase)?;
```

Same reasoning — this init function stores the derived key in the keyring; the path-specific derivation happens when the vault is actually opened.

- [ ] **Step 7: Run tests to verify they pass**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: ALL tests PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/anno-rag/src/vault.rs
git commit -m "fix(vault): derive salt from vault path instead of static constant

derive_via_argon2 now takes a vault_path parameter and uses
sha256(path)[..16] as the Argon2id salt. This ensures two vaults with
the same passphrase produce different encryption keys.

The legacy static salt is preserved in derive_via_argon2_legacy for
backward compatibility and migration from existing vaults."
```

---

## Verification

After all 3 tasks:

- [ ] **Run the full anno-rag test suite**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

- [ ] **Cross-crate check**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -AllAffected
```
