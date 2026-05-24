//! Tests for the `anno_health` and `anno_init_vault` MCP tools.

use anno_rag::config::AnnoRagConfig;
use anno_rag::pipeline::Pipeline;
use anno_rag::vault::derive_key;
use anno_rag_mcp::health::{collect_health, EngineHealth};

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
    let server = anno_rag_mcp::AnnoRagServer::new(pipeline, cfg);

    let json = server.anno_health().await;
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["engine_version"], env!("CARGO_PKG_VERSION"));
    assert!(parsed["available_tools"].is_array());
}

#[tokio::test]
async fn anno_init_vault_rejects_empty_passphrase() {
    let cfg = anno_rag::config::AnnoRagConfig::default();
    let key = anno_rag::vault::derive_key().expect("derive_key in test env");
    let pipeline = anno_rag::pipeline::Pipeline::new(cfg.clone(), key).await.expect("pipeline up");
    let server = anno_rag_mcp::AnnoRagServer::new(pipeline, cfg);

    let params = anno_rag_mcp::InitVaultParams {
        passphrase: String::new(),
    };
    let json = server
        .anno_init_vault(rmcp::handler::server::wrapper::Parameters(params))
        .await;
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
    let cfg = anno_rag::config::AnnoRagConfig::default();
    let key = anno_rag::vault::derive_key().expect("derive_key in test env");
    let pipeline = anno_rag::pipeline::Pipeline::new(cfg.clone(), key).await.expect("pipeline up");
    let server = anno_rag_mcp::AnnoRagServer::new(pipeline, cfg);

    let params = anno_rag_mcp::InitVaultParams {
        passphrase: "correct horse battery staple".to_string(),
    };
    let json = server
        .anno_init_vault(rmcp::handler::server::wrapper::Parameters(params))
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["ok"], true);
    // Passphrase must NEVER appear in the response.
    let json_lower = json.to_lowercase();
    assert!(!json_lower.contains("correct horse battery staple"));
    assert!(!json_lower.contains("passphrase"));
}
