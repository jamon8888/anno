//! Vault admin operations exposed via the `anno-rag vault` CLI subcommand
//! family (spec §14.4).

use crate::error::{Error, Result};
use crate::vault::{KEYRING_ACCOUNT, KEYRING_SERVICE};
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
}

/// Check whether the keyring contains an entry at the configured service/account.
pub fn vault_status() -> Result<VaultStatus> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    let present = entry.get_password().is_ok();
    Ok(VaultStatus {
        service: KEYRING_SERVICE.to_string(),
        account: KEYRING_ACCOUNT.to_string(),
        keyring_entry_present: present,
    })
}

/// Rotate the vault key: generate 32 fresh random bytes and replace the
/// keyring entry. Requires an existing entry; returns Err otherwise.
pub fn vault_rotate() -> Result<()> {
    use rand::TryRngCore;

    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    let _existing = entry
        .get_password()
        .map_err(|e| Error::Vault(format!("no existing keyring entry to rotate: {e}")))?;

    let mut new_key = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut new_key)
        .map_err(|e| Error::Vault(format!("rng: {e}")))?;
    let hex: String = new_key.iter().map(|b| format!("{:02x}", b)).collect();
    entry
        .set_password(&hex)
        .map_err(|e| Error::Vault(format!("keyring set: {e}")))?;
    Ok(())
}
