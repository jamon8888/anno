# anno-privacy-gateway Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### v0.4 — GDPR rights routes + persistent audit + bearer auth

### Added
- **`POST /v1/subjects/find`** — RGPD Art. 15 (right of access). Body
  `{ "subject_ref": "..." }`; returns 0 or 1 matches with
  `original`/`token`/`category`. Behind bearer-auth middleware.
- **`POST /v1/subjects/forget`** — RGPD Art. 17 (right to erasure).
  Idempotent. Emits an `AuditEvent` even on no-op forgets so downstream
  review can confirm the request was handled.
- **`GET /v1/subjects/{subject_ref}/export?format=json|csv`** — RGPD Art. 20
  (portability). Defaults to JSON; CSV emits `original,token,category`
  with header.
- **`JsonlAuditSink`** — append-only `YYYY-MM-DD.jsonl` under
  `config.audit_dir` with per-line sha256 hash chain. Each line is
  `{ts, event, prev_hash, this_hash}` where
  `this_hash = sha256(prev_hash_bytes || serde_json_canonical(event))`.
  The event bytes are round-tripped through `serde_json::Value` before
  hashing so the chain is reproducible off-line by a third-party verifier
  (key-sorted output). A daily `YYYY-MM-DD.sig` file is rewritten on
  every event and contains `HMAC-SHA256(audit_hmac_key, this_hash)` in
  hex. Chain head recovers from disk across restarts.
- **`auth::require_bearer`** middleware — constant-time-compare
  (`subtle::ct_eq`) against `config.bearer_token`. `/health` stays
  public. When no token is configured the middleware is a no-op so
  loopback dev/test flows keep working; operators exposing the gateway
  beyond loopback MUST set `ANNO_GATEWAY_BEARER_TOKEN`.
- **Config fields** `audit_dir`, `audit_hmac_key_hex`, `bearer_token`,
  sourced via `ANNO_GATEWAY_AUDIT_DIR` / `_AUDIT_HMAC_KEY_HEX` /
  `_BEARER_TOKEN` env vars.
- **`PrivacyEngine::vault` / `vault_mut`** accessors so subject handlers
  can drive forget/find through the same vault the request path uses.

### Changed
- **`AppState`** now owns an `Arc<dyn AuditSink>` resolved at startup
  from config (`JsonlAuditSink` when both `audit_dir` and
  `audit_hmac_key_hex` are set, `NoopAuditSink` otherwise) and exposes
  `privacy()` / `audit()` / `config()` / `bearer_token()` accessors.
- **`router(state)`** splits into public (`/health`) and protected
  (`/v1/*`) sub-routers; protected layer carries the bearer middleware.

### Dependencies added
`csv` 1, `hex` 0.4, `hmac` 0.12, `sha2` (workspace), `subtle` 2,
`time` 0.3 (`formatting`, `macros`), `tower` 0.5, `tower-http` 0.6
(`auth`).
