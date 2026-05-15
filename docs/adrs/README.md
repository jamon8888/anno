# Architectural Decision Records

This folder collects the **why** behind the design choices in anno-rag. ADRs are written *after* a decision is locked, not as design proposals (those live in `docs/superpowers/specs/`).

Each ADR is short, dated, and titled by what was decided — never by what was considered. Conventional structure:

- **Context** — what forces are in tension.
- **Decision** — the choice made.
- **Consequences** — what we accept by making this choice.

Numbering is global and append-only. Superseding an ADR creates a new one (e.g. ADR-009 supersedes ADR-003); the old one stays in the record with a "Superseded by" note.

## Index

| # | Title | Status |
|---|---|---|
| [ADR-001](./0001-pseudonymise-at-ingest-not-at-egress.md) | Pseudonymise at ingest, not only at gateway egress | Accepted (v0.1) |
| [ADR-002](./0002-encrypted-vault-aes-256-gcm-passphrase-or-keyring.md) | Encrypted file vault, key from passphrase OR OS keyring | Accepted (v0.1) |
| [ADR-003](./0003-pii-eval-overlap-span-matching.md) | PII eval uses overlap-span matching with greedy left-to-right TP assignment | Accepted (v0.7) |
| [ADR-004](./0004-fr-honorific-regex-complements-ner.md) | French honorific regex complements the NER tier instead of retraining the model | Accepted (v0.7) |
| [ADR-005](./0005-bearer-auth-no-op-when-no-token.md) | Bearer-auth middleware is a no-op when no token configured (loopback dev UX) | Accepted (v0.4) |
| [ADR-006](./0006-audit-chain-canonical-event-bytes.md) | Audit hash chain canonicalises event bytes via Value round-trip | Accepted (v0.4) |
| [ADR-007](./0007-memory-second-collection-not-shared-table.md) | Memories live in a second LanceDB collection, not in the chunks table | Accepted (v0.1 memory) |
| [ADR-008](./0008-forget-cascade-reference-count-zero.md) | Memory-forget cascades to vault entries only when reference count is zero | Accepted (v0.1 memory) |
| [ADR-009](./0009-erasure-slo-via-daily-table-optimize.md) | 24h erasure SLO via daily `Table::optimize` ticker, not synchronous reclaim | Accepted (v0.1 memory) |
| [ADR-010](./0010-mcp-memory-kind-as-string-not-enum.md) | MCP `memory_*` tools take `kind` as a String, not the typed enum | Accepted (v0.1 memory) |
