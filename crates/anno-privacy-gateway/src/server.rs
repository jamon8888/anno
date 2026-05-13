//! Axum HTTP server.

use crate::{upstream, Error, GatewayConfig, PrivacyEngine, Result};
use axum::{
    extract::State,
    routing::{get, post},
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
    pub fn new(config: GatewayConfig) -> Self {
        Self::try_new(config).expect("default privacy engine can initialize")
    }

    /// Build app state and validate persistent vault configuration.
    pub fn try_new(config: GatewayConfig) -> Result<Self> {
        let privacy = PrivacyEngine::from_config(&config)?;
        Ok(Self {
            config,
            client: Client::new(),
            privacy: Arc::new(Mutex::new(privacy)),
        })
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
    let app = router(AppState::try_new(config)?);
    axum::serve(listener, app)
        .await
        .map_err(|e| Error::Upstream(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, routing::post};
    use serde_json::json;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct MockState {
        captured: Arc<Mutex<Option<Value>>>,
    }

    async fn mock_messages(State(state): State<MockState>, Json(body): Json<Value>) -> Json<Value> {
        *state.captured.lock().await = Some(body.clone());
        let serialized = serde_json::to_string(&body).expect("request serializes");
        let token = serialized
            .split('"')
            .find(|part| part.starts_with("PERSON_"))
            .unwrap_or("PERSON_1");
        Json(json!({
            "content": [{"type": "text", "text": format!("Bonjour {token}")}]
        }))
    }

    async fn spawn(app: Router) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    #[tokio::test]
    async fn messages_route_never_sends_cleartext_to_upstream_and_rehydrates() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_messages))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let client = reqwest::Client::new();
        let response: Value = client
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let upstream_body = captured.lock().await.clone().expect("upstream called");
        let upstream_text = serde_json::to_string(&upstream_body).unwrap();
        assert!(!upstream_text.contains("Marie Dupont"));
        assert!(upstream_text.contains("PERSON_"));
        assert_eq!(response["content"][0]["text"], "Bonjour Marie Dupont");
    }

    #[tokio::test]
    async fn files_api_fails_closed() {
        let config = GatewayConfig::default();
        let gateway_addr = spawn(router(AppState::new(config))).await;
        let status = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/files"))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    }
}
