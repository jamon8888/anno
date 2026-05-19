# anno-rag ingest performance experiment (Option 2)

Date: 2026-05-19
Status: Draft, measurement-only
Owner: anno-rag

## Summary

Option 1 keeps the proven part of the ingest work: deterministic/resumable
folder ingest. Option 2 is a separate experiment to find out whether a
thread-budgeted NER pool can ever beat sequential ingest on real machines
without reintroducing the observed regression.

The prior parallel ingest attempt was rejected because the measured full
ingest was about 2x slower than sequential. The likely failure mode is ONNX
Runtime oversubscription: each GLiNER2 Fastino detector owns multiple ONNX
sessions, and `OnnxSessionConfig::num_threads = 0` lets ORT pick its default
intra-op thread policy. Running several detector instances concurrently can
multiply CPU-bound thread pools instead of improving throughput.

This spec treats Option 2 as an experiment, not as product behavior. It must
produce repeatable measurements on both Windows CPU and macOS CoreML before
any pool/fan-out code is proposed for `Pipeline::ingest_folder`.

## Goals

- Measure whether bounded detector parallelism can improve warm full-ingest
  throughput on Windows CPU.
- Measure whether CoreML changes the best macOS strategy before testing a
  detector pool on Apple Silicon.
- Separate NER-only performance from extraction, embedding, LanceDB writes,
  vector index build, and FTS index build.
- Produce machine-readable bench output that can be attached to a PR or issue.
- Keep normal tests and CI fast; this remains a manual/heavy perf harness.

## Non-goals

- Do not reintroduce product `ingest_concurrency` or `ingest_ner_pool` config
  until measurements pass the acceptance gates.
- Do not claim full-ingest speedup from an NER-only win.
- Do not implement true batched GLiNER2 ONNX inference here. That remains the
  larger A-prime path if instance pooling does not win.
- Do not make CoreML a default. It is platform- and model-shape-dependent.

## Current hot path

Relevant code paths on `main`:

- `crates/anno-rag/src/pipeline.rs`
  - `Pipeline::ingest_folder` walks supported files sequentially.
  - `Pipeline::ingest_one` extracts one file, runs NER per chunk, embeds all
    pseudonymized chunks in one `embed_batch`, then writes chunk records.
  - `maybe_build_index` and `maybe_build_fts_index` run at the tail of
    `ingest_folder`.
- `crates/anno-rag/src/detect.rs`
  - `Detector::new` loads `GLiNER2Fastino::from_pretrained`.
  - `Detector::detect` is synchronous and wraps `extract_with_types`.
- `crates/anno/src/backends/hf_loader.rs`
  - `OnnxSessionConfig::num_threads = 0` means "auto/default".
  - `create_onnx_session` only calls `.with_intra_threads(n)` when `n > 0`.
  - `prefer_coreml` registers the CoreML execution provider when the feature
    is enabled.
- `crates/anno/src/backends/gliner2_fastino/sessions.rs`
  - one detector owns eight ONNX sessions.
  - the same `OnnxSessionConfig` is propagated to each session.

ONNX Runtime's threading documentation says intra-op and inter-op thread pools
are tunable, and that default intra-op behavior can create worker threads per
physical core. That makes `pool_size * intra_threads <= physical_cores` the
first rule for any instance-pool experiment.

Reference: https://onnxruntime.ai/docs/performance/tune-performance/threading.html

## Hypotheses

1. The previous pool was slower mainly because each detector used ORT auto
   threading, causing oversubscription.
2. A bounded pool can help only if each detector has a strict intra-op thread
   budget and the product `pool_size * intra_threads` stays under the physical
   core count.
3. Async fan-out alone is not enough because `Detector::detect` is synchronous
   and CPU-bound; any experiment must use dedicated blocking workers or
   `spawn_blocking`.
4. CoreML may change the macOS optimum, but single-session CoreML must beat CPU
   first. Pooling multiple CoreML sessions can lose to CPU because of graph
   compile, CPU/ANE transfers, memory pressure, and thermal throttling.
5. Even if NER improves, full ingest may remain bounded by embedding, store
   writes, vector indexing, or FTS indexing.

## Measurement design

Each bench run must report at least these fields:

```text
INGEST_PERF os=<windows|macos> provider=<cpu|coreml> bench=<ner_only|full_ingest|index_tail>
  docs=<n> chunks=<n> pool=<n> intra_threads=<n> warm=<true|false>
  elapsed_ms=<n> docs_per_s=<f64> chunks_per_s=<f64> rss_peak_mb=<n>
```

Recommended phases:

- `extract_ms`: file read plus `ingest::extract`.
- `doc_skip_ms`: deterministic doc-id plus existing-doc check once Option 1 is
  in the base branch.
- `ner_ms`: detector `detect` time across chunks.
- `vault_ms`: pseudonymization time.
- `embed_ms`: `Embedder::embed_batch`.
- `store_ms`: delete/upsert and table write.
- `anon_write_ms`: anonymized markdown output.
- `index_ms`: vector index build.
- `fts_ms`: FTS index build.

The harness must support:

- cold run: includes model load and first-session initialization;
- warm run: pre-load detector/embedder and process one tiny document outside
  the timed window;
- median of at least 3 warmed runs for acceptance decisions;
- same corpus, same output layout, and fresh temp data dir per measured run;
- optional `--no-index-tail` mode so NER/store improvements are not hidden by
  tail index work.

## Matrix

### Windows CPU

Baseline:

- `provider=cpu`, `pool=1`, `intra_threads=0` (current auto/default).
- `provider=cpu`, `pool=1`, `intra_threads=1`.
- `provider=cpu`, `pool=1`, `intra_threads=2`.
- `provider=cpu`, `pool=1`, `intra_threads=4`.
- `provider=cpu`, `pool=1`, `intra_threads=<physical_cores>`.

Thread-budgeted pool candidates:

- `pool=2`, `intra_threads=max(1, floor(physical_cores / 2))`.
- `pool=4`, `intra_threads=max(1, floor(physical_cores / 4))`.
- `pool=2`, `intra_threads=1`.
- `pool=4`, `intra_threads=1`.

Invalid candidates:

- any `pool * intra_threads > physical_cores`;
- any pool with `intra_threads=0`;
- any result where RSS exceeds 1.5x baseline unless the speedup is large enough
  to justify a separate product discussion.

### macOS CPU and CoreML

CPU baseline mirrors Windows:

- `provider=cpu`, `pool=1`, `intra_threads=0`.
- `provider=cpu`, `pool=1`, tuned `intra_threads`.

CoreML first gate:

- `provider=coreml`, `pool=1`, `intra_threads=0`.
- `provider=coreml`, `pool=1`, tuned `intra_threads` only if ORT accepts it
  cleanly and the metric is stable.

CoreML pool candidates are allowed only after single-session CoreML beats the
macOS CPU baseline:

- `provider=coreml`, `pool=2`, conservative `intra_threads=1`.
- stop if RSS, thermal throttling, or elapsed time regresses.

CoreML enablement note:

- `anno` already exposes `gliner2-fastino-coreml = ["gliner2-fastino",
  "onnx-coreml"]`.
- `anno-rag` does not currently expose a feature that forwards this dependency
  feature. A full `anno-rag` CoreML ingest bench should either add an
  experiment-only forwarding feature, or place the first CoreML comparison in
  the `anno` crate and only move to full ingest after the provider wins there.

## Experiment implementation shape

This should be implemented behind bench/test-only seams first:

1. Add a detector constructor that accepts `OnnxSessionConfig`.
   - Candidate: `Detector::new_with_onnx_config`.
   - It should call `GLiNER2Fastino::from_pretrained_with_config` with
     `GLiNER2FastinoConfig::default().with_onnx(onnx_cfg)`.
2. Add a bench-only worker abstraction.
   - It must use dedicated blocking workers or `tokio::task::spawn_blocking`.
   - It must not rely on `buffer_unordered` around a synchronous CPU-bound
     function as proof of parallelism.
3. Add an ignored integration harness or criterion bench:
   - preferred file: `crates/anno-rag/benches/bench_ingest_option2.rs`;
   - emit `INGEST_PERF ...` lines to stderr/stdout;
   - keep benchmark corpus configurable via env var;
   - default to the existing bench fixture corpus only as a smoke.
4. Keep product config unchanged until acceptance passes.
5. If the best experiment passes, then design the product-facing config and
   CLI defaults in a separate spec.

## Acceptance gates

Windows CPU:

- warmed full ingest median is at least 1.25x faster than sequential baseline;
- correctness matches baseline:
  - same number of ingested docs;
  - same number of chunk records;
  - no duplicated rows on re-ingest once Option 1 is the base;
- peak RSS is <= 1.5x baseline;
- no candidate uses `pool * intra_threads > physical_cores`.

macOS CoreML:

- single-session CoreML full ingest or NER-only benchmark is at least 1.20x
  faster than macOS CPU baseline before testing CoreML pools;
- if CoreML is slower or unstable, remove it from the ingest pool experiment
  and keep it as a backend-specific research note.

No-go:

- NER-only improves but full ingest does not improve by at least 1.10x after
  excluding index tail. Investigate embedding/store/index instead.
- any warmed pool is slower than sequential baseline.
- any result depends on cold model-load noise.

## Recommended commands

Windows CPU:

```powershell
$env:ANNO_RAG_INGEST_BENCH_CORPUS="C:\path\to\corpus"
$env:CARGO_TARGET_DIR="C:\cargo-target"
cargo bench -p anno-rag --bench bench_ingest_option2 --features eval
```

macOS CPU/CoreML, first provider-level check:

```bash
cargo run --release -p anno --example onnx_coreml_bench --features onnx,onnx-coreml
```

macOS full-ingest CoreML check, after adding an `anno-rag` forwarding feature:

```bash
ANNO_RAG_INGEST_BENCH_CORPUS=/path/to/corpus \
cargo bench -p anno-rag --bench bench_ingest_option2 --features gliner2-fastino-coreml
```

## Decision outcomes

- **Kill Option 2:** no warmed full-ingest win. Keep Option 1 only and do not
  reopen pool/fan-out.
- **Productize bounded pool:** Windows CPU passes the gates; add product config
  with conservative defaults and a documented thread budget.
- **CoreML-only path:** macOS CoreML passes but Windows CPU does not. Keep it
  platform-gated and opt-in.
- **Escalate to true batching:** NER-only evidence suggests model execution is
  the bottleneck, but instance pooling fails. Start the A-prime design for true
  batched GLiNER2 ONNX inference.

## Risks

- Silent CoreML fallback can make measurements misleading unless CPU/CoreML are
  compared in the same harness.
- Multiple detector instances multiply model/session memory.
- ORT thread spinning and affinity can affect laptop thermals and repeatability.
- The current full-ingest path includes index/FTS tail work that can hide NER
  improvements.
- A small fixture corpus can overrepresent cold-load and setup costs; acceptance
  must use a corpus large enough to make steady-state throughput visible.
