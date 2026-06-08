//! Axum HTTP server.

use crate::{upstream, Error, GatewayConfig, PrivacyEngine, Result};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
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
    audit: Arc<dyn crate::audit::AuditSink>,
    provider_catalog: Option<crate::provider::ProviderCatalog>,
    file_registry: Arc<crate::file_registry::FileRegistry>,
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
        config.validate_security()?;
        let provider_catalog = match &config.provider_catalog_path {
            Some(path) => {
                Some(crate::provider::ProviderCatalog::from_path(path).map_err(Error::Config)?)
            }
            None => None,
        };
        let privacy = PrivacyEngine::from_config(&config)?;
        let file_registry = Arc::new(crate::file_registry::FileRegistry::new(
            crate::file_registry::FileRegistryConfig {
                root: config.file_store_dir.clone(),
                retain_raw: config.file_retain_raw,
                retain_cleartext: config.file_retain_cleartext,
            },
        ));
        let audit: Arc<dyn crate::audit::AuditSink> =
            match (&config.audit_dir, &config.audit_hmac_key_hex) {
                (Some(dir), Some(key_hex)) => {
                    let key_bytes = hex::decode(key_hex)
                        .map_err(|e| Error::Config(format!("audit hmac key hex: {e}")))?;
                    let key: [u8; 32] = key_bytes
                        .try_into()
                        .map_err(|_| Error::Config("audit hmac key must be 32 bytes".into()))?;
                    Arc::new(
                        crate::audit::JsonlAuditSink::new(dir, key)
                            .map_err(|e| Error::Config(format!("audit init: {e}")))?,
                    )
                }
                (Some(_), None) => {
                    return Err(Error::Config(
                        "audit_dir set but audit_hmac_key_hex missing".into(),
                    ));
                }
                _ => Arc::new(crate::audit::NoopAuditSink),
            };
        Ok(Self {
            config,
            client: Client::new(),
            privacy: Arc::new(Mutex::new(privacy)),
            audit,
            provider_catalog,
            file_registry,
        })
    }

    /// Borrow the shared privacy engine handle.
    #[must_use]
    pub fn privacy(&self) -> &Arc<Mutex<PrivacyEngine>> {
        &self.privacy
    }

    /// Borrow the audit sink.
    #[must_use]
    pub fn audit(&self) -> &Arc<dyn crate::audit::AuditSink> {
        &self.audit
    }

    /// Borrow the runtime config.
    #[must_use]
    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }

    /// Configured bearer token, if any.
    #[must_use]
    pub fn bearer_token(&self) -> Option<&str> {
        self.config.bearer_token.as_deref()
    }
}

/// Build the gateway router.
///
/// `/health` is public; everything under `/v1/*` is gated by the bearer-token
/// middleware ([`crate::auth::require_bearer`]). Loopback-only deployments may
/// run without a token; non-loopback listeners are rejected during app-state
/// initialization unless `config.bearer_token` is set.
pub fn router(state: AppState) -> Router {
    let public = Router::new().route("/health", get(health));
    let file_body_limit = state.config.file_max_bytes.saturating_add(1024 * 1024);
    let file_routes = Router::new()
        .route("/v1/files", post(upload_file).get(list_files_unsupported))
        .route("/v1/files/{id}", get(get_file_metadata).delete(delete_file))
        .route("/v1/files/{id}/content", get(get_file_content))
        .layer(DefaultBodyLimit::max(file_body_limit));

    let protected = Router::new()
        .route("/v1/messages", post(messages))
        .route("/v1/models", get(models))
        .merge(file_routes)
        .route("/v1/subjects/find", post(crate::subjects::find))
        .route("/v1/subjects/forget", post(crate::subjects::forget))
        .route(
            "/v1/subjects/{subject_ref}/export",
            get(crate::subjects::export),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_bearer,
        ));

    public.merge(protected).with_state(state)
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

    if state.provider_catalog.is_some() {
        return provider_messages(state, body).await;
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

async fn provider_messages(state: AppState, mut body: Value) -> Result<MessagesResponse> {
    let catalog = state
        .provider_catalog
        .as_ref()
        .ok_or_else(|| Error::Config("provider catalog missing".to_string()))?;
    let model_id = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Privacy("model is required".to_string()))?
        .to_string();
    let resolved = catalog.resolve_model(&model_id).map_err(Error::Config)?;

    let document_report = {
        let mut privacy = state.privacy.lock().await;
        crate::document_blocks::expand_document_blocks(
            &mut body,
            &state.file_registry,
            resolved.privacy_mode,
            state.config.file_max_bytes,
            &mut privacy,
        )
        .await?
    };
    let privacy_report = {
        let mut privacy = state.privacy.lock().await;
        privacy.transform_request_for_mode(&mut body, resolved.privacy_mode, false)?
    };
    let request = crate::chat::ChatRequest::from_anthropic(&body, &resolved.upstream_model)?;
    let upstream = crate::openai_compat::complete(&state.client, &resolved, &request).await?;
    let mut response = crate::chat::anthropic_response_from_openai(&upstream)?;

    let mut headers = HeaderMap::new();
    let mut fresh_pii_redacted = 0usize;
    if state.config.auto_rehydrate
        && resolved.privacy_mode == crate::privacy_mode::PrivacyMode::Pseudonymized
    {
        let privacy = state.privacy.lock().await;
        let report = privacy.rehydrate_response(&mut response)?;
        fresh_pii_redacted = report.fresh_pii_redacted;
        if fresh_pii_redacted > 0 {
            let count = HeaderValue::from_str(&fresh_pii_redacted.to_string())
                .map_err(|e| Error::Privacy(e.to_string()))?;
            headers.insert("x-anno-pii-leak-redacted", count);
        }
    }

    state.audit.record(crate::audit::AuditEvent {
        request_id: "provider-router".to_string(),
        provider_profile: state.config.provider_profile.clone(),
        provider_id: resolved.provider.id.clone(),
        model_id: resolved.requested_model.clone(),
        upstream_model: resolved.upstream_model.clone(),
        privacy_mode: resolved.privacy_mode.audit_label().to_string(),
        entity_count: privacy_report.entities + document_report.entity_count,
        fresh_pii_redacted,
    });

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
    if state.provider_catalog.is_some() {
        return provider_stream_messages(state, body).await;
    }

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

async fn provider_stream_messages(state: AppState, mut body: Value) -> Result<MessagesResponse> {
    let catalog = state
        .provider_catalog
        .as_ref()
        .ok_or_else(|| Error::Config("provider catalog missing".to_string()))?;
    let model_id = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Privacy("model is required".to_string()))?
        .to_string();
    let resolved = catalog.resolve_model(&model_id).map_err(Error::Config)?;
    {
        let mut privacy = state.privacy.lock().await;
        crate::document_blocks::expand_document_blocks(
            &mut body,
            &state.file_registry,
            resolved.privacy_mode,
            state.config.file_max_bytes,
            &mut privacy,
        )
        .await?;
    }
    {
        let mut privacy = state.privacy.lock().await;
        privacy.transform_request_for_mode(
            &mut body,
            resolved.privacy_mode,
            state.config.streaming.is_enabled(),
        )?;
    }
    let request = crate::chat::ChatRequest::from_anthropic(&body, &resolved.upstream_model)?;
    let upstream = crate::openai_compat::stream(&state.client, &resolved, &request).await?;
    let apply_privacy = resolved.privacy_mode == crate::privacy_mode::PrivacyMode::Pseudonymized;
    let scan_fresh = apply_privacy
        && matches!(
            state.config.stream_privacy,
            crate::config::StreamPrivacyMode::BufferedScan
        );
    let max_chars = state.config.stream_max_buffer_chars;
    let privacy = Arc::clone(&state.privacy);
    let stream = async_stream::stream! {
        let mut raw = String::new();
        let mut text_buffer = crate::stream::StreamBuffer::new(max_chars);
        let mut last_text_frame = None;
        let mut tool_json_buffer = String::new();
        futures_util::pin_mut!(upstream);

        while let Some(chunk) = upstream.next().await {
            let Ok(bytes) = chunk else {
                yield Ok(stream_error_event("upstream_error", "provider stream upstream error"));
                return;
            };
            raw.push_str(&String::from_utf8_lossy(&bytes));

            while let Some((frame_end, delimiter_len)) = next_sse_frame_boundary(&raw) {
                let frame_raw = raw[..frame_end + delimiter_len].to_string();
                raw = raw[frame_end + delimiter_len..].to_string();

                for line in frame_raw.lines().filter_map(|line| line.strip_prefix("data:")) {
                    let data = line.trim();
                    if data == "[DONE]" {
                        if apply_privacy {
                            if let Some(flush) = flush_stream_text(
                                &privacy,
                                &mut text_buffer,
                                &last_text_frame,
                                scan_fresh,
                                true,
                            ).await {
                                yield Ok(flush.event);
                            }
                        }
                        yield Ok(passthrough_event(crate::stream::SseFrame {
                            event: Some("message_stop".to_string()),
                            data: json!({"type": "message_stop"}),
                        }));
                        return;
                    }

                    let Ok(value) = serde_json::from_str::<Value>(data) else {
                        yield Ok(stream_error_event("stream_parse_error", "malformed provider SSE JSON"));
                        return;
                    };
                    let Ok(mut frame) = crate::chat::anthropic_stream_frame_from_openai(&value, 0) else {
                        yield Ok(stream_error_event("stream_parse_error", "unsupported provider stream chunk"));
                        return;
                    };

                    if let Some(text) = frame.text_delta() {
                        if !apply_privacy {
                            yield Ok(passthrough_event(frame));
                            continue;
                        }
                        last_text_frame = Some(frame.clone());
                        if let Some(ready) = text_buffer.push(text) {
                            match transform_stream_ready_text(&privacy, ready, scan_fresh).await {
                                Ok(output) => {
                                    frame.set_text_delta(&output);
                                    yield Ok(passthrough_event(frame));
                                }
                                Err(_) => {
                                    yield Ok(stream_error_event("privacy_error", "stream privacy transform failed"));
                                    return;
                                }
                            }
                        }
                    } else if frame.delta_type() == Some("input_json_delta") {
                        if !apply_privacy {
                            yield Ok(passthrough_event(frame));
                            continue;
                        }
                        let partial = frame
                            .data
                            .get("delta")
                            .and_then(|delta| delta.get("partial_json"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        tool_json_buffer.push_str(&partial);
                        if tool_json_buffer.len() > max_chars {
                            yield Ok(stream_error_event("privacy_error", "stream tool JSON buffer exceeded limit"));
                            return;
                        }
                        if serde_json::from_str::<Value>(&tool_json_buffer).is_ok() {
                            match transform_stream_ready_text(
                                &privacy,
                                std::mem::take(&mut tool_json_buffer),
                                scan_fresh,
                            ).await {
                                Ok(output) => {
                                    if let Some(delta) = frame
                                        .data
                                        .get_mut("delta")
                                        .and_then(Value::as_object_mut)
                                    {
                                        delta.insert("partial_json".to_string(), Value::String(output));
                                    }
                                    yield Ok(passthrough_event(frame));
                                }
                                Err(_) => {
                                    yield Ok(stream_error_event("privacy_error", "stream privacy transform failed"));
                                    return;
                                }
                            }
                        }
                    } else {
                        if apply_privacy {
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
                        }
                        yield Ok(passthrough_event(frame));
                    }
                }
            }
        }

        if apply_privacy {
            if let Some(flush) = flush_stream_text(
                &privacy,
                &mut text_buffer,
                &last_text_frame,
                scan_fresh,
                true,
            ).await {
                yield Ok(flush.event);
            }
        }
    };

    Ok(MessagesResponse::Stream(Sse::new(
        Box::pin(stream) as SseResultStream
    )))
}

async fn models(State(state): State<AppState>) -> Result<Json<Value>> {
    if let Some(catalog) = &state.provider_catalog {
        return Ok(Json(crate::model_catalog::models_response(catalog)));
    }
    upstream::forward_models(&state.client, &state.config.upstream_anthropic_base)
        .await
        .map(Json)
}

async fn list_files_unsupported() -> Error {
    Error::UnsupportedFeature("file listing is not exposed by anno gateway".to_string())
}

async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>> {
    let mut file_name = None;
    let mut content_type = "application/octet-stream".to_string();
    let mut bytes = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| Error::Privacy(format!("read multipart field: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        file_name = field.file_name().map(ToString::to_string);
        content_type = field
            .content_type()
            .map(ToString::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        bytes = field
            .bytes()
            .await
            .map_err(|e| Error::Privacy(format!("read uploaded file bytes: {e}")))?
            .to_vec();
        break;
    }

    if bytes.is_empty() {
        return Err(Error::UnsupportedFeature(
            "multipart upload must include one file field".to_string(),
        ));
    }
    if bytes.len() > state.config.file_max_bytes {
        return Err(Error::UnsupportedFeature(format!(
            "uploaded file exceeds {} bytes",
            state.config.file_max_bytes
        )));
    }

    let filename = file_name.unwrap_or_else(|| "uploaded-document".to_string());
    let extracted =
        crate::document_extract::extract_uploaded_document(&filename, &content_type, bytes.clone())
            .await?;
    let pseudonymized = {
        let mut privacy = state.privacy.lock().await;
        privacy.pseudonymize_plain_text(&extracted.text)?
    };
    let stored = state
        .file_registry
        .put_text_derivatives(
            &extracted.filename,
            &extracted.detected_content_type,
            &bytes,
            &extracted.text,
            &pseudonymized.text,
        )
        .await?;

    state.audit.record(crate::audit::AuditEvent {
        request_id: "file-upload".to_string(),
        provider_profile: state.config.provider_profile.clone(),
        provider_id: "local-file-registry".to_string(),
        model_id: "none".to_string(),
        upstream_model: "none".to_string(),
        privacy_mode: "file-upload".to_string(),
        entity_count: pseudonymized.entities,
        fresh_pii_redacted: 0,
    });

    Ok(Json(file_metadata_json(&stored)))
}

async fn get_file_metadata(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let stored = state.file_registry.get(&id).await?;
    Ok(Json(file_metadata_json(&stored)))
}

async fn delete_file(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>> {
    let deleted = state.file_registry.delete(&id).await?;
    Ok(Json(json!({
        "id": id,
        "object": "file.deleted",
        "deleted": deleted
    })))
}

async fn get_file_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let text = state.file_registry.read_pseudonymized_text(&id).await?;
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        text,
    ))
}

fn file_metadata_json(stored: &crate::file_registry::StoredFile) -> Value {
    json!({
        "id": stored.id.as_str(),
        "object": "file",
        "filename": stored.filename,
        "bytes": stored.size_bytes,
        "created_at": stored.created_at_unix,
        "purpose": "assistants",
        "content_type": stored.content_type,
        "sha256": stored.sha256_hex
    })
}

/// Start the server. Runs until SIGINT (Ctrl-C) or, on Unix, SIGTERM
/// is received — at which point axum stops accepting new connections,
/// in-flight requests drain, and the function returns Ok.
///
/// **Audit-log + LanceDB clean shutdown:** the `JsonlAuditSink` writes
/// each event synchronously (sync_data after every line), so any event
/// emitted by an in-flight handler is already flushed when the handler
/// completes — the drain phase only has to wait for handlers to finish.
/// The LanceDB connection inside `AppState` (via `PrivacyEngine`)
/// closes on the final `Arc` drop after axum returns; nothing additional
/// is needed because LanceDB's `Table` handles flush their pending
/// fragment writes on drop. The structured tracing event below makes
/// the shutdown observable for operators.
pub async fn serve(config: GatewayConfig) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|e| Error::Upstream(e.to_string()))?;
    let app = router(AppState::try_new(config)?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| Error::Upstream(e.to_string()))?;
    tracing::info!(
        target: "anno_privacy_gateway::shutdown",
        event = "stopped",
        "anno-privacy-gateway stopped cleanly"
    );
    Ok(())
}

/// Future that resolves when SIGINT (or, on Unix, SIGTERM) fires.
/// On the first signal, logs at `target = "anno_privacy_gateway::shutdown"`
/// and returns — axum stops accepting new connections and drains in-flight.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(
                target: "anno_privacy_gateway::shutdown",
                "ctrl_c listener failed: {e}"
            );
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!(
                    target: "anno_privacy_gateway::shutdown",
                    "SIGTERM listener failed: {e}"
                );
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            tracing::info!(
                target: "anno_privacy_gateway::shutdown",
                event = "draining",
                signal = "SIGINT",
                "shutdown signal received, draining in-flight requests"
            );
        }
        () = terminate => {
            tracing::info!(
                target: "anno_privacy_gateway::shutdown",
                event = "draining",
                signal = "SIGTERM",
                "shutdown signal received, draining in-flight requests"
            );
        }
    }
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

    #[test]
    fn app_state_rejects_non_loopback_listen_without_bearer_token() {
        let config = GatewayConfig {
            listen: "0.0.0.0:3000".parse().unwrap(),
            bearer_token: None,
            ..GatewayConfig::default()
        };

        let err = match AppState::try_new(config) {
            Ok(_) => panic!("non-loopback gateway without bearer token must be rejected"),
            Err(err) => err,
        };

        assert!(
            err.to_string().contains("ANNO_GATEWAY_BEARER_TOKEN"),
            "error should point operators to the missing token: {err}"
        );
    }

    #[test]
    fn app_state_allows_loopback_listen_without_bearer_token() {
        let config = GatewayConfig {
            listen: "127.0.0.1:3000".parse().unwrap(),
            bearer_token: None,
            ..GatewayConfig::default()
        };

        assert!(AppState::try_new(config).is_ok());
    }

    #[test]
    fn app_state_allows_non_loopback_listen_with_bearer_token() {
        let config = GatewayConfig {
            listen: "0.0.0.0:3000".parse().unwrap(),
            bearer_token: Some("secret".into()),
            ..GatewayConfig::default()
        };

        assert!(AppState::try_new(config).is_ok());
    }

    #[test]
    fn app_state_rejects_non_loopback_listen_with_empty_bearer_token() {
        let config = GatewayConfig {
            listen: "0.0.0.0:3000".parse().unwrap(),
            bearer_token: Some("  ".into()),
            ..GatewayConfig::default()
        };

        assert!(AppState::try_new(config).is_err());
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

    async fn mock_openai_chat(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        *state.captured.lock().await = Some(body);
        Json(json!({
            "choices": [{
                "message": {"role": "assistant", "content": "Bonjour PERSON_1"}
            }]
        }))
    }

    async fn mock_openai_stream_chat(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> axum::response::Sse<
        impl futures_util::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    > {
        *state.captured.lock().await = Some(body);
        let stream = futures_util::stream::iter(vec![
            Ok(axum::response::sse::Event::default().data(
                json!({
                    "choices": [{"delta": {"content": "Bonjour PERSON_1."}}]
                })
                .to_string(),
            )),
            Ok(axum::response::sse::Event::default().data("[DONE]")),
        ]);
        axum::response::Sse::new(stream)
    }

    fn provider_catalog_file(
        tmp: &tempfile::TempDir,
        base_url: &str,
        dpa: bool,
    ) -> std::path::PathBuf {
        let path = tmp.path().join("providers.toml");
        std::fs::write(
            &path,
            format!(
                r#"
allow_cleartext_dpa = true

[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "{base_url}"
api_key_env = ""
dpa_verified = {dpa}
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{{ id = "mistral-large-latest", upstream = "mistral-large-latest" }}]
"#
            ),
        )
        .expect("write provider catalog");
        path
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

    async fn mock_stream_leaky_messages(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> axum::response::Sse<
        impl futures_util::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    > {
        *state.captured.lock().await = Some(body);
        let stream = futures_util::stream::iter(vec![
            Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"Le fournisseur invente Jean "}}).to_string())),
            Ok(axum::response::sse::Event::default()
                .event("content_block_delta")
                .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"Martin et jean.martin@example.com."}}).to_string())),
        ]);
        axum::response::Sse::new(stream)
    }

    async fn mock_stream_malformed_messages(
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
            .data("{not-json"))]);
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

    async fn upload_text_file(addr: SocketAddr, filename: &str, text: &str) -> String {
        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(text.as_bytes().to_vec())
                .file_name(filename.to_string())
                .mime_str("text/plain")
                .unwrap(),
        );
        let uploaded: serde_json::Value = reqwest::Client::new()
            .post(format!("http://{addr}/v1/files"))
            .multipart(form)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        uploaded["id"].as_str().unwrap().to_string()
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
    async fn provider_router_pseudonymizes_before_openai_upstream() {
        let tmp = tempfile::TempDir::new().unwrap();
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/chat/completions", post(mock_openai_chat))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;
        let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);

        let config = GatewayConfig {
            provider_catalog_path: Some(catalog_path),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let response: Value = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "anno/mistral/mistral-large-latest:pseudonymized",
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
    async fn provider_router_cleartext_dpa_sends_cleartext_to_verified_provider() {
        let tmp = tempfile::TempDir::new().unwrap();
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/chat/completions", post(mock_openai_chat))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;
        let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);

        let config = GatewayConfig {
            provider_catalog_path: Some(catalog_path),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let status = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "anno/mistral/mistral-large-latest:cleartext-dpa",
                "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
            }))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, reqwest::StatusCode::OK);
        let upstream_body = captured.lock().await.clone().expect("upstream called");
        let upstream_text = serde_json::to_string(&upstream_body).unwrap();
        assert!(upstream_text.contains("Marie Dupont"));
    }

    #[tokio::test]
    async fn provider_router_file_document_pseudonymized_sends_no_cleartext() {
        let tmp = tempfile::TempDir::new().unwrap();
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/chat/completions", post(mock_openai_chat))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;
        let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);
        let config = GatewayConfig {
            provider_catalog_path: Some(catalog_path),
            file_store_dir: tmp.path().join("files"),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let file_id = upload_text_file(gateway_addr, "notes.txt", "Bonjour Marie Dupont").await;

        let response: serde_json::Value = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "anno/mistral/mistral-large-latest:pseudonymized",
                "messages": [{
                    "role": "user",
                    "content": [{
                        "type": "document",
                        "source": {"type": "file", "file_id": file_id},
                        "title": "notes.txt"
                    }]
                }]
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
    async fn provider_router_file_document_cleartext_dpa_sends_cleartext_to_verified_provider() {
        let tmp = tempfile::TempDir::new().unwrap();
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/chat/completions", post(mock_openai_chat))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;
        let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);
        let config = GatewayConfig {
            provider_catalog_path: Some(catalog_path),
            file_store_dir: tmp.path().join("files"),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let file_id = upload_text_file(gateway_addr, "notes.txt", "Bonjour Marie Dupont").await;

        let status = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "anno/mistral/mistral-large-latest:cleartext-dpa",
                "messages": [{
                    "role": "user",
                    "content": [{
                        "type": "document",
                        "source": {"type": "file", "file_id": file_id},
                        "title": "notes.txt"
                    }]
                }]
            }))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, reqwest::StatusCode::OK);
        let upstream_body = captured.lock().await.clone().expect("upstream called");
        let upstream_text = serde_json::to_string(&upstream_body).unwrap();
        assert!(upstream_text.contains("Marie Dupont"));
    }

    #[tokio::test]
    async fn provider_router_stream_rehydrates_text() {
        let tmp = tempfile::TempDir::new().unwrap();
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/chat/completions", post(mock_openai_stream_chat))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;
        let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);

        let config = GatewayConfig {
            provider_catalog_path: Some(catalog_path),
            streaming: crate::config::StreamingMode::Enabled,
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let body = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/messages"))
            .json(&json!({
                "model": "anno/mistral/mistral-large-latest:pseudonymized",
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
    async fn models_route_uses_provider_catalog_when_configured() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let catalog_path = tmp.path().join("providers.toml");
        std::fs::write(
            &catalog_path,
            r#"
allow_cleartext_dpa = true
[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "https://api.mistral.ai/v1"
api_key_env = "MISTRAL_API_KEY"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{ id = "mistral-large-latest", upstream = "mistral-large-latest" }]
"#,
        )
        .expect("write catalog");

        let config = GatewayConfig {
            provider_catalog_path: Some(catalog_path),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let response: Value = reqwest::Client::new()
            .get(format!("http://{gateway_addr}/v1/models"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["type"], "list");
        assert_eq!(response["data"].as_array().expect("data").len(), 2);
        assert_eq!(
            response["data"][0]["id"],
            "anno/mistral/mistral-large-latest:cleartext-dpa"
        );
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
    async fn stream_buffered_scan_redacts_fresh_pii_split_across_chunks() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_leaky_messages))
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

        assert!(!body.contains("Jean Martin"));
        assert!(!body.contains("jean.martin@example.com"));
        assert!(body.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn stream_token_rehydrate_only_does_not_scan_fresh_pii() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_leaky_messages))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            streaming: crate::config::StreamingMode::Enabled,
            stream_privacy: crate::config::StreamPrivacyMode::TokenRehydrateOnly,
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

        assert!(body.contains("Jean Martin"));
        assert!(body.contains("jean.martin@example.com"));
    }

    #[tokio::test]
    async fn malformed_stream_emits_error_event() {
        let captured = Arc::new(Mutex::new(None));
        let upstream = Router::new()
            .route("/v1/messages", post(mock_stream_malformed_messages))
            .with_state(MockState {
                captured: Arc::clone(&captured),
            });
        let upstream_addr = spawn(upstream).await;

        let config = GatewayConfig {
            upstream_anthropic_base: format!("http://{upstream_addr}"),
            streaming: crate::config::StreamingMode::Enabled,
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
        assert!(body.contains("stream_parse_error"));
    }

    #[tokio::test]
    async fn files_api_uploads_text_and_returns_metadata_without_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = GatewayConfig {
            file_store_dir: tmp.path().join("files"),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;

        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes("Bonjour Marie Dupont".as_bytes().to_vec())
                .file_name("notes.txt")
                .mime_str("text/plain")
                .unwrap(),
        );
        let response: serde_json::Value = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/files"))
            .multipart(form)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert!(response["id"].as_str().unwrap().starts_with("anno_file_"));
        assert_eq!(response["object"], "file");
        assert_eq!(response["filename"], "notes.txt");
        assert!(response.get("text").is_none());
    }

    #[tokio::test]
    async fn files_api_content_returns_pseudonymized_text_only() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = GatewayConfig {
            file_store_dir: tmp.path().join("files"),
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;
        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes("Bonjour Marie Dupont".as_bytes().to_vec())
                .file_name("notes.txt")
                .mime_str("text/plain")
                .unwrap(),
        );
        let uploaded: serde_json::Value = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/files"))
            .multipart(form)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let id = uploaded["id"].as_str().unwrap();

        let content = reqwest::Client::new()
            .get(format!("http://{gateway_addr}/v1/files/{id}/content"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(content.contains("PERSON_"));
        assert!(!content.contains("Marie Dupont"));
    }

    #[tokio::test]
    async fn files_api_allows_configured_upload_above_default_body_limit() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bytes = vec![b'a'; 3 * 1024 * 1024];
        let config = GatewayConfig {
            file_store_dir: tmp.path().join("files"),
            file_max_bytes: 4 * 1024 * 1024,
            ..GatewayConfig::default()
        };
        let gateway_addr = spawn(router(AppState::new(config))).await;
        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(bytes.clone())
                .file_name("large.txt")
                .mime_str("text/plain")
                .unwrap(),
        );

        let response: serde_json::Value = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/v1/files"))
            .multipart(form)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["bytes"], bytes.len());
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
