//! Axum HTTP server.

use crate::{upstream, Error, GatewayConfig, PrivacyEngine, Result};
use axum::{
    extract::State,
    routing::{delete, get, post},
    Json, Router,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    config: GatewayConfig,
    client: Client,
    privacy: Arc<Mutex<PrivacyEngine>>,
}

impl AppState {
    /// Build app state from runtime config.
    #[must_use]
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            privacy: Arc::new(Mutex::new(PrivacyEngine::default())),
        }
    }
}

/// Build the v0.3 router.
#[must_use]
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/messages", post(messages))
        .route("/v1/models", get(models))
        .route("/v1/files", post(files_unsupported).get(files_unsupported))
        .route(
            "/v1/files/{id}",
            get(files_unsupported).delete(files_unsupported),
        )
        .route("/v1/files/{id}/content", get(files_unsupported))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "provider_profile": state.config.provider_profile,
        "auto_rehydrate": state.config.auto_rehydrate,
    }))
}

async fn messages(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>> {
    {
        let mut privacy = state.privacy.lock().await;
        privacy.pseudonymize_request(&mut body)?;
    }

    let mut response =
        upstream::forward_messages(&state.client, &state.config.upstream_anthropic_base, &body)
            .await?;

    if state.config.auto_rehydrate {
        let privacy = state.privacy.lock().await;
        privacy.rehydrate_response(&mut response)?;
    }

    Ok(Json(response))
}

async fn models(State(state): State<AppState>) -> Result<Json<Value>> {
    upstream::forward_models(&state.client, &state.config.upstream_anthropic_base)
        .await
        .map(Json)
}

async fn files_unsupported() -> Error {
    Error::UnsupportedFeature("native Files API is deferred to v0.5".to_string())
}

/// Start the server and run until the listener exits.
pub async fn serve(config: GatewayConfig) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|e| Error::Upstream(e.to_string()))?;
    let app = router(AppState::new(config));
    axum::serve(listener, app)
        .await
        .map_err(|e| Error::Upstream(e.to_string()))
}
