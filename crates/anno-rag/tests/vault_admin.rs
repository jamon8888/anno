//! Tests for vault admin operations (status, rotate).

use anno_rag::vault_admin::{vault_status, VaultStatus};

#[test]
fn vault_status_reports_keyring_presence() {
    let status: VaultStatus = vault_status().expect("status");

    let json = serde_json::to_string(&status).expect("serialize");
    assert!(json.contains("\"keyring_entry_present\""));
    assert!(json.contains("\"service\":\"anno-rag\""));
    assert!(json.contains("\"account\":\"vault-key\""));
    assert!(!json.to_lowercase().contains("passphrase"));
}

#[test]
fn vault_status_serializes_without_passphrase_substring() {
    let status = vault_status().expect("status");
    let pretty = serde_json::to_string_pretty(&status).expect("pretty");
    assert!(!pretty.to_lowercase().contains("passphrase"));
}
