# Anno-RAG — EU AI Act position paper v1

> **Subject:** classification of the anno-rag platform under Regulation (EU) 2024/1689 ("EU AI Act"), with specific focus on **Annex III §8(a)** (administration of justice and democratic processes).
>
> **Status:** v1 — drafted 2026-05-15. **Position paper, not legal advice.** Pending review by the cabinet's nominated legal counsel and the anno team's outside counsel before deployment.
> **Author:** anno team.
> **Recipients:** cabinet legal + DPO; anno team; outside counsel.
> **Closes:** U7 of the readiness spec (`docs/superpowers/specs/2026-05-15-anno-rag-rgpd-aiact-readiness-design.md` §3).

---

## TL;DR

**Anno-rag is NOT a high-risk AI system under Annex III §8(a) in the deployment profile described in the v1 DPIA** — a private legal cabinet using it as a retrieval and drafting-assistance tool for its own client work. The §8(a) trigger is *"intended to be used by a judicial authority or on its behalf"*; private lawyers/cabinets are not judicial authorities and do not act on their behalf in the regulatory sense. The provider role (anno team) and deployer role (the cabinet) attach to General Purpose AI (GPAI)-downstream obligations via Article 50 (transparency) plus general principles, but **not the Chapter III §2 high-risk regime**.

The position **changes** if the cabinet deploys anno-rag inside a **court registry, a prosecutor's office, a notary acting on judicial mandate, or a bailiff**. For those deployments, this paper does not apply — a separate Annex III §8(a) conformity assessment is required.

This paper documents the reasoning, the legal anchors, and the deployment-profile boundary so that the cabinet has a defensible position if challenged by a supervisory authority, and so that the anno team knows what assumptions the deployer is relying on.

---

## 1. Roles under the AI Act

The Act assigns obligations by role. The three roles relevant here:

| Role | Definition (Art. 3) | Who it is for anno-rag |
|---|---|---|
| **Provider** | "*A natural or legal person … that develops an AI system or a general-purpose AI model OR that has an AI system or a general-purpose AI model developed and places it on the market OR puts the system into service under its own name or trademark, whether for payment or free of charge*" (Art. 3(3)). | **anno team** for the anno-rag system (the detection + pseudonymisation + retrieval + memory pipeline). **Anthropic** for the underlying LLM that the privacy gateway routes to (a GPAI model under Art. 51). |
| **Deployer** | "*A natural or legal person … using an AI system under its authority except where the AI system is used in the course of a personal non-professional activity*" (Art. 3(4)). | **The cabinet** that installs anno-rag on its on-premise infrastructure and uses it for client work. |
| **Distributor** | An entity in the supply chain other than the provider/importer that makes an AI system available on the EU market (Art. 3(7)). | Not applicable — the anno team ships directly to the deployer; no third-party distribution. |

**Important:** the cabinet is *not* a provider just because it integrates anno-rag with its own case management. Art. 25 captures the "deployer becomes provider" trigger — it fires when (i) the deployer puts its name/trademark on a high-risk system already on the market, (ii) makes a substantial modification to a high-risk system, or (iii) modifies the intended purpose of a non-high-risk system such that it becomes high-risk. **None of those apply** for the deployment profile in §3 below.

---

## 2. The Annex III §8(a) question

### 2.1 The text

Annex III, point 8, of Regulation (EU) 2024/1689 lists, under the heading **"Administration of justice and democratic processes"**:

> "*(a) AI systems intended to be used by a judicial authority or on their behalf to assist a judicial authority in researching and interpreting facts and the law and in applying the law to a concrete set of facts, or to be used in a similar way in alternative dispute resolution;*"

If anno-rag is *"intended to be used by a judicial authority or on their behalf"*, it falls under Chapter III §2 (Art. 6 + Art. 8–22) — i.e. **high-risk obligations** including (a) risk management system (Art. 9), (b) data governance (Art. 10), (c) technical documentation (Art. 11), (d) logging (Art. 12), (e) transparency to deployer (Art. 13), (f) human oversight (Art. 14), (g) accuracy / robustness / cybersecurity (Art. 15), and (h) conformity assessment + CE marking before placing on the market (Art. 16, 43, 47).

### 2.2 "Judicial authority" — who counts?

The Act does not define "judicial authority" autonomously. Recital 61 clarifies:

> "*Considering the nature of the activities and the risks relating thereto, those high-risk AI systems should not include AI systems intended for purely ancillary administrative activities … of judicial authorities, such as anonymisation or pseudonymisation of judicial decisions, documents or data, communication between personnel, administrative tasks.*"

Two readings emerge:

1. **Strict reading.** "Judicial authority" = courts (judges, judicial registrars), public prosecutors, examining magistrates, and similar state organs that exercise judicial power. Private lawyers / cabinets are **excluded**.
2. **Broad reading.** Any actor that participates in the administration of justice — including private lawyers when they file submissions, draft pleadings, or argue cases. **This reading is not supported by the Act's structure.**

The cabinet plus the European Commission's accompanying communications [^1] consistently use the strict reading. The Recital 61 carve-out for *"anonymisation or pseudonymisation of judicial decisions"* also points to the strict reading — if pseudonymisation by a judicial authority is *itself* outside Annex III, then pseudonymisation in a private legal practice (which is the technical purpose of cloakpipe inside anno-rag) is well outside.

**Conclusion for the cabinet profile:** the cabinet is **not** a judicial authority. Lawyers acting for their clients in legal proceedings are exercising the *right to defence* (CFREU Art. 47); they are not exercising judicial power.

### 2.3 "On their behalf" — agency by mandate

The Act phrases the trigger as *"by a judicial authority or on their behalf"*. The natural reading of "on their behalf" requires an explicit mandate from a judicial authority — a court-appointed expert, an *expert judiciaire*, a notary acting on a notarial-court order, a *commissaire de justice* (huissier) executing a judicial decision. Routine cabinet work for a client is **not** "on behalf of" the court the matter is filed with — it is on behalf of the client.

**Edge cases the cabinet should flag to the anno team:**
- Court-appointed counsel (*avocat commis d'office*) using anno-rag for the assigned case — arguably "on behalf of" the appointing authority. **Defaults to Annex III §8(a)** in this paper. See §5 mitigation.
- Cabinet acting under a *mission d'expertise* mandate from a judge — same conclusion.
- Cabinet acting as *mandataire judiciaire* / *administrateur judiciaire* / *huissier* — same.

For these scenarios, the cabinet should *either* (a) not use anno-rag, *or* (b) trigger an Annex III §8(a) conformity assessment per the anno team's separate v0.8+ deployment package (TODO; not in scope of this paper).

### 2.4 The case-law-style negative argument

The Act took care to **list** the high-risk domains. Where it intended to cover assistance to private legal practice, it would have said so — as it did in Annex III §1(d) for *"AI systems intended to be used to evaluate the eligibility of natural persons for essential private services"*. Section 8 covers judicial and democratic processes specifically. **The omission is meaningful.**

---

## 3. The cabinet's deployment profile

This paper applies only to deployments that satisfy **all** of the following:

| Constraint | Detail |
|---|---|
| **C-1** | The deployer is a **private legal cabinet** (avocat, juriste d'entreprise, in-house counsel), **not** a court, prosecutor's office, judicial expert acting under court mandate, *commissaire de justice*, notary acting on judicial mandate, or administrative tribunal. |
| **C-2** | Anno-rag is used **for the cabinet's own client work** — research, drafting, case-file synthesis, internal note-taking. |
| **C-3** | The system's outputs are **always reviewed by a lawyer** before being used in any external communication (filing, advice, contract). The lawyer remains professionally responsible for the output (per their Ordre / Barreau rules). |
| **C-4** | Anno-rag is **not used to evaluate** the reliability of evidence, recidivism risk, or anything else listed in Annex III §6 (law enforcement) or §8(b)–(c). |
| **C-5** | The cabinet does **not** put its own name / trademark on the system in a way that triggers Art. 25(1)(a) "deployer becomes provider". |
| **C-6** | The cabinet does **not** make substantial modifications to the detection / pseudonymisation / retrieval pipeline (e.g. swapping the NER backend for a different model) without triggering an Art. 25(1)(b) re-classification check. Configuration changes (vault path, env vars, kind filter, etc.) are not substantial modifications. |

If any of C-1 to C-6 is breached, **the position in this paper no longer applies** and a fresh classification analysis is required.

---

## 4. Obligations that DO apply (deployer profile, non-high-risk)

Even outside Annex III, the AI Act imposes obligations. The cabinet should address each:

### 4.1 Article 50 — Transparency to natural persons

| Obligation | Applicability | Cabinet action |
|---|---|---|
| Art. 50(1) — deployer of an AI system that interacts with natural persons must inform them, unless obvious from context | **Indirect.** Anno-rag's privacy gateway routes prompts to Anthropic and returns answers. If the cabinet uses the answers in client communications (e.g. drafted email), and the client interacts with that AI-assisted text, the client should be informed. | **Cabinet adds a clause** to the engagement letter: "*Le cabinet peut utiliser des outils d'intelligence artificielle pour assister la recherche et la rédaction. Toutes les communications restent contrôlées et validées par un avocat.*" |
| Art. 50(2) — provider must ensure outputs are marked as artificially generated (watermark or similar). | **Applies to anno-rag's LLM-routed outputs.** Anthropic's enterprise terms and the Cowork client surface should carry the marking. | **Anno team** adds a follow-up to verify Anthropic's compliance markings flow through the gateway response. |
| Art. 50(3) — emotion recognition / biometric categorisation disclosure | **Not applicable** — anno-rag does neither. | None. |
| Art. 50(4) — deepfake disclosure | **Not applicable.** | None. |

### 4.2 Article 51–55 — General Purpose AI (Anthropic's model)

These are **provider** obligations on Anthropic, not on the cabinet. The cabinet's role is to (a) ensure Anthropic publishes the Art. 53 technical documentation, (b) ensure the Anthropic DPA + sub-processor commitments are current (handled by the DPO during contract renewal), and (c) document that the cabinet's use is in line with the GPAI's stated intended purpose.

### 4.3 Article 26 — Deployer obligations for non-high-risk systems

The Act distinguishes deployer obligations for **high-risk** systems (extensive, Art. 26) from **non-high-risk** (minimal). For non-high-risk, the principal obligations are:

- **Use the system in accordance with the provider's instructions** (anno team publishes deployer guide — TODO).
- **Monitor for changes** that might re-classify the system to high-risk (e.g. if the cabinet expands to court-appointed work).
- **Keep records** of substantial decisions taken on the basis of AI outputs — covered by the cabinet's normal case-file retention.

### 4.4 Article 95 — Codes of conduct (voluntary)

Even where non-high-risk, the Act encourages voluntary adoption of high-risk-style measures for foundation-of-trust systems. Recommended for the cabinet:

- **Maintain the v1 DPIA + the v0.7 anonymisation eval baseline** — both already exist.
- **Document the human oversight model** — every output reviewed by a lawyer. Add to the engagement letter clause.
- **Maintain the JsonlAuditSink chain** — already in v0.4.
- **Conduct an annual fundamental-rights impact assessment (FRIA)** — recommended even though Art. 27 strictly requires it only for high-risk in the public sector. Light-touch version: extend this position paper annually with any new use cases / sub-processors.

---

## 5. Mitigations for the edge cases in §2.3

For the *avocat commis d'office* / court-appointed-expert / *commissaire de justice* scenarios, the cabinet must **either**:

| Option | Steps |
|---|---|
| **A — Disable anno-rag for the mandate** | The lawyer flags the matter in the case management system as "court-appointed" → CMS configuration disables the anno-rag MCP server for that matter's session → the lawyer drafts without AI assistance. |
| **B — Apply the Annex III §8(a) conformity assessment** | The cabinet contracts the anno team for the deployment package (post-v0.8 milestone): risk management file (Art. 9), data governance audit (Art. 10), technical documentation (Art. 11), logging dossier (Art. 12), human-oversight protocol (Art. 14), accuracy + robustness + cybersecurity evidence (Art. 15), notified body conformity assessment (Art. 43). Only feasible at scale; not practical for a one-off mandate. |

**Recommendation:** Option A by default. Option B becomes viable once enough cabinets are in the same boat that the anno team can amortise the conformity-assessment cost.

---

## 6. Anno-team obligations (provider role)

The anno team, as provider of anno-rag, has the following non-high-risk obligations:

| Article | Obligation | Anno-team status |
|---|---|---|
| Art. 50(2) | If the system generates content that could be mistaken for human-authored, mark it. | **TODO**: pass-through Anthropic's marking; document in the v0.4 deployer guide. |
| Art. 95 | Voluntary codes of conduct. | **In progress**: this paper + the DPIA constitute the framework. |
| **If the deployment profile shifts toward Annex III §8(a)** — providers obligations under Chapter III §2 (Art. 8 onwards): |||
| Art. 9 — Risk management system | Iterative documented process across the lifecycle. | **Partially**: readiness spec + DPIA + this paper begin it. Formal RMS doc needed. |
| Art. 10 — Data and data governance | Training/validation/testing data documentation. | **Partially**: the v0.7 anonymisation eval baseline + the 35-doc FR legal corpus are documented. NER models (GLiNER2-Fastino, e5-small) carry their own data cards — must be referenced. |
| Art. 11 — Technical documentation | Comprehensive Annex IV documentation. | **TODO**: condense the v0.4/v0.6/v0.7/v0.1 specs + plans into an Annex IV technical file. |
| Art. 12 — Record-keeping | System must keep logs sufficient for traceability over its lifecycle. | **In place**: `JsonlAuditSink` hash chain + daily HMAC. |
| Art. 13 — Transparency / instructions for use | Deployer-facing instructions for use covering intended purpose, limits, oversight, expected lifetime. | **TODO**: v0.4 deployer guide (next no-compile follow-up). |
| Art. 14 — Human oversight | Design for human oversight. | **In place** by deployment profile: every output reviewed by a lawyer (C-3 in §3). |
| Art. 15 — Accuracy, robustness, cybersecurity | Defined levels + resilience. | **Partially**: v0.7 anonymisation eval gate (1.0 / 1.0 / 0.93 baselines); v0.5 performance budget; v0.4 bearer-auth + audit chain. |
| Art. 16 — CE marking + EU declaration of conformity | Required for high-risk only. | **N/A** under §3 profile. |
| Art. 19 — Quality management system | Required for high-risk providers. | **N/A** under §3 profile; informal QMS in place via the readiness spec + this DPIA + the CI gates. |

---

## 7. Open questions for outside counsel

| # | Question | Why it matters |
|---|---|---|
| Q-1 | Is the strict reading of "judicial authority" in §2.2 correct under French interpretive doctrine + the prepared communications from the European Commission? | Pivotal — the whole position rests on it. |
| Q-2 | Is *avocat commis d'office* covered by "on behalf of a judicial authority"? | Affects mitigation §5. |
| Q-3 | Is *commissaire de justice* executing a judicial decision covered? | Same. |
| Q-4 | Does the "anonymisation or pseudonymisation" Recital 61 carve-out provide a separate, stronger ground for excluding the *cloakpipe* component even if the broader RAG retrieval were re-classified? | Defence in depth — if Q-1 goes against us, cloakpipe alone may still be safe. |
| Q-5 | What is the supervisory authority for the cabinet's profile under the French national framework once the Act is fully operational? | Reporting obligations under Art. 26(5) (notify the supervisory authority of substantial incidents) — need to know who to notify. |
| Q-6 | Does the cabinet need to register the use of anno-rag in any French national AI register? | Possibly — depends on the Décret d'application. |
| Q-7 | What is the cabinet's exposure if a client argues that AI assistance in their case constitutes "automated processing" under GDPR Art. 22 — given the human-in-the-loop oversight? | Belt-and-suspenders argument; covered by C-3 + the DPIA. |
| Q-8 | What is the timeline for the AI Act's Chapter III §2 high-risk obligations becoming applicable to GPAI providers (Anthropic)? | Affects Q-2 (Anthropic's GPAI marking flow). 2 Aug 2026 for GPAI; 2 Aug 2027 for high-risk Annex III. |

---

## 8. Position summary

The cabinet's use of anno-rag, in the deployment profile described in §3, is **not high-risk under Annex III §8(a)** of the EU AI Act. The provider (anno team) and deployer (cabinet) both have transparency and recordkeeping obligations under Art. 50 + Art. 26 + Art. 95, which are addressed by the v0.4 GDPR core + v1 DPIA + this paper + the (forthcoming) v0.4 deployer guide.

The position is **conditional** on:
1. The six constraints in §3 holding throughout the deployment.
2. Outside counsel confirming the strict reading of "judicial authority" in §2.2 and the mandate scope in §2.3.
3. Anthropic's GPAI provider obligations under Art. 51–55 being met by Anthropic and verified by the cabinet DPO on contract renewal.
4. The cabinet documenting the human-oversight C-3 constraint in its engagement letter and ordre/barreau-facing process documentation.

If any condition fails, the cabinet must:
- Stop using anno-rag for the affected mandates (mitigation §5 option A), OR
- Contract the anno team for the Annex III §8(a) conformity assessment package (mitigation §5 option B), OR
- Re-classify and re-document.

---

## 9. Annexes

### Annex A — Legal anchors

| Reference | Use |
|---|---|
| Regulation (EU) 2024/1689 — Art. 3(3) | Definition of "provider". |
| Regulation (EU) 2024/1689 — Art. 3(4) | Definition of "deployer". |
| Regulation (EU) 2024/1689 — Art. 6 + Annex III §8(a) | High-risk trigger we are arguing does **not** fire. |
| Regulation (EU) 2024/1689 — Recital 61 | Carve-out for anonymisation/pseudonymisation by judicial authorities; supportive of our strict reading. |
| Regulation (EU) 2024/1689 — Art. 25 | "Deployer becomes provider" triggers — we argue none fire under §3. |
| Regulation (EU) 2024/1689 — Art. 26 | Deployer obligations (non-high-risk subset applicable). |
| Regulation (EU) 2024/1689 — Art. 50 | Transparency obligations to natural persons. |
| Regulation (EU) 2024/1689 — Art. 51–55 | GPAI obligations (apply to Anthropic). |
| Regulation (EU) 2024/1689 — Art. 95 | Voluntary codes of conduct. |
| Charter of Fundamental Rights, Art. 47 | Right to defence — supports the argument that private lawyers do not exercise judicial power. |
| French Code de l'organisation judiciaire | Definition of "autorité judiciaire" in domestic law. |
| Anthropic DPA (cabinet-specific contract) | GPAI sub-processor terms. |

### Annex B — Decision tree (for the cabinet's case management to flag scenarios)

```
For each new matter the cabinet opens:

    Q: Is the lawyer acting in a private mandate for a client?
        YES → Profile §3 applies → anno-rag use is OK.
        NO  → next question.

    Q: Is the lawyer court-appointed (avocat commis d'office,
       mandataire ad hoc, mission d'expertise, administrateur
       judiciaire, mandataire judiciaire, commissaire de justice
       executing a judicial decision)?
        YES → §5 option A (disable anno-rag for this matter).
        NO  → next question.

    Q: Does the matter involve evaluating evidence credibility,
       recidivism risk, or any task in Annex III §6 / §8(b) / §8(c)?
        YES → STOP. Anno-rag must not be used for this output.
              Re-route to a non-AI workflow.
        NO  → §3 applies → anno-rag use is OK.
```

A small CMS integration (a per-matter flag) is the cleanest implementation. Tracked as a deployer-side TODO; not a v0.4 anno-rag concern.

### Annex C — Mapping back to the readiness spec

| Readiness gap | Status after this paper |
|---|---|
| **U7** AI Act §8(a) position paper | **CLOSED v1** — this document, conditional on Q-1 to Q-8 in §7 being settled by outside counsel. |
| U8 (deployer-side AI Act instructions) | **OPEN** — this paper is the cabinet-facing analysis; the operator-facing deployer guide is the next no-compile follow-up. |
| U9 (human oversight) | **PARTIALLY ADDRESSED** by C-3 in §3 + Annex B decision tree. Full human-oversight protocol is a separate doc. |

### Annex D — Document control

| Version | Date | Author | Change |
|---|---|---|---|
| v1 | 2026-05-15 | anno team | Initial draft; closes U7 conditionally. |
| [next] | TBD | outside counsel | Resolve Q-1 to Q-8; promote to v1 final. |

---

[^1]: European Commission, *Communication on the EU AI Act — guidelines for high-risk AI systems in justice and democratic processes* (forthcoming as of drafting). The cabinet should re-check on every annual review.

*End of position paper v1.*
