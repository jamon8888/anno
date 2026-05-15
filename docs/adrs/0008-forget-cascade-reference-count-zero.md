# ADR-008 — Memory-forget cascades to vault entries only when reference count is zero

**Status:** Accepted (v0.1 memory) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

When `Pipeline::forget_memory(id)` deletes a memory row, what should happen to the vault entries the row referenced?

Two extremes:
1. **Always purge** the referenced vault entries. Maximally aggressive: minimises vault size, simplifies the data flow. **But** if memory A and memory B both reference `PERSON_42` (same person), forgetting A purges the vault entry — memory B's rehydration breaks (the token survives in B's text but no longer maps to a name).
2. **Never cascade.** Vault grows monotonically. Eventually contains entries no row references — phantom mappings. Operationally simpler but defeats the GDPR Art. 17 spirit (the cabinet is supposed to actually delete the original).

The middle path keeps the cascade *and* preserves rehydration: purge a vault entry only when no remaining row references it.

## Decision

**Cascade with reference counting.** Pipeline::forget_memory:

1. Snapshots the candidate tokens from the to-delete rows (BEFORE the deletion).
2. Deletes the memory rows.
3. For each candidate token, calls `Store::token_reference_count(token)` — if zero, calls `Vault::forget(token)`.

The reference count is computed against `memories.token_refs` (and conceptually against `chunks.token_refs` once that column lands — currently chunks have no token_refs, so the chunks side is a no-op).

`Vault::forget` itself is the v0.4 GDPR primitive — idempotent, returns `Some(RemovedMapping)` only when the entry actually went away.

## Consequences

- Cross-memory rehydration consistency: shared tokens survive partial erasure. Memory B still rehydrates correctly after memory A is forgotten.
- Eventual minimisation: the last forget across all referencing rows triggers the vault purge.
- Cost: O(N) scan of `memories.token_refs` per candidate token. For v0.1 corpus sizes (cabinet-scale, thousands of memories) this is acceptable. v0.2 candidate: query through the LabelList index once the lance struct-subfield-filter syntax is confirmed for `List<Struct>` columns.
- The `vault_tokens_purged` count returned in `ForgetResult` is observable — both for the audit log and for the responding API call. Useful for the deployer to verify the cascade actually fired.
- Vault entries that were minted for chunks but never referenced by a memory (the v0.5+ ingest path will populate token_refs on chunks too) are NOT cascade-purged from `Pipeline::forget_memory`. They're handled by the per-matter erasure path (data-subject pack §3.2).
- Property test (`memory_proptest.rs`, `#[ignore]`-gated, 50 cases) drives the invariant.

## Reference

`crates/anno-rag/src/pipeline.rs::forget_memory`, `crates/anno-rag/src/store.rs::token_reference_count`, `crates/anno-rag/src/vault.rs::forget` (anno-rag wrapper of the v0.4 cloakpipe primitive `Vault::remove`).
