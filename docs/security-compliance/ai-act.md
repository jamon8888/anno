# AI Act

Status: Available in v0.11.0-rc.11
Audience: Compliance, Admin
Language: Bilingual

This page describes Hacienda's product posture for AI governance discussions. It
is not legal classification advice and does not certify an EU AI Act risk class.

Cette page decrit la posture produit d'Hacienda pour les discussions de
gouvernance IA. Elle ne constitue pas un avis de classification juridique et ne
certifie aucune classe de risque au titre de l'AI Act.

## Product Posture

Hacienda is an assistive local documentation, retrieval, privacy, and workflow
tool. It is not a replacement for qualified legal, compliance, security, or
administrative review.

Key posture points:

- local-first processing reduces uncontrolled data exposure;
- pseudonymization is used for search/legal retrieval outputs where applicable
  and before optional upstream model calls; trusted local recall and
  rehydration can return cleartext to authorized local clients;
- remote LLM use is optional and configuration-dependent;
- outputs require human validation before regulated use;
- admins remain responsible for deployment context, access controls, logging,
  and provider review.

## Human Oversight

Users should be able to:

- inspect cited source material;
- understand when content was generated or extracted by an AI-assisted workflow;
- reject, correct, or refine extracted fields and summaries;
- escalate uncertain outputs to a qualified reviewer;
- preserve audit evidence for sensitive workflows.

## Current Limits

- Phase 1 documentation does not provide a risk classification worksheet.
- The product does not determine whether a deployment is prohibited, high-risk,
  limited-risk, or minimal-risk.
- The same feature can have different governance implications depending on
  deployment context and use case.

## Planned Phase 2 Deepening

- Risk classification worksheet.
- Control checklist mapped to deployment patterns.
- Evidence collection guidance for model, data, prompt, audit, and human-review
  records.
- Operator acceptance criteria for regulated workflows.

## Related Links

- [Privacy Model](privacy-model.md)
- [RGPD](rgpd.md)
- [Threat Model](threat-model.md)
