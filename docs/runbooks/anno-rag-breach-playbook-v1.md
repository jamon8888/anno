# Anno-RAG — Personal data breach playbook v1

> **Audience:** the cabinet's DPO, managing partner, IT lead, and the anno team's on-call engineer.
>
> **Trigger:** any suspected personal-data breach affecting an anno-rag deployment (vault, audit log, gateway, memories, chunks).
>
> **Goal:** triage within **60 minutes**, decide CNIL notification within **24 hours**, file Art. 33 GDPR notification within **72 hours**, and notify affected data subjects within **72 hours of the decision to notify** when Art. 34 applies.
>
> **Status:** v1 — drafted 2026-05-15. Closes U13 of the readiness spec.
> **Companion docs:** DPIA v1, deployer guide, AI Act position v1, data-subject pack v1.

---

## 0. Definitions

| Term | Definition |
|---|---|
| **Personal data breach** (GDPR Art. 4(12)) | A breach of security leading to the **accidental or unlawful destruction, loss, alteration, unauthorised disclosure of, or access to, personal data** transmitted, stored or otherwise processed. |
| **Confidentiality breach** | Unauthorised disclosure or access. The dominant risk for anno-rag (vault compromise, audit log read by attacker, etc.). |
| **Integrity breach** | Unauthorised alteration. E.g. vault file tampering, audit chain forgery attempt. |
| **Availability breach** | Accidental or unlawful loss of access or destruction. E.g. ransomware encrypting the vault, disk failure wiping the index. |
| **DPA** | Supervisory authority — for the cabinet's deployment in France, **the CNIL** (Commission nationale de l'informatique et des libertés). |
| **Affected data subject** | Any natural person whose personal data is in the breach scope. For anno-rag: cabinet clients, opposing parties, third parties named in case files, cabinet employees. |

---

## 1. Roles + contacts

This playbook assumes the following roles. **Fill in real names + 24×7 contact details before deployment** and keep them updated.

| Role | Name | Primary contact | Backup contact | Responsibility |
|---|---|---|---|---|
| **Breach Coordinator** | [DPO] | [phone] [email] | [backup phone] | Owns the timeline, makes the call on CNIL/Art. 34 notification, drafts notifications. |
| **Managing Partner** | [name] | [phone] [email] | [backup] | Final sign-off on external communications. Authority to engage outside counsel. |
| **IT Lead** | [name] | [phone] [email] | [backup] | Owns containment + forensic preservation + technical investigation. |
| **Anno on-call** | [anno team rota] | [pager] | [escalation] | Technical expert on the anno-rag stack. Investigation support; patch delivery if needed. |
| **Outside counsel** | [firm] | [partner] | — | Engaged when regulatory exposure or litigation risk emerges. |
| **Forensic vendor** (optional) | [pre-contracted firm] | [hotline] | — | Engaged for severe incidents (vault key compromise, ransomware). |
| **CNIL** | Notification portal | https://notifications.cnil.fr/notifications/index | — | DPA contact. |

---

## 2. Detection paths

Where breach signals come from. Each is a **trigger** that activates §3 triage.

| # | Detection path | Signal | Operator action |
|---|---|---|---|
| **D-1** | **Audit chain integrity failure** | `verify_audit.py` (deployer guide §5.4) reports a broken `prev_hash` / `this_hash` / `.sig` mismatch. | Immediate — see §3.1. |
| **D-2** | **Authentication anomaly** | Repeated 401s on `/v1/*` from an unusual source; OR a successful 200 from an IP that does not match the cabinet's authorised range. | Block source; check audit log; §3.1. |
| **D-3** | **Vault decryption failure** | `Vault::open` raises an AES-GCM AEAD-tag error on startup or on a save. Indicates tampering, corruption, or wrong key. | Stop the gateway; freeze the vault file; §3.1. |
| **D-4** | **Index/disk corruption** | LanceDB returns I/O errors on `chunks` or `memories`; backup restore needed. | Containment — preserve current state before restoring; §3.1. |
| **D-5** | **Ransomware indicator** | Files in `$ANNO_GATEWAY_VAULT_PATH` or `$ANNO_GATEWAY_AUDIT_DIR` renamed with an unknown extension, OR a ransom-note file appears. | Isolate the host immediately. §3.1 + escalate to forensic vendor. |
| **D-6** | **Lost / stolen device** | Cabinet laptop / server / backup tape unaccounted for. | Treat as confidentiality breach pending evidence to the contrary; §3.1. |
| **D-7** | **Sub-processor incident** | Anthropic publishes a breach disclosure affecting the period the cabinet was using their API. | DPO triages whether pseudonymised egress was in scope; §3.1. |
| **D-8** | **Credential leak** | Bearer token or vault key appears in a public source (paste site, leaked dump, accidental git commit). | Rotate immediately (deployer guide §6.1); §3.1. |
| **D-9** | **Insider concern** | Cabinet employee accessed data outside their need-to-know; HR or DPO flag. | DPO triage; §3.1. |
| **D-10** | **Subject report** | A data subject (client or third party) contacts the cabinet alleging their data was disclosed. | DPO acknowledges + opens §3.1 even if the initial assessment is "no breach occurred". |

**Detection cadence:** D-1 should run nightly via cron. D-2 needs a reverse-proxy / SIEM rule (track when the v0.5 native rate-limit lands; until then, host-level fail2ban or equivalent). D-3 / D-4 are caught by the gateway / pipeline returning errors — operator must wire alerts.

---

## 3. The 72-hour timeline

### 3.1 T+0 to T+1h — Triage

Goal: confirm or rule out the breach in **under one hour**.

- [ ] **Breach Coordinator opens an incident file** (timestamped Markdown in `/var/log/anno/incidents/YYYY-MM-DD-N.md`). Log every action with UTC timestamps from this point. **This file becomes the audit trail.**
- [ ] **IT Lead containment** — depending on detection path:
  - D-1, D-2: do **not** stop the gateway yet; the live audit chain is evidence. Block the offending source at firewall / reverse proxy.
  - D-3, D-4, D-5: **stop the gateway service** (`systemctl stop anno-privacy-gateway`). Disconnect the host from non-essential network. **Do NOT reboot** — preserves volatile state for forensics.
  - D-6, D-8: rotate credentials per deployer guide §6.1.
  - D-7: no immediate action on cabinet side; await Anthropic's incident details.
- [ ] **Preserve evidence:**
  - Copy `$ANNO_GATEWAY_AUDIT_DIR/` to a write-once location.
  - Copy `$ANNO_GATEWAY_VAULT_PATH` and the LanceDB index directory to a forensic image.
  - Copy systemd journal + reverse-proxy logs for the prior 7 days.
  - Note: **do not** delete the original locations — work on the copies.
- [ ] **Initial scope assessment** (best-effort, refined in §3.2):
  - What category of data is implicated? (PII categories, special categories, audit metadata only?)
  - How many data subjects, in order of magnitude? (Single, dozens, hundreds, thousands?)
  - Is the implicated data pseudonymised? Was the vault separately compromised?
- [ ] **Decision point — confirm or rule out:**
  - If **ruled out** (false positive, e.g. a backup script touched the file): close the incident file with the reasoning. No notification.
  - If **confirmed**: proceed to §3.2.
  - If **uncertain**: proceed to §3.2 in parallel with continued investigation.

### 3.2 T+1h to T+24h — Investigation + risk assessment

Goal: have enough information to make the **Art. 33 notification decision** at the 24-hour mark.

- [ ] **Forensic analysis** by IT Lead (escalate to forensic vendor for D-5 ransomware or D-3 confirmed tampering):
  - Identify the entry point (compromised credential? unpatched CVE? insider? supply-chain?).
  - Identify the scope window — when did the attacker have access, and when were they evicted?
  - Identify the data accessed / modified / lost. Use the audit chain to bound this: events outside the breach window are presumptively unaffected.
- [ ] **DPO risk assessment per Art. 33** — does the breach result in a risk to the rights and freedoms of natural persons? Use the WP29 / EDPB guidance on Art. 33 + the CNIL's "notifier ou non" decision tree. The cabinet's working test:

  | Factor | Low | Medium | High |
  |---|---|---|---|
  | **Data category** | Audit metadata only (no PII) | Standard PII (names, contact) | Special-category (health, religion, criminal, financial-account) |
  | **Identifiability** | Pseudonymised + vault uncompromised | Pseudonymised + partial vault exposure | Cleartext exposed |
  | **Volume** | <10 subjects | 10–500 | >500 |
  | **Recipient** | None (data destroyed without access) | Unknown / unauthorised inside the cabinet | External attacker, public exposure, criminal use |
  | **Permanence** | Recoverable from backups | Partially recoverable | Permanently lost or copied |
  | **Special vulnerabilities** | Adults, no special status | Minors involved | Clients in protective programmes, ongoing litigation |
- [ ] **Decision matrix:**
  - **All factors Low** → no Art. 33 notification (document the reasoning in the incident file; the decision and rationale must be retrievable on DPA request, even when no notification is sent).
  - **Any factor Medium** OR **2+ factors at any level above Low** → **Art. 33 CNIL notification.**
  - **Any factor High** OR likely "high risk to rights and freedoms" → **Art. 33 CNIL notification AND Art. 34 data-subject notification.**
- [ ] **Managing Partner briefing** — DPO + IT Lead present findings + recommendation. MP signs off on the decision before notifications are filed.
- [ ] **Outside counsel engagement** if MP determines regulatory exposure or litigation risk.

### 3.3 T+24h to T+72h — Notification

If Art. 33 applies:

- [ ] **File the CNIL notification** at https://notifications.cnil.fr/notifications/. Required fields (Art. 33(3) GDPR):
  - (a) Nature of the breach (categories + approximate number of data subjects, categories + approximate number of personal-data records concerned).
  - (b) DPO name + contact details.
  - (c) Likely consequences of the breach.
  - (d) Measures taken or proposed to address the breach + mitigate adverse effects.
- [ ] **If detail is incomplete at 72h** (Art. 33(4) allows phased reporting): file what is known; supplement as facts emerge.

If Art. 34 applies (high risk):

- [ ] **Identify affected subjects** from the audit log + vault find. For each subject, draft a §3.4 notice.
- [ ] **Send notices "without undue delay"** — typically within 72 h of the decision to notify. For >100 subjects, public communication (Art. 34(3)(c)) is an acceptable alternative when individual notification involves disproportionate effort.

If Art. 34 does NOT apply but a subject specifically asks whether their data was affected: respond honestly within 30 days (Art. 12 timing for access requests).

### 3.4 Notification templates

#### CNIL Art. 33 notification (free-text fields)

```
NATURE DE LA VIOLATION

Le [date], à [heure UTC], le cabinet [Nom] a détecté [description courte
de la violation : vol de fichier, intrusion réseau, ransomware, etc.]
affectant son système de gestion documentaire interne « anno-rag »
déployé sur site.

CATÉGORIES DE DONNÉES
- Catégories ordinaires : [nom, adresse, coordonnées, contenu de
  correspondances, données contractuelles, données judiciaires].
- Catégories particulières (article 9 RGPD) : [le cas échéant].

CATÉGORIES DE PERSONNES CONCERNÉES
- Clients du Cabinet : ~[N] personnes.
- Parties adverses et tiers nommés dans les dossiers : ~[N] personnes.
- Collaborateurs du Cabinet : ~[N] personnes.

NOMBRE APPROXIMATIF D'ENREGISTREMENTS
~[N] enregistrements pseudonymisés ; ~[N] entrées dans le coffre-fort
de pseudonymes.

[Si le coffre n'a PAS été compromis, mentionner explicitement :
« Les enregistrements applicatifs sont stockés sous forme pseudonymisée
(jetons de la forme PERSON_NNN) ; la table de correspondance vers les
valeurs en clair est chiffrée en AES-256-GCM et n'a pas été
compromise. La ré-identification probabiliste à partir des seuls
enregistrements pseudonymisés est limitée. »]

CONSÉQUENCES PROBABLES
[Liste : risque de ré-identification, atteinte au secret professionnel
de l'avocat, exposition financière (IBAN, SIRET), atteinte à la
réputation des personnes concernées, etc.]

MESURES PRISES OU PROPOSÉES
- Confinement : [actions techniques — arrêt du service, isolement
  réseau, révocation des clés].
- Préservation des preuves : [image forensique, copie du registre
  d'audit].
- Investigation : [investigation interne en cours / mandat à
  l'expert forensique [Nom]].
- Notification des personnes : [en cours / non requise au titre de
  l'article 34, motivation].
- Mesures correctives prévues : [renforcement des contrôles d'accès,
  rotation des clés, mise à niveau du gateway vers la version
  v0.5+ comportant le rate-limit / mTLS, audit complémentaire].
- Délégué à la protection des données : [Nom DPO, email, téléphone].

PIÈCES JOINTES
- Rapport d'incident interne, daté et signé par le DPO.
- Le cas échéant, rapport préliminaire de l'expert forensique.
```

#### Art. 34 notice to data subjects (FR-language template)

```
Objet : Information relative à un incident affectant vos données
        personnelles (Article 34 RGPD)

Madame, Monsieur,

Nous vous informons qu'un incident affectant la sécurité des données
personnelles a été détecté le [date] dans le système de gestion
documentaire interne du cabinet [Nom]. Vos données personnelles ont
été affectées.

NATURE DE L'INCIDENT
[Description en termes clairs et accessibles — éviter le jargon
technique. Ex. : « Un accès non autorisé à notre système a permis à
un tiers de prendre connaissance d'informations vous concernant. »]

DONNÉES CONCERNÉES
[Liste précise : nom, adresse, etc. — UNIQUEMENT celles qui sont
réellement affectées pour cette personne.]

CONSÉQUENCES POSSIBLES
[Hameçonnage ciblé, usurpation d'identité, démarchage, etc.]

MESURES PRISES PAR LE CABINET
- L'origine de l'incident a été identifiée et corrigée.
- Les accès non autorisés ont été révoqués.
- La CNIL a été informée le [date] (référence de la notification :
  [n°]).
- Le DPO du Cabinet, [Nom], reste à votre disposition pour toute
  question.

RECOMMANDATIONS
[Adaptées à la nature des données : surveiller les comptes bancaires
si IBAN exposé, changer les mots de passe si identifiants exposés, se
méfier des courriels suspects, etc.]

VOS DROITS
Vous pouvez à tout moment exercer vos droits d'accès, de
rectification, d'effacement, d'opposition et de portabilité. Pour ce
faire, contactez notre DPO : [email].

Vous pouvez également déposer une réclamation auprès de la CNIL :
https://www.cnil.fr/fr/plaintes.

Nous vous prions de croire en l'expression de nos meilleurs sentiments
et vous présentons nos plus sincères excuses pour les inconvénients
qu'a pu causer cet incident.

[Cabinet [Nom] — Avocat associé responsable]
[Date]
```

#### Art. 34(3)(c) public communication (when individual notification is disproportionate)

```
COMMUNIQUÉ DE PRESSE — INCIDENT DE SÉCURITÉ DES DONNÉES

Le cabinet [Nom] a détecté le [date] un incident affectant la sécurité
des données personnelles dans son système de gestion documentaire
interne.

Conformément à l'article 34 du Règlement Général sur la Protection des
Données, le Cabinet informe les personnes concernées par cette voie en
raison de l'impossibilité matérielle de procéder à des notifications
individuelles dans des délais utiles.

NATURE DE L'INCIDENT : [description].
PÉRIMÈTRE : [catégories, volumes].
MESURES PRISES : [confinement, notification CNIL ref n°, mesures
correctives].
RECOMMANDATIONS POUR LES PERSONNES POTENTIELLEMENT CONCERNÉES :
[liste actionnable].

Pour toute question ou pour vérifier si vos données sont concernées,
contactez le DPO du Cabinet : [email].
```

---

## 4. Anno-rag-specific runbooks

### 4.1 Audit chain integrity failure (D-1)

The most likely "discover a breach after the fact" path.

1. Coordinator + IT Lead jointly run `verify_audit.py` against the suspected day's JSONL + sig.
2. Note the line number where the chain broke.
3. The break tells you the **earliest** possible tamper time. The most recent intact line tells you when the chain was last known good.
4. Cross-reference with reverse-proxy + systemd logs for that window.
5. **Common false positives:**
   - The HMAC key was rotated mid-day (operator error — should never happen; the deployer guide §6.1 documents that rotation breaks past-day verifiability).
   - The JSONL was truncated by a disk-full event — check `df -h` and dmesg.
   - The clock skewed across UTC midnight at the moment of an event.
6. **If false positive ruled out:** treat as confirmed integrity breach. CNIL notification almost certainly required (audit-log tampering is a clear signal of unauthorised access, even if the upstream data wasn't directly read).

### 4.2 Vault decryption failure (D-3)

1. **Do not** retry the decryption with a different key — repeated attempts can mask the original signal.
2. Check the file timestamps: was the vault modified outside expected windows? (Compare with audit log forget+save events.)
3. Compare current `vault.bin` against the most recent good backup using `cmp`.
4. If the file differs from the backup AND the modification timestamp falls outside an expected window → integrity breach, almost certainly Art. 33 (tampering implies unauthorised modification of personal data).
5. Restore from backup. **Audit events written between the backup time and now will reference vault entries that may not be present in the restored vault — this is acceptable (rehydration falls back to leaving the token in place) but document it in the incident file.**

### 4.3 Ransomware (D-5)

1. **Isolate the host immediately** — pull network, do not reboot.
2. Photograph or screen-capture the ransom note before doing anything else.
3. Engage the forensic vendor.
4. **Do NOT pay** — the cabinet's professional-liability insurer typically requires non-payment as a condition of cover.
5. Restore from offsite backup (deployer guide §6.4). Treat the post-restore state as a separate deployment with new credentials.
6. **Confidentiality breach is presumed** unless forensics can demonstrate the attacker did not exfiltrate before encryption. CNIL notification within 72h; Art. 34 typically required given the breadth of cabinet data.

### 4.4 Credential leak (D-8)

1. Rotate the leaked credential immediately (deployer guide §6.1).
2. Determine the leak window: from the first observable leak time to the rotation time.
3. Audit-log review for any `/v1/*` activity in the window from sources outside the cabinet's authorised range.
4. **If activity is observed:** confidentiality breach, Art. 33 trigger.
5. **If no activity observed:** document the determination + the technical controls (constant-time compare, no logging of the token) that support the conclusion. Art. 33 notification still likely if the credential is the vault key or the audit HMAC key.

### 4.5 Sub-processor incident (D-7 — Anthropic)

1. Read Anthropic's incident disclosure carefully — what data, what window, what subjects.
2. The cabinet's egress is pseudonymised — request bodies and response bodies carry tokens, not cleartext. Map this against Anthropic's disclosure.
3. **If Anthropic's exposure window correlates with cabinet traffic AND the disclosure mentions request/response storage:** the cabinet's pseudonymised egress was potentially exposed.
4. Risk assessment: pseudonymised text + the surrounding factual context can sometimes re-identify (DPIA v1 Risk 5). For high-profile matters, treat as a Medium-or-High risk.
5. The cabinet's notification obligation runs from the cabinet's knowledge — i.e. from when Anthropic told the cabinet (or made the disclosure public). Art. 33 timer starts then.

---

## 5. Post-incident

Within 30 days of incident closure:

- [ ] **Lessons-learned review** — DPO + IT Lead + Managing Partner + anno on-call.
- [ ] **Update the incident file** with the final root cause + remediation.
- [ ] **Update this playbook** if a gap or false assumption was identified. Bump the version.
- [ ] **Update the DPIA** if the incident reveals a risk not captured in v1.
- [ ] **Update the anno-rag readiness spec** if a gap closure or a new gap emerged.
- [ ] **Tabletop the next likely scenario** — pick one detection path from §2 and walk through it.
- [ ] **Test the recovery** — restore from backup into a non-production environment and verify the audit chain replays cleanly.

---

## 6. Annual tabletop exercise

The cabinet runs a tabletop **once per year minimum, twice per year recommended**.

| Scenario | Frequency | Owner |
|---|---|---|
| Vault key compromise | Annual | DPO + IT Lead |
| Ransomware on the gateway host | Annual | IT Lead + forensic vendor |
| Audit chain tampering | Bi-annual | DPO + anno on-call |
| Anthropic disclosed incident | Bi-annual | DPO |
| Insider exfiltration | Annual | DPO + HR |

Each tabletop produces an updated version of this playbook. **Do not let the document go more than 12 months without a real exercise** — playbook drift is itself a risk.

---

## 7. Mapping to the readiness spec

| Gap | Status |
|---|---|
| **U13** breach playbook | ✅ Closed v1 — this document. Pending DPO sign-off, fill in contact details, run first tabletop exercise. |

---

## 8. Document control

| Version | Date | Author | Change |
|---|---|---|---|
| v1 | 2026-05-15 | anno team | Initial — closes U13. Requires DPO sign-off + roster fill-in before deployment. |
| [next] | TBD | DPO post-tabletop | Update based on first exercise findings. |
