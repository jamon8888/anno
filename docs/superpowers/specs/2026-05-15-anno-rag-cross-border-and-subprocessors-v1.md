# Anno-RAG — Cross-border transfer review + sub-processor register v1

> **Audience:** the cabinet's DPO and contracting partner.
>
> **Status:** v1 — drafted 2026-05-15. **Closes U11 (cross-border review) + U12 (sub-processor DPA register)** of the readiness spec.
>
> **Anchors:** GDPR Chapter V (Arts. 44–50), Commission SCC 2021 (Decision (EU) 2021/914), EDPB Recommendations 01/2020 on supplementary measures (Schrems II), Anthropic's enterprise DPA + sub-processor list (cabinet-specific).
>
> **Disclaimer:** templates and analysis. Requires DPO + outside-counsel sign-off before being relied upon for actual transfers.

---

## Part A — Cross-border transfer review (U11)

### A.1 Why a review

Anno-rag is on-premise. The only personal-data egress to a non-EEA processor is:

| Path | Destination | Data |
|---|---|---|
| Privacy gateway → Anthropic LLM endpoint | US (Anthropic primary), EU (if Anthropic EU region available + selected) | **Pseudonymised** request bodies + responses. No cleartext PII (DPIA v1 §1.2 data-flow diagram + ADR-001). |
| HuggingFace Hub model downloads | varies (typically US + CDN) | **No personal data** — model weights only, one-shot at install. Out of scope of GDPR Art. 44. |

The cabinet's Schrems-II analysis therefore reduces to **one path: Anthropic** (or whichever LLM provider is wired into `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE`).

### A.2 Transfer Impact Assessment (TIA)

#### A.2.1 Mapping

| TIA element | Anno-rag → Anthropic |
|---|---|
| **Importer** | Anthropic (PBC). For EU residents → Anthropic Ireland Limited, where contractually offered; else Anthropic US. |
| **Exporter** | The cabinet, as controller. |
| **Transfer tool** | SCC 2021 Module 2 (controller → processor) embedded in the Anthropic enterprise DPA. |
| **Data types** | Pseudonymised text bodies (PERSON_NNN, EMAIL_NNN, etc.) + free-text legal narrative in tokenised form. **NO** cleartext PII. **NO** Art. 9 special categories transmitted in cleartext. |
| **Volume** | Estimate: ~[N] requests/month, ~[M] kB/request average. |
| **Subjects** | Indirectly: every person mentioned in any chunk that gets retrieved + fed to the model. The cabinet must size this in §A.4. |
| **Purpose** | LLM completion for drafting + research assistance. |
| **Retention at importer** | Per Anthropic enterprise terms: **zero-retention tier** when contracted. The cabinet must confirm this tier is enabled on its account. |
| **Onward transfers** | Anthropic's sub-processors per their published list (Cloudflare, AWS / GCP infra, etc. — list versioned). |

#### A.2.2 Local-law analysis (Schrems II)

US law issues raised by Schrems II:
- FISA §702 — bulk-access for foreign-intelligence purposes against electronic communication service providers.
- Executive Order 12333 — signals intelligence outside US borders.

**Cabinet's exposure to §702:** Anthropic is plausibly an "electronic communication service provider" under FISA §702. A FISA §702 directive could in principle compel disclosure of stored data.

**Mitigations available:**

| Mitigation | Effectiveness |
|---|---|
| Pseudonymisation at the boundary (ADR-001) | **Strong.** Anthropic stores tokens, not names. Even on FISA disclosure, the disclosed data is the pseudonymised request/response — re-identification requires the vault, which never leaves the cabinet's host. |
| Zero-retention contractual tier | **Strong.** If Anthropic does not retain request/response bodies, there is nothing to disclose under a FISA order. Contractual obligation backed by Anthropic's standard practice. |
| EU-region selection (where available) | **Moderate.** Reduces FISA exposure for stored data; FISA §702 can still reach US-headquartered entities even for EU-hosted data, but the practical reach is lower. |
| TLS in transit + AES-GCM tag on the cabinet-side vault | **Standard.** Encryption-in-transit is table stakes; vault-side AES-GCM is irrelevant to the Anthropic-side exposure (data leaves the vault before egress). |
| The cabinet maintains its own LLM gateway with a kill switch | **Strong operationally.** If Anthropic's posture changes, the cabinet can disconnect by changing one env var. |

**Combined assessment:** the cabinet's transfer is *de facto* effective under SCC 2021 Module 2 **conditional on**:
- The zero-retention tier being contractually active.
- The pseudonymisation pipeline (ADR-001) functioning as designed (v0.7 baseline holds: 1.0/1.0 across the regex tier; 1.0/1.0 on Person/Org; 0.93 on Location).
- The cabinet monitoring Anthropic's sub-processor changes (§B.3 below).

#### A.2.3 Documented residual risks

| Residual | Acceptance basis |
|---|---|
| Pseudonymised text + unique factual context can sometimes be re-identified (DPIA v1 R5). | Acceptable for the cabinet profile — the threat model assumes the importer doesn't have the vault. Re-identification effort is high; for high-profile matters the cabinet should consider an additional layer (e.g. matter-level "no-AI" tag for sensitive cases). |
| FISA §702 could in principle reach Anthropic's audit logs. | Mitigated by zero-retention. The cabinet logs no cleartext at the gateway. |
| Onward-transfer to Anthropic sub-processors (Cloudflare, AWS/GCP) — all standard SCC-covered. | Accepted at the contractual level; the cabinet monitors the published sub-processor list. |
| Anthropic policy change widening training opt-in. | Mitigation: the cabinet has the enterprise contract explicitly opting out of training. Re-confirm on every renewal. |

#### A.2.4 Conclusion

**The cabinet may rely on SCC 2021 Module 2 + the supplementary measures described above** for its anno-rag → Anthropic transfer, **conditional on** the four bullet points in §A.2.2 holding at all times. The DPO must re-validate this conclusion:
- Annually.
- On every Anthropic terms-of-service update.
- On every change to the cabinet's deployment topology (e.g. switching upstream provider).
- On any Schrems-III or similar legal-context shift.

The cabinet records this conclusion + the date in its GDPR records. A copy lives in the Anthropic DPA file.

### A.3 Notification to data subjects

The standing Art. 14 notice in the data-subject pack §2.4 already mentions:

> *"Certains sous-traitants techniques (Anthropic, États-Unis) peuvent accéder à des données pseudonymisées sous le régime des clauses contractuelles types (CCT) approuvées par la Commission européenne, complétées par des mesures techniques supplémentaires (chiffrement AES-256-GCM des correspondances pseudonymes ↔ originaux)."*

That language is sufficient under Art. 13/14(1)(f). No further per-subject notice is required.

### A.4 Operational sizing (to be completed by the DPO)

- [ ] Monthly request volume to Anthropic: ~_____ requests, ~_____ MB.
- [ ] Number of unique subjects implicated per month (estimate via vault size + matter activity): ~_____.
- [ ] Confirmed zero-retention tier: yes / no / pending — last confirmed: _________ by _________.
- [ ] Anthropic sub-processor list version + URL: _________________________ (download a snapshot to the DPA file every quarter).
- [ ] EU region available + selected: yes / no / N/A — date: _________.

---

## Part B — Sub-processor DPA register (U12)

### B.1 Register

| # | Sub-processor | Role | Personal-data scope | DPA on file | Onward transfers | Notes |
|---|---|---|---|---|---|---|
| **S-1** | **Anthropic** (PBC + Anthropic Ireland Ltd) | LLM completion (privacy gateway upstream) | Pseudonymised request + response bodies. **No cleartext PII** under the ADR-001 contract. | **Yes** — enterprise DPA dated _________ . Zero-retention enabled. | Anthropic-published list (Cloudflare, AWS/GCP, etc.). The cabinet monitors quarterly. | TIA in Part A. |
| **S-2** | **HuggingFace Hub** | One-shot model-weight downloads at install time (e5-small, GLiNER2-Fastino) | **None.** Anonymous HTTP fetches of public model files. | **No** (not required — no PII processed). | HF's own CDN + hosting infrastructure. | Air-gapped installs can skip HF entirely by pre-loading model files; documented as a v0.5 candidate. |
| **S-3** | *Cabinet IT vendor* (laptop / server / backup hardware) | Hardware supply + maintenance | The vendor may incidentally see encrypted vault files + audit logs during maintenance. | **DPA required** — standard French cabinet IT contract template. | None expected. | Maintenance access logs maintained by the cabinet IT team. |
| **S-4** | *Cabinet backup provider* (offsite tape / cloud archive) | Backup storage | Encrypted vault + encrypted index + WORM audit log mirror. | **DPA required.** | Their own infrastructure provider. | Backup at-rest encryption is the vendor's responsibility AND the cabinet's belt-and-suspenders via §A vault encryption. |
| **S-5** | *Cabinet's KMS provider* (if v0.5+ U6 lands) | Key custody | The vault AES key + the audit HMAC key. **Critical custody.** | **DPA required** — likely an existing Microsoft / AWS / GCP / HashiCorp contract. | Per the KMS provider. | Move S-5 to "active" once U6 lands. |
| **S-6** | *Outside counsel firm* (for the cabinet's own legal / regulatory work — DPO+counsel review of the AI Act position, the DPIA, etc.) | Advisory | Sees the cabinet's own documents — DPIA + AI Act position + breach reports — which may contain personal data of clients. | **DPA / engagement letter** with confidentiality + data-protection clauses. | None. | Standard French legal-counsel privilege also applies. |
| **S-7** | *Forensic vendor* (per breach playbook §1) | Incident-response | Full host imaging during an incident — may include cleartext after the breach. | **DPA + retainer** signed BEFORE an incident occurs, not during. | None expected. | Activate only when needed. |

(Cabinet IT to fill in actual vendor names + dates in the S-3 / S-4 / S-6 / S-7 rows.)

### B.2 What each row's "DPA on file" actually requires

GDPR Art. 28(3) — every processor contract must cover:
1. The subject-matter + duration.
2. The nature + purpose of the processing.
3. The type of personal data + categories of data subjects.
4. The rights + obligations of the controller.
5. Specific obligations on the processor:
   - (a) process only on documented instructions;
   - (b) ensure personnel confidentiality;
   - (c) implement Art. 32 security measures;
   - (d) respect Art. 28(2)+(4) on sub-processor engagement;
   - (e) assist the controller in responding to subject rights;
   - (f) assist the controller in meeting Arts. 32–36 obligations;
   - (g) on termination, delete or return all personal data;
   - (h) make available all information necessary to demonstrate compliance + allow audits.

The cabinet's DPA template (its standard for legal-IT vendors) should be checked against this list **annually**. Add a checklist to the GDPR records.

### B.3 Sub-processor change-monitoring procedure

For S-1 (Anthropic) and S-5 (KMS) — the high-impact ones:

| Action | Cadence | Owner |
|---|---|---|
| Subscribe to provider's sub-processor-list change feed (email or RSS) | One-time setup | DPO |
| Review every notified change within 30 days | Per notification | DPO |
| If the change adds a new jurisdiction or a sensitive sub-processor: Schrems-II re-evaluation per §A.2.2 | Per change | DPO + outside counsel |
| Anthropic enterprise terms — full review on renewal | Annual | DPO + managing partner |
| KMS vendor — full review on renewal | Annual | DPO + IT Lead |

### B.4 Onward-transfer chain notification (Art. 28(4))

Each "real" sub-processor under Art. 28(2) requires the processor (cabinet) to:
- Get prior specific or general written authorisation.
- Be informed of intended changes with reasonable opportunity to object.

For S-1: Anthropic publishes a sub-processor list; cabinet's general authorisation covers this list as it stands at contract signing. **Change notifications must give the cabinet 30 days to object** — verify this clause is in the cabinet's Anthropic DPA.

If the cabinet objects to a sub-processor change, the options are:
- Continue without the new sub-processor (if Anthropic accommodates).
- Terminate the contract (if Anthropic doesn't accommodate the objection).
- Accept the change (default if no objection).

Document the chosen path in the GDPR records.

---

## Part C — Mapping to the readiness spec

| Gap | Status |
|---|---|
| **U11** Cross-border transfer review | ✅ v1 — Part A. Conditional on the four §A.2.2 bullets. Requires DPO completion of §A.4 sizing. |
| **U12** Sub-processor DPA register | ✅ v1 — Part B with 7-row register template + DPA-content checklist + change-monitoring procedure. Cabinet IT to fill in actual vendor names + DPA dates. |

The remaining open readiness-spec items after this document are **all code or cabinet-operational** (G7, U5, U6, U10) — no further pure-doc gaps. See `docs/superpowers/specs/2026-05-15-anno-rag-rgpd-aiact-readiness-design.md` for the updated state.

---

## Part D — Document control

| Version | Date | Author | Change |
|---|---|---|---|
| v1 | 2026-05-15 | anno team | Initial — closes U11 + U12. Requires DPO + outside-counsel sign-off + cabinet-IT register fill-in before deployment. |
| [next] | TBD | DPO + outside counsel | Post sign-off → v1 final. |
