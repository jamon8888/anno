//! Thin wrapper over cloakpipe's file-based `Vault`.
//!
//! Cloakpipe's vault is `!Sync` (its `get_or_create` is `&mut self`),
//! so we hide it behind `Arc<tokio::sync::Mutex<...>>` for the
//! async pipeline.
//!
//! The 32-byte vault key is sourced via [`derive_key`]:
//! - If `ANNO_RAG_VAULT_PASSPHRASE` env var is set: Argon2id the passphrase
//!   into a 32-byte key (deterministic across runs given the same passphrase).
//! - Else: read a random 32-byte secret from the OS keyring (generated and
//!   stored on first run).

use crate::error::{Error, Result};
use cloakpipe_core::DetectedEntity;
use cloakpipe_core::replacer::Replacer;
use cloakpipe_core::vault::Vault as CpVault;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Async-safe handle to a cloakpipe file-based Vault.
#[derive(Clone)]
pub struct Vault {
    // `Mutex` not `RwLock`: cloakpipe's `pseudonymize` takes `&mut Vault`
    // (it mutates the token map for novel entities), so the hot path
    // always needs an exclusive lock. Reads via `lookup` only happen at
    // deanonymization time on the final answer — not a parallel hot path.
    inner: Arc<Mutex<CpVault>>,
}

impl Vault {
    /// Open or create the vault at `path` with the given 32-byte key.
    pub fn open(path: &Path, key: [u8; 32]) -> Result<Self> {
        let path_str = path
            .to_str()
            .ok_or_else(|| Error::Vault("vault path is not valid UTF-8".into()))?;
        let v = CpVault::open(path_str, key.to_vec())
            .map_err(|e| Error::Vault(format!("cloakpipe vault open: {e}")))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(v)),
        })
    }

    /// Ephemeral in-memory vault — for tests only.
    #[must_use]
    pub fn ephemeral_for_test() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CpVault::ephemeral())),
        }
    }

    /// Pseudonymize `text` given the pre-detected entities. Entities must be
    /// sorted by start offset and non-overlapping (the detector guarantees this).
    pub async fn pseudonymize(&self, text: &str, entities: &[DetectedEntity]) -> Result<String> {
        let mut v = self.inner.lock().await;
        let result = Replacer::pseudonymize(text, entities, &mut *v)
            .map_err(|e| Error::Vault(format!("replacer: {e}")))?;
        Ok(result.text)
    }

    /// Reverse-lookup a pseudo-token to its original.
    /// Returns `None` if the token is unknown.
    #[must_use = "lookup returns the original value — discard means you wanted lookup_exists"]
    pub async fn lookup(&self, token: &str) -> Option<String> {
        let v = self.inner.lock().await;
        v.lookup(token).map(|s| s.to_owned())
    }

    /// Internal: lock the underlying cloakpipe vault. Used by Pipeline to
    /// call Rehydrator and stats. Held briefly — do NOT hold across `.await`
    /// of unrelated work.
    pub(crate) async fn lock_inner(
        &self,
    ) -> tokio::sync::MutexGuard<'_, cloakpipe_core::vault::Vault> {
        self.inner.lock().await
    }
}

/// Derive the 32-byte vault key.
///
/// Order:
/// 1. `ANNO_RAG_VAULT_PASSPHRASE` env var (Argon2id with a fixed app salt — the
///    passphrase IS the entropy source, so a deterministic salt is fine here).
/// 2. OS keyring entry `anno-rag:vault-key`. If missing, generate 32 random
///    bytes via `OsRng`, hex-encode, store in keyring.
pub fn derive_key() -> Result<[u8; 32]> {
    if let Ok(passphrase) = std::env::var("ANNO_RAG_VAULT_PASSPHRASE") {
        return derive_via_argon2(&passphrase);
    }
    derive_via_keyring()
}

fn derive_via_argon2(passphrase: &str) -> Result<[u8; 32]> {
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

fn derive_via_keyring() -> Result<[u8; 32]> {
    use rand::TryRngCore;

    let entry = keyring::Entry::new("anno-rag", "vault-key")
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;

    match entry.get_password() {
        Ok(hex) => parse_hex_key(&hex),
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            rand::rngs::OsRng
                .try_fill_bytes(&mut key)
                .map_err(|e| Error::Vault(format!("OsRng fill: {e}")))?;
            let hex = hex_encode(&key);
            entry
                .set_password(&hex)
                .map_err(|e| Error::Vault(format!("keyring set: {e}")))?;
            Ok(key)
        }
        Err(e) => Err(Error::Vault(format!("keyring get: {e}"))),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // .expect is fine here — writing to String can only fail on OOM
        write!(&mut s, "{:02x}", b).expect("write to String never fails");
    }
    s
}

fn parse_hex_key(hex: &str) -> Result<[u8; 32]> {
    if hex.len() != 64 {
        return Err(Error::Vault(format!(
            "keyring key has unexpected length {} (expected 64 hex chars)",
            hex.len()
        )));
    }
    let mut key = [0u8; 32];
    for (i, byte_pair) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(byte_pair)
            .map_err(|e| Error::Vault(format!("hex utf8: {e}")))?;
        key[i] = u8::from_str_radix(s, 16)
            .map_err(|e| Error::Vault(format!("hex parse at byte {i}: {e}")))?;
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};

    fn email_entity(text: &str, start: usize) -> DetectedEntity {
        DetectedEntity {
            original: text.to_string(),
            start,
            end: start + text.len(),
            category: EntityCategory::Email,
            confidence: 1.0,
            source: DetectionSource::Pattern,
        }
    }

    fn person_entity(text: &str, start: usize) -> DetectedEntity {
        DetectedEntity {
            original: text.to_string(),
            start,
            end: start + text.len(),
            category: EntityCategory::Person,
            confidence: 0.9,
            source: DetectionSource::Ner,
        }
    }

    #[tokio::test]
    async fn pseudonymize_replaces_email_with_token() {
        let v = Vault::ephemeral_for_test();
        let text = "Contact marie@example.fr please.";
        let entity = email_entity("marie@example.fr", 8);
        let out = v.pseudonymize(text, &[entity]).await.unwrap();
        assert!(!out.contains("marie@example.fr"));
        assert!(out.contains("EMAIL_1"));
    }

    #[tokio::test]
    async fn same_entity_gets_same_token_within_vault() {
        let v = Vault::ephemeral_for_test();
        let r1 = v
            .pseudonymize(
                "Marie Dupont est avocate.",
                &[person_entity("Marie Dupont", 0)],
            )
            .await
            .unwrap();
        let r2 = v
            .pseudonymize(
                "Marie Dupont a signé hier.",
                &[person_entity("Marie Dupont", 0)],
            )
            .await
            .unwrap();
        let tok1 = r1
            .split_whitespace()
            .find(|w| w.starts_with("PERSON_"))
            .expect("token in r1");
        let tok2 = r2
            .split_whitespace()
            .find(|w| w.starts_with("PERSON_"))
            .expect("token in r2");
        assert_eq!(tok1, tok2);
    }

    #[tokio::test]
    async fn lookup_returns_original_after_pseudonymize() {
        let v = Vault::ephemeral_for_test();
        let email = "claude@example.com";
        let _ = v
            .pseudonymize(email, &[email_entity(email, 0)])
            .await
            .unwrap();
        assert_eq!(v.lookup("EMAIL_1").await.as_deref(), Some(email));
    }

    #[test]
    fn argon2_passphrase_yields_deterministic_key() {
        unsafe {
            std::env::set_var("ANNO_RAG_VAULT_PASSPHRASE", "test-passphrase-deterministic");
        }
        let k1 = derive_key().expect("first derive");
        let k2 = derive_key().expect("second derive");
        unsafe {
            std::env::remove_var("ANNO_RAG_VAULT_PASSPHRASE");
        }
        assert_eq!(k1, k2);
        assert_ne!(k1, [0u8; 32]);
    }

    #[test]
    fn hex_round_trips() {
        let original = [0xAB, 0xCD, 0x12, 0x34];
        let h = hex_encode(&original);
        assert_eq!(h, "abcd1234");
    }

    #[test]
    fn parse_hex_key_rejects_wrong_length() {
        let r = parse_hex_key("abcd");
        assert!(matches!(r, Err(Error::Vault(_))));
    }
}
