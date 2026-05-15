# anno-privacy-gateway v0.4 — Deployer Guide

> **Audience:** the cabinet's IT operator deploying the privacy gateway on-premise.
>
> **Scope:** v0.4 (`crates/anno-privacy-gateway`) — Anthropic-compatible privacy gateway with bearer-token auth, GDPR subject-rights routes, and tamper-evident JSONL audit register.
>
> **Companion documents:** `docs/superpowers/specs/2026-05-15-anno-rag-dpia-v1.md` (DPIA), `docs/superpowers/specs/2026-05-15-anno-rag-ai-act-position-v1.md` (AI Act position), `docs/superpowers/specs/2026-05-15-anno-rag-rgpd-aiact-readiness-design.md` (readiness spec).

---

## 1. What the gateway does

The privacy gateway exposes an Anthropic-compatible HTTP surface on a single port. It owns the **privacy boundary**: every request is pseudonymised before egress to the upstream LLM provider, every response is rehydrated before returning to the caller. Three additional concerns it owns in v0.4:

1. **Bearer-token auth** on every `/v1/*` route. `/health` stays public for liveness probes.
2. **GDPR subject-rights routes**: `POST /v1/subjects/find` (Art. 15 access), `POST /v1/subjects/forget` (Art. 17 erasure with vault cascade), `GET /v1/subjects/{ref}/export?format=json|csv` (Art. 20 portability).
3. **Tamper-evident Art. 30 audit register**: append-only `YYYY-MM-DD.jsonl` with a sha256 hash chain over canonicalised event bytes, plus a daily HMAC-SHA256 signature file. The chain is reproducible off-line by a third-party verifier.

---

## 2. Quick start (loopback profile)

For dev / single-user / single-host deployments. **Not for multi-user LAN exposure — see §3.**

```bash
# 1. Build the binary.
cargo build --release -p anno-privacy-gateway --bin anno-privacy-gateway

# 2. Set the minimum env. Loopback + no auth + no audit = OK for dev only.
export ANNO_GATEWAY_LISTEN=127.0.0.1:3000
export ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE=http://127.0.0.1:3100

# 3. Run.
target/release/anno-privacy-gateway
```

The gateway is up. Verify with:

```bash
curl -sS http://127.0.0.1:3000/health
# {"status":"ok","provider_profile":"global_anonymized","auto_rehydrate":true}
```

In this profile:
- **No bearer token configured** → the auth middleware is a no-op. **Anyone with network access to `127.0.0.1:3000` can use the gateway.** Loopback is the security boundary.
- **No audit directory configured** → a `NoopAuditSink` is wired. **Nothing is recorded.** Acceptable for dev; **NOT acceptable for production.**

---

## 3. Production profile

For multi-user / multi-host / public-facing deployments. All five env vars below are **required**.

### 3.1 Generate keys

```bash
# Bearer token — 32 random bytes, base64url-encoded (43 chars).
openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
# → e.g. "qX9aB-2Kc4d_eF7g_HiJ8kL9mNoPqRsTuVwXyZaBcDe"

# Audit HMAC key — 32 random bytes, hex (64 chars).
openssl rand -hex 32
# → e.g. "1a2b3c4d5e6f70...64chars"
```

**Custody:**
- Bearer token → distributed to authorised callers (Cowork client config, MCP server config, internal scripts). Treat as a credential — rotate every 90 days.
- Audit HMAC key → **stays on the gateway host AND with the DPO**. The DPO needs it to verify the audit chain off-line. Loss of the key = loss of the cryptographic audit guarantee for entries written under that key.

### 3.2 Environment variables

| Env var | Required | Description |
|---|---|---|
| `ANNO_GATEWAY_LISTEN` | yes | TCP listen address. **Use `127.0.0.1:3000` unless fronted by a reverse proxy** (nginx, Caddy, Traefik). |
| `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE` | yes | Base URL of the Anthropic-compatible upstream sidecar (e.g. `http://127.0.0.1:3100`). |
| `ANNO_GATEWAY_VAULT_PATH` | yes | Filesystem path to the persistent cloakpipe vault. Parent directory must exist; file is created on first write. |
| `ANNO_GATEWAY_VAULT_KEY_HEX` | yes | 64 hex chars = 32-byte AES-256-GCM key. Source from KMS / OS keyring; **never commit**. |
| `ANNO_GATEWAY_BEARER_TOKEN` | yes (production) | Bearer token clients must present. When unset, auth middleware is a no-op. |
| `ANNO_GATEWAY_AUDIT_DIR` | yes (production) | Directory for the daily JSONL + `.sig` files. Will be created if missing. Recommend a **WORM / append-only** mount (see §5.3). |
| `ANNO_GATEWAY_AUDIT_HMAC_KEY_HEX` | yes (production) | 64 hex chars = HMAC-SHA256 key for the daily signature. Distinct from the vault key. |
| `ANNO_GATEWAY_PROVIDER_PROFILE` | no | Label written to every audit event (default `global_anonymized`). |
| `ANNO_GATEWAY_STREAMING` | no | `enabled` / `disabled` (default `disabled`). |
| `ANNO_GATEWAY_STREAM_PRIVACY` | no | `buffered_scan` / `token_rehydrate_only` (default `buffered_scan`). |
| `ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS` | no | Default 4096. |
| `ANNO_GATEWAY_STREAM_MAX_BUFFER_MS` | no | Default 750. |

### 3.3 Sample systemd unit

```ini
# /etc/systemd/system/anno-privacy-gateway.service
[Unit]
Description=anno-privacy-gateway v0.4
After=network.target
Wants=anno-upstream-sidecar.service

[Service]
User=anno
Group=anno
WorkingDirectory=/opt/anno
ExecStart=/opt/anno/bin/anno-privacy-gateway
Restart=on-failure
RestartSec=5s

# Auth + audit + vault. Source from a sealed env file under /etc/anno/gateway.env (mode 0400, owner anno).
EnvironmentFile=/etc/anno/gateway.env

# Sandboxing.
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/anno/vault /var/log/anno/audit

[Install]
WantedBy=multi-user.target
```

The `/etc/anno/gateway.env` file:

```
ANNO_GATEWAY_LISTEN=127.0.0.1:3000
ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE=http://127.0.0.1:3100
ANNO_GATEWAY_VAULT_PATH=/var/lib/anno/vault/cloakpipe.bin
ANNO_GATEWAY_VAULT_KEY_HEX=<64 hex chars>
ANNO_GATEWAY_BEARER_TOKEN=<43+ char token>
ANNO_GATEWAY_AUDIT_DIR=/var/log/anno/audit
ANNO_GATEWAY_AUDIT_HMAC_KEY_HEX=<64 hex chars, distinct from vault key>
ANNO_GATEWAY_PROVIDER_PROFILE=cabinet-on-prem
```

`chmod 0400 /etc/anno/gateway.env && chown anno:anno /etc/anno/gateway.env`.

---

## 4. Subject-rights API

All three routes require `Authorization: Bearer <token>`. `/health` does not.

### 4.1 Find (Art. 15 — right of access)

```bash
curl -sS -X POST http://127.0.0.1:3000/v1/subjects/find \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"subject_ref":"Marie Dupont"}'
```

`subject_ref` can be either the original sensitive value (e.g. `"Marie Dupont"`) or the pseudo-token (`"PERSON_42"`). Returns:

```json
{
  "subject_ref": "Marie Dupont",
  "matches": [
    {
      "original": "Marie Dupont",
      "token": "PERSON_42",
      "category": "Person"
    }
  ]
}
```

### 4.2 Forget (Art. 17 — right to erasure)

```bash
curl -sS -X POST http://127.0.0.1:3000/v1/subjects/forget \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"subject_ref":"Marie Dupont"}'
```

Returns an `ErasureReceipt`:

```json
{
  "subject_ref": "Marie Dupont",
  "mappings_removed": 1,
  "token": "PERSON_42",
  "category": "Person",
  "executed_at": "2026-05-15T13:42:11Z"
}
```

The vault entry is gone immediately. Any text containing the retired token will subsequently rehydrate as the token itself (no original to substitute).

**Idempotent.** A second call with the same `subject_ref` returns `"mappings_removed": 0`, with `token` and `category` null.

**Every call emits an audit event**, including no-op forgets. This lets the cabinet prove the request was handled even when there was nothing to delete.

### 4.3 Export (Art. 20 — right to portability)

```bash
# JSON
curl -sS -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:3000/v1/subjects/Marie%20Dupont/export?format=json"

# CSV
curl -sS -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:3000/v1/subjects/Marie%20Dupont/export?format=csv"
```

CSV header: `original,token,category`.

---

## 5. Audit register

### 5.1 What gets recorded

Every successful request to a `/v1/*` route emits one audit event. The event shape (canonical key-sorted JSON):

```json
{
  "entity_count": 3,
  "fresh_pii_redacted": 0,
  "provider_profile": "cabinet-on-prem",
  "request_id": "forget:2026-05-15T13:42:11Z"
}
```

**The event NEVER contains:** cleartext prompts, rehydrated answers, vault contents, or any string that could re-identify a subject. The minimal payload is intentional — the audit chain is meant to be tamper-evident, not a content log.

### 5.2 Daily file layout

```
$AUDIT_DIR/
├── 2026-05-15.jsonl     # one line per event, with prev_hash + this_hash
├── 2026-05-15.sig       # HMAC-SHA256 of the day's final chain head
├── 2026-05-16.jsonl
├── 2026-05-16.sig
└── ...
```

**One line per event:**

```json
{
  "ts": "2026-05-15T13:42:11.456Z",
  "event": { "entity_count": 0, "fresh_pii_redacted": 0,
             "provider_profile": "cabinet-on-prem",
             "request_id": "forget:..." },
  "prev_hash": "0000000000000000000000000000000000000000000000000000000000000000",
  "this_hash": "2ebafea5baf5a3992a252e8c1a283a018f5fda9a19a6828c81a763ff95c8e00b"
}
```

`this_hash = sha256(prev_hash_bytes || canonical_event_json_bytes)`.

`prev_hash` is 64 hex zeros for the first line of each UTC day.

`.sig` is `HMAC-SHA256(audit_hmac_key, last_this_hash)` in hex (64 chars).

**Chain head recovers across restarts** — on startup the sink reads the last line of today's JSONL and resumes chaining.

### 5.3 Recommended filesystem properties

- **Append-only** mount: `chattr +a $AUDIT_DIR` on Linux ext4, or a WORM-mode filesystem (FUSE / S3 Object Lock / NFSv4 ACL). Prevents an attacker who compromises the gateway host from rewriting history.
- **Mirror to an offsite write-only sink**: every event is also forwarded to a remote SIEM / immutable log service. Detects host-side deletions.
- **Rotation:** the JSONL files don't rotate themselves. Operator-managed retention: 5 years (Art. 30 register norm), then archive.

### 5.4 Verifying the chain off-line

The DPO (or an external auditor) can verify the chain without running the gateway. Save the script as `verify_audit.py`:

```python
#!/usr/bin/env python3
"""Replay an anno-privacy-gateway audit JSONL and verify the chain."""
import hashlib, hmac, json, sys

if len(sys.argv) != 4:
    print("usage: verify_audit.py YYYY-MM-DD.jsonl YYYY-MM-DD.sig audit_hmac_key.hex")
    sys.exit(2)

jsonl_path, sig_path, key_path = sys.argv[1], sys.argv[2], sys.argv[3]
key = bytes.fromhex(open(key_path).read().strip())

prev = "0" * 64
last_this = None
with open(jsonl_path) as f:
    for n, line in enumerate(f, 1):
        obj = json.loads(line)
        canonical = json.dumps(obj["event"], sort_keys=True,
                               separators=(",", ":")).encode()
        h = hashlib.sha256()
        h.update(bytes.fromhex(prev))
        h.update(canonical)
        expected = h.hexdigest()
        if obj["prev_hash"] != prev:
            print(f"BROKEN at line {n}: prev_hash mismatch")
            print(f"  expected: {prev}")
            print(f"  got:      {obj['prev_hash']}")
            sys.exit(1)
        if obj["this_hash"] != expected:
            print(f"BROKEN at line {n}: this_hash mismatch")
            print(f"  expected: {expected}")
            print(f"  got:      {obj['this_hash']}")
            sys.exit(1)
        prev = expected
        last_this = expected

# Check the .sig over the final chain head.
expected_sig = hmac.new(key, last_this.encode(), hashlib.sha256).hexdigest()
got_sig = open(sig_path).read().strip()
if expected_sig != got_sig:
    print(f"BROKEN: .sig mismatch")
    print(f"  expected: {expected_sig}")
    print(f"  got:      {got_sig}")
    sys.exit(1)

print(f"OK — {n} events, final chain head {last_this[:16]}...")
print(f"     HMAC verified.")
```

Run:

```bash
python3 verify_audit.py \
  /var/log/anno/audit/2026-05-15.jsonl \
  /var/log/anno/audit/2026-05-15.sig \
  ~/audit-hmac-key.hex
# OK — 17 events, final chain head 2ebafea5baf5a399...
#      HMAC verified.
```

If the script reports `BROKEN`, the cabinet must:
1. Treat the day as **potentially compromised** for compliance purposes.
2. Trigger the incident-response runbook (DPO notification, possible Art. 33 GDPR breach notification within 72 h).
3. Restore from the offsite mirror (§5.3) and re-verify.

---

## 6. Operations runbook

### 6.1 Key rotation

| Key | Cadence | Procedure |
|---|---|---|
| Bearer token | 90 days, or on staff offboarding | Generate new token; distribute to callers; restart gateway; old token invalid immediately. |
| Vault AES-256-GCM key | 12 months, or on compromise | Re-encrypt vault offline (see `cloakpipe` re-key procedure); update env; restart. |
| Audit HMAC key | Never, **unless compromised**. Rotation breaks off-line verifiability for all prior days. | If forced: archive prior days + their key; new key for new days; document the cut-over in the audit-of-audit. |

### 6.2 Health monitoring

Probe `/health` on the listen port. Expected: 200 OK with the JSON `{status:"ok", ...}`. Failure → process is up but degraded; restart and inspect logs.

### 6.3 Logs

Tracing events of interest are emitted at the `anno_privacy_gateway::*` targets and at `anno_rag::memory::audit` (for memory operations). Pipe stderr to journald or a structured log collector.

### 6.4 Backup

| Asset | Backup target | Cadence |
|---|---|---|
| `$ANNO_GATEWAY_VAULT_PATH` | offsite encrypted | Daily |
| `$ANNO_GATEWAY_AUDIT_DIR` | offsite WORM mirror | Real-time stream + nightly full sync |
| Env file `/etc/anno/gateway.env` | sealed vault (1Password / Bitwarden / pass) | On change |

### 6.5 Incident response

Trigger: a `/v1/*` request fails auth more than `N` times within `T` (operator-configured rate-limit alarm — recommended SIEM rule, not in v0.4 code).

1. Block the source IP at the reverse-proxy / firewall.
2. Inspect the audit log for the period — any successful authenticated requests from unusual subjects?
3. If yes, treat as a likely credential compromise → rotate bearer token immediately, notify users, run §5.4 verification.
4. Report to DPO within 24 h. If personal-data exposure is plausible, Art. 33 GDPR notification within 72 h.

---

## 7. Known limits (v0.4)

| Limit | Mitigation | Roadmap |
|---|---|---|
| Bearer-auth only — no mTLS | Front with a reverse proxy that adds mTLS for non-loopback deployments. | v0.5 candidate: native mTLS support. |
| No rate-limiting | Reverse-proxy / firewall responsibility. | v0.5 candidate: native token-bucket per bearer. |
| Audit `prev_hash` reset at UTC midnight | Acceptable; chain still verifiable per-day. | v0.6 candidate: multi-day rolling chain. |
| `bearer_token=None` is a no-op | Intentional for loopback dev; document explicitly in this guide. | No change planned. |
| Vault key sourced from env var (`_VAULT_KEY_HEX`) | Acceptable for v0.4; integrate KMS in v0.5+. | U6 in readiness spec, v0.5–v0.6. |
| At-rest encryption for LanceDB index dir is operator responsibility | BitLocker / LUKS on the host. | U5 in readiness spec, v0.5. |

---

## 8. Compliance trace

| Obligation | Where met | Evidence |
|---|---|---|
| GDPR Art. 15 (access) | `POST /v1/subjects/find` | §4.1 |
| GDPR Art. 17 (erasure) | `POST /v1/subjects/forget` + audit emit | §4.2 + §5 |
| GDPR Art. 20 (portability) | `GET /v1/subjects/{ref}/export` | §4.3 |
| GDPR Art. 30 (records of processing) | `JsonlAuditSink` daily JSONL + `.sig` | §5 |
| GDPR Art. 32 (security of processing) | bearer auth + AES-256-GCM vault + sha256/HMAC audit chain | §3 + §5 |
| AI Act Art. 50 (transparency) | Cabinet engagement-letter clause | See AI Act position paper §4.1 |
| AI Act Art. 13 (provider instructions to deployer) | **this document** | — |
| AI Act Art. 26 (deployer obligations, non-high-risk subset) | Cabinet operational practice | See AI Act position paper §4.3 |

---

## 9. Document control

| Version | Date | Author | Change |
|---|---|---|---|
| v1 | 2026-05-15 | anno team | Initial — covers v0.4 gateway + v1 DPIA + v1 AI Act position. |
| [next] | TBD | anno team | Update on v0.5 mTLS + rate-limit landing. |
