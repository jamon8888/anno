//! End-to-end GDPR test: forget → audit chain verifies off-line.
//!
//! Verifies that:
//! 1. Three `/v1/subjects/forget` requests succeed and authenticate.
//! 2. Each one writes a JSONL line under the configured audit dir.
//! 3. The sha256 chain holds (line N's `prev_hash` = line N-1's `this_hash`).
//! 4. `this_hash = sha256(prev_hash_bytes || serde_json(event))` per line.

use anno_privacy_gateway::server::{router, AppState};
use anno_privacy_gateway::GatewayConfig;
use axum::body::Body;
use axum::http::Request;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

fn cfg(audit_dir: &std::path::Path, hmac_key_hex: &str) -> GatewayConfig {
    GatewayConfig {
        bearer_token: Some("secret".into()),
        audit_dir: Some(audit_dir.to_path_buf()),
        audit_hmac_key_hex: Some(hmac_key_hex.to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn forget_persists_to_audit_chain() {
    let tmp = tempfile::TempDir::new().unwrap();
    let key_hex = hex::encode([7u8; 32]);
    let app = router(AppState::try_new(cfg(tmp.path(), &key_hex)).unwrap());

    for i in 0..3 {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/subjects/forget")
            .header("authorization", "Bearer secret")
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"subject_ref":"s{i}"}}"#)))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), 200, "forget request {i} succeeded");
    }

    let today = time::OffsetDateTime::now_utc()
        .format(&time::macros::format_description!("[year]-[month]-[day]"))
        .unwrap();
    let body = std::fs::read_to_string(tmp.path().join(format!("{today}.jsonl"))).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 3, "three audit events written");

    let mut prev = "0".repeat(64);
    for (i, line) in lines.iter().enumerate() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(
            v["prev_hash"].as_str().unwrap(),
            prev,
            "line {i} prev_hash chains correctly"
        );
        let event_bytes = serde_json::to_vec(&v["event"]).unwrap();
        let mut h = Sha256::new();
        h.update(hex::decode(&prev).unwrap());
        h.update(&event_bytes);
        let expected = hex::encode(h.finalize());
        assert_eq!(
            v["this_hash"].as_str().unwrap(),
            expected,
            "line {i} this_hash matches sha256(prev||event)"
        );
        prev = expected;
    }

    // The .sig file holds HMAC of the final chain head; it must be 64 hex chars.
    let sig = std::fs::read_to_string(tmp.path().join(format!("{today}.sig"))).unwrap();
    assert_eq!(sig.trim().len(), 64);
}

#[tokio::test]
async fn forget_without_bearer_does_not_write_audit() {
    let tmp = tempfile::TempDir::new().unwrap();
    let key_hex = hex::encode([8u8; 32]);
    let app = router(AppState::try_new(cfg(tmp.path(), &key_hex)).unwrap());

    // No Authorization header — auth middleware should reject before the
    // handler runs, so no audit event must be written.
    let req = Request::builder()
        .method("POST")
        .uri("/v1/subjects/forget")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"subject_ref":"s"}"#))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);

    let today = time::OffsetDateTime::now_utc()
        .format(&time::macros::format_description!("[year]-[month]-[day]"))
        .unwrap();
    assert!(
        !tmp.path().join(format!("{today}.jsonl")).exists(),
        "no JSONL file should have been created"
    );
}
