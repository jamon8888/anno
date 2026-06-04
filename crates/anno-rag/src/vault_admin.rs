//! Vault admin operations exposed via the `anno-rag vault` CLI subcommand
//! family (spec §14.4).

use crate::error::{Error, Result};
use crate::vault::{VaultKeyStatusSource, KEYRING_ACCOUNT, KEYRING_SERVICE};
use serde::Serialize;

/// Output of `anno-rag vault status`. Never echoes the stored key.
#[derive(Debug, Clone, Serialize)]
pub struct VaultStatus {
    /// OS keyring service name queried.
    pub service: String,
    /// OS keyring account name queried.
    pub account: String,
    /// Whether a keyring entry exists at the configured service/account.
    pub keyring_entry_present: bool,
    /// Effective vault key source.
    pub key_source: VaultKeyStatusSource,
    /// Whether the selected source has key material or source configuration.
    pub key_present: bool,
    /// Whether the selected source can produce a valid vault key.
    pub key_usable: bool,
    /// Whether the selected key source persists across process restarts.
    pub key_persistent: bool,
    /// Non-secret user-facing key status message.
    pub key_message: String,
}

/// Check the effective vault key source without exposing secret material.
pub fn vault_status() -> Result<VaultStatus> {
    let key_status = crate::vault::vault_key_status()?;
    let keyring_entry_present = key_status.source == VaultKeyStatusSource::Keyring;
    Ok(VaultStatus {
        service: KEYRING_SERVICE.to_string(),
        account: KEYRING_ACCOUNT.to_string(),
        keyring_entry_present,
        key_source: key_status.source,
        key_present: key_status.present,
        key_usable: key_status.usable,
        key_persistent: key_status.persistent,
        key_message: key_status.message,
    })
}

/// Rotate the vault key: generate 32 fresh random bytes and replace the
/// keyring entry. Requires an existing entry; returns Err otherwise.
pub fn vault_rotate() -> Result<()> {
    use rand::Rng;

    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    let _existing = entry
        .get_password()
        .map_err(|e| Error::Vault(format!("no existing keyring entry to rotate: {e}")))?;

    let mut new_key = [0u8; 32];
    rand::rng().fill_bytes(&mut new_key);
    let hex: String = new_key.iter().map(|b| format!("{:02x}", b)).collect();
    entry
        .set_password(&hex)
        .map_err(|e| Error::Vault(format!("keyring set: {e}")))?;
    Ok(())
}
