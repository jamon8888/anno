# ADR-007 — Memories live in a second LanceDB collection, not in the `chunks` table

**Status:** Accepted (v0.1 memory) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

The anno-memory v0.1 module adds a persistent layer for Cowork session memories: facts the user told the assistant, preferences the user has, references to entities the user cares about. Two storage options:

1. **Same `chunks` table, distinguished by a `kind` column.** Reuses an existing schema, indexes, and ingest path. Saves a table.
2. **New `memories` table, sharing the LanceDB connection.** Distinct schema, distinct indexes, distinct retention policy.

Option 1 is tempting for simplicity, but memories and chunks have substantively different schemas:

- Chunks: doc-anchored — `doc_id`, `chunk_idx`, `source_path`, `page`, `char_start`/`char_end`, `text_hash`. Designed for verbatim citation back to a source document.
- Memories: session-anchored — `id` (UUIDv7), `session_id`, `kind`, `created_at`, `accessed_at`, plus the v0.2 forward-compat `valid_from`/`valid_to`/`entity_refs`. Designed for retrieval by recency + category + entity, not by source-document offset.

Shoehorning memories into the chunks schema means either nullable noise in the chunk fields or a bag of `Option<>` columns that's painful to query. Indexes diverge too — chunks needs no `kind` index, memories needs LabelList on `token_refs` and BTree on `created_at` + `session_id`.

## Decision

**A second LanceDB collection named `memories`**, opened by `Store::open` alongside `chunks` from the same `lancedb::connect()` connection. Schema in `memories_schema(embedding_dim)`. Indexes set up via `Store::setup_memory_indexes` after the first row exists.

The forward-compat columns `valid_from` / `valid_to` / `entity_refs` are reserved in v0.1 but populated trivially (`valid_from = created_at`, `valid_to = None`, `entity_refs = []`). v0.2 activates them.

## Consequences

- Two collections to back up, replicate, and migrate. Operationally accepted — backup script handles the parent directory.
- Per-collection indexes (BTree on `created_at` + `session_id`, Bitmap on `kind`, LabelList on `token_refs` + `entity_refs`) match the access patterns of the memory layer without distorting the chunks index strategy.
- The v0.4 GDPR Art. 17 cascade (Pipeline::forget) still hits the vault — both collections reference the same vault tokens; a single `Vault::forget` purges across both.
- The 24h erasure SLO (ADR-009) applies to the memories collection via its own `Table::optimize` ticker; chunks have their own retention story tied to matter close (see data-subject pack §3.2).
- A future v0.3-memory might consolidate by extending the chunks schema if the retention models converge. Not on the v0.1 critical path.

## Reference

`crates/anno-rag/src/store.rs::memories_schema`, `::Store::open`, `::setup_memory_indexes`. v0.1 memory plan §File-structure.
