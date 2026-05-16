# LoCoMo sanity fixture

10-item smoke fixture exercising every v0.1 + v0.2 memory feature on a
deterministic planted graph. **Not a statistical accuracy benchmark.**

## What's here

- `scenarios.toml` — 10 numbered scenarios, each with a sequence of
  `save_memory` calls and a sequence of recall / graph_recall /
  forget / list operations with expected outcomes.

## What it covers

| Scenario | Feature exercised |
|---|---|
| 01 | Direct preference recall (v0.1 baseline) |
| 02 | `kinds` filter |
| 03 | `session_id` filter |
| 04 | Conflict resolver — Preferences auto-invalidate |
| 05 | Facts are append-only (no auto-invalidation) |
| 06 | Bi-temporal `as_of` pre-invalidation |
| 07 | `recall_memory(graph_expand=true)` one-hop expansion |
| 08 | `graph_recall` two-hop traversal |
| 09 | Forget cascade with partial vault retention |
| 10 | `list_memories` cursor pagination |

## What it does NOT do

- Measure `accuracy@k` against a held-out ground-truth.
- Provide a statistical baseline to gate CI on.
- Generalise to the full LoCoMo benchmark (1000+ items).

For those, see the still-deferred **v0.1 T13** + **v0.2 T9** tasks.

## Harness status

**Not yet written.** The fixture is ready; an `anno-rag` test reading
this file would be a v0.8 follow-up. Until then this is documentation
of the v0.2 behavioural surface and a ready input for whoever picks
up the harness.

Implementation outline lives at the bottom of `scenarios.toml`.

## Why ship the fixture without the harness

Two reasons:

1. **The data is the hard part.** Writing 10 French-legal memory
   scenarios with planted entity graphs + expected outcomes is the
   real work; the harness is mostly TOML parsing + assertion loops.
   Shipping the fixture unblocks the harness implementer.
2. **It doubles as behavioural documentation.** A new contributor
   reading `scenarios.toml` learns what every v0.2 feature is
   supposed to do, with concrete French legal examples.
