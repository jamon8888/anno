//! Shared application state for the proxy server.

use cloakpipe_audit::AuditLogger;
use cloakpipe_core::{
    config::CloakPipeConfig,
    detector::Detector,
    session::SessionManager,
    vault::Vault,
};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared state accessible from all request handlers.
pub struct AppState {
    pub config: CloakPipeConfig,
    pub detector: Detector,
    pub vault: Arc<Mutex<Vault>>,
    pub audit: AuditLogger,
    pub http_client: reqwest::Client,
    pub api_key: String,
    pub sessions: Arc<SessionManager>,
}

impl AppState {
    pub fn new(
        config: CloakPipeConfig,
        detector: Detector,
        vault: Vault,
        audit: AuditLogger,
        api_key: String,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.proxy.timeout_seconds))
            .build()
            .expect("Failed to build HTTP client");

        let sessions = Arc::new(SessionManager::new(config.session.clone()));

        Self {
            config,
            detector,
            vault: Arc::new(Mutex::new(vault)),
            audit,
            http_client,
            api_key,
            sessions,
        }
    }
}
