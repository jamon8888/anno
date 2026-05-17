//! GDPR Art. 15 / 17 / 20 handlers — find, forget, export.

use crate::audit::AuditEvent;
use crate::server::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};

/// Request body for `find` and `forget`.
#[derive(Debug, Deserialize)]
pub struct SubjectRefBody {
    /// The subject reference — either the original sensitive value or the
    /// pseudo-token. The vault searches both directions.
    pub subject_ref: String,
}

/// One match returned by the `find` and `export` handlers.
#[derive(Debug, Serialize)]
pub struct SubjectMatch {
    /// Original (sensitive) value.
    pub original: String,
    /// Pseudo-token in the vault.
    pub token: String,
    /// Category key (e.g. `"Person"`, `"Email"`, `"NIR"`).
    pub category: String,
}

/// Response body for `find` and JSON-formatted `export`.
#[derive(Debug, Serialize)]
pub struct FindResponse {
    /// The reference the caller looked up.
    pub subject_ref: String,
    /// Zero or more matches.
    pub matches: Vec<SubjectMatch>,
}

/// Receipt returned by `forget`.
#[derive(Debug, Serialize)]
pub struct ErasureReceipt {
    /// Whatever the caller passed (original or token).
    pub subject_ref: String,
    /// Number of vault mappings removed (0 if subject was unknown).
    pub mappings_removed: usize,
    /// Token that was retired, if a mapping was found.
    pub token: Option<String>,
    /// Category of the retired mapping, if any.
    pub category: Option<String>,
    /// UTC timestamp of the operation, RFC 3339.
    pub executed_at: String,
}

/// Query parameters for `export`.
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// `"json"` (default) or `"csv"`.
    pub format: Option<String>,
}

/// `POST /v1/subjects/find` — RGPD Art. 15 (right of access).
pub async fn find(
    State(state): State<AppState>,
    Json(body): Json<SubjectRefBody>,
) -> Json<FindResponse> {
    let privacy = state.privacy().lock().await;
    let matches = privacy
        .vault()
        .find(&body.subject_ref)
        .into_iter()
        .map(|m| SubjectMatch {
            original: m.original,
            token: m.token,
            category: format!("{:?}", m.category),
        })
        .collect();
    Json(FindResponse {
        subject_ref: body.subject_ref,
        matches,
    })
}

/// `POST /v1/subjects/forget` — RGPD Art. 17 (right to erasure). Emits an
/// audit event after every call (including no-op forgets) so downstream
/// review can confirm the request was handled.
pub async fn forget(
    State(state): State<AppState>,
    Json(body): Json<SubjectRefBody>,
) -> Json<ErasureReceipt> {
    let mut privacy = state.privacy().lock().await;
    let removed = privacy.vault_mut().remove(&body.subject_ref);
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let receipt = match removed {
        Some(m) => ErasureReceipt {
            subject_ref: body.subject_ref.clone(),
            mappings_removed: 1,
            token: Some(m.token),
            category: Some(format!("{:?}", m.category)),
            executed_at: now,
        },
        None => ErasureReceipt {
            subject_ref: body.subject_ref.clone(),
            mappings_removed: 0,
            token: None,
            category: None,
            executed_at: now,
        },
    };
    drop(privacy);

    state.audit().record(AuditEvent {
        request_id: format!("forget:{}", receipt.executed_at),
        provider_profile: state.config().provider_profile.clone(),
        entity_count: receipt.mappings_removed,
        fresh_pii_redacted: 0,
    });

    Json(receipt)
}

/// `GET /v1/subjects/{subject_ref}/export?format=json|csv` — RGPD Art. 20
/// (right to portability). Default format is JSON.
pub async fn export(
    State(state): State<AppState>,
    Path(subject_ref): Path<String>,
    Query(q): Query<ExportQuery>,
) -> Result<(HeaderMap, Vec<u8>), StatusCode> {
    let privacy = state.privacy().lock().await;
    let matches: Vec<SubjectMatch> = privacy
        .vault()
        .find(&subject_ref)
        .into_iter()
        .map(|m| SubjectMatch {
            original: m.original,
            token: m.token,
            category: format!("{:?}", m.category),
        })
        .collect();
    drop(privacy);

    let mut headers = HeaderMap::new();
    let body = match q.format.as_deref().unwrap_or("json") {
        "csv" => {
            headers.insert(header::CONTENT_TYPE, "text/csv".parse().unwrap());
            let mut buf = Vec::new();
            {
                let mut w = csv::Writer::from_writer(&mut buf);
                w.write_record(["original", "token", "category"])
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                for m in &matches {
                    w.write_record([&m.original, &m.token, &m.category])
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                }
                w.flush().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            buf
        }
        _ => {
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            serde_json::to_vec_pretty(&FindResponse {
                subject_ref,
                matches,
            })
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };
    Ok((headers, body))
}

#[cfg(test)]
mod tests {
    use crate::server::{router, AppState};
    use crate::GatewayConfig;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn cfg() -> GatewayConfig {
        GatewayConfig {
            bearer_token: Some("secret".into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn forget_unknown_subject_returns_zero_receipt() {
        let app = router(AppState::try_new(cfg()).unwrap());
        let req = Request::builder()
            .method("POST")
            .uri("/v1/subjects/forget")
            .header("authorization", "Bearer secret")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"subject_ref":"nobody"}"#))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), 200);
        let body = axum::body::to_bytes(res.into_body(), 8 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["mappings_removed"].as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn find_route_rejects_missing_bearer() {
        let app = router(AppState::try_new(cfg()).unwrap());
        let req = Request::builder()
            .method("POST")
            .uri("/v1/subjects/find")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"subject_ref":"x"}"#))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn health_is_exempt_from_bearer_auth() {
        let app = router(AppState::try_new(cfg()).unwrap());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
