# Anno-RAG DPIA — v1.0

> **Article 35 GDPR data protection impact assessment** for the anno-rag platform (anno, anno-rag, anno-privacy-gateway, cloakpipe).
>
> **Status:** v1 — drafted 2026-05-15, no DPO sign-off yet. **NOT a substitute for a CNIL-acknowledged PIA**; pending review by the cabinet's nominated DPO.
> **Reviewer:** [pending]
> **Approval:** [pending]
> **Review cadence:** annual, or on any material change to detector / vault / sub-processor.

---

## 0. Scope of this DPIA

This DPIA covers the **anno-rag platform v0.4 + v0.6 + v0.7 + anno-memory v0.1**, deployed on-premise by a legal/professional-services cabinet (the **deployer / controller**) as an internal RAG and persistent-memory layer over the cabinet's case files, with optional egress to an LLM provider (Anthropic) through a privacy gateway.

In scope:
- The anno NER pipeline (anno crate).
- The anno-rag detection + pseudonymisation + retrieval pipeline.
- The anno-rag memory module (Cowork session memory).
- The anno-privacy-gateway HTTP service (Anthropic-compatible).
- The cloakpipe AES-256-GCM file vault.

Out of scope (covered by separate DPIAs / contracts):
- The cabinet's case management system (case files at rest before ingestion).
- Anthropic's own processing (covered by the Anthropic DPA, sub-processor).
- The OS keyring / passphrase derivation infrastructure (host responsibility).
- Frontend clients (Claude Desktop / Cowork).

---

## 1. Context

### 1.1 Description of the processing

| Aspect | Description |
|---|---|
| **Operation name** | Cabinet legal-RAG + session memory with PII pseudonymisation. |
| **Purpose** | (a) Retrieve relevant passages from the cabinet's own corpus to ground LLM-assisted drafting; (b) maintain PII-safe persistent memory of user preferences and case facts across sessions; (c) route prompts to an external LLM (Anthropic) without exposing cleartext PII. |
| **Lawful basis** | Art. 6(1)(f) GDPR — **legitimate interest** of the controller (the cabinet) in providing efficient legal services to its own clients; balanced against data-subject rights via the pseudonymisation + erasure machinery documented below. For employee-data side-effects: Art. 6(1)(b) (contract performance) when employee data is processed as part of case work. **For special categories (Art. 9)** appearing in case files (health data in personal-injury cases, religious/political/union information in employment cases): **explicit consent** from the data subject via the client engagement letter, OR Art. 9(2)(f) (establishment, exercise or defence of legal claims). |
| **Data categories** | Names, addresses, phone numbers, emails, IBAN, SIRET, NIR (French SSN), organisation names, location names, dates, free-text legal narrative. **Possibly Art. 9 special categories** in the underlying documents — depends on the case mix. |
| **Data subjects** | Cabinet clients, opposing parties, third parties named in case files (witnesses, experts), cabinet employees mentioned in correspondence. |
| **Retention** | Case files: per the cabinet's retention policy (typically 5–10 years post-matter-close under French legal-records rules). Vault entries: removed via `Pipeline::forget` cascade on subject erasure request; physical reclamation within 24 h via the v0.1 daily compaction ticker. Audit register: 5 years (Art. 30 register retention norm). |
| **Recipients (internal)** | Cabinet partners, associates, paralegals, IT admin (vault key custody). |
| **Recipients (external — sub-processors)** | **Anthropic** (LLM inference; covered by Anthropic's DPA + sub-processor list). **HuggingFace Hub** (one-time model download on first run; no PII transmitted). |
| **Cross-border transfers** | Anthropic processes in the EU and the US under SCCs + supplementary measures. Model download from HF Hub is one-shot at install time; no personal data transmitted. |
| **Automated decision-making** | None with legal effect on data subjects. RAG retrieval surfaces relevant passages; legal reasoning + drafting remains the responsibility of the lawyer. **No Art. 22 profile.** |

### 1.2 Data flow diagram (textual)

```
[client document]
    ↓ (deposit by cabinet user)
[anno-rag ingest]                          ← clear PII still present
    ↓ detect (regex tier + GLiNER2-Fastino NER)
[DetectedEntity[]]                          ← spans + categories, no replacement yet
    ↓ pseudonymize (cloakpipe Replacer + Vault)
[(text_pseudo, mappings)]                   ← clear PII isolated to vault
    ↓ embed (e5-small multilingual)
[(text_pseudo, embedding)]
    ↓ insert
[LanceDB.chunks]                            ← on-disk row never contains cleartext PII
                                             vault.bin holds the only cleartext copy
                                             (AES-256-GCM encrypted)

[memory_save("note about Marie Dupont")]
    ↓ detect → pseudonymize_with_refs
[(text_pseudo, token_refs)]
    ↓ embed
    ↓ insert
[LanceDB.memories]                          ← same property; token_refs is the cascade payload

[search("vente Bordeaux")] / [recall("...")]
    ↓ detect query → pseudonymize query
    ↓ embed query (e5 query prefix)
    ↓ hybrid (vector + FTS) → RRFReranker
[SearchHit/MemoryHitRow[]]                  ← still tokenised
    ↓ rehydrate via vault
[plaintext to caller]                       ← only the original requester sees clear PII

[gateway /v1/messages]
    ↓ pseudonymize_request
[pseudo body] → Anthropic                   ← network egress contains tokens, not names
                ← response
    ↓ rehydrate_response
[plaintext to caller]
    + audit event (no PII) → JsonlAuditSink (hash-chained + daily HMAC)

[/v1/subjects/forget {subject_ref}]
    ↓ Pipeline::forget (vault entry deleted)
    + audit event
[Pipeline::forget_memory id+cascade]
    ↓ delete row in memories
    ↓ token_reference_count == 0 → Vault::forget(token)
    ↓ daily Table::optimize → physical reclamation within 24 h
```

---

## 2. Fundamental principles

### 2.1 Necessity and proportionality

| Principle | Evaluation | Evidence |
|---|---|---|
| **Lawfulness** | ✅ Legitimate interest (Art. 6(1)(f)) for cabinet operations; explicit consent / Art. 9(2)(f) for special-category data in case files. | Engagement letter template (in the cabinet's records) cites legal-records retention + AI-assisted-drafting clause. |
| **Purpose limitation** | ✅ Data ingested for one purpose (case work) and pseudonymisation prevents re-use in non-case contexts. | `text_pseudo` is the only form embedded; rehydration is per-call, not bulk. |
| **Data minimisation** | ⚠️ The cabinet ingests entire case files, which may exceed strict need. **Recommendation:** ingest at the document level only when a case is actively worked; archive ingested embeddings on matter close. | Implementation hook: `Pipeline::forget` per-subject or per-document; v0.2 candidate: per-matter erasure flag. |
| **Accuracy** | ✅ Source documents are the cabinet's own; rehydration preserves the original verbatim. | Detector emits offset-accurate spans; `Rehydrator::rehydrate` does literal substitution. |
| **Storage limitation** | ⚠️ Vault entries persist for the lifetime of the index — no automatic expiry. **Recommendation:** a v0.5 task should add a TTL on vault entries that have not been referenced in N months. | Tracked as a follow-up; documented in the readiness spec U4. |
| **Integrity & confidentiality** | ✅ Vault is AES-256-GCM. Token replacement is offset-accurate. Audit chain is sha256 + HMAC. | Cloakpipe `Vault::save` / `Vault::open` paths; `JsonlAuditSink::write_line`. |
| **Accountability** | ✅ Art. 30 register persists for 5 years; chain reproducible off-line; this DPIA exists. | `JsonlAuditSink` daily files + `.sig`. |

### 2.2 Rights of data subjects

| Right | Implementation | Code reference |
|---|---|---|
| **Information (Art. 13/14)** | The cabinet provides Art. 13 notice to clients via engagement letter; Art. 14 notice to third parties named in case files is the cabinet's responsibility (a non-trivial gap — see §4 risks). | Out of scope of this code; cabinet-process. |
| **Access (Art. 15)** | `POST /v1/subjects/find` over the privacy gateway (bearer-auth). Returns the vault matches for the subject reference. | `crates/anno-privacy-gateway/src/subjects.rs::find` |
| **Rectification (Art. 16)** | Indirect — the cabinet re-ingests corrected documents; old chunks are deleted via `Pipeline::forget` or per-document erasure. v0.5 candidate: explicit rectification API. | Tracked as a v0.5 task. |
| **Erasure (Art. 17)** | `POST /v1/subjects/forget` → `Pipeline::forget`. Cascades vault entries. `Pipeline::forget_memory` cascades memory rows + vault tokens. Physical reclamation within 24 h via daily `Table::optimize`. | `crates/anno-rag/src/pipeline.rs::forget` + `forget_memory`; `crates/anno-privacy-gateway/src/subjects.rs::forget` |
| **Portability (Art. 20)** | `GET /v1/subjects/{ref}/export?format=json\|csv`. Returns the matches as machine-readable bytes. | `crates/anno-privacy-gateway/src/subjects.rs::export` |
| **Restriction (Art. 18)** | Partial. v0.2's `memory_invalidate` (planned) and v0.1's `valid_to`-column close half of this; full restriction (no further processing) requires a per-subject "frozen" flag — v0.5 candidate. | Tracked. |
| **Objection (Art. 21)** | Manual process — the cabinet receives the request; the DPO triggers `Pipeline::forget` for the relevant subject. | Procedural. |
| **No Art. 22 (automated decision)** | ✅ Confirmed — anno-rag does retrieval; the lawyer drafts. | N/A. |

---

## 3. Risks

Methodology: CNIL PIA approach — each risk is rated by **severity** (negligible / limited / significant / maximum) and **likelihood** (negligible / limited / significant / maximum). Targets: any risk with severity ≥ significant must be reduced to severity = limited or below after mitigation; any with likelihood = maximum must be reduced to ≤ significant.

### Risk 1 — Illegitimate access to personal data

| Aspect | Detail |
|---|---|
| **Threat sources** | (a) External attacker on the host network; (b) malicious or compromised cabinet employee; (c) stolen laptop / device. |
| **Threat vectors** | (a) Network: gateway exposed beyond loopback without auth; (b) endpoint: vault file readable on disk; (c) lateral: WSL ext4 access from a different Windows user. |
| **Impact on subjects** | Re-identification of clients, opposing parties, third parties — including special-category data where present (health, religious affiliation, union membership). Reputational harm, possible blackmail vectors. |
| **Severity (gross)** | **Maximum** — loss of legal-professional privilege + breach of Art. 9 special categories. |
| **Likelihood (gross)** | **Significant** — on-premise but not air-gapped; cabinet IT maturity varies. |
| **Mitigations in place** | (a) `auth::require_bearer` middleware with constant-time compare on the gateway (`subtle::ct_eq`). `/health` exempt. **`bearer_token=None` mode is a no-op** for loopback dev — operators MUST set `ANNO_GATEWAY_BEARER_TOKEN` before exposing beyond 127.0.0.1. (b) Vault is AES-256-GCM with a 32-byte key derived from a passphrase via Argon2id (interactive m=64 MiB, t=3) OR a random 32-byte key from the OS keyring. (c) `text_pseudo` is the only form embedded; the LanceDB row carries no cleartext PII. |
| **Mitigations to implement** | (i) **U5** at-rest encryption for the LanceDB directory itself (OS-level: BitLocker on Windows, LUKS on Linux). Currently relies on filesystem permissions. (ii) **U6** KMS integration — key custody on a hardware module / Azure Key Vault / AWS KMS rather than a passphrase-on-disk derivation. (iii) **mTLS** option on the gateway for non-localhost deployments (currently only bearer-token). (iv) **Rate-limiting** on the gateway routes to slow brute-force attempts. |
| **Severity (net, after mitigation)** | **Significant** — drops from Maximum because the vault is encrypted at rest and the gateway is auth-gated, but special-category data + the vault-key-on-host model keep it above limited. |
| **Likelihood (net)** | **Limited** — significantly reduced by bearer-auth + encrypted vault + the on-premise boundary. |
| **Residual risk acceptance** | Conditional on §U5 + §U6 implementation in v0.5. Until then, **the deployment is acceptable only in loopback or single-host configurations**. |

### Risk 2 — Unwanted modification of personal data

| Aspect | Detail |
|---|---|
| **Threat sources** | (a) Software bug in the detector / vault; (b) deliberate tampering with the vault file; (c) ransomware. |
| **Threat vectors** | (a) Detector mis-tokenises a span, producing a vault entry whose original mapping is wrong → rehydration substitutes the wrong name into a legal document. (b) `vault.bin` truncation or corruption → silent rehydration failures. (c) Encryption-on-write ransomware mangles `vault.bin`. |
| **Impact on subjects** | Wrong identity attached to a legal claim — could damage a third party's reputation, mis-direct subpoena, etc. |
| **Severity (gross)** | **Significant**. |
| **Likelihood (gross)** | **Limited** — detector is well-tested (v0.7 baselines: 1.0/1.0 recall on regex tier, 1.0 on Person/Org), and vault tampering is detectable via the AES-256-GCM AEAD tag. |
| **Mitigations in place** | (a) v0.7 anonymisation eval gate (98% tolerance on regex tier); 35-doc French legal corpus; honorific regex closes the FR Person gap. (b) `Vault::save` is atomic (write-to-tmp + rename). (c) AES-256-GCM AEAD means any tamper produces a decryption error, not a silent wrong value. (d) The hash-chained audit log records every write; tampering after-the-fact requires also forging the daily HMAC signature. |
| **Mitigations to implement** | (i) Periodic backup of `vault.bin` to a separate volume, retention 30 days. (ii) v0.5 candidate: detector regression tests on a labelled "edge case" corpus (apostrophes, hyphens, multi-word honorifics, accented locations). (iii) v0.6 candidate: per-write checksum of the rehydrated form when used in generated documents (warn if a re-detect of the rehydrated text doesn't match the original detect). |
| **Severity (net)** | **Limited** — the mitigations are largely in place. |
| **Likelihood (net)** | **Negligible**. |

### Risk 3 — Disappearance of personal data

| Aspect | Detail |
|---|---|
| **Threat sources** | (a) Storage failure; (b) accidental deletion; (c) bug in the cascade path producing over-eager erasure. |
| **Threat vectors** | (a) Disk corruption losing `vault.bin` or `chunks/`. (b) Operator deletes the index directory. (c) `Pipeline::forget` cascades incorrectly when two memories share a token (the v0.1 cascade checks reference count = 0 before vault purge, but bugs are possible). |
| **Impact on subjects** | Loss of the cabinet's ability to respond to a subject's Art. 15 access request — paradoxically a GDPR breach in the other direction (failure of obligations). |
| **Severity (gross)** | **Limited**. |
| **Likelihood (gross)** | **Limited**. |
| **Mitigations in place** | (a) `Pipeline::forget` returns an `ErasureReceipt` with `mappings_removed` count — observable cascade behaviour. (b) The v0.1 forget-cascade property test (`memory_proptest.rs`, 50 cases) drives the invariant. (c) Audit log retains the receipt even after physical reclamation — the cabinet can prove which entries were forgotten. |
| **Mitigations to implement** | (i) Daily backup of the LanceDB directory (operator responsibility; doc'd in v0.4 deployer guide — TODO). (ii) v0.5 candidate: dry-run preview before `forget` cascade in the MCP tool (currently shows the count, not the per-token detail). |
| **Severity (net)** | **Negligible**. |
| **Likelihood (net)** | **Negligible**. |

### Risk 4 — Sub-processor leakage (Anthropic)

| Aspect | Detail |
|---|---|
| **Threat sources** | (a) Token-pattern reverse-engineering by an attacker observing Anthropic's logs; (b) Anthropic policy change widening their training opt-in; (c) Anthropic personnel access. |
| **Threat vectors** | (a) Even pseudonymous tokens (`PERSON_42`) plus surrounding context can sometimes be re-identified by inference — for unique facts ("le PDG de la Société Générale a démissionné jeudi 12 mars"), the rehydration target is not actually obscure to a knowledgeable reader. (b) Anthropic could update its data-handling terms; the cabinet must monitor and notice. |
| **Impact on subjects** | Disclosure of identity to Anthropic, possibly to Anthropic-employed reviewers, possibly to model training corpora. |
| **Severity (gross)** | **Significant** — for high-profile clients in active matters. |
| **Likelihood (gross)** | **Limited** — Anthropic offers zero-retention enterprise terms (the cabinet must opt in). |
| **Mitigations in place** | (a) Anthropic DPA — sub-processor commitments + zero-retention option per the enterprise contract. (b) Image rejection at the gateway in strict mode (`reject_images = true`) — prevents accidental PII leak via screenshots / faxes. (c) The privacy gateway logs no cleartext via the audit sink — `AuditEvent` carries only request_id + counts. |
| **Mitigations to implement** | (i) **U7** — explicit AI Act §8(a) deployer position paper documenting the provider/deployer split and the cabinet's responsibility. (ii) **U11** — cross-border-transfer review with the cabinet's DPO (Anthropic-US under SCCs + supplementary measures). (iii) Token entropy review — verify that `PERSON_42` numbering, when combined with vault size, doesn't betray the corpus structure. (Currently sequential per category; v0.6 candidate: randomise the counter on a per-vault basis.) |
| **Severity (net)** | **Limited** — provided the cabinet opts into zero-retention and updates the DPO contract review on every Anthropic terms revision. |
| **Likelihood (net)** | **Negligible to Limited**. |

### Risk 5 — Re-identification via the embedding index

| Aspect | Detail |
|---|---|
| **Threat sources** | (a) Adversary with access to the index file but not the vault. |
| **Threat vectors** | (a) e5-small embeddings of `text_pseudo` carry semantic information — for unique factual claims, the embedding alone can locate near-matching public records, possibly identifying the subject without ever decrypting the vault. |
| **Impact on subjects** | Probabilistic re-identification of subjects who feature in unique factual claims. |
| **Severity (gross)** | **Limited** — only works on subjects with public-record footprint and unique factual claims. |
| **Likelihood (gross)** | **Limited** — requires file access to a fileset already protected by Risk 1 mitigations. |
| **Mitigations in place** | (a) The same encryption-at-rest + bearer-auth mitigations as Risk 1. (b) Risk 1's net residual is the upper bound on Risk 5's likelihood. |
| **Mitigations to implement** | (i) v0.6 candidate: an explicit "differential-privacy embedding" mode that adds noise to indexed vectors. Documented as `ANNO_RAG_EMBED_DP_EPSILON` in the spec; not on the v0.4 critical path. |
| **Severity (net)** | **Negligible**. |
| **Likelihood (net)** | **Negligible**. |

### Risk 6 — Audit log integrity

| Aspect | Detail |
|---|---|
| **Threat sources** | (a) Insider deletes or modifies audit entries to hide unauthorised access; (b) attacker forges entries after compromise. |
| **Severity (gross)** | **Significant** — failure of Art. 30 + Art. 33 (breach notification) obligations. |
| **Likelihood (gross)** | **Limited**. |
| **Mitigations in place** | (a) `JsonlAuditSink` hash chain — each line's `this_hash = sha256(prev_hash \|\| canonical_event_json)`; modifying one line breaks every subsequent line. (b) Daily HMAC signature file (`YYYY-MM-DD.sig`) over the final chain head using a separate `audit_hmac_key`. (c) Chain head recovers across restarts. (d) Canonical key-sorted JSON for the hashed bytes — a third-party verifier can replay the chain off-line. (e) The e2e test `forget_persists_to_audit_chain` verifies all of this on every CI run. |
| **Mitigations to implement** | (i) **WORM / append-only filesystem** for the audit directory (operator responsibility). (ii) **Daily HMAC key rotation** to a verifier-known third-party (e.g. an external timestamping service). (iii) **Offsite mirror** of the audit JSONL files to detect host-side deletions. |
| **Severity (net)** | **Limited**. |
| **Likelihood (net)** | **Negligible**. |

---

## 4. Open gaps & follow-ups

| ID | Gap | Owner | Target |
|---|---|---|---|
| **G-1** | Art. 14 notice to third parties (not direct clients) — process gap, not code. | Cabinet + DPO | Pre-deployment. |
| **G-2** | U5 at-rest encryption for the index directory (OS-level). | Cabinet IT | v0.5. |
| **G-3** | U6 KMS-based vault key custody. | Anno team + Cabinet IT | v0.5–v0.6. |
| **G-4** | mTLS option on the privacy gateway. | Anno team | v0.5. |
| **G-5** | Rate limiting + abuse detection on the gateway. | Anno team | v0.5. |
| **G-6** | Daily backup of `vault.bin` + LanceDB index. | Cabinet IT | Pre-deployment. |
| **G-7** | Vault TTL on long-unreferenced entries. | Anno team | v0.5–v0.6. |
| **G-8** | Edge-case detector regression corpus. | Anno team | v0.8. |
| **G-9** | Token counter randomisation (anti-correlation). | Anno team | v0.6. |
| **G-10** | DP-noise embedding mode (`ANNO_RAG_EMBED_DP_EPSILON`). | Anno team | v0.6+. |
| **G-11** | WORM filesystem for audit log + offsite mirror. | Cabinet IT | Pre-deployment. |
| **G-12** | AI Act §8(a) position paper (U7 in readiness spec). | Anno team + Cabinet legal | Pre-deployment. |
| **G-13** | Anthropic DPA review on every terms revision. | Cabinet DPO | Recurring. |
| **G-14** | Restriction (Art. 18) — full "frozen subject" flag. | Anno team | v0.5. |

---

## 5. Validation

This DPIA documents a **conditional acceptance**:

- ✅ **Acceptable** for loopback / single-host deployments with G-6 (backup) + G-11 (audit WORM) in place.
- ⚠️ **Acceptable for multi-host LAN** only after G-2 (at-rest encryption) + G-4 (mTLS) land.
- ⚠️ **Not acceptable** for public-internet exposure of the gateway until G-3 (KMS) + G-5 (rate limit) + G-12 (AI Act position) are also in place.

The DPO must re-evaluate this assessment on any of:
- Material change to the detector (new categories, new model backend).
- Material change to Anthropic's terms or sub-processor list.
- New deployment topology beyond what §0 scopes.
- Any **security incident** affecting the vault, the audit log, or the gateway.

### Sign-off block

| Role | Name | Date | Signature |
|---|---|---|---|
| Controller representative | [pending] | | |
| DPO | [pending] | | |
| Technical owner (Anno team) | [pending] | | |
| Auditor (optional, recommended for cabinet > 50 employees) | [pending] | | |

---

## 6. Annexes

### Annex A — Architecture references

- Readiness spec: `docs/superpowers/specs/2026-05-15-anno-rag-rgpd-aiact-readiness-design.md`
- v0.4 GDPR core spec + plan: `docs/superpowers/specs/2026-05-13-anno-rag-v0.4-http-gdpr.md`, `docs/superpowers/plans/2026-05-15-anno-rag-v0.4-gdpr-core.md`
- v0.7 anonymisation eval: `docs/superpowers/specs/2026-05-15-anno-rag-v0.7-anonymization-eval-email-design.md`
- Anno-memory v0.1 design: `docs/superpowers/specs/2026-05-15-anno-memory-v0.1-design.md`

### Annex B — Code references (commit-anchored)

All paths relative to the anno repository at SHA `b69f2e47` (post-anno-memory-v0.1 merge):

| Concern | File | Symbol |
|---|---|---|
| Detector entry | `crates/anno-rag/src/detect.rs` | `Detector::new` / `detect` |
| Vault — open / save / encrypt | `vendor/cloakpipe/crates/cloakpipe-core/src/vault.rs` | `Vault::open` / `Vault::save` |
| Vault — Art. 17 primitive | same | `Vault::remove` |
| Vault — Art. 15 primitive | same | `Vault::find` |
| Pipeline — Art. 17 / 15 / 20 | `crates/anno-rag/src/pipeline.rs` | `forget` / `find_subject` / `export_subject` |
| Pipeline — memory Art. 17 cascade | same | `forget_memory` |
| Audit chain | `crates/anno-privacy-gateway/src/audit.rs` | `JsonlAuditSink::write_line` |
| Bearer auth | `crates/anno-privacy-gateway/src/auth.rs` | `require_bearer` |
| Subject routes | `crates/anno-privacy-gateway/src/subjects.rs` | `find` / `forget` / `export` |

### Annex C — Verifying the audit chain off-line

For an auditor / DPO who wants to verify that no entry was retroactively modified:

```bash
# 1. Take a copy of YYYY-MM-DD.jsonl + YYYY-MM-DD.sig.
# 2. Replay the chain:
python3 - <<'EOF'
import hashlib, json, sys
prev = "0" * 64
for line in open(sys.argv[1]):
    obj = json.loads(line)
    canonical = json.dumps(obj["event"], sort_keys=True, separators=(",", ":")).encode()
    h = hashlib.sha256()
    h.update(bytes.fromhex(prev))
    h.update(canonical)
    expected = h.hexdigest()
    assert expected == obj["this_hash"], f"chain broken at {obj['ts']}"
    assert obj["prev_hash"] == prev, f"prev_hash mismatch at {obj['ts']}"
    prev = expected
print("chain OK, last hash:", prev)
EOF

# 3. Verify the daily HMAC (using the audit-hmac-key from the cabinet KMS):
python3 -c "
import hmac, hashlib, sys
key = bytes.fromhex(open(sys.argv[1]).read().strip())
chain_head = sys.argv[2]
sig_expected = hmac.new(key, chain_head.encode(), hashlib.sha256).hexdigest()
print('match:', sig_expected == open(sys.argv[3]).read().strip())
" key.hex 2ebafea5... YYYY-MM-DD.sig
```

If the chain replay succeeds AND the HMAC matches, the audit register has not been tampered with since the day it was written (modulo HMAC-key compromise — see G-11).

### Annex D — Definitions

- **Controller** — the cabinet deploying anno-rag, responsible for the lawful basis and the data subjects.
- **Processor** — anno-rag itself, processing on behalf of the controller.
- **Sub-processor** — Anthropic (LLM inference), HuggingFace Hub (one-shot model download).
- **Data subject** — any identified or identifiable natural person whose data appears in the cabinet's case files.
- **Pseudonymisation** (GDPR Art. 4(5)) — processing such that personal data can no longer be attributed to a specific data subject without the use of additional information (the vault). Note: pseudonymised data is **still personal data** under GDPR — this is why the DPIA is needed at all.
- **Vault** — the cloakpipe AES-256-GCM file holding the `(token → original)` mapping. The cabinet's IT custody point.
- **Cascade** — the v0.1 `forget_memory` behaviour that removes a vault entry only when its token's reference count across memories drops to zero.
- **Erasure SLO** — the maximum delay between a `Pipeline::forget*` call returning and the physical disappearance of the bytes from disk. Currently 24 h via the daily `Table::optimize` ticker; the operator can shorten the interval via `compaction_interval_secs`.

---

*End of DPIA v1.0.*
