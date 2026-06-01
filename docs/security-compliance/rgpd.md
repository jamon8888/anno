# RGPD

Status: Available in v0.11.0-rc.11
Audience: Compliance, Admin
Language: Bilingual

This page documents product controls that can support RGPD/GDPR governance. It
is not legal advice, a compliance certification, or a substitute for a DPIA,
lawful-basis review, or counsel review.

Cette page decrit des controles produit pouvant aider une demarche RGPD. Ce
n'est pas un avis juridique, une certification de conformite, ni un remplacement
d'une AIPD/DPIA, d'une analyse de base legale ou d'une revue par conseil.

## Product Controls

| Control Area | Product Support |
|---|---|
| Data minimization | Pseudonymized indexing and retrieval reduce cleartext exposure in search and remote-model paths. |
| Confidentiality | Local vault and local-first storage keep mappings inside the configured trust boundary. |
| Access control | MCP relies on trusted local clients; gateway protected routes can require bearer auth when configured. |
| Erasure support | Subject forget and memory forget workflows provide local erasure primitives where supported, but they are not complete deletion across source documents, backups, exports, tabular review state, or every historical copy. |
| Auditability | Gateway, subject-rights, memory, and legal workflows expose or emit audit-relevant events where configured. |
| Human oversight | Outputs are assistive and must be reviewed by qualified users for regulated decisions. |

## Responsibilities Outside The Product

Operators and controllers remain responsible for:

- lawful basis and purpose limitation;
- retention schedules and deletion policies;
- separate deletion of source documents, backups, exports, tabular review data,
  and external copies when a data-subject request requires it;
- secrets, keyring, and backup protection;
- user access management and endpoint hardening;
- validation of generated outputs before use;
- processor, subprocessor, and remote-provider review;
- incident response and data breach notification decisions.

## Practical Review Questions

- Which datasets are indexed, and for what purpose?
- Which users can call rehydration or subject export workflows?
- Is the gateway bound to loopback or protected by bearer auth and network
  controls?
- Are backups encrypted and tested?
- Are AI-assisted conclusions reviewed by a qualified human?

## Related Links

- [Privacy Model](privacy-model.md)
- [Audit Logging](audit-logging.md)
- [Operations](../admins/operations.md)
