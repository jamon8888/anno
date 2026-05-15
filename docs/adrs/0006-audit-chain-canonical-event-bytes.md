# ADR-006 — Audit hash chain canonicalises event bytes via Value round-trip

**Status:** Accepted (v0.4) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

`JsonlAuditSink` writes each event as a JSONL line with a sha256 hash chain (`this_hash = sha256(prev_hash_bytes || event_bytes)`). For the chain to be verifiable off-line by a third party (DPO, external auditor), the **event_bytes** must be reproducible from the JSONL line alone.

The first implementation serialised the `AuditEvent` struct directly: `serde_json::to_vec(event)`. That produced bytes in **field-declaration order**: `{request_id, provider_profile, entity_count, fresh_pii_redacted}`. The e2e test then re-read the JSONL, parsed each line back into `serde_json::Value`, and re-serialised the `event` field to recompute the hash. But `serde_json::Value::Object` is backed by `BTreeMap` (alphabetical) when the `preserve_order` feature is off — which it is by default. The recomputed bytes came out **alphabetical**: `{entity_count, fresh_pii_redacted, provider_profile, request_id}`. Different bytes → different hash → chain verification fails.

This was caught by the v0.4 e2e test `forget_persists_to_audit_chain`. We could fix it either way:

1. Switch the e2e test to re-hash from the struct (require the verifier to deserialise into `AuditEvent` first). Tightly couples the verifier to the Rust type.
2. Canonicalise the event bytes the same way both at write time and at verify time — round-trip through `serde_json::Value` before hashing.

## Decision

**Canonicalise at write time** by serialising the `AuditEvent` to `Value` first, then `to_vec(&value)`. The disk bytes become key-sorted (BTreeMap ordering). Any third-party tool can replay the chain by parsing the JSONL line, taking the `event` field as `Value`, re-serialising, hashing — exactly what the `verify_audit.py` script in the deployer guide §5.4 does.

## Consequences

- The chain is verifiable off-line in any language that has a JSON parser and a sha256 implementation — no Rust-type coupling.
- Marginal write-time cost (one extra `to_value` allocation per event) is negligible against the I/O.
- The verifier script in the deployer guide is straightforward and portable.
- If the `AuditEvent` schema evolves (new fields), the canonicalisation still holds — the BTreeMap reorders the larger set deterministically.
- Changing the encoding (e.g. switching to CBOR) would break old days. Versioned by the file's existence: `2026-MM-DD.jsonl` files written under this contract are forever locked to it.

## Reference

`crates/anno-privacy-gateway/src/audit.rs::JsonlAuditSink::write_line` (the `to_value → to_vec` two-step), `crates/anno-privacy-gateway/tests/e2e_gdpr.rs::forget_persists_to_audit_chain`, deployer guide §5.4, commit `16c0035d`.
