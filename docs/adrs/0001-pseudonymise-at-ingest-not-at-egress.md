# ADR-001 — Pseudonymise at ingest, not only at gateway egress

**Status:** Accepted (v0.1) · **Date:** 2026-05-12 · **Deciders:** anno team

## Context

There are two natural points to apply PII pseudonymisation:

1. **At gateway egress** — clear text is stored on disk; the gateway tokenises just before sending to the LLM provider.
2. **At ingest** — clear text is tokenised once on the way into the index; everything downstream operates on tokens.

Option 1 is simpler (single chokepoint, no double-tokenisation when re-using indexed content). Option 2 has stronger guarantees: any disk leak, accidental dump, debug log, backup snapshot, replicated database, or sub-processor egress carries tokens, not names.

The cabinet's threat model (DPIA v1 §3 R1) treats *unauthorised access to the on-disk index* as a real risk — not just network egress. So the disk must carry pseudo text already.

## Decision

**Pseudonymise at ingest.** The `chunks` LanceDB column is named `text_pseudo` to make this contract visible. Rehydration is per-call at the boundary (Pipeline::rehydrate). Cleartext PII is held in exactly one place: the encrypted vault.

## Consequences

- Defense-in-depth: a stolen index file alone does not re-identify subjects without the vault.
- Embedding quality is preserved (e5-small embeds the tokenised form; tokens are semantically close to their categories so retrieval still works on category-level concepts).
- Cost: an extra detect+pseudonymise pass at ingest. Amortised — done once, queried thousands of times.
- The privacy gateway gets to do the *minimal* job (a thin pseudonymisation+rehydration shim), not the *only* protective job.
- Any future analytics path (e.g. cross-matter aggregation) inherits the same guarantee for free.

## Reference

`crates/anno-rag/src/store.rs::chunks_schema` (text_pseudo column), `crates/anno-rag/src/pipeline.rs::ingest_one` (detect → pseudonymise → embed flow).
