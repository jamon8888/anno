# Design — anno-rag Batch Ingest Scaling (1000 docs)

**Date**: 2026-05-18 (rev. 2 — corrected after codebase review)
**Status**: Draft for review
**Scope**: `crates/anno-rag` only (no `anno`-crate changes). Target:
ingest ~1000 documents (cap) on a recent laptop, reliably and resumably,
materially faster than today. OCR-heavy corpora out of scope (separate
spec). A genuine batched-ONNX rework of the NER backend is out of scope
(noted as the larger future lever "A′").

## 1. Problem

`Pipeline::ingest_folder` is a sequential `for entry in walker { ingest_one }`
loop. For ~1000 clean-text docs (~15–30k chunks):

- **NER dominates.** `ingest_one` calls `Detector::detect(&chunk.text)`
  once **per chunk**. gliner2-fastino transformer NER on CPU is
  ~50–300 ms/chunk → **45–90 min single-threaded**.
- **Re-ingest duplicates the corpus.** `ingest_one` sets
  `doc_id = Uuid::now_v7()` (pipeline.rs:112). `Store::upsert` is
  idempotent via `merge_insert(&["doc_id","chunk_idx"])` and `chunk_id`
  is UUID-v5 of `(doc_id, chunk_idx)` — but because `doc_id` is fresh
  random per call, re-running `ingest_folder` produces a *new* doc_id
  for the same file, so `merge_insert` never matches the prior rows →
  **every re-run duplicates the entire corpus**, and an interrupted
  batch cannot resume.
- Embedding is **already batched** per doc (`embedder.embed_batch`),
  not the bottleneck. Storage/retrieval already scale fine.

## 2. Two hard constraints (resolved against the code — not assumptions)

These were verified during review and are the reason the obvious
designs don't work:

1. **NER inference serializes on a single mutexed session.**
   gliner2-fastino uses `Arc<Mutex<ort::session::Session>>`
   (`crates/anno/src/backends/gliner2_fastino/sessions.rs:35`). A single
   shared `Detector` ⇒ document-level fan-out **cannot** parallelize
   NER — all docs block on the same mutex.
2. **There is no real batched NER in the backend.**
   `GLiNER2Fastino::batch_extract_streaming`
   (`crates/anno/src/backends/gliner2_fastino/mod.rs:803`) is
   documented and coded as a **sequential per-text loop** — *"the
   'batch' in the name refers to chunked progress reporting, not
   parallel batched inference"*. There is no `[N, L]` batched tensor
   forward to wrap. A thin `anno-rag` "detect_batch" over it would
   yield **no inference speedup**.

⇒ The only way to cut the dominant NER cost **without modifying the
`anno` backend** is to run **multiple independent NER engines in
parallel** (each its own mutexed session), driven by bounded document
fan-out. That is Lever A″ below.

## 3. Design — A″ instance pool + B fan-out + C deterministic doc_id

### Lever A″ — Bounded pool of independent NER engines (primary)

- Today: `Pipeline` holds one `OnceCell<Arc<Detector>>`; `Detector`
  wraps one `GLiNER2Fastino` (its own 8 mutexed `SessionSlot`s).
- Change: introduce a **fixed-size pool of `Detector` instances**
  (`Vec<Arc<Detector>>`, size = `ingest_ner_pool` config, default
  chosen from an RSS budget — see Risks). Each instance has its own
  ONNX sessions, so NER on instance *i* runs concurrently with NER on
  instance *j* (different mutexes).
- A simple acquire/release (e.g. an `async` semaphore + round-robin, or
  a `tokio::sync::Mutex<Vec<…>>` free-list) hands a free `Detector` to
  each in-flight `ingest_one`.
- Pool is **lazily built** (mirror the existing `OnceCell` lazy-load;
  build instances on first ingest, not at `Pipeline::new`) so non-ingest
  callers keep the ~200 MB startup RSS.
- `detect()` calls stay per-chunk (no backend change); parallelism comes
  from *N engines running at once*, not from batching within one.

This is the only lever that actually reduces wall-clock NER on the
current backend. RSS is the cost: ≈ `pool_size × gliner2-model-resident`
(bounded — see Risks).

### Lever B — Bounded document fan-out (the vehicle)

Mirror the existing `tabular::fanout::run_review` pattern: a bounded
`tokio::sync::Semaphore` (degree = `ingest_concurrency`, default
`num_cpus`) spawning `ingest_one` per document. Each task:

- acquires a `Detector` from the A″ pool (blocks until one is free —
  this naturally bounds NER concurrency to `ingest_ner_pool`),
- runs extraction / per-chunk detect (on its pooled engine) /
  pseudonymize / `embed_batch` / `Store::upsert` / anon-md write,
- releases the engine.

`Embedder` and `Store` stay `Arc`/`&self`-shared. **Open item the plan
must close:** confirm `Store::upsert` (`merge_insert` + `add`/`delete`)
is safe under concurrent tasks; if not, serialize upsert behind a small
mutex or collect-and-flush per N docs. Concurrency degree and pool size
are independent knobs (`ingest_concurrency` ≥ `ingest_ner_pool`; extra
concurrency still overlaps non-NER work while NER engines are busy).

### Lever C — Deterministic doc_id (idempotency for free) + skip

Two parts, both small because the existing `merge_insert(&["doc_id",
"chunk_idx"])` already does the dedup if `doc_id` is stable:

- **C1 (correctness, tiny):** replace `doc_id = Uuid::now_v7()` with
  `doc_id = Uuid::new_v5(NS, file_identity)` where `file_identity` is a
  content/file hash (raw file bytes hash is cheapest and catches "same
  file, moved"; the plan picks the exact hash). Same file ⇒ same
  `doc_id` ⇒ existing `merge_insert` **overwrites** its own rows
  instead of duplicating. Changed content ⇒ new `doc_id`; delete rows
  of the old doc_id for that source_path (a `delete` by `source_path`
  before upsert, or a stale-doc_id sweep) so there is no orphan
  duplication. Reuses the UUID-v5 helper already in store.rs:1215.
- **C2 (resume speed, small):** before extraction, skip a file whose
  `doc_id` (= hash) already has rows in the table
  (`Store::doc_exists(doc_id) -> bool`, a cheap `count_rows(filter)`).
  Turns re-runs into "only new/changed files," makes an interrupted
  batch resume the remainder, and makes a re-run on an unchanged folder
  cost ~seconds. C1 guarantees *no duplication even without C2*; C2 is
  the speed half.

## 4. Out of scope (honest)

- **Lever A′ — genuine batched ONNX NER inside the `anno`
  gliner2-fastino backend** (pad N texts → one `[N,L]` forward). This
  is the highest-ceiling lever but is a substantial change in the
  `anno` crate's ONNX pipeline + a feasibility check on whether
  gliner2's architecture batches well. **Separate, larger spec.** Noted
  so it is not forgotten; explicitly not in this plan.
- **OCR / scanned PDFs** — different cost class (1–5 s/page); separate
  OCR-batch spec. Gate/skip here, don't fold in.
- **FTS `optimize()` O(table)** — one-time end-of-batch cost is
  seconds-to-low-minutes at 15–30k chunks; only matters at 100k+.
- **GPU / NER quantization** — CPU-only target.

## 5. Testing

- Unit: deterministic `doc_id` — same file bytes → same `doc_id` across
  two calls; different content → different `doc_id`.
- Unit: `Store::doc_exists` — false on empty, true after an upsert with
  that doc_id.
- Integration (`#[ignore]`, heavy): ingest a 5-file fixture folder →
  chunk count C. Re-run `ingest_folder` → **still C** (no duplication,
  C1) and all 5 skipped (C2). Add a 6th file → re-run → only it
  ingested. Edit one file's content → re-run → its old rows replaced,
  count consistent (no orphans).
- Concurrency-safety integration (`#[ignore]`): ingest a ~30-doc
  synthetic corpus with `ingest_concurrency` > 1 and
  `ingest_ner_pool` > 1; assert final chunk count equals the
  sequential baseline (no lost/dup rows under concurrent upsert).
- Throughput smoke (`#[ignore]`, recorded not asserted-tight): ~50-doc
  synthetic corpus, `(concurrency=1,pool=1)` vs
  `(concurrency=num_cpus,pool=N)`; record wall time + peak RSS. Sanity:
  parallel run materially faster; RSS within the budget.
- Extend `benches/bench_ingest.rs` for the 50-doc corpus to track
  regressions.

## 6. Risks

| Risk | Mitigation |
|---|---|
| **RSS blow-up from N NER model copies** (the core A″ cost) | `ingest_ner_pool` default derived conservatively (the plan measures one gliner2 instance's resident size and sets default so `pool × size + embedder + LanceDB` stays within a stated laptop budget, e.g. ≤ ~4 on 16 GB). Pool size is config; document the RAM/throughput trade-off. |
| `Store::upsert` not concurrency-safe under fan-out | Plan verifies; serialize upsert or collect-and-flush per N docs. Concurrency-safety integration test (§5) is the gate. |
| gliner2 model load time × N (pool warm-up cost) | Lazy-build pool on first ingest; amortized over a 1000-doc batch (one-time ~seconds × N). Acceptable for batch; documented. |
| Changed-content orphan rows (old doc_id rows linger) | C1 includes an explicit delete-by-`source_path` (or stale-doc_id sweep) before upsert; covered by the "edit a file" integration test. |
| Honest expectation management | §7 states the realistic ceiling and that A′ (not done here) is the only path past it. |

## 7. Expected outcome (honest, re-derived after review)

NER is ~70% of wall time, serialized per engine, **not batchable**
without backend work. So the realistic gain is bounded by how many
independent NER engines RSS allows:

- **A″ + B + C, pool ≈ 3–4 on a 16 GB laptop:** ~45–90 min →
  **~12–25 min** for 1000 clean-text docs (NER parallelized across
  3–4 engines + non-NER fully overlapped), **resumable & idempotent**.
  Re-run on unchanged folder ≈ seconds.
- Without the instance pool (B + C only): ~1.5–2× (only non-NER
  parallelizes) — not worth it alone; A″ is the point.
- The ceiling past this is **A′** (true batched ONNX in the backend),
  a separate larger effort. Sub-10-min for 1000 docs is **not**
  realistic on the current backend on a laptop — stated plainly.
- Scan-heavy/OCR corpora: unchanged, out of scope.

The durable win is as much **C (unmanageable → resumable/incremental)**
as the **~3–5× from A″+B**; both matter for "1000 docs, managed."
