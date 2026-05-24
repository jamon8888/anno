//! Tests for the `anno_health` and `anno_init_vault` MCP tools.
//!
//! Tests call implementation functions directly — the `#[tool_router]` macro
//! generates private methods, so they are not accessible from integration tests.

use anno_rag::config::AnnoRagConfig;
use anno_rag::pipeline::Pipeline;
use anno_rag::vault::derive_key;
use anno_rag_mcp::health::{collect_health, init_vault_with_passphrase, EngineHealth};

#[tokio::test]
async fn anno_health_reports_engine_version_and_tool_set() {
    let cfg = AnnoRagConfig::default();
    let key = derive_key().expect("derive_key in test env");
    let pipeline = Pipeline::new(cfg.clone(), key).await.expect("pipeline up");

    let health: EngineHealth = collect_health(&pipeline, &cfg).await;

    assert_eq!(health.engine_version, env!("CARGO_PKG_VERSION"));
    assert!(health.available_tools.contains(&"search".to_string()));
    assert!(health.available_tools.contains(&"rehydrate".to_string()));
    assert!(health.available_tools.contains(&"detect".to_string()));
    assert!(health.available_tools.contains(&"anno_health".to_string()));
    assert!(health.available_tools.contains(&"anno_init_vault".to_string()));
    assert!(!health.build_target.is_empty());
    assert!(!health.signed);
}

#[tokio::test]
async fn anno_health_tool_returns_json_with_engine_version() {
    let cfg = AnnoRagConfig::default();
    let key = derive_key().expect("derive_key in test env");
    let pipeline = Pipeline::new(cfg.clone(), key).await.expect("pipeline up");

    // Test via collect_health (the MCP tool calls this internally)
    let health = collect_health(&pipeline, &cfg).await;
    let json = serde_json::to_string_pretty(&health).expect("serializable");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["engine_version"], env!("CARGO_PKG_VERSION"));
    assert!(parsed["available_tools"].is_array());
}

#[tokio::test]
async fn anno_init_vault_rejects_empty_passphrase() {
    let result = init_vault_with_passphrase("");
    let json = serde_json::to_string_pretty(&result).expect("serializable");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("passphrase"));
}

#[tokio::test]
async fn anno_init_vault_rejects_short_passphrase() {
    let result = init_vault_with_passphrase("short");
    let json = serde_json::to_string_pretty(&result).expect("serializable");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("passphrase"));
}

#[tokio::test]
async fn anno_init_vault_writes_to_keyring_with_nonempty_passphrase() {
    let result = init_vault_with_passphrase("correct horse battery staple");
    let json = serde_json::to_string_pretty(&result).expect("serializable");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["ok"], true);
    // Passphrase must NEVER appear in the response.
    let json_lower = json.to_lowercase();
    assert!(!json_lower.contains("correct horse battery staple"));
    assert!(!json_lower.contains("passphrase"));
}
