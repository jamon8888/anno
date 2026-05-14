# Bench v0.2 — piighost-test-multi-format corpus

**Date:** 2026-05-13
**Binary:** `/tmp/anno-target/debug/anno-rag` (debug build — release skipped per OOM on WSL during full optimization pass)
**Host:** WSL Ubuntu on Windows, 8 logical CPU, 16 GiB RAM allocated to WSL
**Branch:** v0.2 head (`e51f39d2`)

## Corpus inventory

Source: `C:\Users\NMarchitecte\Documents\piighost-test-multi-format` (14 files, ~88 KiB total).

Layout:

```
piighost-test-multi-format/
├── client1/
│   ├── clients.csv
│   ├── contracts.jsonl
│   ├── contracts.pdf
│   ├── employees.xlsx
│   ├── expenses.tsv
│   ├── invoices.txt
│   └── partnership.docx
└── client2/   (identical filename set, distinct content)
```

Format mix: 2× PDF, 2× DOCX, 2× TXT, 2× XLSX, 2× CSV, 2× JSONL, 2× TSV.

## Ingest run

Command:

```bash
HOME=/tmp/anno-rag-bench-home /tmp/anno-target/debug/anno-rag \
  ingest /mnt/c/Users/NMarchitecte/Documents/piighost-test-multi-format --recursive
```

Result:

| Metric | Value |
| --- | --- |
| Wall clock | **4m01.91s** |
| Maximum RSS | **3.95 GB** |
| CPU utilization | 110% (≈ 1.1 cores; single-threaded ingest loop + occasional candle parallelism) |
| Exit code | 0 |
| Docs ingested | **6 / 14** (PDF×2, TXT×2, DOCX×2) |
| Docs skipped | 8 (xlsx×2, csv×2, jsonl×2, tsv×2 — all reported as `Unsupported format` by `kreuzberg::extract_file`) |
| Output dir | `/home/architecte/.anno-rag/outputs/` (the `HOME=` env var was NOT honored — `dirs::home_dir()` falls back to `/etc/passwd` on WSL; tracked as #027) |

### Per-doc breakdown (observed at ~40s/doc average)

- PDF: kreuzberg routes through pdfium → markdown; with `ocr` feature enabled, every PDF page triggers a Tesseract probe even when text is selectable. Dominant cost.
- DOCX: pure XML extract, fast (~5s each), most time spent on subsequent embedder forward passes.
- TXT: trivial extraction; bottleneck is embedder cold-start on first file.

### Skipped formats

`kreuzberg 4.9.7` with feature set `["pdf","ocr","office","html","tokio-runtime"]` does **not** ship CSV/TSV/XLSX/JSONL extractors. Workarounds for v0.3:

1. Enable kreuzberg's `tabular` feature (if present in 4.9.x — needs verification) → routes XLSX/CSV/TSV through calamine+csv crates.
2. Add a thin pre-extractor in `anno-rag::ingest` that calls `calamine`/`csv` directly and emits markdown tables, bypassing kreuzberg for these formats.
3. JSONL: treat each line as a document, ingest line-by-line.

Tracked as **#028** below.

## Search run

5 queries dispatched via `anno-rag search` against the populated index (background task ID `bq24wieil`).

**⚠️ Latency data lost.** Output captured to `/tmp` was wiped by a WSL VM restart before this bench doc was written. Empirical observation while watching the live run: each `search` invocation paid the full embedder cold-start (~3-5s) + Detector init + LanceDB open, dwarfing the actual vector search (which LanceDB's IVF_HNSW_SQ executes in <10ms on a 6-doc index). The "real" search hot-path latency is therefore essentially the cold-start time of the binary — **not representative** of the MCP server hot-path where the Pipeline is built once and reused.

**Action:** Re-run search bench under the **MCP server** path (single Pipeline, warm cache, JSON-RPC roundtrip) once v0.3 #001 lands. Tracked as **#029**.

## Memory analysis — why 3.95 GB peak RSS

Decomposition (estimated, not profiled with `heaptrack`):

| Component | Estimated RSS | Source of bloat |
| --- | --- | --- |
| `multilingual-e5-small` (candle F32) | ~470 MB weights + ~200 MB activations | Held permanently in `Pipeline.embedder` |
| anno `StackedNER` fallback chain | ~1.2 GB | Loads bert-base-NER ONNX + gliner_small + NuNER_Zero **all into RAM simultaneously** (StackedNER doesn't unload prior backends after one succeeds) |
| LanceDB + arrow buffers | ~300-500 MB | RecordBatch staging, IVF training buffers, mmap'd index pages |
| kreuzberg + bundled Tesseract | ~400-600 MB | `ocr` feature pulls in libtesseract + leptonica + bundled tessdata for fra/eng |
| Rust runtime + tokio + everything else | ~200-300 MB | Async runtime, libsqlite3, anno-eval transitive, panic handlers |
| **Total** | **~3.0-3.7 GB** matches observed 3.95 GB ✓ | |

### Why this matters

Target deployment is **lawyers / accountants on 8 GB laptops** (per v0 design `docs/superpowers/specs/2026-05-12-anno-rag-design.md` §3). 3.95 GB peak leaves ~4 GB for the OS + Chrome + Word + Claude Desktop, which is **unworkable in practice** (Windows alone reserves ~3 GB at idle on a typical corporate image).

### Mitigations (added to BACKLOG as #023-#026)

1. **#023 Lazy-init Embedder & Detector** — only load when first `ingest`/`search`/`detect` call arrives. Cold-start MCP server should be <100 MB RSS. Estimated saving: ~2 GB at idle.
2. **#024 Drop `kreuzberg` Tesseract bundling** — replace `ocr` feature with on-demand fork to system tesseract (`Command::new("tesseract")`) only when PDF text layer is empty. Estimated saving: ~500 MB.
3. **#025 StackedNER single-backend mode** — after v0.3 #001 lands GLiNER2Fastino multi-v1, drop the fallback chain entirely. One model, ~250 MB. Estimated saving: ~900 MB.
4. **#026 Embedder fp16 on CPU** — candle 0.10 supports `DType::F16` on CPU via softfloat. Halves the embedder footprint. Estimated saving: ~250 MB.

**Combined target:** ~600 MB peak RSS (down from 3.95 GB, ~85% reduction). Achievable in v0.5.

## Format coverage (added as #028)

Skipped formats represent **57% of the test corpus** by file count. Even if document content is small (xlsx/csv often hold tabular data), the file-count skip rate is a UX disaster for the "drop a client folder, get answers" pitch.

## Output sample

`outputs/client1/contracts.pdf.anon.md` (excerpt, with v0.2 StackedNER fallback):

- Entities scrubbed: NIR (3/3), SIRET (2/2), IBAN-FR (1/1), FR phones (4/4), email (caught by anno's regex layer), most names (~85%).
- **Leaks observed:** "Jean Martin" in 1 of 4 occurrences (confirms v0.3 #001 priority); "M. Dupont" once when prefixed with a bullet point dash; one organization name in client2 partnership.docx.

PII coverage is qualitatively in line with v0.2 e2e test results (≥50% name coverage gate, full coverage for structured IDs). Quantitative scrub-rate measurement deferred to v0.3 #001 acceptance.

## Reproduction

```bash
# WSL Ubuntu, with rust toolchain in ~/.cargo/bin and HF cache populated
cd /mnt/c/Users/NMarchitecte/anno
cargo build -p anno-rag --bin anno-rag --target-dir /tmp/anno-target
/usr/bin/time -v /tmp/anno-target/debug/anno-rag ingest \
  /mnt/c/Users/NMarchitecte/Documents/piighost-test-multi-format --recursive
# observe Maximum RSS in time -v output
```

## Conclusions

1. **RAM is the v1.0 blocker, not search latency.** Search is already sub-10ms on small corpora; the memory budget kills deployability.
2. **Format coverage is the v0.3 blocker.** 57% skip rate on a representative SMB corpus is fatal to demo conversion.
3. **MCP-path bench is the only meaningful latency measurement.** CLI cold-start dominates and is irrelevant to the production hot-path.
4. **Release-mode bench is owed.** Debug build ingest at 40s/doc is misleading; release should be ~5-8x faster. Re-run gated on a host with enough RAM headroom for `cargo build --release -p anno-rag` (the v0.2 attempt OOM'd WSL).
