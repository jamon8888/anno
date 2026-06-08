//! OpenAI-compatible provider HTTP adapter.

use crate::{chat::ChatRequest, provider::ResolvedModel, Error, Result};
use serde_json::Value;

/// Execute one non-streaming OpenAI-compatible chat completion.
pub async fn complete(
    client: &reqwest::Client,
    resolved: &ResolvedModel,
    request: &ChatRequest,
) -> Result<Value> {
    let request = apply_auth(
        client
            .post(endpoint(&resolved.provider.base_url, "chat/completions"))
            .json(&request.to_openai_json()),
        resolved,
    )?;
    let response = request
        .send()
        .await
        .map_err(|e| Error::Upstream(format!("provider request failed: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(Error::Upstream(format!(
            "provider returned status {status}: {body}"
        )));
    }
    response
        .json::<Value>()
        .await
        .map_err(|e| Error::Upstream(format!("provider response is not JSON: {e}")))
}

/// Execute one streaming OpenAI-compatible chat completion.
pub async fn stream(
    client: &reqwest::Client,
    resolved: &ResolvedModel,
    request: &ChatRequest,
) -> Result<impl futures_util::Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>>> {
    let request = apply_auth(
        client
            .post(endpoint(&resolved.provider.base_url, "chat/completions"))
            .json(&request.to_openai_json()),
        resolved,
    )?;
    let response = request
        .send()
        .await
        .map_err(|e| Error::Upstream(format!("provider stream request failed: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(Error::Upstream(format!(
            "provider stream returned status {status}: {body}"
        )));
    }
    Ok(response.bytes_stream())
}

fn apply_auth(
    request: reqwest::RequestBuilder,
    resolved: &ResolvedModel,
) -> Result<reqwest::RequestBuilder> {
    let api_key_env = resolved.provider.api_key_env.trim();
    if api_key_env.is_empty() {
        return Ok(request);
    }
    let api_key = std::env::var(api_key_env).map_err(|_| {
        Error::Config(format!(
            "provider {} requires API key env {api_key_env}",
            resolved.provider.id
        ))
    })?;
    if api_key.trim().is_empty() {
        return Err(Error::Config(format!(
            "provider {} API key env {api_key_env} is empty",
            resolved.provider.id
        )));
    }
    Ok(request.bearer_auth(api_key))
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chat::ChatRequest,
        privacy_mode::PrivacyMode,
        provider::{ProviderConfig, ProviderKind, ProviderModel, ResolvedModel},
    };
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
        Json, Router,
    };
    use serde_json::{json, Value};
    use std::{net::SocketAddr, sync::Arc};
    use tokio::{net::TcpListener, sync::Mutex};

    #[derive(Clone)]
    struct MockState {
        captured: Arc<Mutex<Option<Value>>>,
        auth: Arc<Mutex<Option<String>>>,
    }

    async fn mock_chat(
        State(state): State<MockState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        *state.captured.lock().await = Some(body);
        *state.auth.lock().await = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        (
            StatusCode::OK,
            Json(json!({
                "choices": [{
                    "message": {"role": "assistant", "content": "Bonjour PERSON_1"}
                }]
            })),
        )
    }

    async fn spawn(app: Router) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    fn resolved(base_url: String, api_key_env: &str) -> ResolvedModel {
        ResolvedModel {
            provider: ProviderConfig {
                id: "mistral".to_string(),
                kind: ProviderKind::OpenAiCompatible,
                base_url,
                api_key_env: api_key_env.to_string(),
                dpa_verified: true,
                allowed_privacy_modes: vec![PrivacyMode::Pseudonymized],
                models: vec![ProviderModel {
                    id: "mistral-large-latest".to_string(),
                    upstream: "mistral-large-latest".to_string(),
                }],
            },
            upstream_model: "mistral-large-latest".to_string(),
            privacy_mode: PrivacyMode::Pseudonymized,
            requested_model: "anno/mistral/mistral-large-latest:pseudonymized".to_string(),
        }
    }

    fn chat_request() -> ChatRequest {
        ChatRequest::from_anthropic(
            &json!({
                "messages": [{"role": "user", "content": "Bonjour PERSON_1"}],
                "max_tokens": 128
            }),
            "mistral-large-latest",
        )
        .unwrap()
    }

    #[tokio::test]
    async fn openai_compat_posts_chat_completions_with_bearer() {
        unsafe {
            std::env::set_var("ANNO_TEST_MISTRAL_KEY", "test-key");
        }
        let captured = Arc::new(Mutex::new(None));
        let auth = Arc::new(Mutex::new(None));
        let app = Router::new()
            .route("/v1/chat/completions", post(mock_chat))
            .with_state(MockState {
                captured: Arc::clone(&captured),
                auth: Arc::clone(&auth),
            });
        let addr = spawn(app).await;

        let response = complete(
            &reqwest::Client::new(),
            &resolved(format!("http://{addr}/v1"), "ANNO_TEST_MISTRAL_KEY"),
            &chat_request(),
        )
        .await
        .unwrap();

        assert_eq!(
            response["choices"][0]["message"]["content"],
            "Bonjour PERSON_1"
        );
        assert_eq!(
            captured.lock().await.as_ref().unwrap()["model"],
            "mistral-large-latest"
        );
        assert_eq!(auth.lock().await.as_deref(), Some("Bearer test-key"));
        unsafe {
            std::env::remove_var("ANNO_TEST_MISTRAL_KEY");
        }
    }

    #[tokio::test]
    async fn openai_compat_allows_local_provider_without_api_key() {
        let captured = Arc::new(Mutex::new(None));
        let auth = Arc::new(Mutex::new(None));
        let app = Router::new()
            .route("/v1/chat/completions", post(mock_chat))
            .with_state(MockState {
                captured: Arc::clone(&captured),
                auth: Arc::clone(&auth),
            });
        let addr = spawn(app).await;

        complete(
            &reqwest::Client::new(),
            &resolved(format!("http://{addr}/v1"), ""),
            &chat_request(),
        )
        .await
        .unwrap();

        assert!(captured.lock().await.is_some());
        assert_eq!(auth.lock().await.as_deref(), None);
    }

    #[tokio::test]
    async fn openai_compat_errors_when_api_key_env_missing() {
        unsafe {
            std::env::remove_var("ANNO_TEST_MISSING_KEY");
        }
        let err = complete(
            &reqwest::Client::new(),
            &resolved("http://127.0.0.1:9/v1".to_string(), "ANNO_TEST_MISSING_KEY"),
            &chat_request(),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("ANNO_TEST_MISSING_KEY"));
    }
}
