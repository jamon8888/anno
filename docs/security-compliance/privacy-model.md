# Privacy Model

Status: Available in v0.11.0-rc.11
Audience: Compliance, Admin, Developer, User
Language: Bilingual

Hacienda reduces exposure by tokenizing detected sensitive values before local
indexing/search outputs and before optional remote LLM or upstream provider
calls. Local detection, OCR, vault operations, and explicit rehydration are
trusted cleartext operations inside the local boundary.

Hacienda reduit l'exposition en remplacant les donnees sensibles detectees par
des tokens avant l'indexation, les sorties de recherche et les appels optionnels
a un fournisseur distant. La detection locale, l'OCR, le vault et la
rehydratation explicite peuvent traiter du texte clair dans le perimetre local
de confiance.

## Core Controls

| Control | Behavior |
|---|---|
| Local vault | Stores token mappings locally in encrypted form. |
| Pseudonymize at ingest and indexing | Detected sensitive values are replaced with stable tokens before being stored in searchable local indexes. |
| Tokenized retrieval | Search and legal RAG outputs return pseudonymized evidence by default. |
| Local rehydration | Trusted local users can explicitly convert tokens back to cleartext through the vault. |
| Gateway filtering | HTTP prompts are tokenized before optional upstream provider calls; responses can be rehydrated locally for trusted clients. |

## Trust Boundary

Inside the local trust boundary:

- source documents may contain clear PII;
- OCR and local detectors may process cleartext;
- the vault maps clear values to pseudonym tokens;
- `rehydrate` and subject export workflows can intentionally return cleartext
  to a trusted local caller.

Across the remote/upstream boundary:

- prompts should be tokenized before provider calls;
- gateway subject routes must be protected because lookup/export can return
  original sensitive values;
- operators must verify configuration before using remote LLM workflows.

## Limits

- Detectors are not perfect; false negatives can leave sensitive values
  un-tokenized.
- Human review is required before relying on outputs in legal, compliance, or
  regulated workflows.
- Memory workflows have mode-specific privacy behavior. Validate
  `ANNO_RAG_MEMORY_NER_MODE` before storing sensitive memory content.
- Pseudonymization reduces exposure; it is not anonymization certification.

## Related Links

- [RGPD](rgpd.md)
- [Audit Logging](audit-logging.md)
- [Threat Model](threat-model.md)
