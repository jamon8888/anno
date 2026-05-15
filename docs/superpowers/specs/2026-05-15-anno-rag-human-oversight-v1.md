# Anno-RAG — Human oversight protocol v1

> **Audience:** the cabinet's managing partner, DPO, and every avocat using anno-rag.
> **Status:** v1 — drafted 2026-05-15. Closes **U9** of the readiness spec.
> **Anchors:** EU AI Act Art. 14 (provider obligation; deployer-mirror in Art. 26), GDPR Art. 22, déontologie of the French Barreau (RIN art. 6 — devoir de compétence).

---

## 1. Why this exists

AI Act Art. 14 obliges the **provider** of an AI system to design it for human oversight. AI Act Art. 26 obliges the **deployer** to operate it under human oversight when high-risk. The cabinet's profile is **non-high-risk** (per the AI Act position paper v1) — but human oversight is still required by:

- The AI Act position paper §3 constraint C-3: *"The system's outputs are always reviewed by a lawyer before being used in any external communication."* The whole non-high-risk classification depends on C-3 holding.
- The DPIA v1 §2.2 row on automated decision-making: *"No Art. 22 profile — anno-rag does retrieval; the lawyer drafts."* Same dependency.
- RIN art. 6 (devoir de compétence): an avocat is professionally responsible for the work product. AI assistance does not transfer that responsibility.

This document specifies **what "human oversight" concretely means** for the cabinet's anno-rag deployment, **who is accountable**, and **how compliance is observable** (so the cabinet can prove it after the fact).

---

## 2. Scope

Covers every interaction with anno-rag where the output may be used outside the cabinet:

| Interaction | In scope | Why |
|---|---|---|
| MCP `search` over the cabinet's corpus | ✅ | The retrieved passages will influence drafting. |
| MCP `rehydrate` of LLM-returned text | ✅ | The rehydrated text may be quoted in a deliverable. |
| MCP `memory_save` / `memory_recall` / `memory_list` | ✅ | Memories shape future replies. |
| MCP `memory_forget` | ✅ | Erasure is GDPR-relevant — must be authorised. |
| `/v1/messages` via the privacy gateway | ✅ | The model's output may be used in a deliverable. |
| MCP `vault_stats` / `detect` (dry-run) | ➖ | Diagnostic only — no output to a client. Light oversight. |
| `/v1/subjects/find` / `/v1/subjects/forget` / `/v1/subjects/{ref}/export` | ✅ | Direct effect on the data subject's rights. |

---

## 3. The three oversight tiers

Different interactions need different levels of oversight. The cabinet operates **three tiers**.

### 3.1 Tier 1 — Real-time review (default)

Every output the cabinet uses externally — pleadings, advice notes, contracts, correspondence — passes Tier 1.

**Operator:** the responsible avocat for the matter.

**What it means:**
- Read the AI-generated text in full before it leaves the cabinet.
- Verify factual claims against the underlying case file. (RAG retrieval surfaces passages — the avocat checks they say what the model claims they say.)
- Verify legal citations. (Models hallucinate citations; the avocat is responsible for every citation that ships.)
- Verify pseudonyms rehydrated correctly. (Names in the final document match the case file.)
- Verify no leak of unrelated case data. (Cross-matter contamination is rare but possible via memory; the avocat catches it.)

**Compliance trace:** the avocat's normal sign-off on outgoing communications is the evidence. No separate logging required for Tier 1 unless the cabinet wants belt-and-suspenders.

**Override:** the avocat may decide an output is unfit for use and discard it. No process — that's the default behaviour the regime enables.

### 3.2 Tier 2 — Reviewed authorisation (operational decisions)

Decisions that **change persistent state** — memory writes, subject erasures, vault operations — need an explicit authorisation step.

**Operator:** the responsible avocat for the matter + (for erasure) the DPO if the request comes from outside the cabinet.

**Operations under Tier 2:**

| Operation | Authoriser | Logged where |
|---|---|---|
| `memory_save` of a new fact / preference | Responsible avocat (implicit by initiating the call) | `anno_rag::memory::audit` tracing event |
| `memory_forget` initiated internally | Responsible avocat | Tracing event |
| `memory_forget` initiated by a subject request | **DPO** must confirm before execution | Tracing event + DPO file note |
| `/v1/subjects/find` initiated by a subject request | **DPO** | Gateway audit JSONL |
| `/v1/subjects/forget` | **DPO** + responsible avocat for matter scope | Gateway audit JSONL + DPO file note |
| `/v1/subjects/{ref}/export` | **DPO** | Gateway audit JSONL |
| Vault file backup / restore | IT Lead with DPO sign-off | Operator runbook log |
| Vault key rotation | IT Lead with managing-partner sign-off | Operator runbook log |
| Audit HMAC key rotation | IT Lead with DPO + managing-partner sign-off | Operator runbook log |
| Compaction policy change (`compaction_interval_secs` etc.) | IT Lead with DPO sign-off (because it shifts the erasure SLO) | Operator runbook log |

**Compliance trace:** the existing structured tracing at `anno_rag::memory::audit` + the `JsonlAuditSink` chain provide the technical evidence. DPO file notes for subject-driven operations are kept with the matter / DPO file.

**Override:** a Tier 2 operation can be skipped only by management decision documented in the matter file. The DPO must be informed within 24 h.

### 3.3 Tier 3 — Threshold scenarios (refuse to use AI)

Scenarios where the cabinet **does not** use anno-rag at all. The avocat must recognise these on matter-intake and configure the case-management system to disable the MCP server for the matter's session.

| Scenario | Reason |
|---|---|
| Court-appointed counsel mandate (*avocat commis d'office*) | Triggers AI Act position paper §5 mitigation — disable AI assistance for the mandate. |
| Mandate as judicial expert / *mission d'expertise* under court order | Same. |
| Acting as *commissaire de justice* executing a judicial decision | Same. |
| Matter requires evaluating evidence reliability, recidivism risk, or anything in AI Act Annex III §6 or §8(b)–(c) | Re-classifies into high-risk; the cabinet has not done a conformity assessment. |
| Matter involves special-category data + the client has explicitly objected to AI-assisted processing | Respect the objection regardless of lawful basis. |
| Cabinet partner discretion | Override available — document the reason in the matter file. |

**Compliance trace:** the case-management system carries a per-matter flag *"AI-assisted: yes/no"*. The decision tree from AI Act position paper §9 Annex B feeds the flag.

---

## 4. Competence + training

AI Act Art. 14(4)(a) — the human overseeing the AI must have *"the necessary competence, training and authority"*. The cabinet's implementation:

- **Initial training** (1 hour) for every avocat before they get MCP access:
  - What anno-rag does + does not do (it retrieves, it does not adjudicate).
  - What pseudonymisation guarantees (and what it does NOT guarantee — see DPIA v1 §3 R5).
  - The three tiers above.
  - How to recognise a model hallucination (citations check, factual-claim cross-reference).
  - How to use `memory_forget` and `/v1/subjects/forget`.
- **Annual refresher** (30 min): updates on platform changes, lessons-learned from any incident in the prior year, AI Act / GDPR updates.
- **Acknowledgement** signed at training + at every refresher; kept in the cabinet's HR / training file.

---

## 5. The "do not over-rely" trap (AI Act Art. 14(4)(b))

Art. 14(4)(b) — the overseer must be aware of *"the possible tendency of automatically relying or over-relying on the output"*. Three operational rules:

1. **The model never has the last word on a citation.** Every citation in an outgoing deliverable must be verified against the source.
2. **The model never has the last word on a quantitative claim.** Every number (deadline, amount, percentage) must be verified against the source.
3. **The model never has the last word on a strategic recommendation.** The avocat makes the call.

If an avocat finds themselves shipping AI-generated text without performing these checks, **that is the over-reliance trap**, and it is a déontologie issue under RIN art. 6, not merely an internal policy issue.

---

## 6. Stopping the system (AI Act Art. 14(4)(e))

Art. 14(4)(e) — the overseer must be able to *"intervene on the operation of the high-risk AI system or interrupt the system through a 'stop' button or a similar procedure"*. Even though the cabinet operates non-high-risk, the equivalent must be observable:

| Stop action | How | Who can trigger |
|---|---|---|
| **Per-matter stop** | Disable MCP for that matter's session in the case-management flag (§3.3). | Responsible avocat or any partner. |
| **Per-user stop** | Revoke the user's bearer token; restart the gateway. | IT Lead on request from MP or DPO. |
| **Whole-cabinet stop** | `systemctl stop anno-privacy-gateway` + `systemctl stop anno-rag-mcp`. | IT Lead. |
| **Vault freeze** | Move `vault.bin` to a read-only mount + restart. New tokenisations fail; reads continue from the existing entries. | IT Lead. |
| **Permanent shutdown** | All of the above + archive the index + delete keys. | Managing partner. |

**Drill:** the per-user stop should be exercised at least once a year (e.g. as part of the U13 tabletop). Confirm the avocat actually loses access and the audit log reflects the cut-off.

---

## 7. Monitoring the human overseer

Provider obligations under AI Act Art. 14(2) extend to the **design** of oversight. For the cabinet's profile, the monitoring obligation reduces to:

- **Detection of obvious over-reliance** — `memory_save` events without prior `memory_recall` of the same kind suggest unreflected automation. Not currently instrumented; a v0.6 candidate metric.
- **Detection of competence gaps** — repeated `memory_forget` immediately after `memory_save` may indicate misuse. Same v0.6 candidate.
- **Audit of subject-rights handling** — the DPO reviews the gateway audit log monthly for `subjects/forget` events that lack a corresponding DPO file note → process gap.

The cabinet should NOT instrument deeper surveillance of avocats' use — the goal is competence support, not staff monitoring. Counter to the AI Act Art. 26(7) obligation to inform workers + their representatives of AI use, which the cabinet's labour-law counsel must cover.

---

## 8. Mapping to neighbouring documents

| Anchor | This doc closes |
|---|---|
| AI Act position paper v1 §3 (C-3) | The C-3 constraint operationalised here in §3 (three tiers). |
| DPIA v1 §2.2 (no Art. 22 profile) | Confirmed by the tier-1 + tier-3 rules — no AI-only decision with legal effect. |
| Data-subject pack v1 §1.2 (operations register) | Tier-2 sign-off for subject-rights operations comes from this protocol. |
| v0.4 deployer guide §6 | The "stop the gateway" path here in §6 is the operator runbook. |
| Breach playbook v1 §3.1 | Containment actions (stop the gateway, freeze vault) reuse the procedures here. |

---

## 9. Compliance checklist for the DPO's annual review

- [ ] Every avocat with MCP access has signed the §4 acknowledgement within the past 12 months.
- [ ] Tier-2 logged events for the past month show appropriate authorisation (responsible avocat for in-scope ops, DPO for subject-driven ops).
- [ ] No Tier-3 matter has shipped an AI-assisted output (audit by sampling).
- [ ] The §6 per-user stop drill has been exercised in the past 12 months.
- [ ] The competence-gap metrics (when v0.6 ships) have been reviewed and show no concerning pattern.
- [ ] The annual refresher has been delivered.
- [ ] This document has been re-validated by the DPO + managing partner.

---

## 10. Document control

| Version | Date | Author | Change |
|---|---|---|---|
| v1 | 2026-05-15 | anno team | Initial — closes U9. Pending DPO + MP sign-off. |
| [next] | TBD | DPO | Update after first annual review. |
