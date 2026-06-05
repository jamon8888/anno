//! Bearer-token authentication middleware.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

use crate::server::AppState;

/// Reject requests without a valid `Authorization: Bearer <token>` header.
/// `/health` is exempt — operational checks should not require credentials.
///
/// # Errors
/// Returns `401 Unauthorized` if the header is missing or the token mismatches.
/// When no bearer token is configured, this middleware is a no-op. Startup
/// validation rejects that configuration for non-loopback listeners.
pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if req.uri().path() == "/health" {
        return Ok(next.run(req).await);
    }
    // When no token is configured, the middleware is a no-op. Operators
    // who run on loopback may rely on the network boundary; deployers who
    // expose the gateway publicly MUST set `bearer_token` (env
    // ANNO_GATEWAY_BEARER_TOKEN) — the v0.4 readiness spec calls this out
    // as gap G6 and the deployer guide must surface the requirement.
    let Some(configured) = state.bearer_token() else {
        return Ok(next.run(req).await);
    };
    let header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let provided = header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let eq: bool = provided.as_bytes().ct_eq(configured.as_bytes()).into();
    if !eq {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}
