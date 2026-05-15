# ADR-009 — 24h erasure SLO via daily `Table::optimize` ticker, not synchronous reclaim

**Status:** Accepted (v0.1 memory) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

GDPR Art. 17 ("right to erasure") doesn't specify a maximum delay between a valid request and the physical disappearance of the bytes. EDPB / WP29 guidance and supervisory-authority practice converge on a few weeks as the implicit ceiling, with "without undue delay" as the operative phrase.

LanceDB's deletion model is tombstone-based: `Table::delete(filter)` marks rows as deleted, but the bytes remain on disk in fragment files until a compaction (`Table::optimize`) reclaims them. Two ways to achieve "physically gone":

1. **Synchronous reclaim after every `forget_memory` / `forget_subject`.** Strong guarantee: when the API returns 200, the bytes are gone. But every erasure pays a O(table) optimize cost. For a 100k-row memory table, that's tens of seconds per call — unacceptable response time.
2. **Asynchronous reclaim on a fixed interval.** API returns immediately on tombstone; a background ticker compacts at a configured cadence. Operator chooses the cadence to balance SLO against compaction cost.

The cabinet's profile (DPIA v1 §3 R3) treats physical-vs-logical erasure as a known gap: the audit log records the forget receipt, the row is logically gone, the bytes follow on a known schedule.

## Decision

**Asynchronous daily compaction.** `Pipeline::spawn_compaction_task` boots a tokio ticker (`compaction_interval_secs`, default `86400` = 24h). Each tick runs `Store::optimize_memories(min_age)`. `compaction_min_age_secs` (default 1h) is informational on lance 0.29 (which reclaims everything compaction-eligible via `OptimizeAction::All`) — a v0.2 candidate is to plumb the age through `CompactionOptions` once that surface stabilises.

The 24h cadence is configurable per-deployment. Cabinets with stricter SLO can set `compaction_interval_secs = 3600` (hourly) at the cost of more compaction work.

## Consequences

- API response time is constant — `forget_memory` returns as soon as the tombstone is written.
- Worst-case physical-erasure delay = the configured interval (default 24h). Documented in the data-subject pack §3.2 retention table and in the MCP tool's response (`note: "logically forgotten; physical erasure within 24h"`).
- The audit log records the forget receipt at the moment the API returned — that's the legally relevant timestamp for the cabinet's accountability.
- Compaction failures are logged at `target = "anno_rag::memory::audit"` with `event = "compaction_failed"`; the ticker continues. A v0.5 candidate is to surface this as a Prometheus metric.
- Operators wanting "actually gone right now" semantics can call `Pipeline::compact_now()` explicitly — admin-triggered compaction, runs once.
- v0.5 candidate: a one-shot compaction trigger immediately after every Art. 17 subject-rights-route forget call. Trades some response time for a stronger guarantee on the user-facing rights path. Tracked.

## Reference

`crates/anno-rag/src/pipeline.rs::compact_now` + `::spawn_compaction_task`, `crates/anno-rag/src/store.rs::optimize_memories`, data-subject pack §3.2, DPIA v1 R3.
