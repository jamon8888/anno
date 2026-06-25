//! Thin wrapper over cloakpipe's file-based `Vault`.
//!
//! Cloakpipe's vault is `!Sync` (its `get_or_create` is `&mut self`),
//! so we hide it behind `Arc<tokio::sync::Mutex<...>>` for the
//! async pipeline.
//!
//! The 32-byte vault key is sourced via [`derive_key`]:
//! - If `ANNO_RAG_VAULT_PASSPHRASE` env var is set: Argon2id the passphrase
//!   into a 32-byte key (deterministic across runs given the same passphrase).
//! - Else: read a random 32-byte secret from the OS keyring, falling back to
//!   a Windows DPAPI-protected local file when the keyring path is unavailable.

use crate::error::{Error, Result};
use crate::legal::offsets::{PseudoOffsetMap, Substitution};
use cloakpipe_core::replacer::Replacer;
use cloakpipe_core::vault::Vault as CpVault;
use cloakpipe_core::DetectedEntity;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

pub use cloakpipe_core::vault::{MatchedMapping, RemovedMapping};

/// OS keyring service name for anno-rag vault entries.
pub const KEYRING_SERVICE: &str = "anno-rag";
/// OS keyring account name for the vault key entry.
pub const KEYRING_ACCOUNT: &str = "vault-key";
/// Windows DPAPI-protected fallback file name.
pub const DPAPI_FILE_NAME: &str = "vault-key.dpapi";

/// Source selected by [`vault_key_status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultKeyStatusSource {
    /// `ANNO_RAG_VAULT_PASSPHRASE` is configured.
    #[serde(rename = "env_derived")]
    EnvPassphrase,
    /// OS keyring entry `anno-rag:vault-key`.
    Keyring,
    /// Windows DPAPI-protected fallback file.
    DpapiFile,
    /// KMS environment variables are configured, but the adapter is not implemented yet.
    KmsUnimplemented,
    /// No usable vault key source was found.
    Missing,
}

/// Non-secret status for the configured vault key source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VaultKeyStatus {
    /// Selected status source.
    pub source: VaultKeyStatusSource,
    /// Whether the source currently has key material or source configuration.
    pub present: bool,
    /// Whether the source can produce a valid 32-byte vault key.
    pub usable: bool,
    /// Whether the key source persists across process restarts.
    pub persistent: bool,
    /// User-facing status message. Never includes key bytes or passphrase text.
    pub message: String,
}

/// Async-safe handle to a cloakpipe file-based Vault.
#[derive(Clone)]
pub struct Vault {
    // `Mutex` not `RwLock`: cloakpipe's `pseudonymize` takes `&mut Vault`
    // (it mutates the token map for novel entities), so the hot path
    // always needs an exclusive lock. Reads via `lookup` only happen at
    // deanonymization time on the final answer — not a parallel hot path.
    inner: Arc<Mutex<CpVault>>,
}

/// Pseudonymized text plus local replacement metadata for reports.
#[derive(Debug, Clone)]
pub struct PseudonymizeReport {
    /// Pseudonymized text.
    pub text: String,
    /// Replacement metadata ordered by raw offset.
    pub replacements: Vec<ReplacementRecord>,
    /// Offset map used by legal span translation.
    pub offset_map: PseudoOffsetMap,
}

/// One replacement made by the vault.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplacementRecord {
    /// Original local cleartext value. Never return through MCP.
    pub original: String,
    /// Pseudonym token.
    pub token: String,
    /// Entity category display.
    pub category: String,
    /// Detection confidence.
    pub confidence: f64,
    /// Detection source display.
    pub source: String,
    /// Raw-text byte start.
    pub raw_start: u32,
    /// Raw-text byte end.
    pub raw_end: u32,
    /// Pseudonymized-text byte start.
    pub pseudo_start: u32,
    /// Pseudonymized-text byte end.
    pub pseudo_end: u32,
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
        let result = Replacer::pseudonymize(text, entities, &mut v)
            .map_err(|e| Error::Vault(format!("replacer: {e}")))?;
        v.save()
            .map_err(|e| Error::Vault(format!("save after pseudonymize: {e}")))?;
        Ok(result.text)
    }

    /// Pseudonymize `text` and also return the `(category, token)` pairs the
    /// vault minted — needed by the memory layer for the GDPR Art. 17
    /// cascade. The text returned matches [`Self::pseudonymize`] exactly.
    ///
    /// # Errors
    /// Returns [`Error::Vault`] on cloakpipe replacer failure.
    pub async fn pseudonymize_with_refs(
        &self,
        text: &str,
        entities: &[DetectedEntity],
    ) -> Result<(String, Vec<crate::memory::TokenRef>)> {
        let mut v = self.inner.lock().await;
        let result = Replacer::pseudonymize(text, entities, &mut v)
            .map_err(|e| Error::Vault(format!("replacer: {e}")))?;
        v.save()
            .map_err(|e| Error::Vault(format!("save after pseudonymize_with_refs: {e}")))?;
        // result.mappings is token -> original. Walk the entities so the
        // returned refs carry the detector's category as the label (e.g.
        // "Person", "Email", "NIR"), not the raw original value.
        let mut refs: Vec<crate::memory::TokenRef> = Vec::with_capacity(entities.len());
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for e in entities {
            // The vault may have collapsed alias variants onto a canonical
            // entry; look up via the originals map.
            let token = result
                .mappings
                .iter()
                .find(|(_, orig)| orig.as_str() == e.original)
                .map(|(tok, _)| tok.clone());
            if let Some(token) = token {
                if seen.insert(token.clone()) {
                    refs.push(crate::memory::TokenRef {
                        label: format!("{:?}", e.category),
                        token,
                    });
                }
            }
        }
        Ok((result.text, refs))
    }

    /// Pseudonymize `text` and return the span substitution map.
    ///
    /// Existing call sites should continue using [`Self::pseudonymize`] or
    /// [`Self::pseudonymize_with_refs`]. This additive API is for legal
    /// enrichment, where GLiNER spans detected on raw text must be translated
    /// into pseudonymized chunk coordinates.
    ///
    /// # Errors
    /// Returns [`Error::Vault`] on cloakpipe replacer or vault persistence
    /// failure, or when a detected entity cannot be matched to a minted token.
    pub async fn pseudonymize_with_map(
        &self,
        text: &str,
        entities: &[DetectedEntity],
    ) -> Result<(String, PseudoOffsetMap)> {
        let mut v = self.inner.lock().await;
        let result = Replacer::pseudonymize(text, entities, &mut v)
            .map_err(|e| Error::Vault(format!("replacer: {e}")))?;
        v.save()
            .map_err(|e| Error::Vault(format!("save after pseudonymize_with_map: {e}")))?;
        drop(v);

        let mut sorted_entities: Vec<&DetectedEntity> = entities.iter().collect();
        sorted_entities.sort_by_key(|entity| entity.start);

        let mut subs = Vec::with_capacity(sorted_entities.len());
        let mut pseudo_cursor: usize = 0;
        let mut raw_cursor: usize = 0;

        for entity in sorted_entities {
            if entity.start < raw_cursor || entity.end > text.len() {
                continue;
            }

            pseudo_cursor += entity.start - raw_cursor;
            let token = result
                .mappings
                .iter()
                .find(|(_, original)| original.as_str() == entity.original)
                .map(|(token, _)| token.as_str())
                .ok_or_else(|| {
                    Error::Vault(format!(
                        "pseudonymize_with_map: no mapping for original {:?}",
                        entity.original
                    ))
                })?;
            let pseudo_start = pseudo_cursor as u32;
            let pseudo_end = (pseudo_cursor + token.len()) as u32;
            subs.push(Substitution {
                raw_start: entity.start as u32,
                raw_end: entity.end as u32,
                pseudo_start,
                pseudo_end,
            });
            pseudo_cursor += token.len();
            raw_cursor = entity.end;
        }

        Ok((result.text, PseudoOffsetMap { subs }))
    }

    /// Pseudonymize text and return local replacement metadata for reports.
    ///
    /// # Errors
    /// Returns [`Error::Vault`] on replacer or vault persistence failures.
    pub async fn pseudonymize_with_report_map(
        &self,
        text: &str,
        entities: &[DetectedEntity],
    ) -> Result<PseudonymizeReport> {
        let mut v = self.inner.lock().await;
        let result = Replacer::pseudonymize(text, entities, &mut v)
            .map_err(|e| Error::Vault(format!("replacer: {e}")))?;
        v.save()
            .map_err(|e| Error::Vault(format!("save after pseudonymize_with_report_map: {e}")))?;
        drop(v);

        let mut sorted_entities: Vec<&DetectedEntity> = entities.iter().collect();
        sorted_entities.sort_by_key(|entity| entity.start);

        let mut subs = Vec::with_capacity(sorted_entities.len());
        let mut replacements = Vec::with_capacity(sorted_entities.len());
        let mut pseudo_cursor: usize = 0;
        let mut raw_cursor: usize = 0;

        for entity in sorted_entities {
            if entity.start < raw_cursor || entity.end > text.len() {
                continue;
            }

            pseudo_cursor += entity.start - raw_cursor;
            let token = result
                .mappings
                .iter()
                .find(|(_, original)| original.as_str() == entity.original)
                .map(|(token, _)| token.as_str())
                .ok_or_else(|| {
                    Error::Vault(format!(
                        "pseudonymize_with_report_map: no mapping for original {:?}",
                        entity.original
                    ))
                })?;
            let pseudo_start = pseudo_cursor as u32;
            let pseudo_end = (pseudo_cursor + token.len()) as u32;
            subs.push(Substitution {
                raw_start: entity.start as u32,
                raw_end: entity.end as u32,
                pseudo_start,
                pseudo_end,
            });
            replacements.push(ReplacementRecord {
                original: entity.original.clone(),
                token: token.to_string(),
                category: format!("{:?}", entity.category),
                confidence: entity.confidence,
                source: format!("{:?}", entity.source),
                raw_start: entity.start as u32,
                raw_end: entity.end as u32,
                pseudo_start,
                pseudo_end,
            });
            pseudo_cursor += token.len();
            raw_cursor = entity.end;
        }

        Ok(PseudonymizeReport {
            text: result.text,
            replacements,
            offset_map: PseudoOffsetMap { subs },
        })
    }

    /// Reverse-lookup a pseudo-token to its original.
    /// Returns `None` if the token is unknown.
    #[must_use = "lookup returns the original value — discard means you wanted lookup_exists"]
    pub async fn lookup(&self, token: &str) -> Option<String> {
        let v = self.inner.lock().await;
        v.lookup(token).map(|s| s.to_owned())
    }

    /// Non-async best-effort reverse lookup. Used by v0.2 graph-recall to
    /// hydrate display strings for entity nodes without dragging an
    /// `.await` through the BFS. Returns `None` if the vault is currently
    /// locked by another caller — the graph builder falls back to showing
    /// the raw token id in `display`.
    #[must_use]
    pub fn lookup_blocking(&self, token: &str) -> Option<String> {
        let inner = self.inner.try_lock().ok()?;
        inner.lookup(token).map(|s| s.to_owned())
    }

    /// Internal: lock the underlying cloakpipe vault. Used by Pipeline to
    /// call Rehydrator and stats. Held briefly — do NOT hold across `.await`
    /// of unrelated work.
    pub(crate) async fn lock_inner(
        &self,
    ) -> tokio::sync::MutexGuard<'_, cloakpipe_core::vault::Vault> {
        self.inner.lock().await
    }

    /// Remove the vault mapping for `subject_ref` (original or token). Persists
    /// the vault to disk on success. Returns the removed mapping for audit, or
    /// `None` if no mapping matched.
    ///
    /// # Errors
    /// Returns [`Error::Vault`] if persisting the vault to disk fails.
    pub async fn forget(&self, subject_ref: &str) -> Result<Option<RemovedMapping>> {
        let mut v = self.inner.lock().await;
        let removed = v.remove(subject_ref);
        if removed.is_some() {
            v.save()
                .map_err(|e| Error::Vault(format!("save after forget: {e}")))?;
        }
        Ok(removed)
    }

    /// Find every vault entry matching `subject_ref` (original or token).
    pub async fn find_subject(&self, subject_ref: &str) -> Vec<MatchedMapping> {
        let v = self.inner.lock().await;
        v.find(subject_ref)
    }
}

/// Where the 32-byte vault key comes from. v0.4 ships two real sources
/// ([`Self::Passphrase`] + [`Self::Keyring`]); [`Self::Kms`] is a
/// scaffolded stub that an external KMS adapter will plug into in v0.5+
/// (Azure Key Vault, AWS KMS, HashiCorp Vault — DPIA v1 §3 R1 mitigation
/// G-3). See `docs/adrs/0002-encrypted-vault-aes-256-gcm-passphrase-or-keyring.md`
/// for the per-source rationale.
#[derive(Debug, Clone)]
pub enum VaultKeySource {
    /// Argon2id KDF over an operator-supplied passphrase. Deterministic
    /// across runs given the same passphrase + fixed app salt.
    Passphrase(String),
    /// OS keyring entry `anno-rag:vault-key`. Generated + stored on first
    /// run if the entry is missing; Windows DPAPI is used as fallback when
    /// the keyring path is unavailable.
    Keyring,
    /// External KMS provider — v0.5+. Currently a stub: returns
    /// [`Error::Vault`] with a clear message pointing at the adapter
    /// implementation as TODO.
    Kms {
        /// Provider name (e.g. `"azure-key-vault"`, `"aws-kms"`,
        /// `"hashicorp-vault"`). Reserved for the v0.5+ adapter lookup.
        provider: String,
        /// Provider-specific key id (URI, ARN, alias).
        key_id: String,
    },
}

impl VaultKeySource {
    /// Pick the source from environment. Priority order:
    ///
    /// 1. `ANNO_RAG_VAULT_KMS_PROVIDER` + `ANNO_RAG_VAULT_KMS_KEY_ID` → KMS.
    /// 2. `ANNO_RAG_VAULT_PASSPHRASE` → Passphrase.
    /// 3. Default → Keyring, then Windows DPAPI fallback.
    #[must_use]
    pub fn from_env() -> Self {
        let provider = std::env::var("ANNO_RAG_VAULT_KMS_PROVIDER").ok();
        let key_id = std::env::var("ANNO_RAG_VAULT_KMS_KEY_ID").ok();
        if let (Some(provider), Some(key_id)) = (provider, key_id) {
            if !provider.is_empty() && !key_id.is_empty() {
                return Self::Kms { provider, key_id };
            }
        }
        if let Ok(passphrase) = std::env::var("ANNO_RAG_VAULT_PASSPHRASE") {
            return Self::Passphrase(passphrase);
        }
        Self::Keyring
    }

    /// Derive the 32-byte key from this source.
    ///
    /// # Errors
    /// Returns [`Error::Vault`] on KDF / keyring / KMS-stub failure.
    pub fn derive(&self) -> Result<[u8; 32]> {
        match self {
            Self::Passphrase(p) => derive_via_argon2_legacy(p),
            Self::Keyring => derive_via_keyring(),
            Self::Kms { provider, key_id } => Err(Error::Vault(format!(
                "KMS key source not implemented in v0.4 \
                 (provider={provider}, key_id={key_id}). \
                 Configure ANNO_RAG_VAULT_PASSPHRASE or the OS keyring \
                 instead. Tracked as U6 in the readiness spec; v0.5+ \
                 will land a KmsAdapter trait + per-provider impls."
            ))),
        }
    }
}

/// Returns `true` if a usable vault key source is currently configured.
///
/// Delegates to [`vault_key_status`] so the check agrees with the real key
/// derivation logic — the same passphrase validation, keyring hex parsing,
/// and DPAPI fallback path that [`derive_key`] uses. Returns `false` if
/// status resolution itself fails.
pub fn is_vault_key_usable() -> bool {
    vault_key_status().map(|s| s.usable).unwrap_or(false)
}

/// Derive the 32-byte vault key from the environment. Thin wrapper around
/// [`VaultKeySource::from_env`] + [`VaultKeySource::derive`] kept for
/// backwards compatibility with v0.1–v0.3 call sites.
///
/// Order:
/// 1. `ANNO_RAG_VAULT_KMS_PROVIDER` + `ANNO_RAG_VAULT_KMS_KEY_ID` (stub in
///    v0.4 — returns an explanatory error).
/// 2. `ANNO_RAG_VAULT_PASSPHRASE` env var (Argon2id with a fixed app salt).
/// 3. OS keyring entry `anno-rag:vault-key`. If missing, generate 32 random
///    bytes via `OsRng`, hex-encode, store in keyring, or use the Windows
///    DPAPI fallback when the keyring path is unavailable.
///
/// # Errors
/// Returns [`Error::Vault`] on KDF / keyring failure, or if the KMS source
/// is selected (v0.4 stub).
pub fn derive_key() -> Result<[u8; 32]> {
    VaultKeySource::from_env().derive()
}

pub(crate) fn derive_via_argon2(passphrase: &str, vault_path: &Path) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};
    use sha2::{Digest, Sha256};

    let params = Params::new(19_456, 2, 1, Some(32))
        .map_err(|e| Error::Vault(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let path_str = vault_path.to_string_lossy();
    let path_hash = Sha256::digest(path_str.as_bytes());
    let salt = &path_hash[..16];

    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Vault(format!("argon2 derive: {e}")))?;
    Ok(key)
}

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

fn dpapi_file_path() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| Error::Vault("could not determine local data directory".into()))?
        .join("anno-rag");
    std::fs::create_dir_all(&base)?;
    Ok(base.join(DPAPI_FILE_NAME))
}

#[cfg(windows)]
fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB as DATA_BLOB,
    };

    let cb_data =
        u32::try_from(data.len()).map_err(|_| Error::Vault("DPAPI input is too large".into()))?;
    let input = DATA_BLOB {
        cbData: cb_data,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = DATA_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::Vault(format!(
            "DPAPI protect failed: {}",
            std::io::Error::last_os_error()
        )));
    }

    let protected =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as HLOCAL);
    }
    Ok(protected)
}

#[cfg(not(windows))]
fn dpapi_protect(_data: &[u8]) -> Result<Vec<u8>> {
    Err(Error::Vault(
        "Windows DPAPI fallback is only available on Windows".into(),
    ))
}

#[cfg(windows)]
fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB as DATA_BLOB,
    };

    let cb_data =
        u32::try_from(data.len()).map_err(|_| Error::Vault("DPAPI input is too large".into()))?;
    let input = DATA_BLOB {
        cbData: cb_data,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = DATA_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::Vault(format!(
            "DPAPI unprotect failed: {}",
            std::io::Error::last_os_error()
        )));
    }

    let unprotected =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        LocalFree(output.pbData as HLOCAL);
    }
    Ok(unprotected)
}

#[cfg(not(windows))]
fn dpapi_unprotect(_data: &[u8]) -> Result<Vec<u8>> {
    Err(Error::Vault(
        "Windows DPAPI fallback is only available on Windows".into(),
    ))
}

/// Read the Windows DPAPI-protected fallback key, if present.
///
/// # Errors
/// Returns [`Error::Vault`] on unreadable, undecryptable, or malformed DPAPI
/// fallback data.
pub fn read_dpapi_key() -> Result<Option<[u8; 32]>> {
    let path = dpapi_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let protected = std::fs::read(&path)
        .map_err(|e| Error::Vault(format!("DPAPI key file read {}: {e}", path.display())))?;
    let mut unprotected = dpapi_unprotect(&protected)?;
    if unprotected.len() != 32 {
        return Err(Error::Vault(format!(
            "DPAPI key file has unexpected length {} (expected 32 bytes)",
            unprotected.len()
        )));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&unprotected);
    unprotected.fill(0);
    Ok(Some(key))
}

/// Write the Windows DPAPI-protected fallback key.
///
/// # Errors
/// Returns [`Error::Vault`] if DPAPI protection or file persistence fails.
pub fn write_dpapi_key(key: &[u8; 32]) -> Result<()> {
    let path = dpapi_file_path()?;
    let protected = dpapi_protect(key)?;
    std::fs::write(&path, protected)
        .map_err(|e| Error::Vault(format!("DPAPI key file write {}: {e}", path.display())))?;
    Ok(())
}

fn store_key_in_keyring(key: &[u8; 32]) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    entry
        .set_password(&hex_encode(key))
        .map_err(|e| Error::Vault(format!("keyring set: {e}")))?;
    let stored = entry.get_password().map_err(|e| {
        Error::Vault(format!(
            "keyring set verification failed: {e}; vault key was not persisted in keyring"
        ))
    })?;
    let stored_key = parse_hex_key(&stored)?;
    if stored_key != *key {
        return Err(Error::Vault(
            "keyring set verification read back a different key".into(),
        ));
    }
    Ok(())
}

fn random_vault_key() -> [u8; 32] {
    use rand::Rng;

    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    key
}

fn write_dpapi_key_after_keyring_error(key: &[u8; 32], keyring_error: Error) -> Result<()> {
    write_dpapi_key(key).map_err(|dpapi_error| {
        Error::Vault(format!(
            "{keyring_error}; Windows DPAPI fallback write failed: {dpapi_error}"
        ))
    })
}

fn generate_dpapi_fallback_key(keyring_error: Error) -> Result<[u8; 32]> {
    let key = random_vault_key();
    write_dpapi_key_after_keyring_error(&key, keyring_error)?;
    Ok(key)
}

fn generate_key_with_keyring_or_dpapi() -> Result<[u8; 32]> {
    let key = random_vault_key();
    if let Err(keyring_error) = store_key_in_keyring(&key) {
        write_dpapi_key_after_keyring_error(&key, keyring_error)?;
    }
    Ok(key)
}

fn dpapi_status_after_keyring_missing() -> Result<VaultKeyStatus> {
    let path = dpapi_file_path()?;
    match read_dpapi_key() {
        Ok(Some(_)) => Ok(VaultKeyStatus {
            source: VaultKeyStatusSource::DpapiFile,
            present: true,
            usable: true,
            persistent: true,
            message: format!(
                "vault key is stored in Windows DPAPI file {}",
                path.display()
            ),
        }),
        Ok(None) => Ok(VaultKeyStatus {
            source: VaultKeyStatusSource::Missing,
            present: false,
            usable: false,
            persistent: false,
            message: "no vault key is configured".to_string(),
        }),
        Err(e) => Ok(VaultKeyStatus {
            source: VaultKeyStatusSource::DpapiFile,
            present: path.exists(),
            usable: false,
            persistent: path.exists(),
            message: format!("Windows DPAPI fallback is not usable: {e}"),
        }),
    }
}

/// Return non-secret status for the effective vault key source.
///
/// # Errors
/// Returns [`Error::Vault`] if local status paths cannot be resolved.
pub fn vault_key_status() -> Result<VaultKeyStatus> {
    let provider = std::env::var("ANNO_RAG_VAULT_KMS_PROVIDER").ok();
    let key_id = std::env::var("ANNO_RAG_VAULT_KMS_KEY_ID").ok();
    if let (Some(provider), Some(key_id)) = (provider, key_id) {
        if !provider.is_empty() && !key_id.is_empty() {
            return Ok(VaultKeyStatus {
                source: VaultKeyStatusSource::KmsUnimplemented,
                present: true,
                usable: false,
                persistent: false,
                message: format!(
                    "KMS vault key source is configured but not implemented (provider={provider})"
                ),
            });
        }
    }

    if let Ok(passphrase) = std::env::var("ANNO_RAG_VAULT_PASSPHRASE") {
        let usable = derive_via_argon2_legacy(&passphrase).is_ok();
        return Ok(VaultKeyStatus {
            source: VaultKeyStatusSource::EnvPassphrase,
            present: true,
            usable,
            persistent: false,
            message: "vault key is derived from configured environment secret".to_string(),
        });
    }

    let entry = match keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(entry) => entry,
        Err(e) => {
            let mut status = dpapi_status_after_keyring_missing()?;
            if status.source == VaultKeyStatusSource::Missing {
                status.message =
                    format!("keyring is unavailable and no fallback key was found: {e}");
            }
            return Ok(status);
        }
    };

    match entry.get_password() {
        Ok(hex) => {
            let usable = parse_hex_key(&hex).is_ok();
            Ok(VaultKeyStatus {
                source: VaultKeyStatusSource::Keyring,
                present: true,
                usable,
                persistent: true,
                message: if usable {
                    "vault key is stored in the OS keyring".to_string()
                } else {
                    "OS keyring vault key is malformed".to_string()
                },
            })
        }
        Err(keyring::Error::NoEntry) => dpapi_status_after_keyring_missing(),
        Err(e) => {
            let mut status = dpapi_status_after_keyring_missing()?;
            if status.source == VaultKeyStatusSource::Missing {
                status.message = format!("keyring read failed and no fallback key was found: {e}");
            }
            Ok(status)
        }
    }
}

fn derive_via_keyring() -> Result<[u8; 32]> {
    let entry = match keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(entry) => entry,
        Err(e) => {
            if let Some(key) = read_dpapi_key()? {
                return Ok(key);
            }
            return generate_dpapi_fallback_key(Error::Vault(format!("keyring open: {e}")));
        }
    };

    match entry.get_password() {
        Ok(hex) => parse_hex_key(&hex),
        Err(keyring::Error::NoEntry) => {
            if let Some(key) = read_dpapi_key()? {
                return Ok(key);
            }
            generate_key_with_keyring_or_dpapi()
        }
        Err(e) => {
            if let Some(key) = read_dpapi_key()? {
                return Ok(key);
            }
            generate_dpapi_fallback_key(Error::Vault(format!("keyring get: {e}")))
        }
    }
}

/// Argon2id-derive a 32-byte key from `passphrase` and store the hex-encoded
/// bytes in the OS keyring at service [`KEYRING_SERVICE`] / account
/// [`KEYRING_ACCOUNT`], overwriting any existing entry.
///
/// Used by `anno_init_vault` (spec §14.3 Path B).
pub fn store_passphrase_derived_key_in_keyring(passphrase: &str) -> Result<()> {
    initialize_vault_key_from_passphrase(passphrase).map(|_| ())
}

/// Argon2id-derive a 32-byte key from `passphrase` and persist it in the OS
/// keyring, falling back to a Windows DPAPI-protected file if the keyring write
/// path is unavailable.
///
/// Used by `anno_init_vault` and startup repair flows. The passphrase itself is
/// never returned or logged.
pub fn initialize_vault_key_from_passphrase(passphrase: &str) -> Result<VaultKeyStatus> {
    let key = derive_via_argon2_legacy(passphrase)?;
    match store_key_in_keyring(&key) {
        Ok(()) => Ok(VaultKeyStatus {
            source: VaultKeyStatusSource::Keyring,
            present: true,
            usable: true,
            persistent: true,
            message: "vault key was stored in the OS keyring".to_string(),
        }),
        Err(keyring_error) => {
            write_dpapi_key(&key).map_err(|dpapi_error| {
                Error::Vault(format!(
                    "{keyring_error}; Windows DPAPI fallback write failed: {dpapi_error}"
                ))
            })?;
            Ok(VaultKeyStatus {
                source: VaultKeyStatusSource::DpapiFile,
                present: true,
                usable: true,
                persistent: true,
                message: "vault key was stored in a Windows DPAPI fallback file".to_string(),
            })
        }
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
        let s =
            std::str::from_utf8(byte_pair).map_err(|e| Error::Vault(format!("hex utf8: {e}")))?;
        key[i] = u8::from_str_radix(s, 16)
            .map_err(|e| Error::Vault(format!("hex parse at byte {i}: {e}")))?;
    }
    Ok(key)
}

/// Generate a random 32-byte vault key and persist it in the OS keyring
/// (falling back to a Windows DPAPI-protected file when the keyring is
/// unavailable). Intended for first-run setup by the Tauri installer.
///
/// Idempotent: if a usable key already exists this is a no-op.
/// This function never reads or returns key material — it only stores it.
///
/// # Errors
/// Returns [`Error::Vault`] if both the keyring and the DPAPI fallback fail.
pub fn init_keyring() -> Result<()> {
    if is_vault_key_usable() {
        return Ok(());
    }
    generate_key_with_keyring_or_dpapi().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
    use std::ffi::OsString;

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
    async fn pseudonymize_with_report_map_returns_replacements() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault_path = dir.path().join("vault.enc");
        let vault = Vault::open(&vault_path, [7u8; 32]).expect("vault");
        let text = "Jean Dupont appelle jean@example.test.";
        let entities = vec![
            cloakpipe_core::DetectedEntity {
                original: "Jean Dupont".to_string(),
                start: 0,
                end: 11,
                category: cloakpipe_core::EntityCategory::Person,
                confidence: 0.98,
                source: cloakpipe_core::DetectionSource::Ner,
            },
            cloakpipe_core::DetectedEntity {
                original: "jean@example.test".to_string(),
                start: 20,
                end: 37,
                category: cloakpipe_core::EntityCategory::Email,
                confidence: 1.0,
                source: cloakpipe_core::DetectionSource::Pattern,
            },
        ];

        let report = vault
            .pseudonymize_with_report_map(text, &entities)
            .await
            .expect("pseudo report");

        assert!(!report.text.contains("Jean Dupont"));
        assert_eq!(report.replacements.len(), 2);
        assert_eq!(report.replacements[0].original, "Jean Dupont");
        assert!(report.replacements[0].token.starts_with("PERSON_"));
        assert_eq!(report.replacements[0].raw_start, 0);
        assert_eq!(report.replacements[0].raw_end, 11);
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

    #[tokio::test]
    async fn forget_removes_existing_mapping() {
        use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
        let v = Vault::ephemeral_for_test();
        let entities = vec![DetectedEntity {
            original: "Marie Dupont".into(),
            start: 0,
            end: 12,
            category: EntityCategory::Person,
            confidence: 1.0,
            source: DetectionSource::Pattern,
        }];
        v.pseudonymize("Marie Dupont", &entities).await.unwrap();

        let receipt = v.forget("Marie Dupont").await.unwrap();
        assert_eq!(receipt.unwrap().original, "Marie Dupont");

        // Second call returns None (idempotent).
        assert!(v.forget("Marie Dupont").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn find_subject_returns_match_then_empty_after_forget() {
        use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
        let v = Vault::ephemeral_for_test();
        let entities = vec![DetectedEntity {
            original: "a@b.fr".into(),
            start: 0,
            end: 6,
            category: EntityCategory::Email,
            confidence: 1.0,
            source: DetectionSource::Pattern,
        }];
        v.pseudonymize("a@b.fr", &entities).await.unwrap();

        let m = v.find_subject("a@b.fr").await;
        assert_eq!(m.len(), 1);

        v.forget("a@b.fr").await.unwrap();
        assert!(v.find_subject("a@b.fr").await.is_empty());
    }

    #[test]
    fn argon2_passphrase_yields_deterministic_key() {
        let _guard = crate::env_guard::lock_env();
        let previous_passphrase = std::env::var_os("ANNO_RAG_VAULT_PASSPHRASE");
        unsafe {
            std::env::set_var("ANNO_RAG_VAULT_PASSPHRASE", "test-passphrase-deterministic");
        }
        let k1 = derive_key().expect("first derive");
        let k2 = derive_key().expect("second derive");
        unsafe {
            restore_env("ANNO_RAG_VAULT_PASSPHRASE", previous_passphrase);
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

    #[test]
    fn key_source_kms_returns_stub_error_with_provider_in_message() {
        let s = VaultKeySource::Kms {
            provider: "aws-kms".into(),
            key_id: "arn:aws:kms:eu-west-3:1234:key/abc".into(),
        };
        let err = s.derive().expect_err("v0.4 KMS path is a stub");
        let msg = err.to_string();
        assert!(
            msg.contains("KMS key source not implemented"),
            "msg = {msg}"
        );
        assert!(msg.contains("aws-kms"), "msg should name the provider");
        assert!(msg.contains("U6"), "msg should point at the readiness gap");
    }

    #[test]
    fn key_source_passphrase_derives_deterministically() {
        let a = VaultKeySource::Passphrase("test-deterministic".into())
            .derive()
            .unwrap();
        let b = VaultKeySource::Passphrase("test-deterministic".into())
            .derive()
            .unwrap();
        assert_eq!(a, b, "argon2id with fixed salt must be deterministic");
    }

    #[test]
    fn key_source_passphrase_differs_per_input() {
        let a = VaultKeySource::Passphrase("one".into()).derive().unwrap();
        let b = VaultKeySource::Passphrase("two".into()).derive().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn vault_key_status_prefers_env_passphrase() {
        let _guard = crate::env_guard::lock_env();
        let previous_passphrase = std::env::var_os("ANNO_RAG_VAULT_PASSPHRASE");
        let previous_kms_provider = std::env::var_os("ANNO_RAG_VAULT_KMS_PROVIDER");
        let previous_kms_key_id = std::env::var_os("ANNO_RAG_VAULT_KMS_KEY_ID");

        unsafe {
            std::env::set_var("ANNO_RAG_VAULT_PASSPHRASE", "test-status-passphrase");
            std::env::remove_var("ANNO_RAG_VAULT_KMS_PROVIDER");
            std::env::remove_var("ANNO_RAG_VAULT_KMS_KEY_ID");
        }

        let status = vault_key_status().expect("status from env passphrase");

        unsafe {
            restore_env("ANNO_RAG_VAULT_PASSPHRASE", previous_passphrase);
            restore_env("ANNO_RAG_VAULT_KMS_PROVIDER", previous_kms_provider);
            restore_env("ANNO_RAG_VAULT_KMS_KEY_ID", previous_kms_key_id);
        }

        assert_eq!(status.source, VaultKeyStatusSource::EnvPassphrase);
        assert!(status.present);
        assert!(status.usable);
        assert!(!status.persistent);
        assert!(!status.message.to_lowercase().contains("test-status"));
    }

    #[test]
    fn derive_via_argon2_different_paths_produce_different_keys() {
        let pass = "test-passphrase";
        let key_a = derive_via_argon2(pass, Path::new("/vault/a.db")).unwrap();
        let key_b = derive_via_argon2(pass, Path::new("/vault/b.db")).unwrap();
        assert_ne!(
            key_a, key_b,
            "different vault paths must produce different keys"
        );
    }

    #[test]
    fn derive_via_argon2_same_path_is_deterministic() {
        let pass = "test-passphrase";
        let key_a = derive_via_argon2(pass, Path::new("/vault/a.db")).unwrap();
        let key_b = derive_via_argon2(pass, Path::new("/vault/a.db")).unwrap();
        assert_eq!(
            key_a, key_b,
            "same path + same passphrase must produce same key"
        );
    }

    #[test]
    fn derive_via_argon2_legacy_compat() {
        let pass = "test-passphrase";
        let legacy_key = derive_via_argon2_legacy(pass).unwrap();
        assert_eq!(legacy_key.len(), 32);
    }

    unsafe fn restore_env(name: &str, value: Option<OsString>) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
    }

    /// B11: vault reuses the same token for a known original, guaranteeing that
    /// `pseudonymize_query` produces a consistent pseudonymized form even when
    /// the entity was first seen in a document.
    #[tokio::test]
    async fn pseudonymize_reuses_known_token_for_query() {
        let vault = Vault::ephemeral_for_test();
        let name = "Marc Dubois";

        // Simulate prior ingestion: pseudonymize a document containing the name.
        let doc_entities = vec![cloakpipe_core::DetectedEntity {
            original: name.to_string(),
            start: 11,
            end: 22,
            category: cloakpipe_core::EntityCategory::Person,
            confidence: 0.95,
            source: cloakpipe_core::DetectionSource::Ner,
        }];
        let doc_pseudo = vault
            .pseudonymize("Le patient Marc Dubois souffre.", &doc_entities)
            .await
            .unwrap();
        // Extract the token assigned during ingestion.
        let token: String = doc_pseudo
            .split_whitespace()
            .find(|w| w.starts_with("PERSON_"))
            .expect("person token in doc")
            .to_string();

        // Now pseudonymize a query containing the same name.
        let query_entities = vec![cloakpipe_core::DetectedEntity {
            original: name.to_string(),
            start: 9,
            end: 20,
            category: cloakpipe_core::EntityCategory::Person,
            confidence: 0.90,
            source: cloakpipe_core::DetectionSource::Ner,
        }];
        let query_pseudo = vault
            .pseudonymize(&format!("Who is {name}?"), &query_entities)
            .await
            .unwrap();

        // The vault must reuse the same token — otherwise the query embedding
        // won't match the document embedding.
        assert!(
            !query_pseudo.contains(name),
            "cleartext name must not appear in pseudonymized query"
        );
        assert!(
            query_pseudo.contains(&token),
            "query must reuse the same token '{token}' as the document"
        );
    }
}
