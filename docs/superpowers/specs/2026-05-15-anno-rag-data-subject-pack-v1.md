# Anno-RAG — Data-subject information pack v1

> **Audience:** the cabinet's DPO + a managing partner reviewing the GDPR posture before deployment.
>
> **Status:** v1 — drafted 2026-05-15. **Templates, not legal counsel.** All three sections require legal review before being inserted into a binding engagement letter or third-party notice.
>
> **Closes** U2 (lawful basis register), U3 (Art. 14 notice template), U4 (retention policy) from the readiness spec (`docs/superpowers/specs/2026-05-15-anno-rag-rgpd-aiact-readiness-design.md` §3).
>
> **Companion documents:** DPIA v1, AI Act position v1, v0.4 deployer guide.

---

## 0. Why this pack exists

The DPIA v1 closes the "what risks exist and what mitigates them" question. This pack closes three open ends that depend on cabinet-side process more than on code:

| Gap | What it is | This pack's answer |
|---|---|---|
| **U2** | Lawful basis register — for each processing operation in the cabinet's anno-rag deployment, document the Art. 6 lawful basis + Art. 9 condition (where applicable). | §1 |
| **U3** | Art. 14 notice template — the cabinet ingests case files containing personal data of third parties (opposing counsel, witnesses, experts). Art. 14 GDPR obliges notice to those subjects. | §2 |
| **U4** | Retention policy — how long does the cabinet keep ingested embeddings + vault tokens after the underlying matter is closed? Currently the index has no automatic expiry. | §3 |

Each section is structured as **(a) the legal anchor, (b) the cabinet's recommended position, (c) the operational implementation**.

---

## 1. Lawful basis register (U2)

### 1.1 Method

For each distinct processing operation the cabinet runs over personal data via anno-rag, this register answers four questions:

1. **What** is processed?
2. **Who** are the data subjects?
3. **Which Art. 6 ground** legitimises it?
4. **Which Art. 9 condition** legitimises it, *if* special-category data is involved?

A processing operation is "distinct" when it has a different purpose, a different set of data subjects, or a different lawful basis from another. The cabinet should maintain this register as a living document, updated on any material change.

### 1.2 Operations register

| # | Operation | Data | Subjects | Art. 6 basis | Art. 9 condition (if special-category) |
|---|---|---|---|---|---|
| **P-1** | Ingest of client case files into the anno-rag corpus | Names, addresses, phone, email, NIR, SIRET, IBAN, free-text narrative | Cabinet clients, opposing parties, third parties named in case files | Art. 6(1)(b) (performance of the engagement contract with the client) for client data; **Art. 6(1)(f) (legitimate interest)** for opposing-party / third-party data, balanced via DPIA v1 §3 mitigations | Art. 9(2)(f) (legal claims) — covers personal-injury health data, employment-case sensitive categories, criminal-defence data |
| **P-2** | PII detection + pseudonymisation in the cloakpipe vault | Same as P-1 | Same as P-1 | Same as P-1 (data minimisation measure in service of P-1's basis) | Same as P-1 |
| **P-3** | Embedding + indexing of pseudonymised chunks into LanceDB | `text_pseudo` only — no cleartext PII | Same as P-1, indirectly | Same as P-1 (technical processing in service of P-1's purpose) | N/A — pseudonymised text carries no Art. 9 data directly; the vault holds the rehydration mapping under Art. 9(2)(f) |
| **P-4** | RAG retrieval over the cabinet's own corpus for drafting + research | `text_pseudo` + embeddings + FTS | Same as P-1, indirectly | Same as P-1 | Same as P-1 |
| **P-5** | LLM routing via the privacy gateway to Anthropic | Pseudonymised request bodies; tokens, not cleartext | Same as P-1 (indirectly — tokens map back) | Same as P-1 (Anthropic acts as sub-processor under Anthropic DPA) | Same as P-1 |
| **P-6** | Persistent session memory (`memories` collection) | `text_pseudo` + token_refs + entity_refs | Same as P-1, plus cabinet **users** (the lawyers' own preferences and session state) | Art. 6(1)(b) for cabinet-user data (employment contract); Art. 6(1)(f) for third-party data within saved memories | Same as P-1 |
| **P-7** | Art. 30 audit register (`JsonlAuditSink`) | Request id, provider profile, entity count, redaction count — **never PII** | None directly — events carry no identifying data | Art. 6(1)(c) (legal obligation under GDPR Art. 30) | N/A |
| **P-8** | Backups of vault + index | Same as P-1 (encrypted at rest) | Same as P-1 | Same as P-1 (necessary for integrity per Art. 32) | Same as P-1 |
| **P-9** | Subject-rights handling (find / forget / export) | Cleartext PII briefly at the request boundary | The requesting data subject | Art. 6(1)(c) (legal obligation under GDPR Art. 12–22) | N/A |

### 1.3 Notes on Art. 6(1)(f) "legitimate interest"

For P-1 to P-6 where third-party data is processed, the cabinet relies on legitimate interest. The Art. 6(1)(f) three-part test:

| Test prong | Cabinet's position |
|---|---|
| **(i) Legitimate interest pursued** | Effective legal representation of the cabinet's own client. CFREU Art. 47 (right to defence) underpins this. |
| **(ii) Necessity** | The data is necessary because case files cannot be redacted in advance without destroying their evidential value. Pseudonymisation in anno-rag is the *minimisation* step that makes processing proportionate. |
| **(iii) Balancing** | Third-party rights are protected by: (a) cleartext PII never leaves the cabinet's vault (DPIA v1 risk R1 mitigations); (b) Art. 17 erasure is honoured via `/v1/subjects/forget` with vault cascade; (c) sub-processor egress carries only pseudo-tokens; (d) the audit register lets the cabinet prove non-misuse. **The cabinet judges the third party's interests do not override the legitimate interest because the processing is internal, pseudonymised, and erasure-on-request is operational.** |

The balancing must be **documented per matter** when a third party objects. The default register entry is operative; per-objection responses go to the DPO file.

### 1.4 Notes on Art. 9(2)(f) "legal claims"

Art. 9 special categories (health, religion, union membership, sexual orientation, ethnic origin, political opinion, criminal data, biometric, genetic) may appear in case files even when the matter is not framed around them — e.g. an employment file mentioning a union meeting, a personal-injury file with health data, a custody file with religious-upbringing references.

Art. 9(2)(f) authorises processing when *"necessary for the establishment, exercise or defence of legal claims or whenever courts are acting in their judicial capacity"*. This is **the cabinet's primary Art. 9 ground**. It is broad and specifically designed for legal practice; it does NOT require explicit consent from the data subject.

**Operational consequence:** the cabinet does NOT need to refuse a matter just because the file mentions a special category. It DOES need to (a) ingest those files via anno-rag with the same pseudonymisation guarantees as the rest, (b) document in the matter file that Art. 9(2)(f) applies, (c) respond to Art. 17 erasure requests the same way as for any other category.

### 1.5 Sub-processors

The register implicates two sub-processors:

| Sub-processor | Operation | Anchor |
|---|---|---|
| **Anthropic** | P-5 LLM routing | Anthropic DPA (cabinet-specific); zero-retention enterprise terms; SCCs for US transfer |
| **HuggingFace Hub** | One-shot model download at install time | Public model card; **no PII transmitted** — only model weights download |

Add to the sub-processor section of the cabinet's GDPR records.

---

## 2. Art. 14 notice template (U3)

### 2.1 The obligation

GDPR Art. 14 obliges the controller to inform data subjects **whose data the controller has obtained from a source other than the data subject themself**. The cabinet's typical Art. 14 cases via anno-rag:

| Trigger | Example | Notice required? |
|---|---|---|
| Opposing party in adversarial litigation | "M. X demande…" appears in client's account of the dispute | **Yes**, but with Art. 14(5)(b) exception possible (see §2.3) |
| Witness named in correspondence | The cabinet's client describes a meeting; the email cc'd a witness | **Yes**, with exception possible |
| Third-party expert | Court-appointed expert's report mentions other parties | **Yes**, with exception possible |
| Cabinet employees mentioned in client communications | Email forwarded by client mentions "votre collaborateur Y" | **Yes**, with exception possible if employee is under contract |
| Identifiable third parties in a contract | Other party to a deal the cabinet is structuring | **Yes** — the notice is typically delivered during deal closing by the principals |

### 2.2 Required content (Art. 14(1)–(2))

- (a) identity + contact details of the controller (the cabinet);
- (b) contact details of the DPO if appointed;
- (c) purposes of processing + lawful basis;
- (d) categories of personal data concerned;
- (e) recipients (including sub-processors);
- (f) transfers outside the EEA + safeguards;
- (g) retention period (per §3 of this pack);
- (h) rights (access, rectification, erasure, restriction, objection, portability, complaint to CNIL);
- (i) right to withdraw consent (if applicable — usually not, since Art. 9(2)(f) is the ground);
- (j) source of the data;
- (k) automated decision-making (Art. 22) — **none in the cabinet's case** — see AI Act position paper §1.

### 2.3 Art. 14(5) exceptions the cabinet may invoke

Art. 14(5) excuses the notice when:

| Letter | Exception | Cabinet usage |
|---|---|---|
| (a) | The subject already has the information | Often true when the third party knows they are involved in the matter (e.g. adversarial party served with proceedings). |
| (b) | Providing notice **proves impossible or would involve a disproportionate effort** | Common for "third party mentioned in a 200-page document set"; must be **documented** in the matter file. Disproportionate-effort requires the cabinet to **publish a generic notice** (per Art. 14(5)(b), 2nd para). |
| (c) | Notice would obviously compromise the objective of the processing | Useful in adversarial proceedings where pre-notification would tip off the opposing party in an asset-discovery case. Use sparingly. |
| (d) | Professional secrecy / legal-professional privilege | **The cabinet's strongest ground** in many cases. The matter is privileged (avocat-client privilege under FR law); notifying the third party would breach it. Document case-by-case. |

**Recommendation:** the cabinet should default to **publishing a generic standing notice on its public website + maintaining the per-matter case for an exception** when the third party is not directly notified. This is the Art. 14(5)(b)+(d) combination most large French cabinets are settling on.

### 2.4 Template — generic standing notice (cabinet website)

```
─────────────────────────────────────────────────────────────────────
INFORMATION SUR LE TRAITEMENT DE VOS DONNÉES PAR LE CABINET [Nom]
Article 14 du Règlement Général sur la Protection des Données (RGPD)

Le cabinet [Nom] (le « Cabinet ») peut être amené à traiter des données
à caractère personnel vous concernant lorsque celles-ci apparaissent
dans les dossiers confiés à l'un de ses avocats par un client. Cette
notice satisfait l'obligation d'information prévue à l'article 14 RGPD.

1. RESPONSABLE DU TRAITEMENT
   Cabinet [Nom], [adresse complète], inscrit au Barreau de [Ville].
   Contact : [email général]
   DPO : [Nom + email du DPO]

2. FINALITÉS ET BASES LÉGALES
   Vos données peuvent être traitées dans le cadre des missions de
   représentation et de conseil juridique confiées au Cabinet par ses
   clients :
   - Article 6(1)(b) RGPD pour les données du client lui-même ;
   - Article 6(1)(f) RGPD (intérêt légitime du Cabinet à fournir une
     représentation juridique effective à son client) pour les données
     des tiers ;
   - Article 9(2)(f) RGPD pour les catégories particulières de données
     (santé, opinions politiques, appartenance syndicale, etc.) lorsque
     leur traitement est nécessaire à la constatation, à l'exercice ou
     à la défense d'un droit en justice.

3. CATÉGORIES DE DONNÉES TRAITÉES
   Selon le dossier : identité, coordonnées, données professionnelles,
   contenu de correspondances, données contractuelles, données
   judiciaires, et, le cas échéant, données de l'article 9 RGPD.

4. SOURCES
   Vos données nous parviennent de notre client, de pièces de
   procédure communiquées par d'autres parties ou par les juridictions,
   ou de sources publiques (registres officiels, presse).

5. DESTINATAIRES
   Vos données sont conservées au sein du Cabinet et accessibles
   uniquement aux avocats et collaborateurs dûment habilités. Elles
   peuvent être communiquées :
   - aux juridictions saisies du dossier ;
   - aux autres parties dans le respect du contradictoire ;
   - à nos sous-traitants techniques (notamment Anthropic dans le
     cadre de la rédaction assistée par intelligence artificielle —
     les données sont pseudonymisées avant transmission).

6. TRANSFERTS HORS UE
   Certains sous-traitants techniques (Anthropic, États-Unis) peuvent
   accéder à des données pseudonymisées sous le régime des clauses
   contractuelles types (CCT) approuvées par la Commission européenne,
   complétées par des mesures techniques supplémentaires (chiffrement
   AES-256-GCM des correspondances pseudonymes ↔ originaux).

7. DURÉES DE CONSERVATION
   - Pendant la durée du mandat ;
   - Puis pendant la durée de prescription civile, pénale ou
     disciplinaire applicable au dossier (typiquement 5 à 10 ans) ;
   - Au-delà, sur archivage anonymisé.

8. VOS DROITS
   Vous disposez des droits suivants : accès, rectification, effacement
   (article 17 RGPD), limitation, opposition, portabilité, retrait du
   consentement (lorsque applicable), réclamation auprès de la CNIL.
   Pour les exercer : [email du DPO].
   Note : certains droits peuvent être restreints lorsque leur
   exercice porterait atteinte au secret professionnel de l'avocat
   ou aux droits de la défense.

9. DÉCISION AUTOMATISÉE
   Le Cabinet n'utilise pas de système de décision entièrement
   automatisée produisant des effets juridiques vous concernant.
   Les outils d'aide à la rédaction sont utilisés sous contrôle
   constant d'un avocat qui demeure responsable des décisions
   prises.

Date de mise à jour : [JJ/MM/AAAA]
─────────────────────────────────────────────────────────────────────
```

### 2.5 Template — per-matter direct notice (when not exempted)

```
─────────────────────────────────────────────────────────────────────
NOTICE PARTICULIÈRE DE TRAITEMENT — Article 14 RGPD
À l'attention de : [Nom + adresse]
Dossier : [Référence interne]
Date : [JJ/MM/AAAA]

Madame, Monsieur,

Nous portons à votre connaissance que le cabinet [Nom] est amené à
traiter des données à caractère personnel vous concernant dans le
cadre du dossier référencé ci-dessus, confié à notre cabinet par notre
client [Nom du client, sauf si le secret professionnel s'y oppose,
auquel cas indiquer "un client dont l'identité est couverte par le
secret professionnel"].

[Reprendre les rubriques 2 à 9 de la notice générique en les
particularisant au dossier : catégories effectivement traitées, source
précise (« information communiquée par notre client le … » ou « pièce
n° X du dossier »), durée prévisionnelle de conservation.]

Pour exercer vos droits ou poser toute question relative à ce
traitement, vous pouvez écrire à notre DPO : [email du DPO].

Cordialement,
[Avocat responsable] / [Cabinet]
─────────────────────────────────────────────────────────────────────
```

### 2.6 Operational checklist

For the cabinet's matter-opening workflow:

- [ ] On matter open, the responsible avocat assesses whether identifiable third parties appear in the case file beyond the client.
- [ ] If yes, classify each third-party set into one of: (a) already informed (adversarial party served); (b) covered by the standing notice on the website; (c) needs direct notice; (d) Art. 14(5)(b) disproportionate-effort exception; (e) Art. 14(5)(d) professional-secrecy exception.
- [ ] For (c), send the §2.5 template + log the dispatch in the matter file.
- [ ] For (d) and (e), **document the exception in the matter file** with one paragraph of reasoning. Make this discoverable by the DPO.
- [ ] On matter close, the avocat confirms with the DPO that no outstanding Art. 14 obligations remain.

---

## 3. Retention policy (U4)

### 3.1 Anchors

| Source | Rule |
|---|---|
| French legal-records retention (déontologie, RIN) | Avocats: 5 years post-matter close minimum; 10 years for matters likely to give rise to professional-liability claims; longer for matters involving minors (until adult age + statute of limitations). |
| Code de la consommation L. 218-2 | 2 years for consumer claims. |
| Code de commerce L. 110-4 | 5 years for commercial obligations. |
| Code civil 2224 | 5 years general civil prescription. |
| Code de procédure pénale 7 / 8 / 9 | 6 / 6 / 20 years criminal prescription. |
| GDPR Art. 5(1)(e) | Personal data kept no longer than necessary for the processing purposes. |
| Anno-rag-specific | Vault, embeddings, audit register — distinct retentions below. |

### 3.2 Per-asset retention table

| Asset | Retention rule | Trigger for deletion |
|---|---|---|
| **Cleartext case files** (out of scope of anno-rag — held by the cabinet's case management) | Per the cabinet's existing déontologie policy: 5–10 years post-matter close. | Matter-close + retention-period elapsed → cabinet's normal records-disposal process. |
| **`text_pseudo` chunks in `chunks` LanceDB collection** | Same as the cleartext source. Pseudonymised text is still personal data under GDPR; retention runs with the source. | Matter-close + retention-period elapsed → `Pipeline::forget` for each subject named in the matter, OR per-document erasure (v0.5+ candidate). |
| **`memories` collection rows** | **2 years** for general session memories; **0 days (drop on session end)** for `MemoryKind::Context`; matter-bound retention for `Fact` / `Preference` / `Reference` linked to a specific matter. | TTL ticker (v0.6 candidate) OR explicit `Pipeline::forget_memory`. |
| **Vault entries** | Cascade-driven: a vault entry survives as long as **any** memory row OR chunk row references it. When the last reference is forgotten, `Pipeline::forget_memory`'s cascade purges the vault entry within 24 h (physical reclamation SLO). | Cascade. |
| **`JsonlAuditSink` daily files** | **5 years** (Art. 30 register norm). Daily `.sig` files stay with their JSONL. | Operator-managed disposal at the 5-year mark. Archive to cold offline storage; do NOT delete in-place (chain integrity for the period would be lost). |
| **Backups (`vault.bin`, index dir)** | 30 days rolling for daily; 12 months for monthly. | Backup-rotation policy at the cabinet IT level. |
| **Anthropic-side logs (sub-processor)** | Per the Anthropic DPA — zero-retention enterprise tier. | Anthropic-side. |

### 3.3 Operational implementation

**Today (v0.4):**
- Matter-close triggers a manual workflow: the avocat lists the subjects in the matter, calls `/v1/subjects/forget` for each, verifies the cascade via the audit log.
- Audit log retention is operator-managed (cron + archive script).
- No automatic TTL on `chunks` or `memories` rows.

**v0.5 candidates:**
- **TTL field on `chunks`** — populate at ingest time with the matter's predicted close + retention. A daily sweeper deletes expired chunks via `Pipeline::forget_matter`. (Schema add — forward-compat hooks already in v0.1.)
- **Per-matter `Pipeline::forget_matter(matter_ref)`** — one-shot deletion of every chunk + memory tagged to a matter, with cascade. Operationally easier than per-subject.
- **Automatic memory TTL** — `compaction_min_age_secs` already exists; extend with a max-age policy.

**Tracked as readiness-spec follow-ups; not v0.4 blockers.**

### 3.4 Special cases

| Scenario | Rule |
|---|---|
| Matter involving a minor | Retention extends to the minor's age of majority + the underlying prescription period. The avocat tags the matter; the records-disposal workflow respects the tag. |
| Matter likely to give rise to professional-liability claims | 10 years minimum (RIN). Avocat tags at matter-close. |
| Matter terminated by client withdrawal before substantive work | 5 years from termination per déontologie. |
| Subject exercising Art. 17 erasure mid-matter | Honoured immediately via the cascade UNLESS Art. 17(3)(e) applies (necessary for the establishment, exercise or defence of legal claims). When (3)(e) applies, the cabinet documents the refusal-to-erase in the matter file and notifies the subject. |
| Subject exercising Art. 17 after matter close, within retention period | Same — Art. 17(3)(e) may still apply if appellate or enforcement actions remain plausible. Otherwise honoured. |
| Subject exercising Art. 17 after retention period | Should already be erased by the records-disposal process. If still present, honoured immediately. |

---

## 4. Mapping to the readiness spec

| Gap | Closed by |
|---|---|
| **U2** | §1 (operations register, 9 entries) |
| **U3** | §2 (Art. 14 notice templates — generic + per-matter + checklist) |
| **U4** | §3 (per-asset retention table + operational implementation + special cases) |

The remaining open gaps from the readiness spec — U5 (at-rest encryption), U6 (KMS), U9 (human oversight protocol detail), U10–U13 — are still pending. U5 + U6 are tracked for v0.5 anno code + cabinet IT. U9 is partially addressed by the AI Act position paper §3 (C-3 constraint) + this pack §3.4; full protocol is a separate doc.

---

## 5. Document control

| Version | Date | Author | Change |
|---|---|---|---|
| v1 | 2026-05-15 | anno team | Initial — closes U2, U3, U4. **Requires DPO + legal-counsel review before deployment.** |
| [next] | TBD | DPO + outside counsel | Validate templates, fill cabinet-specific identifiers, promote to v1 final. |
