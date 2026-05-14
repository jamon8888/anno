//! Axum HTTP server.

use crate::{upstream, Error, GatewayConfig, PrivacyEngine, Result};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::{Stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::{convert::Infallible, pin::Pin, sync::Arc};
use tokio::{
    sync::Mutex,
    time::{timeout, Duration},
};

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    config: GatewayConfig,
    client: Client,
    privacy: Arc<Mutex<PrivacyEngine>>,
}

type SseResultStream = Pin<Box<dyn Stream<Item = std::result::Result<Event, Infallible>> + Send>>;

enum MessagesResponse {
    Json(HeaderMap, Json<Value>),
    Stream(Sse<SseResultStream>),
}

impl IntoResponse for MessagesResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Json(headers, body) => (headers, body).into_response(),
            Self::Stream(stream) => stream.into_response(),
        }
    }
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
) -> Result<MessagesResponse> {
    let wants_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);

    if wants_stream {
        return stream_messages(state, body).await;
    }

    {
        let mut privacy = state.privacy.lock().await;
        privacy.pseudonymize_request(&mut body)?;
    }

    let mut response =
        upstream::forward_messages(&state.client, &state.config.upstream_anthropic_base, &body)
            .await?;

    let mut headers = HeaderMap::new();
    if state.config.auto_rehydrate {
        let privacy = state.privacy.lock().await;
        let report = privacy.rehydrate_response(&mut response)?;
        if report.fresh_pii_redacted > 0 {
            let count = HeaderValue::from_str(&report.fresh_pii_redacted.to_string())
                .map_err(|e| Error::Privacy(e.to_string()))?;
            headers.insert("x-anno-pii-leak-redacted", count);
        }
    }

    Ok(MessagesResponse::Json(headers, Json(response)))
}

async fn transform_stream_ready_text(
    privacy: &Arc<Mutex<PrivacyEngine>>,
    mut ready: String,
    scan_fresh: bool,
) -> Result<String> {
    let privacy = privacy.lock().await;
    let report = privacy.transform_stream_text(&mut ready, scan_fresh)?;
    Ok(report.output)
}

fn stream_error_event(error_type: &str, message: &str) -> Event {
    Event::default().event("error").data(
        json!({
            "type": "error",
            "error": {
                "type": error_type,
                "message": message
            }
        })
        .to_string(),
    )
}

fn stream_text_event(mut frame: crate::stream::SseFrame, text: String) -> Event {
    frame.set_text_delta(&text);
    passthrough_event(frame)
}

fn passthrough_event(frame: crate::stream::SseFrame) -> Event {
    Event::default()
        .event(frame.event.unwrap_or_else(|| "message".to_string()))
        .data(frame.data.to_string())
}

struct StreamFlush {
    event: Event,
    fatal: bool,
}

async fn flush_stream_text(
    privacy: &Arc<Mutex<PrivacyEngine>>,
    text_buffer: &mut crate::stream::StreamBuffer,
    last_text_frame: &Option<crate::stream::SseFrame>,
    scan_fresh: bool,
    finish: bool,
) -> Option<StreamFlush> {
    let ready = if finish {
        text_buffer.finish()
    } else {
        text_buffer.flush_if_safe()
    }?;
    let Some(frame) = last_text_frame.clone() else {
        return Some(StreamFlush {
            event: stream_error_event(
                "stream_state_error",
                "stream text buffer has no source frame",
            ),
            fatal: true,
        });
    };
    match transform_stream_ready_text(privacy, ready, scan_fresh).await {
        Ok(output) => Some(StreamFlush {
            event: stream_text_event(frame, output),
            fatal: false,
        }),
        Err(_) => Some(StreamFlush {
            event: stream_error_event("privacy_error", "stream privacy transform failed"),
            fatal: true,
        }),
    }
}

fn next_sse_frame_boundary(raw: &str) -> Option<(usize, usize)> {
    let lf = raw.find("\n\n").map(|index| (index, 2));
    let crlf = raw.find("\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
        (None, None) => None,
    }
}

async fn stream_messages(state: AppState, mut body: Value) -> Result<MessagesResponse> {
    {
        let mut privacy = state.privacy.lock().await;
        privacy
            .pseudonymize_request_with_streaming(&mut body, state.config.streaming.is_enabled())?;
    }

    let upstream = upstream::forward_messages_stream(
        &state.client,
        &state.config.upstream_anthropic_base,
        &body,
    )
    .await?;

    let scan_fresh = matches!(
        state.config.stream_privacy,
        crate::config::StreamPrivacyMode::BufferedScan
    );
    let max_chars = state.config.stream_max_buffer_chars;
    let max_ms = state.config.stream_max_buffer_ms;
    let privacy = Arc::clone(&state.privacy);
    let stream = async_stream::stream! {
        let mut raw = String::new();
        let mut text_buffer = crate::stream::StreamBuffer::new(max_chars);
        let mut last_text_frame = None;
        futures_util::pin_mut!(upstream);

        loop {
            let next_chunk = timeout(Duration::from_millis(max_ms), upstream.next()).await;
            let chunk = match next_chunk {
                Ok(Some(chunk)) => chunk,
                Ok(None) => {
                    if let Some(flush) = flush_stream_text(
                        &privacy,
                        &mut text_buffer,
                        &last_text_frame,
                        scan_fresh,
                        true,
                    ).await {
                        yield Ok(flush.event);
                    }
                    return;
                }
                Err(_) => {
                    if !scan_fresh {
                        if let Some(flush) = flush_stream_text(
                            &privacy,
                            &mut text_buffer,
                            &last_text_frame,
                            scan_fresh,
                            false,
                        ).await {
                            let fatal = flush.fatal;
                            yield Ok(flush.event);
                            if fatal {
                                return;
                            }
                        }
                    }
                    continue;
                }
            };

            let Ok(bytes) = chunk else {
                yield Ok(stream_error_event("upstream_error", "stream upstream error"));
                return;
            };
            raw.push_str(&String::from_utf8_lossy(&bytes));

            while let Some((frame_end, delimiter_len)) = next_sse_frame_boundary(&raw) {
                let frame_raw = raw[..frame_end + delimiter_len].to_string();
                raw = raw[frame_end + delimiter_len..].to_string();

                let Ok(mut frame) = crate::stream::SseFrame::parse(&frame_raw) else {
                    yield Ok(stream_error_event("stream_parse_error", "malformed SSE frame"));
                    return;
                };

                if frame.delta_type() == Some("input_json_delta") {
                    yield Ok(stream_error_event(
                        "unsupported_stream_delta",
                        "streaming tool input deltas are not privacy-safe in v0.4",
                    ));
                    return;
                } else if let Some(text) = frame.text_delta() {
                    last_text_frame = Some(frame.clone());
                    if let Some(ready) = text_buffer.push(text) {
                        match transform_stream_ready_text(&privacy, ready, scan_fresh).await {
                            Ok(output) => {
                                frame.set_text_delta(&output);
                                yield Ok(Event::default()
                                    .event(frame.event.clone().unwrap_or_else(|| "content_block_delta".to_string()))
                                    .data(frame.data.to_string()));
                            }
                            Err(_) => {
                                yield Ok(stream_error_event("privacy_error", "stream privacy transform failed"));
                                return;
                            }
                        }
                    }
                } else {
                    if let Some(flush) = flush_stream_text(
                        &privacy,
                        &mut text_buffer,
                        &last_text_frame,
                        scan_fresh,
                        true,
                    ).await {
                        let fatal = flush.fatal;
                        yield Ok(flush.event);
                        if fatal {
                            return;
                        }
                    }
                    yield Ok(passthrough_event(frame));
                }
            }
        }
    };

    Ok(MessagesResponse::Stream(Sse::new(
        Box::pin(stream) as SseResultStream
    )))
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
    use axum::{extract::State, http::header, routing::post};
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

    async fn mock_leaky_messages(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        *state.captured.lock().await = Some(body);
        Json(json!({
            "content": [{
                "type": "text",
                "text": "Le fournisseur a inventé Jean Martin et jean.martin@example.com."
            }]
        }))
    }

    async fn mock_stream_messages(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> axum::response::Sse<
        impl futures_util::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    > {
        *state.captured.lock().await = Some(body.clone());
        let serialized = serde_json::to_string(&body).expect("request serializes");
        let token = serialized
            .split('"')
            .find(|part| part.starts_with("PERSON_"))
            .unwrap_or("PERSON_1")
            .to_string();

        let stream = futures_util::stream::iter(vec![
            Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"Bonjour "}}).to_string())),
            Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":token[0..3].to_string()}}).to_string())),
            Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":token[3..].to_string() + "."}}).to_string())),
            Ok(axum::response::sse::Event::default()
                .event("message_stop")
                .data(json!({"type":"message_stop"}).to_string())),
        ]);
        axum::response::Sse::new(stream)
    }

    async fn mock_stream_stop_after_unfinished_text(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> axum::response::Sse<
        impl futures_util::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    > {
        *state.captured.lock().await = Some(body.clone());
        let serialized = serde_json::to_string(&body).expect("request serializes");
        let token = serialized
            .split('"')
            .find(|part| part.starts_with("PERSON_"))
            .unwrap_or("PERSON_1")
            .to_string();

        let stream = futures_util::stream::iter(vec![
            Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":format!("Bonjour {token}")}}).to_string())),
            Ok(axum::response::sse::Event::default()
                .event("message_stop")
                .data(json!({"type":"message_stop"}).to_string())),
        ]);
        axum::response::Sse::new(stream)
    }

    async fn mock_stream_crlf_messages(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> impl axum::response::IntoResponse {
        *state.captured.lock().await = Some(body.clone());
        let serialized = serde_json::to_string(&body).expect("request serializes");
        let token = serialized
            .split('"')
            .find(|part| part.starts_with("PERSON_"))
            .unwrap_or("PERSON_1");
        let body = format!(
            "event: content_block_delta\r\ndata: {}\r\n\r\nevent: message_stop\r\ndata: {{\"type\":\"message_stop\"}}\r\n\r\n",
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":format!("Bonjour {token}.")}})
        );
        ([(header::CONTENT_TYPE, "text/event-stream")], body)
    }

    async fn mock_stream_input_json_delta_messages(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> axum::response::Sse<
        impl futures_util::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    > {
        *state.captured.lock().await = Some(body);
        let stream = futures_util::stream::iter(vec![Ok(axum::response::sse::Event::default()
            .event("content_block_delta")
            .data(
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": "{\"name\":\"Jean Martin\""
                    }
                })
                .to_string(),
            ))]);
        axum::response::Sse::new(stream)
    }

    async fn mock_stream_fresh_pii_slow_chunks(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> axum::response::Sse<
        impl futures_util::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    > {
        *state.captured.lock().await = Some(body);
        let stream = async_stream::stream! {
            yield Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Jean"}}).to_string()));
            tokio::time::sleep(Duration::from_millis(50)).await;
            yield Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" Martin."}}).to_string()));
        };
        axum::response::Sse::new(stream)
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
    async fn stream_true_fails_closed_when_disabled() {
        let config = GatewayConfig::default();
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let status = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "stream": true,
                "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
            }))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn stream_route_never_sends_cleartext_and_rehydrates_split_token() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_messages))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            streaming: crate::config::StreamingMode::Enabled,
            stream_privacy: crate::config::StreamPrivacyMode::BufferedScan,
            stream_max_buffer_chars: 4096,
            stream_max_buffer_ms: 750,
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let response = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "stream": true,
                "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body = response.text().await.unwrap();
        assert!(body.contains("Marie Dupont"));
        assert!(!body.contains("PERSON_"));

        let upstream_body = captured.lock().await.clone().expect("upstream called");
        let upstream_text = serde_json::to_string(&upstream_body).unwrap();
        assert!(!upstream_text.contains("Marie Dupont"));
        assert!(upstream_text.contains("PERSON_"));
    }

    #[tokio::test]
    async fn stream_flushes_buffer_before_stop_and_preserves_index() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_stop_after_unfinished_text))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            streaming: crate::config::StreamingMode::Enabled,
            stream_privacy: crate::config::StreamPrivacyMode::BufferedScan,
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let body = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "stream": true,
                "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
            }))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        let text_pos = body.find("Marie Dupont").expect("rehydrated text emitted");
        let stop_pos = body.find("message_stop").expect("stop event emitted");
        assert!(text_pos < stop_pos);
        assert!(body.contains("\"index\":0"));
    }

    #[tokio::test]
    async fn stream_route_accepts_crlf_sse_boundaries() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_crlf_messages))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            streaming: crate::config::StreamingMode::Enabled,
            stream_privacy: crate::config::StreamPrivacyMode::BufferedScan,
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let body = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "stream": true,
                "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
            }))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(body.contains("Marie Dupont"));
        assert!(!body.contains("PERSON_"));
    }

    #[tokio::test]
    async fn stream_input_json_delta_fails_closed() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_input_json_delta_messages))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            streaming: crate::config::StreamingMode::Enabled,
            stream_privacy: crate::config::StreamPrivacyMode::BufferedScan,
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let body = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "stream": true,
                "messages": [{"role": "user", "content": "Bonjour"}]
            }))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(body.contains("event: error"));
        assert!(body.contains("unsupported_stream_delta"));
        assert!(!body.contains("Jean Martin"));
    }

    #[tokio::test]
    async fn stream_buffered_scan_does_not_timeout_flush_partial_fresh_pii() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_fresh_pii_slow_chunks))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            streaming: crate::config::StreamingMode::Enabled,
            stream_privacy: crate::config::StreamPrivacyMode::BufferedScan,
            stream_max_buffer_ms: 1,
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let body = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "stream": true,
                "messages": [{"role": "user", "content": "Bonjour"}]
            }))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(!body.contains("Jean"));
        assert!(!body.contains("Martin"));
        assert!(body.contains("[REDACTED]"));
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

    #[tokio::test]
    async fn response_pii_leak_redaction_is_reported_in_header() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_leaky_messages))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let response = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "claude",
                "messages": [{"role": "user", "content": "Bonjour"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(
            response
                .headers()
                .get("x-anno-pii-leak-redacted")
                .and_then(|value| value.to_str().ok()),
            Some("2")
        );

        let body: Value = response.json().await.unwrap();
        let text = body["content"][0]["text"].as_str().unwrap();
        assert!(!text.contains("Jean Martin"));
        assert!(!text.contains("jean.martin@example.com"));
    }
}
