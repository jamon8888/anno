# Session 2026-05-15 — Output summary

> **Audience:** the next person opening this branch / repo. Read this first.
>
> **Scope of the session:** v0.4 GDPR core, lance 0.29 bump, anno-memory v0.1 + v0.2 T1, plus a full GDPR + AI Act documentation pack and the architecture decision records covering all of v0.4 / v0.6 / v0.7 / v0.1.
>
> **Status:** code + docs merged to `origin/jamon8888/main`. `upstream/arclabs561` deliberately not touched.

---

## 1. Headline

After today, the anno-rag platform is **operationally close to deployable** for an on-premise legal-cabinet profile (loopback / single-host). What remains is mostly cabinet-IT work (at-rest encryption, KMS custody, contract fill-ins) and one small code task (MCP graceful shutdown). The doc pack covers the GDPR + AI Act surface a DPO will look for; everything is v1 awaiting sign-off.

## 2. Shipped code

| Branch | Merge commit | Content |
|---|---|---|
| `feat/v0.7-anon-eval` (earlier session) | `be3db5bd` | v0.7 anonymisation eval (35-doc FR corpus, regex tier 1.0/1.0, NER Person 1.0/1.0 after honorific regex, Org 1.0/1.0, Loc 0.93/0.98) |
| `feat/v0.4-gdpr-core` | `61702abd` | **v0.4 GDPR core** — Pipeline::forget / find_subject / export_subject, JsonlAuditSink with sha256 hash chain + daily HMAC, bearer-auth middleware, /v1/subjects/{find,forget,export} routes |
| `feat/lancedb-0.29-bump` | `25852db7` | **PR-A** — workspace pin lancedb 0.27.2 → 0.29.0 + arrow 57 → 58 + lance-index 4 → 6 (0 source changes, 45/45 anno-rag lib tests green) |
| `feat/anno-memory-v0.1` | `b69f2e47` | **anno-memory v0.1 (12/15)** — second LanceDB collection, scalar indexes, FTS + RRF hybrid search, save / recall / forget / list pipeline methods + MCP tools, 24h erasure SLO via daily Table::optimize, vault-token cascade on forget |
| `feat/anno-memory-v0.2` | not merged — pushed only | **PR-D T1 (1/11)** — deterministic entity canonicalizer (NFD diacritic strip + ASCII-punct strip + alias table) |

## 3. Shipped docs

| Branch | Merge commit | Content | Closes |
|---|---|---|---|
| `docs/dpia-v1` | `f1361130` | Art. 35 DPIA — 6 risks rated, 14 open gaps, conditional-acceptance validation | U1 |
| `docs/ai-act-position-v1` | `7d9c2e33` | AI Act §8(a) deployer position — strict reading of "judicial authority", 6-constraint deployment profile, 8 open Qs for outside counsel | U7 |
| `docs/v0.4-deployer-guide` | `031b821e` | Operator runbook — quick-start + production env vars + key generation + sample systemd unit + subject-rights curl recipes + off-line audit-chain verifier (`verify_audit.py`) + ops runbook | U8 |
| `docs/data-subject-pack-v1` | `4331694e` | Lawful basis register (9-row operations table) + Art. 14 notice (FR-language generic + per-matter templates) + retention policy (per-asset table + special cases) | U2 + U3 + U4 |
| `docs/breach-playbook-v1` | `4dae0ed2` | Art. 33 + 34 breach response — 10 detection paths, 72-hour timeline, 3 FR-language notification templates, 5 incident-specific runbooks, annual tabletop schedule | U13 |
| `docs/human-oversight-v1` | `73bca850` | Three-tier oversight protocol (real-time / authorised / refuse-AI) operationalising the AI Act C-3 constraint and the DPIA no-Art-22 commitment | U9 |
| `docs/adrs-v0.4-to-v0.7` | `fb8af66b` | 10 ADRs covering pseudonymise-at-ingest, encrypted vault, eval matching policy, FR honorific regex, bearer-auth soft-enforce, canonical audit bytes, memories second collection, reference-count cascade, 24h erasure SLO, MCP kind=String | — (cross-cutting) |
| `docs/u11-u12-templates` | `5473c5ce` | Schrems-II TIA for the Anthropic transfer (FISA §702 / EO 12333 mitigations) + 7-row sub-processor DPA register + change-monitoring procedure | U11 + U12 |
| `docs/session-summary-and-readiness-update` | this commit | This document + readiness-spec status update | — |

## 4. Readiness-spec state after today

| Gap class | Before today | After today |
|---|---|---|
| **Code** (G1–G7) | All open | G1–G6 ✅, G7 open (~1 day) |
| **Doc** (U1–U13) | All open | U1, U2, U3, U4, U7, U8, U9, U11, U12, U13 ✅ v1 (DPO sign-off pending); U5, U6, U10 open |

## 5. Remaining follow-ups, ranked

### Code (would need compile cycles)

1. **G7** — MCP graceful shutdown. Wire `tokio::signal::ctrl_c()` to flush the audit sink + close LanceDB cleanly before the process exits. ~1 day including a regression test. Small change.
2. **U10** — Detector-output logging per request (model version, input length, span counts, no raw content). Add to `Pipeline::detect`'s callers; piggyback on the existing `anno_rag::memory::audit` tracing target. ~1 day.
3. **U5** — At-rest encryption for the LanceDB index directory. Operator solves at the OS layer today (BitLocker / LUKS); a v0.5 plan could add per-file AES-GCM in cooperation with the lance team. ~1 week.
4. **U6** — KMS-managed vault key custody. Adapter trait + Azure Key Vault / AWS KMS / HashiCorp Vault implementations. ~1–2 weeks.
5. **PR-D T2–T11** — anno-memory v0.2 (10 tasks): wire anno::StackedNER into entity_refs, bi-temporal as_of, conflict resolver, 2-hop graph traversal, MCP wiring, property test, LoCoMo eval, docs, PR. ~1 week.
6. **anno-memory v0.1 T13** — deferred LoCoMo subset eval (50 conversation/question fixture + bench_locomo + recorded baseline). ~3 days.

### Cabinet-side (no code)

- DPO + outside-counsel review of the 9 v1 doc deliverables → promote each to v1 final with sign-off.
- Cabinet IT fills in the contact rosters in the breach playbook §1 and the sub-processor register §B.1.
- DPO completes the operational sizing in the cross-border review §A.4.
- First annual tabletop exercise within 90 days (breach playbook §6).
- Engagement-letter update with the Art. 14 + AI-assistance clauses.

## 6. What I'd recommend doing next

If next session is **doc / strategy-oriented**:
- Promote all v1 docs to v1-final once outside-counsel turns Q1–Q8 in the AI Act paper.
- Run the first breach-playbook tabletop.

If next session is **code-oriented and patient about compile time**:
- G7 first (small, isolated, high value).
- Then PR-D T2–T11 in one push if compile-cache stays warm.

If next session is **code-oriented but short**:
- Pick up G7 alone.
- Or finish anno-memory v0.1 T13 (LoCoMo eval).

## 7. Open branches on `origin/jamon8888`

| Branch | Status |
|---|---|
| `main` | All merges from today landed |
| `feat/anno-memory-v0.2` | T1 only; T2–T11 pending |

All other today's branches were merged + deleted after merge (per the cleanup pattern established earlier).

## 8. Repository state guarantees

- ✅ `upstream/arclabs561` is untouched throughout.
- ✅ `origin/jamon8888/main` carries every merge.
- ✅ Test gates pass: v0.7 anonymisation eval (regex + NER), v0.4 e2e audit chain, anno-memory v0.1 store + property test (the `#[ignore]`-d ones verified by hand).
- ⚠️ Two file-system paths remain orphaned (`.claude/worktrees/kind-wilson-5b7010` + `.claude/worktrees/busy-shaw-a137fb`) from earlier worktree removals where the OS held a file lock — empty dirs only; `rm -rf` when no process holds them.
