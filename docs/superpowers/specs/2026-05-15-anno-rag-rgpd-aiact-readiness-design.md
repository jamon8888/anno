# anno-rag — RGPD & EU AI Act readiness

**Date:** 2026-05-15
**Status:** brainstorm — design ready for plan
**Scope:** anno-rag detector (regex pack + GLiNER2-Fastino NER) and the surrounding pipeline (vault, store, search) as of branch `feat/v0.7-anon-eval`.
**Audience:** engineering + DPO + commercial leads.

---

## 0. Why this exists

anno-rag is being sold into French law firms (on-prem) and is also planned as a hosted SaaS for EU customers. Both deployments process personal data on a large scale and route it through an AI system. RGPD and the EU AI Act apply.

This document records, with code citations:

1. What the project can defensibly **claim today**, as actually shipped on this branch.
2. **Gaps** — obligations the project does not yet meet, separated into "specified-but-not-shipped" and "not-yet-considered".
3. **Tradeoffs** — design choices where the cheap option and the compliant option diverge.

---

## 1. Claims supportable by shipped code

> **Audit note (2026-05-15, revised):** an initial draft of this section claimed the v0.4 GDPR HTTP layer was shipped in `anno-rag`. It is not — `anno-rag::Pipeline` exposes only `new/ingest_one/ingest_folder/search/rehydrate/detect/vault_stats`, and `anno-rag::mcp::serve_stdio` is stdio-only. **However**, a separate crate `anno-privacy-gateway` (962 LOC `server.rs`, 668 LOC `privacy.rs`, 313 LOC `stream.rs`) ships an axum HTTP gateway with Anthropic-API-compatible `/v1/messages` + `/v1/models` routes, streaming SSE, and **on-the-wire pseudonymisation + rehydration** between client and upstream LLM. The gateway is merged on `origin/main` and is present on this branch unchanged. The claims below distinguish anno-rag from anno-privacy-gateway and credit each only for code that actually exists in that crate.

| # | Claim | RGPD / AI Act anchor | Code evidence | Caveat |
|---|---|---|---|---|
| C1 | **Pseudonymisation primitive** — output stream is pseudonymised personal data | RGPD Art. 4(5), Recital 26 | `crates/anno-rag/src/vault.rs` — Argon2id KDF or OS-keyring 32-byte secret; cloakpipe `Vault::open` + `Replacer::pseudonymize`; vault `!Sync`, wrapped in `Arc<Mutex>`; `Pipeline::rehydrate` reverses the mapping | "Pseudonymised" only holds insofar as the detector recall ≥ 1.0 per category. Current Person recall = 0.76 (NER tier) — real names leak through. See G1. |
| C2 | **Data minimisation in the index** | RGPD Art. 5(1)(c) | `crates/anno-rag/src/store.rs` lines 11–14, 48, 79–104, 313 — both the embedding and the FTS index are built on the `text_pseudo` column only; the raw `text_orig` never reaches LanceDB | The detector itself still reads raw text in-process; minimisation is **at storage**, not in transit through the detector |
| C3 | **Accuracy of personal data + measurement loop** | RGPD Art. 5(1)(d), AI Act Art. 15 | `crates/anno-rag/tests/fixtures/pii_baseline.toml` + `crates/anno-rag/tests/pii_regex.rs` (CI-gated) and `crates/anno-rag/tests/pii_ner.rs` (nightly, `--ignored`). Measured on the 35-doc French legal corpus: regex 1.0/1.0, NER Org 1.0/1.0, Loc 0.93/0.98, Person 0.76/1.00 | The honorific-regex patch lifting Person is in flight on this branch (unstaged) |
| C4 | **Detector runs locally** | RGPD Ch. V (Art. 44+) | `crates/anno-rag/src/detect.rs` — regex + `gliner2_fastino` via candle; model cache at `~/.cache/huggingface/hub/models--fastino--gliner2-multi-v1`. No telemetry call sites in the crate. | "Local" stops at the detector — if the downstream LLM is hosted (cloud-LLM mode), the pseudonymised output crosses the boundary. See T3. |
| C5 | **De-anonymisation path is gated by the vault** | RGPD Art. 32(1)(a), Art. 25 | `Pipeline::rehydrate` (pipeline.rs:216) reads through the vault under `lock_inner()`; without the vault key (Argon2id passphrase or OS keyring secret) no token can be reversed | OS keyring secret is generated on first run and stored unencrypted by some keyring backends — depends on host OS configuration |
| C6 | **Vault stats without disclosure** | RGPD Art. 5(1)(c) | `Pipeline::vault_stats` (pipeline.rs:237) returns counts only (`total_mappings`, per-category counts), not the mapping itself | — |
| C7 | **AI Act Art. 4 — AI literacy baseline** | AI Act Art. 4 | `docs/superpowers/specs/*`, `docs/runbooks/*`, `README.md`, `CHANGELOG.md`, and this document. Detector behavior, measured accuracy, and known limitations are documented. | Full deployer-side literacy program is the cabinet's obligation. |
| C8 | **AI Act Art. 50 — out of scope for the detector** | AI Act Art. 50 | The detector classifies spans; it does not generate text or media. `Pipeline::detect` / `pseudonymize` are deterministic transformations, not generative | Triggers if a generator is later embedded |
| C9 | **No GPAI obligations on the embedded models** | AI Act Art. 51 / Annex XIII | GLiNER2-Fastino is a small ~100M-param specialised NER model; `intfloat/multilingual-e5-small` is a sentence-embedding model. Both are well below the 10²⁵ FLOP training-compute threshold for GPAI-with-systemic-risk | If we later swap in a foundation model the analysis must be re-run |
| C10 | **On-the-wire pseudonymisation gateway (LLM proxy)** | RGPD Art. 4(5), Recital 26, Art. 25 (data protection by design) | `crates/anno-privacy-gateway/src/server.rs` exposes axum routes `/v1/messages`, `/v1/models`, `/v1/files`; `privacy.rs` (668 LOC) pseudonymises every outbound request body before `upstream::forward_messages` is called and rehydrates responses on the way back; `stream.rs` does the same for SSE streams. Integration test `messages_route_never_sends_cleartext_to_upstream_and_rehydrates` (server.rs:554) is asserted in CI. | Cleartext never leaves the gateway *if* the detector achieves 1.0/1.0 — see G1 |
| C11 | **Audit event shape + sink abstraction** | RGPD Art. 30 (data shape only) | `crates/anno-privacy-gateway/src/audit.rs` — `AuditEvent { request_id, provider_profile, entity_count, fresh_pii_redacted }` (cleartext-free) + `AuditSink` trait | **Only `NoopAuditSink` is implemented** — persistent storage explicitly deferred ("persistent audit lands after v0.3", audit.rs:25). Counts as design-ready, not as a shipped Art. 30 register. |
| C12 | **Streaming PII redaction across SSE boundaries** | RGPD Art. 32 | `stream.rs` — pseudonymisation runs incrementally on streamed token deltas; test `mock_stream_leaky_messages` verifies a leaking upstream is still redacted on the way out | — |

---

## 2. Gaps

> **Status update (2026-05-16):**
> - Code gaps **G1–G7** closed (v0.7 + v0.4 GDPR core + G7 graceful shutdown). Reclassified as claims C13–C18 below.
> - Doc gaps **U1, U2, U3, U4, U7, U8, U9, U11, U12, U13** closed v1 (pending DPO + outside-counsel sign-off). See the doc-pack mapping below.
> - **U10** (AI Act Art. 12 / Art. 72 detector logging) closed as code — see C19 below.
> - **U6** scaffolding shipped (`VaultKeySource` enum with KMS stub variant — see C20 below). The actual KMS adapter implementations (Azure Key Vault / AWS KMS / HashiCorp Vault) remain v0.5+ work.
> - **Still open:** U5 (at-rest encryption — operator + v0.5 code); U6 real KMS adapter (v0.5+).
> - PR-D (anno-memory v0.2, 1/11 tasks shipped on `feat/anno-memory-v0.2`) is paused but unblocked.

### Doc-pack mapping (closed gaps → doc anchors)

| Closed gap | Document |
|---|---|
| U1 DPIA | `docs/superpowers/specs/2026-05-15-anno-rag-dpia-v1.md` |
| U2 lawful basis | `docs/superpowers/specs/2026-05-15-anno-rag-data-subject-pack-v1.md` §1 |
| U3 Art. 14 notice | same §2 |
| U4 retention policy | same §3 |
| U7 AI Act position | `docs/superpowers/specs/2026-05-15-anno-rag-ai-act-position-v1.md` |
| U8 deployer guide | `docs/runbooks/anno-privacy-gateway-v0.4-deployer-guide.md` |
| U9 human oversight | `docs/superpowers/specs/2026-05-15-anno-rag-human-oversight-v1.md` |
| U11 cross-border review | `docs/superpowers/specs/2026-05-15-anno-rag-cross-border-and-subprocessors-v1.md` Part A |
| U12 sub-processor register | same Part B |
| U13 breach playbook | `docs/runbooks/anno-rag-breach-playbook-v1.md` |
| ADRs 1–10 (cross-cutting design rationale) | `docs/adrs/` |

Split into **G — specified but not shipped** (v0.4 GDPR spec is the design; the code is missing) and **U — undefined** (no spec yet).

### Claims added by v0.4 GDPR core

| # | Claim | Anchor | Code evidence |
|---|---|---|---|
| C13 | **Right to erasure** | RGPD Art. 17 | `Pipeline::forget` (pipeline.rs) + `Vault::forget` (vault.rs) + `cloakpipe-core::Vault::remove`; route `POST /v1/subjects/forget` (gateway/subjects.rs) emits an `AuditEvent` per call. Counter-monotonicity preserved. |
| C14 | **Right of access** | RGPD Art. 15 | `Pipeline::find_subject` + `Vault::find_subject` + `cloakpipe-core::Vault::find`; route `POST /v1/subjects/find` (auth-gated). |
| C15 | **Right to portability** | RGPD Art. 20 | `Pipeline::export_subject(ExportFormat::Json\|Csv)` + route `GET /v1/subjects/{subject_ref}/export?format=json\|csv` (auth-gated). |
| C16 | **Persistent Art. 30 register** | RGPD Art. 30 | `JsonlAuditSink` (audit.rs) — append-only `YYYY-MM-DD.jsonl` with sha256 hash chain over canonicalised event bytes (round-tripped through `serde_json::Value` for key-sorted determinism so a third party can replay off-line); daily `YYYY-MM-DD.sig` HMAC-SHA256 file. Chain head recovers across restarts. e2e test `forget_persists_to_audit_chain` (tests/e2e_gdpr.rs) verifies the chain. |
| C17 | **Bearer-token auth on the gateway** | RGPD Art. 32 | `auth::require_bearer` middleware with constant-time compare (`subtle::ct_eq`). `/health` stays public. When no token is configured the middleware is a no-op so loopback dev/test flows keep working; operators must set `ANNO_GATEWAY_BEARER_TOKEN` before exposing the gateway beyond loopback. e2e test `forget_without_bearer_does_not_write_audit` verifies short-circuit. |
| C18 | **Graceful shutdown on the gateway + MCP server** | RGPD Art. 30 (audit-log durability), Art. 32 (integrity of processing) | Gateway: `server::serve` wires `axum::serve(...).with_graceful_shutdown(shutdown_signal())` racing `tokio::signal::ctrl_c()` against `SIGTERM` (Unix). In-flight handlers drain; `JsonlAuditSink` per-line `sync_data` means no audit events are lost. MCP: `mcp::serve_stdio` uses `rmcp::service.cancellation_token()` to interrupt `service.waiting()` on signal — Pipeline drops on return → LanceDB connection closes + vault flushes pending saves. Structured tracing at `anno_privacy_gateway::shutdown` + `anno_rag::mcp::shutdown` makes the lifecycle observable. |
| C19 | **Detector audit event (Art. 12 logging + Art. 72 post-market monitoring)** | AI Act Art. 12 + Art. 72 | `Detector::detect` (detect.rs) emits a structured `tracing::info!` at `target = "anno_rag::detect::audit"` with: `detector_version` (crate `CARGO_PKG_VERSION`), `ner_model_id` (`NER_MODEL_ID` const), `input_chars`, `elapsed_us`, `spans_total`, `spans_from_pattern` / `_ner` / `_other`, `per_category` (compact JSON map). **Cleartext-free** — no raw text or detected values. Deployers pipe this target to their SIEM / Art. 30 register. |
| C20 | **Vault-key-source scaffolding (KMS-ready)** | RGPD Art. 32 + DPIA v1 §3 R1 mitigation G-3 | `VaultKeySource` enum in `crates/anno-rag/src/vault.rs`: `Passphrase(String)` (Argon2id), `Keyring` (OS keyring), `Kms { provider, key_id }` (v0.4 stub returning a clear error pointing at U6). `VaultKeySource::from_env` selects from env vars `ANNO_RAG_VAULT_KMS_PROVIDER`+`_KEY_ID`, then `ANNO_RAG_VAULT_PASSPHRASE`, else Keyring. `derive_key()` is a thin compat wrapper. Real KMS adapter impls (Azure Key Vault / AWS KMS / HashiCorp Vault) plug into the `Kms` arm in v0.5+. Tests: stub message contains provider name + "U6"; passphrase derivation deterministic; differs per input. |

### G — specified but not shipped on this branch

| Id | Obligation | Spec reference | Severity |
|---|---|---|---|
| G1 | Person detector recall < 1.0 (currently 0.76) — real names leak into the "pseudonymised" stream | v0.7 plan tasks 6–8 + in-flight honorific regex (this branch, unstaged) | **Critical** for cloud-LLM mode |
| G2 | Right to erasure — `Pipeline::forget(subject)`, CLI subcommand, MCP/HTTP route | `docs/superpowers/specs/2026-05-13-anno-rag-v0.4-http-gdpr.md` §1, §4 | **Critical** before any production use |
| G3 | Right of access — `Pipeline::find_subject(ref)`, exporters | v0.4 spec §1, §4 | **Critical** before any production use |
| G4 | Right to portability (machine-readable export) — `/v1/subjects/:ref/export?format=json\|csv` | v0.4 spec | **High** — useful even where Art. 20 is not strictly applicable |
| G5 | Art. 30 records of processing — **persistent** sink (JSONL register with sha256 hash chain + daily HMAC), `anno-rag register` exporter. Shape and trait exist (C11); only `NoopAuditSink` is wired today. | v0.4 spec §1 + `anno-privacy-gateway/src/audit.rs:25` | **Critical** before any production use |
| G6 | Bearer-token (or mTLS) auth on the gateway HTTP routes. Gateway is currently unauthenticated — anyone who can reach `/v1/messages` can send through the upstream Anthropic key. Bind to loopback or put behind a reverse proxy as a stopgap. | v0.4 spec §3 — not in `Cargo.toml` deps of the gateway either | **Critical** before exposing the gateway beyond `127.0.0.1` |
| G7 | MCP graceful shutdown that flushes the audit log + closes LanceDB cleanly | v0.4 spec §1 | Medium |

### U — undefined (no spec yet)

| Id | Obligation | What's missing | Severity |
|---|---|---|---|
| U1 | DPIA — RGPD Art. 35 | Document. CNIL's "list of types of processing requiring a DPIA" includes profiling and large-scale processing of legal-sector data. Inputs (categories, accuracy, audit, erasure) exist or are specified; the DPIA itself is unwritten. | **High** — blocks SaaS launch |
| U2 | Lawful basis enumeration per purpose — RGPD Art. 6, 9 | A short artifact stating: ingest = Art. 6(1)(f) legitimate interest; search = (f); pseudonymise = (f); Art. 30 register = Art. 6(1)(c) legal obligation; commercial exports = Art. 6(1)(b) contract | Medium — required for any subject request response |
| U3 | Information notice — RGPD Art. 13, 14 | Template for the Art. 14 notice (data not collected from the subject — opposing parties, witnesses). Law firms typically invoke Art. 14(5) exemptions (legal claims, prof. secrecy); the invocation needs documenting. | Medium |
| U4 | Retention schedule + automatic deletion — RGPD Art. 5(1)(e) | Time-based TTL on chunks; audit-log retention bound | Medium |
| U5 | At-rest encryption of LanceDB index — RGPD Art. 32(1)(a) | BACKLOG #019; today the index is plaintext on disk | **High** for SaaS, dependency on host FDE for on-prem |
| U6 | Vault key custody — SaaS mode — RGPD Art. 4(5), Art. 32 | Per-tenant KMS or client-side key derivation. "anno-holds-key" defeats pseudonymisation vs anno-the-processor. | **Critical** for SaaS |
| U7 | AI Act Annex III §8(a) position paper — AI Act Art. 6, Annex III, Recital 61 | Documented stance with two readings. See T1. | High |
| U8 | AI Act Art. 13 deployer information — usage instructions, intended purpose, accuracy, known limitations, human oversight | Customer-facing deployer guide | Medium |
| U9 | AI Act Art. 14 human-in-the-loop — checkpoint between detection and pseudonymisation | Currently auto-pseudonymises. Optional review UI for spans below confidence threshold. | Mode-dependent |
| U10 | AI Act Art. 12 logging + Art. 72 post-market monitoring | Detector-output logs per request (model version, input length, span counts, no raw content) | Mode-dependent |
| U11 | Cross-border transfer assessment — RGPD Ch. V (cloud-LLM mode) | TIA + SCCs for OpenAI/Anthropic/Mistral-cloud | **Critical** if cloud-LLM enabled |
| U12 | Sub-processor list + DPA template — RGPD Art. 28 (SaaS mode) | Customer-facing DPA | **Critical** for SaaS |
| U13 | Breach notification procedure — RGPD Art. 33, 34 | 72h playbook | Medium |

---

## 3. Tradeoffs

### T1 — AI Act Annex III §8(a) positioning

> **Article 8(a):** "AI systems intended to be used by a judicial authority or on their behalf to assist a judicial authority in researching and interpreting facts and the law and in applying the law to a concrete set of facts, or to be used in a similar way in alternative dispute resolution."

**Reading A — not high-risk (defensive).**
- anno is a document-search and redaction tool, not a legal-reasoning aid.
- **Recital 61** is anno's strongest citation: "AI systems intended for purely ancillary administrative activities that do not affect the actual administration of justice in individual cases, such as anonymisation or pseudonymisation of judicial decisions, documents or data … should not be classified as high-risk."
- Obligations under Reading A: Art. 4 (literacy), Art. 50 if generator is later added, GPAI Code if a foundation model is embedded. The embedded models (GLiNER2-Fastino, e5-small) are not GPAI under Art. 51(2).

**Reading B — high-risk (offensive).**
- The lawyer-deployer uses anno to research case files and prepare positions. Retrieval ranking shapes what facts the lawyer sees, which crosses into "researching and interpreting facts".
- Counter to Recital 61: the recital exempts pure anonymisation; the RAG retrieval layer on top arguably breaks the exemption.
- Obligations: Chapter III Section 2 (Art. 9–15), conformity assessment (Art. 43), EU database registration (Art. 49), post-market monitoring (Art. 72), incident reporting (Art. 73), and possibly a FRIA (Art. 27).

**Recommended position — provider/deployer split.**
- anno-the-product = Reading A.
- anno-the-deployer (the cabinet) may trigger Reading B by their own use; the cabinet then inherits Art. 26 deployer obligations.
- Provider documentation (G U8) makes this distinction explicit and gives the cabinet the levers (logging, oversight hooks, accuracy tables) to operate under Reading B if their DPO requires it.

### T2 — Detector accuracy vs patch cost

| Lever | Recall gain | Precision risk | Engineering cost |
|---|---|---|---|
| FR honorific regex (in-flight diff) | Person 0.76 → ~0.98 estimated | Low — corpus has no "Monsieur le Président" traps; pattern requires 2+ capitalised words after the honorific | Hours; 1 commit |
| Re-prompt GLiNER2 with FR-specific labels | Possible Person + Location bump | Unknown — needs eval | Low — config change + re-baseline |
| Fine-tune GLiNER2 on FR legal corpus | Higher ceiling | Medium — overfit risk on 35-doc corpus | Days + corpus rights review (training data is itself an Art. 10 data-governance concern) |
| Replace GLiNER2 with CamemBERT-NER or sentence-bert-fr | Likely highest Person recall | Adds dep + license review (CamemBERT is MIT, fine) | Weeks |
| Ensemble (regex ∪ GLiNER ∪ camembert) | Recall via union | Precision via vote | Architectural change |

**Recommendation:** ship the FR honorific regex now (already drafted on this branch, unstaged) → re-measure → decide further work on data. Other levers are v0.8+.

### T3 — Local vs cloud LLM

| Dimension | Local LLM (Ollama / Mistral 7B) | Cloud LLM (Claude / GPT / Mistral cloud) |
|---|---|---|
| RGPD Ch. V transfer | None | Transfer to non-EU controller; needs SCCs + TIA (Schrems II legacy) |
| Pseudonymisation robustness | Lower stakes — vault stays with model | Critical — Person recall < 1.0 = real names sent abroad |
| Sub-processor chain | None | LLM provider + their cloud + their training partners |
| Quality on FR legal | Lower today; tracks model size | Higher today |
| Latency | Higher (no GPU on the cabinet's typical laptop) | Lower if cabinet has fast network |
| DPA + audit | Local Art. 30 register sufficient | LLM provider DPA + Schrems II TIA |

**Recommendation:** local LLM is the default; cloud LLM is opt-in with a config-time legal-impact warning.

### T4 — Vault key custody (SaaS mode)

| Custody model | Pseudonymisation valid vs anno-the-processor? | UX cost | Engineering cost |
|---|---|---|---|
| Cabinet holds passphrase (current) | Yes — anno never sees originals | Cabinet must remember passphrase | Already implemented |
| anno holds passphrase server-side | **No** — anno sees plaintext-equivalent → anno becomes an Art. 28 processor of unpseudonymised personal data; loses Recital 26 protection | Zero | Low |
| Per-tenant KMS (AWS KMS / OVH KMS / cabinet HSM) | Yes, if cabinet retains key control | Medium — KMS onboarding | High |
| Client-side derivation (PBKDF in browser/desktop) | Yes | High — UX flow | High |

**Recommendation:** SaaS mode requires per-tenant KMS or client-side derivation. "anno-holds-key" is unacceptable for legal practice.

### T5 — Detector conservatism

- **Over-detect** (high recall, lower precision) → too much pseudonymisation → retrieval quality drops (the embedder sees `__PERSON_42__` where the doc said "Tribunal de commerce" — the legal entity is lost). The NER tier already exhibits one false friend: "location de navires de plaisance" tagged as `Location` because the multilingual model reads English "location" into French "location" (= rental).
- **Under-detect** (low recall, high precision) → PII leaks. Currently Person 0.76 is the leak.
- **Pareto target:** regex tier ≥ 0.98/0.98 (achieved at 1.0/1.0), NER tier ≥ 0.95/0.95 per category, residual leak documented in the deployer notice (U8). The notice is a compliance lever — it shifts residual risk to the deployer's informed acceptance.

---

## 4. Recommended next moves (engineering order)

1. **Finish v0.7** — land the honorific regex + Task 6/7/8 (NER bench, CI wiring, CHANGELOG). Closes G1 to a defensible ceiling.
2. **Implement v0.4 GDPR core** — G2/G3/G4/G5/G6. Pipeline::forget, Pipeline::find_subject, Art. 30 register, bearer-auth HTTP. The spec already exists; this is execution.
3. **Author U1 (DPIA) + U2 (lawful basis) + U3 (Art. 14 notice) + U7 (AI Act §8(a) position paper)** — documents, not code.
4. **v0.8 — encryption at rest (U5) + KMS custody (U6)** — unlocks SaaS mode.
5. **v0.9 — Art. 13 deployer guide (U8), Art. 14 human-oversight hook (U9), Art. 12 detector logging (U10)** — completes AI Act provider obligations under Reading A.
6. **Cross-border (U11) + sub-processor + DPA template (U12) + breach playbook (U13)** — gating items for cloud-LLM + SaaS GA.

This sequence keeps each milestone shippable on its own (each is ≤ 1 release worth of work) and brings deployment unlocks in dependency order.

---

## 5. What this document is not

- Not legal advice. Every claim needs a DPO sign-off, and the AI Act §8(a) position needs a written opinion from EU AI Act counsel before commercial launch.
- Not exhaustive. RGPD Articles 24 (controller responsibility), 26 (joint controllers), 29 (processing under authority), and AI Act Articles 16–23 (provider obligations downstream of high-risk classification) are mentioned only where they bite directly; the full Article-by-Article matrix is deliberately not the chosen structure for this brainstorm.
