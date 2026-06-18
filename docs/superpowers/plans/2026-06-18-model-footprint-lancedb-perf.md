# Model Footprint & LanceDB Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut model load size / cold-start and speed up search by (a) shipping fp16 GLiNER ONNX graphs, (b) tuning the LanceDB IVF_HNSW_SQ index and search path, and (c) optionally extending the existing INT8 quantization precedent to the NER encoder — every footprint/quality change gated by a new recall+latency bench that does not exist today.

**Architecture:** Footprint reductions are measurement-first. Task 1 builds the missing `bench_recall.rs` gate (intended in the v0.5 spec but never shipped). Tasks 2–4 are low/zero quality-risk wins (fp16 ONNX NER, explicit cosine distance, IVF/HNSW tuning, `nprobes`/`refine_factor`). Task 5 (INT8 NER) and Task 6 (embedder swap to `multilingual-e5-small`, 384-dim) are gated experiments: they only land if recall holds ≥ 95% of the Solon-large baseline. The embedder default stays at Solon-large/F32 until/unless the gate proves the swap safe.

**Tech Stack:** Rust, candle 0.10 (embedder BERT), ONNX Runtime via `gliner2_fastino` (NER), LanceDB 0.29 (`IVF_HNSW_SQ` + FTS hybrid + RRF), criterion 0.5 + sysinfo for benches.

---

## Background & prior decisions (read before starting)

This plan deliberately respects choices already recorded in the repo. Do not "optimize" past them without the Task 1 gate proving it safe.

| Prior decision | Source | Implication for this plan |
|---|---|---|
| Embedder default upgraded e5-small → bge-m3 → **Solon-large (1024-dim), chosen for French legal recall** | [CONFIGURATION.md:68,84](../../CONFIGURATION.md), [anno-rag-design.md:1167](../specs/2026-05-12-anno-rag-design.md) | Downsizing the embedder is a **known quality regression risk**. Now in scope as **Task 6**, but ONLY as a gated experiment — ships as default only if recall@10 ≥ 95% of the Solon-large baseline on French legal queries. |
| Embedder F16 default was planned, then reverted to **F32 default + F16 opt-in** because F16 produced **NaN on CPU** (softmax overflow) | [embed.rs:106-114](../../../crates/anno-rag/src/embed.rs), [v0.5 spec §3.5](../specs/2026-05-13-anno-rag-v0.5-performance-budget.md) | Embedder dtype stays F32-default. F16 is GPU-only and out of scope here. |
| INT8 / INT4 embedder quantization **explicitly deferred** "until we see whether fp16 alone suffices" | [v0.5 spec §2, §6](../specs/2026-05-13-anno-rag-v0.5-performance-budget.md) | Keep deferred. This plan does NOT quantize the embedder. |
| INT8 quantization **already shipped for the reranker** (~571 MB vs ~2.3 GB fp32) | [cross-encoder-rerank-design.md:257](../specs/2026-05-16-anno-rag-cross-encoder-rerank-design.md) | INT8 is an accepted, in-tree technique. Task 5 extends the precedent to NER, not the embedder. |
| LanceDB IVF tuning declared a **non-goal at v0.5 corpus sizes** ("defaults adequate") | [v0.5 spec §2, §6](../specs/2026-05-13-anno-rag-v0.5-performance-budget.md) | Now in scope: Tasks 3–4 move beyond defaults, gated by the bench. |
| `bench_recall.rs` (embedder recall@10 ≥ 95% gate) was specified but **never built** | [v0.5 spec §3.1](../specs/2026-05-13-anno-rag-v0.5-performance-budget.md); absent from `crates/anno-rag/benches/` | Task 1 builds it. It is the prerequisite for every other footprint change. |

### Current state (verified)

- NER download/inventory **prefer `fp32_v2`**, fall back to `fp16_v2`: [download_models.rs:170-173](../../../crates/anno-rag/src/download_models.rs), [model_inventory.rs:70](../../../crates/anno-rag-mcp/src/model_inventory.rs). fp16 graphs exist on the same HF repo.
- Vector index built with **`IvfHnswSqIndexBuilder::default()`** — no explicit distance type, partitions, or SQ refine: [store.rs:1245-1252](../../../crates/anno-rag/src/store.rs).
- Vector search uses **`nearest_to(...)` with no `nprobes`/`refine_factor`/`distance_type`**: [store.rs:1186-1194](../../../crates/anno-rag/src/store.rs). The index is `IVF_HNSW_**SQ**` (vectors stored int8), so search currently pays SQ precision loss with no recovery pass.
- Embedder vectors are L2-normalized ([embed.rs:238-247](../../../crates/anno-rag/src/embed.rs)), so L2 ordering equals cosine ordering — but the distance type is implicit.
- Total weights ≈ **970 MiB** ([model-cache.md:19](../../reference/model-cache.md)).

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/anno-rag/benches/bench_recall.rs` | recall@10 + p50/p95 latency over reference queries; emits a markdown report | **Create** |
| `crates/anno-rag/benches/fixtures/recall_queries.json` | reference (query, relevant-doc-substring) pairs | **Create** |
| `crates/anno-rag/Cargo.toml` | register `[[bench]] bench_recall` | Modify |
| `crates/anno-rag/src/config.rs` | add `ner_onnx_precision` (`"fp16"`/`"fp32"`) + LanceDB tuning knobs | Modify |
| `crates/anno-rag/src/download_models.rs` | flip ONNX candidate order based on precision | Modify (`download_ner_sync`, `NER_ONNX_BASES` usage) |
| `crates/anno-rag-mcp/src/model_inventory.rs` | make required ONNX set precision-aware | Modify |
| `crates/anno-rag/src/store.rs` | tuned index build + tuned vector search | Modify (`maybe_build_index`, hybrid + dense search) |
| `docs/CONFIGURATION.md`, `docs/reference/model-cache.md`, `docs/DOCKER.md` | default embedder model/dim + total size (Task 6, only if gate passes) | Modify |

---

## Task 1: Recall + latency bench harness (the gate)

**Files:**
- Create: `crates/anno-rag/benches/bench_recall.rs`
- Create: `crates/anno-rag/benches/fixtures/recall_queries.json`
- Modify: `crates/anno-rag/Cargo.toml` (bench registration)
- Reuse: `crates/anno-rag/benches/common/mod.rs` (`pipeline_in_tempdir`, `bench_corpus_dir`)

This harness is the contract every later task is measured against. It ingests the existing bench corpus, runs reference queries, and reports recall@10 plus search latency. It writes a JSON baseline so a later run can assert "≥ 95% of baseline recall".

- [ ] **Step 1: Create the reference query fixture**

Create `crates/anno-rag/benches/fixtures/recall_queries.json`. Each entry pairs a French legal query with a substring that must appear in the chunk text of a relevant hit. Use phrases you can confirm exist in `crates/anno-rag/tests/fixtures/bench_corpus`.

```json
[
  { "query": "résiliation du contrat avec préavis", "relevant_substring": "résiliation" },
  { "query": "clause de confidentialité entre les parties", "relevant_substring": "confidentialit" },
  { "query": "montant du loyer mensuel", "relevant_substring": "loyer" },
  { "query": "obligations de l'employeur envers le salarié", "relevant_substring": "employeur" },
  { "query": "délai de prescription de l'action", "relevant_substring": "prescription" }
]
```

- [ ] **Step 2: Write the failing bench (compile-fail first)**

Create `crates/anno-rag/benches/bench_recall.rs`. It is a criterion harness whose `bench_function` measures query latency, but it ALSO computes recall@10 once up front and `panic!`s if recall drops below an env-provided floor — so it doubles as a CI gate.

```rust
//! recall@10 + search latency gate. Ingests the bench corpus, runs reference
//! queries, computes recall@10, and writes a JSON baseline. Set
//! `ANNO_RECALL_FLOOR=<0.0-1.0>` to fail the run when recall drops below it
//! (used by CI to enforce ">= 95% of baseline" on footprint-changing PRs).
#![allow(clippy::unwrap_used, missing_docs)]
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use serde::Deserialize;
use tokio::runtime::Runtime;

#[derive(Deserialize)]
struct RefQuery {
    query: String,
    relevant_substring: String,
}

fn load_ref_queries() -> Vec<RefQuery> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/fixtures/recall_queries.json");
    let raw = std::fs::read_to_string(&path).expect("read recall_queries.json");
    serde_json::from_str(&raw).expect("parse recall_queries.json")
}

fn bench_recall(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let queries = load_ref_queries();

    let (pipeline, _tmp) = rt.block_on(async {
        let (p, tmp) = common::pipeline_in_tempdir().await;
        let n = p
            .ingest_folder(&common::bench_corpus_dir(), true, &tmp.path().join("outputs"))
            .await
            .expect("ingest");
        assert!(n > 0, "bench corpus ingested 0 documents — warm the HF cache first");
        (p, tmp)
    });

    // Compute recall@10 once (a hit is relevant if any returned chunk text
    // contains the expected substring, case-insensitive).
    let mut relevant = 0usize;
    for q in &queries {
        let hits = rt.block_on(pipeline.search(&q.query, 10)).unwrap();
        let needle = q.relevant_substring.to_lowercase();
        if hits.iter().any(|h| h.text.to_lowercase().contains(&needle)) {
            relevant += 1;
        }
    }
    let recall = relevant as f64 / queries.len() as f64;
    eprintln!("recall@10 = {recall:.3} ({relevant}/{})", queries.len());

    // Write baseline JSON for later comparison runs.
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/recall_baseline.json");
    let _ = std::fs::write(&out, format!("{{\"recall_at_10\": {recall}}}"));

    if let Ok(floor) = std::env::var("ANNO_RECALL_FLOOR") {
        let floor: f64 = floor.parse().expect("ANNO_RECALL_FLOOR must be a float");
        assert!(recall >= floor, "recall@10 {recall:.3} below floor {floor:.3}");
    }

    // Latency arm: drives criterion's normal p50/p95 reporting.
    let probe = queries[0].query.clone();
    c.bench_function("recall_query_latency", |b| {
        b.to_async(&rt).iter(|| async { pipeline.search(&probe, 10).await.unwrap() });
    });
}
criterion_group!(benches, bench_recall);
criterion_main!(benches);
```

- [ ] **Step 3: Register the bench in Cargo.toml**

In `crates/anno-rag/Cargo.toml`, in the `[[bench]]` section list, add (match the existing style of the other `[[bench]]` entries — `harness = false`):

```toml
[[bench]]
name = "bench_recall"
harness = false
```

Confirm `serde`/`serde_json` are available to benches (they are workspace deps used elsewhere in `benches/`). If `serde` is not already a `dev-dependency`, add `serde = { workspace = true, features = ["derive"] }` and `serde_json = { workspace = true }` under `[dev-dependencies]`.

- [ ] **Step 4: Compile the bench (verify it builds)**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```
Expected: clean check. If `pipeline.search` / `h.text` field names mismatch, fix against `crates/anno-rag/src/store.rs` (`batch_to_hit`) and `pipeline.rs` — do not invent field names.

- [ ] **Step 5: Run the bench to establish the baseline**

Requires the HF model cache warm (or `ANNO_MODELS_DIR` set). Run:
```powershell
$env:ANNO_MODELS_DIR = "E:\anno-models"  # or your populated cache
cargo bench -p anno-rag --bench bench_recall
```
Expected: prints `recall@10 = X.XXX (n/5)` and writes `target/recall_baseline.json`. Record the baseline recall value in the PR description.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/benches/bench_recall.rs crates/anno-rag/benches/fixtures/recall_queries.json crates/anno-rag/Cargo.toml
git commit -m "test(anno-rag): add recall@10 + latency bench gate (v0.5 spec §3.1, never shipped)"
```

---

## Task 2: fp16 GLiNER ONNX graphs (zero-quality-risk footprint win)

**Files:**
- Modify: `crates/anno-rag/src/config.rs` (add `ner_onnx_precision`)
- Modify: `crates/anno-rag/src/download_models.rs` (`download_ner_sync` candidate order)
- Modify: `crates/anno-rag-mcp/src/model_inventory.rs` (precision-aware required set)
- Test: unit tests in each modified file

Flip the download/inventory preference from `fp32_v2` to `fp16_v2` (configurable). Halves the ~500 MB NER footprint with near-lossless inference. fp16 graphs already exist on the HF repo and are the documented macOS fallback.

- [ ] **Step 1: Write the failing config test**

In `crates/anno-rag/src/config.rs` tests module, add:

```rust
#[test]
fn ner_onnx_precision_defaults_to_fp16() {
    let c = AnnoRagConfig::default();
    assert_eq!(c.ner_onnx_precision, "fp16");
}

#[test]
fn ner_onnx_precision_round_trips_fp32() {
    let json = r#"{"ner_onnx_precision":"fp32"}"#;
    let c: AnnoRagConfig = serde_json::from_str(json).unwrap();
    assert_eq!(c.ner_onnx_precision, "fp32");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: FAIL — no field `ner_onnx_precision`.

- [ ] **Step 3: Add the config field + default**

In `crates/anno-rag/src/config.rs`, add a default fn near `default_ner_model_id` (line ~179):

```rust
fn default_ner_onnx_precision() -> String {
    "fp16".to_string()
}
```

Add the field to the struct (near `ner_model_id`, line ~314), mirroring the existing `#[serde(default = ...)]` + doc attribute style used by neighbours:

```rust
    /// ONNX graph precision for the NER detector: "fp16" (default, ~250 MB)
    /// or "fp32" (~500 MB, exact). fp16 is near-lossless for inference.
    #[serde(default = "default_ner_onnx_precision")]
    pub ner_onnx_precision: String,
```

Add to the `Default` impl (line ~740 area): `ner_onnx_precision: default_ner_onnx_precision(),`.

- [ ] **Step 4: Run the config test to verify it passes**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: PASS.

- [ ] **Step 5: Write the failing download-order test**

In `crates/anno-rag/src/download_models.rs` tests module, add a pure helper test (no network):

```rust
#[test]
fn onnx_candidates_fp16_first_when_precision_fp16() {
    let c = onnx_candidates("encoder", "fp16");
    assert_eq!(c[0], "fp16_v2/encoder_fp16.onnx");
    assert_eq!(c[1], "fp32_v2/encoder_fp32.onnx");
}

#[test]
fn onnx_candidates_fp32_first_when_precision_fp32() {
    let c = onnx_candidates("encoder", "fp32");
    assert_eq!(c[0], "fp32_v2/encoder_fp32.onnx");
    assert_eq!(c[1], "fp16_v2/encoder_fp16.onnx");
}
```

- [ ] **Step 6: Run to verify it fails**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: FAIL — `onnx_candidates` not found.

- [ ] **Step 7: Extract the candidate-order helper and use it**

In `crates/anno-rag/src/download_models.rs`, add the helper:

```rust
/// Ordered HF-relative candidate paths for one ONNX graph `base`, preferring
/// `precision` ("fp16" or "fp32") and falling back to the other.
fn onnx_candidates(base: &str, precision: &str) -> Vec<String> {
    let fp16 = format!("fp16_v2/{base}_fp16.onnx");
    let fp32 = format!("fp32_v2/{base}_fp32.onnx");
    if precision == "fp32" {
        vec![fp32, fp16]
    } else {
        vec![fp16, fp32]
    }
}
```

Thread `precision: &str` through `download_ner` → `download_ner_sync` (pass `cfg.ner_onnx_precision.clone()` from `download` at line ~43). Replace the hardcoded `candidates` block in `download_ner_sync` (lines ~169-179) with:

```rust
    for base in NER_ONNX_BASES {
        let candidates = onnx_candidates(base, precision);
        let c_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
        c_refs
            .iter()
            .find_map(|c| repo.get(c).ok())
            .ok_or_else(|| Error::Detect(format!("gliner2 onnx graph '{base}' not found")))?;
    }
```

Also update the tokenizer candidate order in `download_ner_sync` (lines ~150-154) to put `fp16_v2/tokenizer.json` first when `precision == "fp16"`.

- [ ] **Step 8: Run to verify the helper test passes**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: PASS.

- [ ] **Step 9: Make the inventory precision-aware**

In `crates/anno-rag-mcp/src/model_inventory.rs`, the required-files generator at line ~66-72 hardcodes `fp32_v2/{base}_fp32.onnx`. Add a precision parameter (default callers pass `"fp16"`, matching the new config default) so a populated fp16 cache is reported READY. Mirror the existing fp16 test at line ~495.

```rust
fn ner_onnx_files(ner_onnx_dir: &str, precision: &str) -> Vec<String> {
    let (subdir, suffix) = if precision == "fp32" {
        ("fp32_v2", "fp32")
    } else {
        ("fp16_v2", "fp16")
    };
    let mut files: Vec<String> = NER_ONNX_BASES
        .iter()
        .map(|base| format!("{ner_onnx_dir}/{subdir}/{base}_{suffix}.onnx"))
        .collect();
    files.push(format!("{ner_onnx_dir}/{subdir}/tokenizer.json"));
    files
}
```

Update callers to read precision from config. Keep the readiness check tolerant: a cache populated with EITHER precision should count as ready (accept fp16 OR fp32 present) to avoid breaking existing fp32 caches — add a test asserting an fp32-only cache is still READY.

- [ ] **Step 10: Run the mcp crate tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```
Expected: PASS (including the existing `fp16_onnx_required_files_return_ready` test).

- [ ] **Step 11: fmt + clippy, then commit**

Per repo rule [fmt+clippy before PR]. Run:
```powershell
cargo fmt -p anno-rag -p anno-rag-mcp
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode clippy
git add crates/anno-rag/src/config.rs crates/anno-rag/src/download_models.rs crates/anno-rag-mcp/src/model_inventory.rs
git commit -m "feat(models): default NER ONNX to fp16 (~250 MB), fp32 opt-in via ner_onnx_precision"
```

- [ ] **Step 12: Manual footprint check (evidence, not assertion)**

Re-download into a clean dir and confirm size dropped:
```powershell
cargo run -p anno-rag-bin -- download-models --dir E:\anno-models-fp16
# Expected: NER portion ~250 MiB (was ~500 MiB); total cache ~720 MiB (was ~970 MiB)
```
Then run the Task 1 gate against the fp16 cache and confirm recall is unchanged vs baseline:
```powershell
$env:ANNO_MODELS_DIR = "E:\anno-models-fp16"; $env:ANNO_RECALL_FLOOR = "0.99"
cargo bench -p anno-rag --bench bench_recall
```
Expected: recall@10 ≥ 0.99 × baseline (fp16 inference is near-lossless). Record numbers in PR.

---

## Task 3: Explicit cosine distance + tuned IVF_HNSW_SQ build

**Files:**
- Modify: `crates/anno-rag/src/config.rs` (index tuning knobs)
- Modify: `crates/anno-rag/src/store.rs` (`maybe_build_index`, line ~1220)
- Test: `crates/anno-rag/src/store.rs` tests + Task 1 gate

The index is built with `IvfHnswSqIndexBuilder::default()` and no distance type. Set distance explicitly to cosine (vectors are L2-normalized, so ranking is equivalent — this guards against a future non-normalized model) and scale partitions to corpus size for better recall/latency.

- [ ] **Step 1: Add index tuning config knobs (with defaults that preserve current behavior)**

In `crates/anno-rag/src/config.rs`, add fields + defaults:

```rust
fn default_index_distance() -> String { "cosine".to_string() }
fn default_index_num_partitions() -> Option<usize> { None } // None = auto (lance default)
```

```rust
    /// Vector index distance metric: "cosine" (default) | "l2" | "dot".
    #[serde(default = "default_index_distance")]
    pub index_distance: String,

    /// IVF partition count. None = LanceDB auto (≈ sqrt(rows)).
    #[serde(default = "default_index_num_partitions")]
    pub index_num_partitions: Option<usize>,
```

Add both to the `Default` impl. Add a test asserting defaults (`"cosine"`, `None`).

- [ ] **Step 2: Write the failing distance-mapping test**

In `crates/anno-rag/src/store.rs` tests, add:

```rust
#[test]
fn distance_type_maps_from_config_string() {
    use lancedb::DistanceType;
    assert!(matches!(distance_from_str("cosine"), DistanceType::Cosine));
    assert!(matches!(distance_from_str("l2"), DistanceType::L2));
    assert!(matches!(distance_from_str("dot"), DistanceType::Dot));
    assert!(matches!(distance_from_str("garbage"), DistanceType::Cosine)); // safe default
}
```

- [ ] **Step 3: Run to verify it fails**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: FAIL — `distance_from_str` not found.

- [ ] **Step 4: Implement `distance_from_str` and use it in `maybe_build_index`**

In `crates/anno-rag/src/store.rs` add:

```rust
fn distance_from_str(s: &str) -> lancedb::DistanceType {
    match s {
        "l2" => lancedb::DistanceType::L2,
        "dot" => lancedb::DistanceType::Dot,
        _ => lancedb::DistanceType::Cosine,
    }
}
```

`Store` must be able to read the config. If `Store` does not already hold the relevant config values, thread `index_distance: String` and `index_num_partitions: Option<usize>` into the struct at construction (open path, line ~202). Then update the builder in `maybe_build_index` (lines ~1245-1252):

```rust
        let mut builder = IvfHnswSqIndexBuilder::default()
            .distance_type(distance_from_str(&self.index_distance));
        if let Some(parts) = self.index_num_partitions {
            builder = builder.num_partitions(parts as u32);
        }
        self.tbl
            .create_index(&["vector"], Index::IvfHnswSq(builder))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("create_index: {e}")))?;
        Ok(true)
```

Note: confirm exact builder method names against the pinned LanceDB (`cargo doc -p lancedb --open`, type `IvfHnswSqIndexBuilder`). In 0.29 these are `distance_type` and `num_partitions`; adjust if the signature differs (`u32` vs `usize`).

- [ ] **Step 5: Run unit tests to verify pass**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: PASS.

- [ ] **Step 6: Run the recall gate to confirm no regression**

```powershell
$env:ANNO_MODELS_DIR = "E:\anno-models-fp16"; $env:ANNO_RECALL_FLOOR = "0.95"
cargo bench -p anno-rag --bench bench_recall
```
Expected: recall@10 ≥ 0.95 × baseline AND `recall_query_latency` p95 not worse than baseline. Record both.

- [ ] **Step 7: fmt + clippy + commit**

```powershell
cargo fmt -p anno-rag
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode clippy
git add crates/anno-rag/src/config.rs crates/anno-rag/src/store.rs
git commit -m "perf(store): explicit cosine distance + configurable IVF partitions on vector index"
```

---

## Task 4: Search-time `nprobes` + `refine_factor` (recover SQ precision)

**Files:**
- Modify: `crates/anno-rag/src/config.rs` (search knobs)
- Modify: `crates/anno-rag/src/store.rs` (dense + hybrid search, lines ~1186-1194)
- Test: Task 1 gate (recall + latency)

The index stores int8 (SQ) vectors. Without `refine_factor`, search ranks on quantized distances and never re-scores with full-precision vectors — leaving recall on the table. Adding `refine_factor` recovers most SQ loss; `nprobes` trades latency for recall. Both default conservatively so behavior only improves.

- [ ] **Step 1: Add search tuning config knobs**

In `crates/anno-rag/src/config.rs`:

```rust
fn default_search_nprobes() -> usize { 20 }       // lance default; explicit for clarity
fn default_search_refine_factor() -> u32 { 10 }   // re-score top (k*refine) with full vectors
```

```rust
    /// IVF probes scanned per query. Higher = better recall, slower.
    #[serde(default = "default_search_nprobes")]
    pub search_nprobes: usize,

    /// SQ refine factor: re-rank (k × factor) candidates with full-precision
    /// vectors. 0 disables. Recovers recall lost to scalar quantization.
    #[serde(default = "default_search_refine_factor")]
    pub search_refine_factor: u32,
```

Add to `Default` impl + a defaults test.

- [ ] **Step 2: Thread knobs into `Store` and the query builders**

Thread `search_nprobes`/`search_refine_factor` into `Store` (same construction path as Task 3). In the hybrid search (lines ~1186-1194) and any dense-only `nearest_to` path, apply them:

```rust
        let mut q = self
            .tbl
            .query()
            .nearest_to(query_vec.to_vec())?
            .distance_type(distance_from_str(&self.index_distance))
            .nprobes(self.search_nprobes);
        if self.search_refine_factor > 0 {
            q = q.refine_factor(self.search_refine_factor);
        }
        let stream = q
            .full_text_search(FullTextSearchQuery::new(query_text.to_string()))
            .rerank(Arc::new(RRFReranker::default()))
            .limit(k)
            .execute()
            .await?;
```

Confirm method names/types against `cargo doc -p lancedb` (`VectorQuery::nprobes(usize)`, `refine_factor(u32)`, `distance_type(DistanceType)` in 0.29). Adjust if the pinned version differs.

- [ ] **Step 3: Compile**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```
Expected: clean.

- [ ] **Step 4: Sweep recall vs latency to pick defaults (evidence-driven)**

Run the gate at a few settings and record the recall/latency tradeoff:
```powershell
$env:ANNO_MODELS_DIR = "E:\anno-models-fp16"
foreach ($rf in 0,5,10,20) {
  $env:ANNO_RAG_SEARCH_REFINE_FACTOR = "$rf"
  cargo bench -p anno-rag --bench bench_recall
}
```
Expected: recall climbs and plateaus as `refine_factor` rises; latency rises modestly. Pick the smallest value at the recall plateau as the default and update `default_search_refine_factor` to match. Record the table in the PR.

> If `ANNO_RAG_SEARCH_REFINE_FACTOR` env override isn't wired, add it to the env-override block in `config.rs` (near the other `ANNO_RAG_*` parses, line ~830+) so the sweep doesn't require recompiles.

- [ ] **Step 5: Final gate run + unit tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
$env:ANNO_RECALL_FLOOR = "0.95"; cargo bench -p anno-rag --bench bench_recall
```
Expected: tests PASS; recall@10 ≥ baseline (should be ≥, since refine recovers SQ loss); p95 within SLO (< 200ms warm per v0.5).

- [ ] **Step 6: fmt + clippy + commit**

```powershell
cargo fmt -p anno-rag
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode clippy
git add crates/anno-rag/src/config.rs crates/anno-rag/src/store.rs
git commit -m "perf(store): tune vector search nprobes + SQ refine_factor (recover quantization recall)"
```

---

## Task 5 (optional, gated): INT8 GLiNER ONNX encoder

**Files:**
- Create: `scripts/quantize_gliner_onnx.py` (offline INT8 dynamic quantization)
- Modify: `crates/anno-rag/src/config.rs` (`ner_onnx_precision` accepts `"int8"`)
- Modify: `crates/anno-rag/src/download_models.rs` (int8 candidate path)
- Modify: `crates/anno-rag-mcp/src/model_inventory.rs` (int8 readiness)

Only do this if Task 2's fp16 still leaves NER footprint/CPU latency as the bottleneck. Extends the **existing** reranker INT8 precedent ([cross-encoder-rerank-design.md:257](../specs/2026-05-16-anno-rag-cross-encoder-rerank-design.md)) to NER. INT8 graphs must be produced offline and hosted (or quantized at download time); they are NOT on the stock HF repo.

- [ ] **Step 1: Write the offline quantizer script**

Create `scripts/quantize_gliner_onnx.py` using `onnxruntime.quantization.quantize_dynamic` over the 8 graphs (`NER_ONNX_BASES`), writing `int8_v2/{base}_int8.onnx`. Quantize encoder/span_rep/classifier; leave tiny control graphs (`count_pred_argmax`, `count_lstm_fixed`) fp32 if quantization hurts them.

```python
import sys, pathlib
from onnxruntime.quantization import quantize_dynamic, QuantType

BASES = ["encoder","token_gather","span_rep","schema_gather",
         "count_pred_argmax","count_lstm_fixed","scorer","classifier"]
SKIP = {"count_pred_argmax","count_lstm_fixed"}  # control graphs: keep fp32

src = pathlib.Path(sys.argv[1]) / "fp32_v2"
dst = pathlib.Path(sys.argv[1]) / "int8_v2"; dst.mkdir(exist_ok=True)
for base in BASES:
    fp32 = src / f"{base}_fp32.onnx"
    if base in SKIP:
        (dst / f"{base}_fp32.onnx").write_bytes(fp32.read_bytes()); continue
    quantize_dynamic(str(fp32), str(dst / f"{base}_int8.onnx"), weight_type=QuantType.QInt8)
print("int8 graphs written to", dst)
```

- [ ] **Step 2: Extend `onnx_candidates` for int8**

In `download_models.rs`, when `precision == "int8"` prefer `int8_v2/{base}_int8.onnx`, then fp16, then fp32. Reuse the helper + add a unit test mirroring Task 2 Step 5.

- [ ] **Step 3: Run unit tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: PASS.

- [ ] **Step 4: Quantize, then run the recall gate against int8 NER**

```powershell
python scripts/quantize_gliner_onnx.py E:\anno-models-fp16\gliner2-multi-v1-onnx
$env:ANNO_RAG_NER_ONNX_PRECISION = "int8"; $env:ANNO_RECALL_FLOOR = "0.95"
cargo bench -p anno-rag --bench bench_recall
```
Expected: NER footprint ~125 MiB; recall@10 ≥ 0.95 × baseline. **If recall fails, do not ship int8** — fp16 (Task 2) is the floor. Record the decision.

- [ ] **Step 5: fmt + clippy + commit (only if recall held)**

```powershell
cargo fmt -p anno-rag -p anno-rag-mcp
git add scripts/quantize_gliner_onnx.py crates/anno-rag/src/config.rs crates/anno-rag/src/download_models.rs crates/anno-rag-mcp/src/model_inventory.rs
git commit -m "feat(models): optional int8 NER ONNX (extends reranker INT8 precedent), gated on recall@10"
```

---

## Task 6 (gated): Embedder swap to `multilingual-e5-small` (384-dim)

**Files:**
- Modify: `crates/anno-rag/benches/bench_recall.rs` (env-parameterized embed model)
- Modify (only if the gate passes): `crates/anno-rag/src/config.rs` (`default_embed_model`, `default_embed_dim`, `default_memory_embedding_dim`)
- Modify (only if the gate passes): `docs/CONFIGURATION.md`, `docs/reference/model-cache.md`, `docs/DOCKER.md` (default model + dim + ~size)
- Test: Task 1 recall gate, run twice (Solon baseline vs e5-small)

The embedder is fully config-driven — `chunks_schema(cfg.embed_dim)` / `memories_schema(cfg.memory_embedding_dim)` ([store.rs:215-227](../../../crates/anno-rag/src/store.rs)) and `Embedder::load` reads `cfg.embed_model`/`embed_dim` ([embed.rs:35-74](../../../crates/anno-rag/src/embed.rs)). So the swap is **config + re-ingest**, not a code rewrite. The work here is almost entirely *measurement and a go/no-go decision* — and a migration note, because a 1024-dim index cannot serve 384-dim queries.

Swap target: `embed_model = "intfloat/multilingual-e5-small"`, `embed_dim = 384`, `memory_embedding_dim = 384`. Expected footprint: embedder ~470 → ~120 MB, **plus the vector index shrinks ~2.6×** and cosine math speeds up proportionally.

> **Migration warning (must be in the PR + CHANGELOG if this ships):** changing `embed_dim` changes the LanceDB `vector` column width. Existing vaults/indexes built at 1024-dim are **incompatible** and must be re-ingested. The bench uses fresh temp dirs so measurement is unaffected, but a default change is a breaking re-index for existing users. Ship behind a clear upgrade note, not silently.

- [ ] **Step 1: Parameterize the recall bench by embed model (no recompile per model)**

The Task 1 bench builds a Pipeline from `AnnoRagConfig::default()` (Solon-large). The env overrides `ANNO_RAG_EMBED_MODEL` / `ANNO_RAG_EMBED_DIM` already exist in `config.rs`. Confirm `common::pipeline_in_tempdir()` honors them (it builds `AnnoRagConfig { data_dir, ..Default::default() }` — env overrides are applied in `Pipeline::new`/config load; verify by reading `config.rs` env-override block ~line 830). If `pipeline_in_tempdir` does NOT apply env overrides, change it to load config via the same override path the binary uses, so the bench reflects `ANNO_RAG_EMBED_*`.

Add a `memory_embedding_dim` env override if absent (`ANNO_RAG_MEMORY_EMBEDDING_DIM` per [CONFIGURATION.md:353](../../CONFIGURATION.md)) so both dims move together.

- [ ] **Step 2: Compile**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```
Expected: clean.

- [ ] **Step 3: Download the candidate embedder into a separate cache**

```powershell
$env:ANNO_RAG_EMBED_MODEL = "intfloat/multilingual-e5-small"
cargo run -p anno-rag-bin -- download-models --dir E:\anno-models-e5small
# Expected: embedder portion ~120 MiB (was ~470 MiB)
```

- [ ] **Step 4: Establish the Solon-large baseline (if not already saved)**

```powershell
$env:ANNO_MODELS_DIR = "E:\anno-models-fp16"
Remove-Item Env:ANNO_RAG_EMBED_MODEL -ErrorAction SilentlyContinue
cargo bench -p anno-rag --bench bench_recall   # writes target/recall_baseline.json (Solon)
```
Record `recall_solon` from the printed `recall@10 = ...` line.

- [ ] **Step 5: Run the gate for e5-small at the 95%-of-baseline floor**

```powershell
$env:ANNO_MODELS_DIR = "E:\anno-models-e5small"
$env:ANNO_RAG_EMBED_MODEL = "intfloat/multilingual-e5-small"
$env:ANNO_RAG_EMBED_DIM = "384"
$env:ANNO_RAG_MEMORY_EMBEDDING_DIM = "384"
$env:ANNO_RECALL_FLOOR = "$([math]::Round($recall_solon * 0.95, 3))"
cargo bench -p anno-rag --bench bench_recall
```
Expected: prints `recall@10` for e5-small and its `recall_query_latency` p95. Compare footprint, latency, and recall against Solon.

- [ ] **Step 6: Decision gate (record the verdict in the PR)**

- **If `recall_e5 ≥ 0.95 × recall_solon`:** proceed to Step 7 (make it the default).
- **If it fails:** do NOT change the default. Document e5-small as the supported low-footprint opt-in (it already is, per [CONFIGURATION.md:85](../../CONFIGURATION.md)) and stop here. The win is still available to footprint-constrained users via config; the legal-recall default is preserved.

- [ ] **Step 7: (Only if gate passed) Flip the defaults**

In `crates/anno-rag/src/config.rs`:
```rust
fn default_embed_model() -> String { "intfloat/multilingual-e5-small".to_string() }
fn default_embed_dim() -> usize { 384 }
// and the memory embedding dim default → 384 (match)
```
Update the existing config round-trip tests that assert `1024` / `Solon-embeddings-large-0.1` (e.g. [config.rs:1135](../../../crates/anno-rag/src/config.rs)) to the new defaults, and add a test asserting the new default model + dim.

- [ ] **Step 8: (Only if gate passed) Run unit tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: PASS (with updated default assertions).

- [ ] **Step 9: (Only if gate passed) Update docs + migration note**

Update `docs/CONFIGURATION.md` (lines ~68-88, ~325-326, ~374-376), `docs/reference/model-cache.md` (the ~970 MiB total → new total), and `docs/DOCKER.md` (ENV defaults) to the new default model/dim. Add a **breaking-change / re-index** note to `CHANGELOG` and the upgrade docs (1024-dim → 384-dim requires re-ingest).

- [ ] **Step 10: (Only if gate passed) fmt + clippy + commit**

```powershell
cargo fmt -p anno-rag
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode clippy
git add crates/anno-rag/src/config.rs crates/anno-rag/benches/bench_recall.rs docs/CONFIGURATION.md docs/reference/model-cache.md docs/DOCKER.md
git commit -m "perf(embed): default to multilingual-e5-small (384-dim) — gated on recall@10 >= 95% of Solon baseline; BREAKING: requires re-index"
```

---

## Out of scope (deliberately deferred — see prior decisions)

- **Embedder quantization (INT8/INT4)** — deferred by v0.5 §6; do not touch in this plan. (Note: Task 6 *swaps* the embedder to a smaller model rather than quantizing the existing one — a different lever.)
- **Embedder F16 default** — known CPU NaN regression; F32 stays default.
- **GPU/Metal/CUDA tuning** — target hardware is CPU laptop (v0.5 §2).
- **LanceDB on-disk format / IVF rebuild scheduling** — separate concern.

## Self-review checklist (run before handing off to execution)

- [ ] Every footprint change (Tasks 2, 5, 6) and search change (Tasks 3–4) is gated by the Task 1 recall bench.
- [ ] Embedder default (Solon-large/F32) only changes in Task 6, and only if recall@10 ≥ 95% of baseline; the swap PR carries the breaking re-index migration note.
- [ ] No embedder dtype change (F16 CPU-NaN) and no embedder quantization (deferred); Task 6 is a model *swap*, not a precision change.
- [ ] Config defaults preserve current behavior where ambiguous (`index_num_partitions = None`, `nprobes = 20`), except the intended changes: `ner_onnx_precision = "fp16"`, `index_distance = "cosine"`, and (gated) the Task 6 embedder default.
- [ ] `distance_from_str` / `onnx_candidates` / `ner_onnx_files` signatures are consistent across the tasks that call them.
- [ ] LanceDB builder/query method names verified against the pinned 0.29 via `cargo doc -p lancedb` before relying on `nprobes`/`refine_factor`/`num_partitions`.
- [ ] `fmt` + `clippy` (`--jobs 2`) run before each commit; fmt committed separately if large.

## References

- [v0.5 Performance Budget spec](../specs/2026-05-13-anno-rag-v0.5-performance-budget.md) — bench harness intent, fp16/quantization deferrals, LanceDB non-goal
- [Cross-encoder rerank design](../specs/2026-05-16-anno-rag-cross-encoder-rerank-design.md) — in-tree INT8 precedent
- [anno-rag design](../specs/2026-05-12-anno-rag-design.md) — embedder selection history
- [CONFIGURATION.md](../../CONFIGURATION.md), [model-cache.md](../../reference/model-cache.md) — current defaults, ~970 MiB total
- Code: [embed.rs](../../../crates/anno-rag/src/embed.rs), [download_models.rs](../../../crates/anno-rag/src/download_models.rs), [store.rs](../../../crates/anno-rag/src/store.rs), [model_inventory.rs](../../../crates/anno-rag-mcp/src/model_inventory.rs)
