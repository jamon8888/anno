# anno-rag — Design Spec

**Date:** 2026-05-12 (rev. 2 — research integration)
**Status:** draft, pending implementation plan
**Author:** brainstorming session + 4 parallel research deep-dives (Kreuzberg, Cloakpipe-core, LanceDB, Legal-RAG SOTA)
**Validated decisions:** §15 (33 entries)
**Implements:** new crate `crates/anno-rag` in the anno workspace

**Revision history:**
- rev. 1 (2026-05-12 morning): initial design from brainstorming session; based on aperçus of each dependency
- rev. 2 (2026-05-12 afternoon): integrated 4 parallel research reports. Key changes:
  - Kreuzberg already provides hierarchical structure + section-aware chunking → simplified `ingest/structure.rs` from parser to thin typing layer; dropped lift of hacienda chunker for MVP; Tesseract bundled (no longer documented prerequisite).
  - LanceDB ≥ 0.27 ships native FTS, hybrid + `RRFReranker`, time-travel → dropped custom RRF, dropped custom supersessions table; switched to `IVF_HNSW_SQ` index; added `forget()` purge routine (delete + Prune + Compact).
  - Cloakpipe `Detector` is a struct, not a trait → adopted wrapper-pipeline pattern; `SqliteVault` not file `Vault`; **custom audit module** (cloakpipe-audit not hash-chained); Argon2id KDF over OS-keyring secret.
  - Legal-RAG SOTA → **switched embedder to `BAAI/bge-m3`**; kept `camembert-L6-mmarcoFR` reranker (small-footprint constraint), documented `bge-reranker-v2-m3` as power-user pair; **added §6.6 Contextual Prefixes + Summary-Augmented Chunking** and **§6.7 Quote-grammar verifier**; added internal eval set in LegalBench-RAG format.
  - Process-safety: added advisory lock file (LanceDB not multi-process safe).
- rev. 3 (2026-05-12 evening): Rust architecture pass + version audit. Key changes:
  - **§5 single mega-crate → 7 library crates + 3 binaries** (anno-rag-core / -ingest / -detect / -embed / -store / -audit / -eval + cli/mcp/api binaries) — mirrors anno's existing pattern
  - **NEW §5.1 — Traits at the seams** (Ingest, Detect, Embed, Rerank, Store, Audit, PrivilegeGate) declared in `anno-rag-core` for substitutability + mockability
  - **NEW §5.2 — Error model** (thiserror enum, fatal vs recoverable distinction)
  - **NEW §5.3 — Newtype IDs** (DocId/ChunkId/SubjectId/CitationId, compile-time disambiguation)
  - **NEW §5.4 — Build performance discipline** with verified dep block, profile tuning, linker swap (mold/lld/rust-lld), cranelift codegen, sccache wrapper, target build-times
  - **NEW §5.5 — Workspace conventions** (lints, MSRV 1.81, edition 2024, cargo-deny)
  - Version audit (24 deps verified):
    - **arrow 57 → 58** (lancedb 0.27 requires it)
    - **axum 0.7 → 0.8** (breaking: path-params `/:id` → `/{id}`, `Sync` on handlers, native async-in-trait)
    - **fs2 → fd-lock 4** (fs2 unmaintained, fd-lock is cross-platform successor)
    - **tokio "1" → "1.51" LTS** for predictability
    - **candle, ort, rmcp** pinned to exact versions
    - **cargo-watch → bacon** in dev tooling (cargo-watch on life support)
    - Models: kept bge-m3 + camembert-L6 + gliner2-multi-v1; added `camembert-L8` and `bge-reranker-v2.5-gemma2-lightweight` as opt-in alternatives

---

## 1. Purpose

Build a local, GDPR-compliant **document anonymizer + RAG service** for French regulated professions (lawyers, tax/accounting, HR, eventually healthcare). The service runs on a single desktop or small server, ingests documents from a local folder, produces pseudonymized copies, indexes them for retrieval, and exposes the index to **Claude Cowork (or any MCP-compatible client) as the user-facing chat surface**.

The local service is the privacy boundary. Every chunk that crosses out to a remote LLM is pseudonymized through a vault stored on disk and never leaves the host in cleartext.

## 2. Out of scope (MVP)

- Desktop GUI (Cowork is the UI)
- Local LLM inference (remote LLM via Cowork is the design)
- LoRA training (anno is inference-only; LoRAs are trained externally and shipped as pack assets)
- Multi-tenant deployments (single-tenant per host)
- Cross-jurisdiction citation graph (FR only at v1)
- HDS-certified hosting for medical (out — flagged and refused by `medical-fr` pack)
- Watch mode / auto-reindex on file change (deferred to v1.1)

## 3. User-facing surfaces

Three interfaces, all backed by the same service core:

- **CLI** (`anno-rag`): ops + admin (ingest, search, forget, find-subject, export, register, pack, profile)
- **MCP server** (stdio): retrieval tools for Cowork / Claude Code / Cursor
- **HTTP API** (axum): same operations for non-MCP clients

Single Rust binary. Cross-compiled for Windows (`x86_64-pc-windows-msvc`) and macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`). No Python sidecars. **Tesseract + Leptonica are statically linked** via the `kreuzberg-tesseract` feature (no system install required). Only `fra.traineddata` (~10 MB) is fetched: bundled in the installer at `~/.anno-rag/tessdata/fra.traineddata`, or auto-downloaded on first OCR call with operator consent.

## 4. Architecture

### 4.1 Topology — Strategy A (confirmed)

The anno workspace owns it. **Seven library crates + three binary crates** under `crates/anno-rag-*` (full decomposition in §5). `vendor/cloakpipe/` is a path-dep workspace member (upstream `rohansx/cloakpipe`, Apache-2.0). Hacienda code is **not** depended on as a Cargo dep — specific source files are surgically lifted into the appropriate `anno-rag-*/src/` location with `// Adapted from hacienda — MIT OR Apache-2.0` headers and the original LICENSE preserved at `packs/THIRD_PARTY/hacienda/`.

```
anno/
├── crates/
│   ├── anno/                       # existing
│   ├── anno-cli/                   # existing
│   ├── anno-eval/                  # existing
│   └── anno-rag/                   # NEW
│       ├── src/
│       ├── packs/                  # repo ships general-fr + legal-fr at v1
│       ├── policies/
│       └── THIRD_PARTY/hacienda/   # attribution + LICENSE
└── vendor/
    └── cloakpipe/                  # path-dep, fork of rohansx/cloakpipe
```

### 4.2 Data flow — ingest

```
folder/file.{pdf,docx,pptx,xlsx,eml,html,md,jpg,png,odt,rtf,epub,...}
   │
   ▼  kreuzberg::extract_file with ExtractionConfig {
          include_document_structure: true,
          chunking: ChunkingConfig { chunker_type: MarkdownAware,
                                     max_chars: 2048, max_overlap: 256 },
          ocr: OcrConfig { backend: Tesseract, language: "fra" },
          pages: PageConfig { extract_pages: true, .. },
          ..
      }
ExtractionResult {
    content: String (full markdown),
    document: DocumentStructure (Heading/Section/Paragraph/Table/Citation tree),
    chunks: Vec<Chunk> (section-aware via markdown chunker),
    pages: Vec<PageContent> (byte-accurate offsets),
    tables: Vec<Table>,
    elements: Vec<Element> (17 RT-DETR types if layout feature enabled)
}
   │
   ▼  ingest::structure::type_nodes  (NEW — thin typing layer)
   │       maps kreuzberg's generic Heading/Section nodes to
   │       legal-fr-typed nodes: Article{number, alinéa}, Considérant,
   │       Préambule, Dispositif, etc. using pack-supplied regex rules.
   │       NOT a parser — kreuzberg already detected the boundaries.
typed_chunks[] with StructuralPath metadata
   │
   ▼  contextual_prefix::apply  (NEW — Anthropic Contextual Retrieval pattern)
   │       prepend `Title: {doc.title}\nSection: {heading_path}\n
   │                Valid: {valid_from}..{valid_to}\n\n` to each chunk
   │       before embedding (chunk-as-stored stays raw — prefix is
   │       for the embedder only).
chunks_for_embedding[]
   │
   ▼  detect::cloakpipe (PatternDetector + FinancialDetector + CustomConfig.patterns)
   ▼  detect::anno      (gliner2_fastino_candle + active LoRA from pack)
   ▼  merge + dedup spans (helper from cloakpipe detector/mod.rs:160)
entities[] (NIR, SIRET, IBAN-FR, PER, ORG, ECLI, code refs, …)
   │
   ▼  citation::extract  (NEW — ECLI, Cass./CE/CA, code articles, Légifrance IDs)
   │      regex donors: BO-ECLI Parser (JURIX 2017), pyJudilibre
citations[]
   │
   ▼  pseudonymize::wrapper  (cloakpipe Replacer + SqliteVault, AES-256-GCM)
   │      key derived via Argon2id from OS-keyring secret, 32-byte raw
pseudonymized text  +  entity↔token mapping persisted in vault.db
   │
   ▼  summary::generate  (NEW — for docs > N pages, generate doc-level summary
                          via Cowork-supplied LLM, cache via Anthropic prompt cache)
   │       summary is itself pseudonymized + indexed in `summaries` table
   │       for section-level retrieval (Summary-Augmented Chunking)
   │
   ▼  embed::candle  (BAAI/bge-m3 — multi-vector dense + sparse + 8k ctx)
chunk_embeddings[]  (dense f32×1024 + optional sparse for hybrid)
   │
   ▼  store::lance  (LIFTED from hacienda-engine — schema extended:
                     +structural_path, +citation_ids, +contextual_prefix_hash,
                     +chunk_hash, +privilege, +summary_id (for SAC),
                     +valid_from/to, +as_of_law_date)
   │
   ▼  lance.merge_insert(["doc_id","chunk_idx"])  ← idempotent upsert
   ▼  optionally: lance.tag(version, "ingest-YYYY-MM-DD-HH-MM")
LanceDB tables: chunks, summaries, documents, citations, subjects, audit
   │
   ▼  outputs/{file}.anon.{md|txt}   — pseudonymized copy on disk
       (option C requirement: anonymized copies + searchable index)
```

### 4.3 Data flow — query (via MCP from Cowork)

```
Cowork → MCP tool: search(query, top_k, filters, as_of?, expand_citations?)
   │
   ▼  query_router  (NEW — conditional HyDE, v2)
   │      detect ECLI/article/code-cite in query → skip rewriting
   │      vague natural-language → apply HyDE prompt via Cowork LLM
   │
   ▼  pseudonymize query (cloakpipe Replacer, session-scoped)
   │
   ▼  embed query  (candle, bge-m3, multi-functional output)
   │
   ▼  Stage 1 — section-level retrieval  (Summary-Augmented Chunking)
   │      lance search on `summaries` table → top-N candidate docs/sections
   │
   ▼  Stage 2 — chunk-level retrieval within candidate sections
   │      tbl.query()
   │         .full_text_search(FullTextSearchQuery::new(q))   ← native BM25
   │         .nearest_to(q_vec)                                ← vector
   │         .limit(50)
   │         + RRFReranker (lancedb::rerankers, built-in)      ← native RRF k=60
   │
   ▼  privilege::gate (pass 1)  — drop chunks where privilege != none
   │
   ▼  store::ranking::authority_boost  (LIFTED from hacienda —
   │                                    conclusions+0.30, operative+0.20)
   │
   ▼  store::ranking::cross_encoder_rerank
   │      default model: camembert-L6-mmarcoFR (~140 MB)
   │      optional pair-with-bge-m3: bge-reranker-v2-m3 (~570 MB)
   │      top-50 → top-K
   │
   ▼  expand_with_citations  (optional — graph hop along citation edges)
   ▼  privilege::gate (pass 2)  — drop any privileged neighbours surfaced
   │
   ▼  return {
        chunks[]: { content, score, structural_path, citations[],
                    chunk_hash, source { doc_id, page, char_start, char_end } },
        verifiable: true   ← caller can re-hash content vs chunk_hash
      }
```

Notes:
- **Hybrid + RRF are both native in LanceDB ≥ 0.27** — we use `lancedb::rerankers::RRFReranker` rather than rolling our own.
- **Cross-encoder rerank is in-process** (ONNX/candle) — LanceDB has no native cross-encoder.
- **Privilege gating runs twice**: pass 1 saves rerank compute; pass 2 catches privileged neighbours surfaced by citation graph expansion.
- The chunks Cowork receives are pseudonymized. Cowork sends them to Claude. Claude generates a response over pseudonyms. To rehydrate, the user calls the `rehydrate` MCP tool — locally on the box. Cleartext never leaves the host.
- For citation verification (avoid hallucinated citations in generated output), Cowork can call `verify_citations(text)` post-generation — see §9.

## 5. Crate layout — decomposed into 7 library crates + 3 binaries

Single mega-crate has been rejected. anno's existing pattern (`anno`/`anno-cli`/`anno-eval`) is mirrored.

```
anno/
├── crates/
│   ├── anno-rag-core/           # types, traits, errors, IDs. Zero IO. ~500 LOC.
│   │   └── src/
│   │       ├── lib.rs           # public API surface (traits + types)
│   │       ├── traits.rs        # Ingest, Detect, Embed, Rerank, Store,
│   │       │                    #   Audit, PrivilegeGate — see §5.1
│   │       ├── ids.rs           # newtype DocId/ChunkId/SubjectId/CitationId
│   │       ├── error.rs         # thiserror enum — see §5.2
│   │       ├── config.rs        # AnnoRagConfig (TOML, serde)
│   │       ├── entity.rs        # Entity, EntityCategory, DetectionSource
│   │       │                    #   (mirror of cloakpipe types for stability)
│   │       └── structural.rs    # StructuralPath, Role, LegalNodeType
│   │
│   ├── anno-rag-ingest/         # kreuzberg wrapper + structure typing + summary
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── kreuzberg.rs     # facade over kreuzberg::extract_file —
│   │       │                    #   isolates API churn (4.x weekly bumps).
│   │       │                    #   Sets include_document_structure: true,
│   │       │                    #   chunker_type: MarkdownAware
│   │       ├── structure.rs     # SLIM typing layer over kreuzberg's
│   │       │                    #   DocumentNode tree (NOT a parser)
│   │       ├── prefix.rs        # contextual prefix per chunk
│   │       │                    #   (Anthropic Contextual Retrieval pattern)
│   │       └── summary.rs       # doc-level summary for SAC
│   │
│   ├── anno-rag-detect/         # cloakpipe + anno wrapper pipeline
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pipeline.rs      # AnnoCloakPipeline composing both detectors
│   │       ├── anno_adapter.rs  # anno → cloakpipe DetectedEntity bridge
│   │       ├── patterns.rs      # FR regex pack → Vec<CustomPattern>
│   │       └── vault.rs         # SqliteVault wrapper + Argon2id KDF
│   │   # Deps: cloakpipe-core, anno, regex, argon2, keyring — NO lancedb/candle
│   │
│   ├── anno-rag-embed/          # candle-based embedder + cross-encoder rerank
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── bge_m3.rs        # BAAI/bge-m3 dense embedder
│   │       ├── e5_small.rs      # intfloat/multilingual-e5-small (alt)
│   │       └── camembert_ce.rs  # cross-encoder rerank (camembert-L6/L8 + gemma2)
│   │   # SEPARATED from -store: candle is heavy (~90s cold compile);
│   │   # -store depends on the Embed/Rerank traits, not candle directly
│   #
│   ├── anno-rag-store/          # lancedb + ranking + citation graph
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── lance.rs         # schema (chunks, summaries, documents,
│   │       │                    #   citations, subjects), open, merge_insert
│   │       ├── search.rs        # native hybrid: FTS + vector + RRFReranker
│   │       ├── ranking.rs       # authority boost + cross-encoder rerank
│   │       │                    #   (calls Rerank trait from -core)
│   │       ├── graph_rag.rs     # entity-centric + citation graph
│   │       ├── versioning.rs    # thin layer over lance.tag()/checkout
│   │       ├── index.rs         # IVF_HNSW_SQ build, background task
│   │       ├── citation/
│   │       │   ├── mod.rs
│   │       │   ├── ecli.rs      # ECLI parser (BO-ECLI Parser JURIX 2017)
│   │       │   ├── french.rs    # Cass./CE/CA refs (pyJudilibre donor)
│   │       │   └── verify.rs    # verify_citations(text) for MCP
│   │       ├── privilege.rs     # privilege classification + gating
│   │       └── forget.rs        # Art. 17 erasure routine (delete + Prune +
│   │                            #   Compact + drop tags + receipt)
│   │   # Deps: anno-rag-core, lancedb, arrow, sha2 — NO candle, kreuzberg
│   │
│   ├── anno-rag-audit/          # hash-chained jsonl + Art. 30 register
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── chain.rs         # sha256-chained JSONL writer
│   │       ├── sig.rs           # daily HMAC signature
│   │       ├── register.rs      # Art. 30 CSV/PDF exporter
│   │       └── subject.rs       # Art. 15 find / Art. 20 export
│   │   # Deps: anno-rag-core, sha2, hmac, rusqlite — light
│   │
│   ├── anno-rag-eval/           # LegalBench-RAG / BSARD harness
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── dataset.rs       # loader for LegalBench-RAG, BSARD formats
│   │       ├── metrics.rs       # P@k, R@k, char-span precision, hallucination rate
│   │       └── runner.rs        # async pipeline runner with metrics output
│   │   # Off by default (--features eval). Deps: anno-rag-store + benchmarks
│   │
│   ├── anno-rag-cli/            # bin: anno-rag
│   │   └── src/main.rs          # clap subcommands; depends on all libs
│   ├── anno-rag-mcp/            # bin: anno-rag-mcp
│   │   └── src/main.rs          # rmcp stdio server — depends on -core/-detect/-store/-audit
│   │                            #   NO clap (config via env + TOML)
│   └── anno-rag-api/            # bin: anno-rag-serve
│       └── src/main.rs          # axum HTTP — depends on -core/-detect/-store/-audit
│                                #   NO rmcp
│
├── vendor/cloakpipe/            # path-dep workspace member (Apache-2.0)
└── packs/
    ├── general-fr-v1/
    │   ├── pack.toml
    │   ├── patterns.toml        # NIR, SIRET, SIREN, IBAN-FR, RIB,
    │   │                        # FR phone (+33), plaque immatric.,
    │   │                        # n° TVA intracom., CNI, passeport-FR
    │   ├── entity_types.toml
    │   ├── policy.toml          # retention=5y default, audit=full
    │   └── README.md
    ├── legal-fr-v1/
    │   ├── pack.toml
    │   ├── patterns.toml        # extends general-fr + numéro RG, ECLI,
    │   │                        # juridictions, magistrate roles,
    │   │                        # code article refs, n° pourvoi
    │   ├── entity_types.toml
    │   ├── citation_rules.toml  # ECLI + Cass./CE/CA regex, code abbreviations
    │   ├── policy.toml          # retention=10y, privilege rules, audit=verbose
    │   └── README.md
    └── THIRD_PARTY/hacienda/    # attribution + LICENSE + PROVENANCE.md
```

**Dependency graph (no cycles, narrow seams):**

```
anno-rag-cli  ──→  anno-rag-ingest ──┐
                    anno-rag-detect ──┤
                    anno-rag-embed ───┤
                    anno-rag-store ───┼──→  anno-rag-core
                    anno-rag-audit ───┤
                    anno-rag-eval  ──┘
anno-rag-mcp  ──→  -detect, -store, -audit  ──→  anno-rag-core
anno-rag-api  ──→  -detect, -store, -audit  ──→  anno-rag-core
```

`anno-rag-store` depends on `anno-rag-embed` **only through the `Embed` / `Rerank` traits** declared in `anno-rag-core` — this lets `-store` compile without pulling in candle (~90s cold build saved on every store-only edit).

### 5.1 Traits at the seams (anno-rag-core)

All major substitution points are traits, declared in `anno-rag-core::traits`:

```rust
#[async_trait::async_trait]
pub trait Ingest: Send + Sync {
    async fn extract(&self, src: &Path) -> Result<ExtractedDoc>;
}

#[async_trait::async_trait]
pub trait Detect: Send + Sync {
    async fn detect(&self, text: &str) -> Result<Vec<Entity>>;
}

#[async_trait::async_trait]
pub trait Embed: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vector>>;
    fn dim(&self) -> usize;
    fn model_id(&self) -> &str;
}

#[async_trait::async_trait]
pub trait Rerank: Send + Sync {
    async fn rerank(&self, query: &str, candidates: &[Chunk]) -> Result<Vec<Scored>>;
    fn model_id(&self) -> &str;
}

#[async_trait::async_trait]
pub trait Store: Send + Sync {
    async fn upsert(&self, chunks: &[Chunk]) -> Result<()>;
    async fn search(&self, q: &SearchQuery) -> Result<Vec<Scored>>;
    async fn delete_subject(&self, subject: &SubjectId) -> Result<DeleteReceipt>;
    async fn list_versions(&self) -> Result<Vec<Version>>;
}

pub trait Audit: Send + Sync {
    fn log(&self, event: AuditEvent);
    fn verify_chain(&self) -> Result<ChainStatus>;
}

pub trait PrivilegeGate: Send + Sync {
    fn allow(&self, chunk: &Chunk, requester: &Requester) -> bool;
}
```

Where Rust 1.75+ native async-in-trait is enough (no dyn dispatch needed), we drop `#[async_trait]`. We keep it for trait objects (e.g. `Box<dyn Embed>` for runtime swap by config).

### 5.2 Error model (anno-rag-core::error)

```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
    // -- recoverable: log + skip --
    #[error("ingest failed for {path}: {source}")]
    Ingest { path: PathBuf, source: IngestError },

    #[error("detection failed: {0}")] Detect(#[from] DetectError),

    // -- fatal: refuse to start / refuse to continue --
    #[error("vault corruption: {0}")]                    VaultCorrupted(String),
    #[error("audit chain corruption at line {0}")]       AuditCorrupted(u64),
    #[error("privilege violation: {kind}")]              PrivilegeDenied { kind: &'static str },
    #[error("schema migration required ({from} → {to})")]
                                                          SchemaMismatch { from: u32, to: u32 },

    // -- pass-through --
    #[error(transparent)] Io(#[from] std::io::Error),
    #[error(transparent)] Lance(#[from] lancedb::Error),
    #[error(transparent)] Config(#[from] config::ConfigError),
}

pub type Result<T> = std::result::Result<T, Error>;
```

**Fatal vs recoverable:** any variant under "fatal" causes the binary to exit with non-zero code AFTER writing a final audit-log entry. Recoverable errors are tracked per-doc in the ingest run report and don't halt the batch. Binary entry points use `anyhow::Context` to add backtrace context; lib code stays on the typed enum.

### 5.3 Newtype IDs (anno-rag-core::ids)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DocId(pub Uuid);     // UUID v7 (time-sortable)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ChunkId(pub Uuid);   // UUID v5 (deterministic = hash(doc_id, chunk_idx))

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SubjectId(pub String);    // "PER_42" pseudonym ref

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CitationId(pub String);   // "ECLI:FR:CCASS:..."
```

Function signatures take the typed ID, not raw `Uuid` or `String`. Prevents `find_subject(doc_id)` confusion at compile time.

### 5.4 Build performance discipline

**Verified workspace dependency block (audited May 2026):**

```toml
# anno/Cargo.toml  (workspace root)
[workspace.package]
edition       = "2024"
rust-version  = "1.81"
license       = "MIT OR Apache-2.0"

[workspace.dependencies]
# ----- core IO + types -----
serde              = { version = "1",       features = ["derive"] }
serde_json         = "1"
toml               = "0.8"
thiserror          = "2"
anyhow             = "1"
tracing            = "0.1"
tracing-subscriber = { version = "0.3",     features = ["env-filter", "json"] }
uuid               = { version = "1",       features = ["v5", "v7", "serde"] }
chrono             = { version = "0.4",     features = ["serde"] }

# ----- async runtime: tokio LTS line, narrow features -----
tokio              = { version = "1.51",    default-features = false, features = [
                       "rt-multi-thread", "macros", "sync", "io-util", "fs", "time", "signal"
                     ] }
async-trait        = "0.1"
futures            = "0.3"

# ----- ingestion -----
kreuzberg          = { version = "=4.9.7",  default-features = false, features = [
                       "build-tesseract", "download-tessdata", "tokio-runtime"
                     ] }

# ----- detection (PII / NER) -----
# cloakpipe-core via path-dep, no version
# anno via path-dep
regex              = "1"
argon2             = "0.5"
keyring            = "3"

# ----- vector store -----
lancedb            = "=0.27.2"
arrow              = { version = "=58",     default-features = false, features = ["csv"] }
arrow-array        = { version = "=58",     default-features = false }
arrow-schema       = { version = "=58",     default-features = false }
arrow-select       = { version = "=58",     default-features = false }

# ----- ML inference (in anno-rag-embed only — separated for build speed) -----
candle-core         = "=0.10"
candle-nn           = "=0.10"
candle-transformers = "=0.10"
ort                 = { version = "=2.0.0-rc.12", default-features = false, features = ["load-dynamic"] }
# load-dynamic skips the ~3 min ORT C++ build in dev; system ORT runtime expected

# ----- MCP + HTTP surfaces -----
rmcp               = { version = "1.6",     features = ["server", "transport-io", "macros"] }
schemars           = "0.8"
axum               = "0.8"
tower              = "0.5"
tower-http         = { version = "0.6",     features = ["cors", "trace"] }
hyper              = { version = "1",       features = ["full"] }

# ----- audit + crypto -----
sha2               = "0.10"
hmac               = "0.12"
rusqlite           = { version = "0.32",    features = ["bundled"] }

# ----- process safety + lockfile -----
fd-lock            = "4"   # replaces deprecated fs2 — cross-platform RW lock

# ----- CLI binary -----
clap               = { version = "4",       features = ["derive", "env"] }

# ----- testing (dev-deps) -----
tempfile           = "3"
insta              = { version = "1",       features = ["yaml", "json"] }
proptest           = "1"
```

**Compiler profile (`anno/Cargo.toml` workspace root):**

```toml
[profile.dev]
opt-level         = 0
debug             = "line-tables-only"   # 2× faster link, still usable backtraces
incremental       = true
codegen-units     = 256
split-debuginfo   = "unpacked"           # macOS/Linux: faster link

[profile.dev.package."*"]                # dependencies built optimized → run faster in dev
opt-level         = 1
debug             = false

[profile.dev.build-override]
opt-level         = 0

[profile.release]
lto               = "thin"
codegen-units     = 16
debug             = "line-tables-only"
strip             = "symbols"

[profile.release-fast]                   # CI / nightly profile
inherits          = "release"
lto               = false
codegen-units     = 256
```

**Build environment (`anno/.cargo/config.toml`):**

```toml
[build]
rustc-wrapper = "sccache"                # cross-branch artifact cache

[unstable]
codegen-backend  = true

[profile.dev]
codegen-backend  = "cranelift"           # 30% faster debug codegen (stable on Linux x86_64)

[target.x86_64-unknown-linux-gnu]
linker     = "clang"
rustflags  = ["-C", "link-arg=-fuse-ld=mold"]

[target.aarch64-apple-darwin]
# mold is Linux-only OSS; on macOS use lld (brew install llvm) or default ld_prime
rustflags  = ["-C", "link-arg=-fuse-ld=lld"]

[target.x86_64-pc-windows-msvc]
linker     = "rust-lld.exe"              # bundled with Rust 1.74+
rustflags  = ["-C", "link-arg=/STACK:8000000"]
```

**Dev workflow tooling:**

- `cargo install bacon` — `cargo-watch` is officially on life support, `bacon` is the maintained successor
- `cargo install cargo-nextest` — 3-10× faster than `cargo test` on async suites
- `cargo install sccache` — cross-machine artifact cache
- `cargo install cargo-deny` — license + advisory checks
- macOS users: `brew install llvm` for `lld`; mold OSS is Linux-only (commercial `sold` covers Darwin)

**Build-time targets** (8-core machine, warm cache except where noted):

| Action | Target | Comment |
|---|---|---|
| `cargo check -p anno-rag-detect` | < 2s | the inner dev loop |
| `cargo check --workspace` | < 5s | full workspace typecheck |
| `cargo build -p anno-rag-detect` | < 8s | per-crate incremental |
| `cargo build --workspace` (incremental) | < 30s | after pull |
| `cargo build --workspace` (cold) | < 3 min | empty target/ |
| `cargo nextest run -p anno-rag-detect` | < 10s | single crate |
| `cargo nextest run --workspace` | < 60s | full suite |
| `cargo build --release --workspace` | < 8 min | shipping build |

CI enforces these as soft gates: `cargo build --timings` runs in a dedicated job; regression > 25% from baseline fails the build.

### 5.5 Workspace conventions

```toml
# anno/Cargo.toml — workspace-wide lints
[workspace.lints.rust]
unsafe_code        = "forbid"          # we have zero need
unused_must_use    = "deny"
missing_docs       = "warn"            # pub items must have ///

[workspace.lints.clippy]
pedantic           = { level = "warn", priority = -1 }
unwrap_used        = "deny"            # forces Result everywhere in lib code
expect_used        = "deny"
panic              = "deny"            # except main / tests via #[allow]
missing_errors_doc = "warn"
todo               = "warn"
```

**MSRV:** Rust **1.81** (enables native async-in-trait everywhere we don't need dyn dispatch; tokio 1.51 LTS requires ≥ 1.71; candle 0.10 and arrow 58 comfortable from 1.78).

**Edition:** **2024** — all listed deps compile cleanly under it.

**rust-toolchain.toml** pins the channel so contributors don't accidentally use older compilers:

```toml
[toolchain]
channel    = "1.81"
components = ["rustfmt", "clippy", "rust-analyzer"]
```

**`cargo deny` enforcement** in CI: catches GPL leak, flags Elastic-2.0 (kreuzberg) for awareness, blocks RUSTSEC advisories.

**Visibility:** `pub(crate)` by default; `pub` only at workspace API boundary (re-exports from `anno-rag-core::lib`).

## 6. Legal-RAG additions (detail)

Seven additions (§§6.1–6.7). The first five were specified up-front; §6.6 (Contextual Prefixes + Summary-Augmented Chunking) and §6.7 (Quote-grammar verifier) were added after the research-integration pass when the Legal-RAG SOTA agent flagged them as high-impact / low-cost.


### 6.1 Hierarchical sectioning (`ingest/structure.rs`) — kreuzberg-driven

**Problem:** A chunk that splits mid-article ("Article 1240. — Tout fait quelconque de l'homme,..." cut after "homme") returns half the rule and breaks retrieval precision.

**Solution — much simpler than v1:** Kreuzberg already does the hard work. With `include_document_structure: true` and `chunker_type: MarkdownAware`, `ExtractionResult` returns:
- `document: DocumentStructure` — hierarchical tree with `NodeContent::{Heading{level}, Section, Paragraph, Table, Citation, Footnote, DefinitionList, ...}` and `ContentLayer::{Body, Header, Footer, Footnote}` (so running headers/page numbers are auto-segregated)
- `chunks: Vec<Chunk>` — already section-aware via the markdown-aware chunker
- For PDFs: k-means heading clustering on font/size/position (`pdf_options.hierarchy`)

`ingest/structure.rs` is now a **thin typing layer**, not a parser:

```rust
// Given a kreuzberg DocumentNode tree, type each Heading/Section node
// using regex rules supplied by the active pack. Default rules in legal-fr:
fn type_node(node: &DocumentNode, rules: &PackRules) -> Option<LegalNodeType> {
    match &node.node_content {
        NodeContent::Heading { level, text, .. } => {
            for rule in &rules.legal_node_typing {
                if let Some(m) = rule.regex.captures(text) {
                    return Some(LegalNodeType::from_match(rule.kind, m));
                }
            }
            None
        }
        _ => None,
    }
}
```

**Pack rules** (in `legal-fr/patterns.toml`, new section `[[legal_node_typing]]`):
- `kind = "Livre"`, `regex = "^LIVRE\\s+([IVXLC]+)$"`
- `kind = "Titre"`, `regex = "^TITRE\\s+([IVXLC]+)(?:\\s+BIS)?$"`
- `kind = "Article"`, `regex = "^Article\\s+(?:L\\.|R\\.|D\\.)?\\s*(\\d+(?:-\\d+)?)(?:\\s+BIS|\\s+TER)?$"`
- `kind = "Considérant"`, `regex = "^Considérant\\s+(?:que)?$"`
- `kind = "Dispositif"`, `regex = "^(?:DISPOSITIF|PAR\\s+CES\\s+MOTIFS)$"`

**Chunking rule (also handled by kreuzberg with markdown chunker):** Article boundaries are sacred. We set `ChunkingConfig { max_chars: 2048, max_overlap: 256 }` (kreuzberg uses character not token counts; ~512 tokens ≈ 2048 chars for FR). When an article exceeds 2048 chars, kreuzberg's markdown-aware chunker recurses on alinéa boundaries with the parent heading preserved in the chunk metadata.

**Output schema addition:**
```rust
struct StructuralPath {
    code: Option<String>,         // "civil", "commerce", "travail", ...
    livre: Option<String>,
    titre: Option<String>,
    chapitre: Option<String>,
    section: Option<String>,
    article: Option<String>,      // "1240", "L.132-1", "R.433-3"
    alinea: Option<u32>,
    role: Role,                   // Heading | Préambule | Dispositif | Annexe | …
    content_layer: ContentLayer,  // Body | Header | Footer | Footnote (from kreuzberg)
}
```

**Net effect:** weeks of bespoke French legal regex parsing saved. We add ~200 lines of typing rules, not a parser.

### 6.2 Citation graph (`citation/`)

**Problem:** Legal RAG without citation following can't answer "show me art. 1240 and what it cross-references" or "find this case + everything that cites it."

**Solution:** Extract citations during ingest, store them as edges in a LanceDB table, expose graph-hop expansion in search.

**Extractors** (regex donors: [BO-ECLI Parser](https://ceur-ws.org/Vol-2143/paper4.pdf) JURIX 2017, [pyJudilibre](https://pypi.org/project/pyjudilibre/)):
- **ECLI** — `ECLI:[A-Z]{2}:[A-Z0-9.]+:\d{4}:[A-Z0-9.]+`
- **Cass.** — `Cass\.\s*(?:civ\.|com\.|crim\.|soc\.)\s*\d+,\s*\d{1,2}\s+\w+\s+\d{4},\s*n°\s*\d{2}-\d{5}`
- **CE** — `CE,\s*(?:sect\.|ass\.|réf\.)?\s*\d{1,2}\s+\w+\s+\d{4},\s*n°\s*\d{6}`
- **CA** — `CA\s+\w+,\s*\d{1,2}\s+\w+\s+\d{4},\s*n°\s*\d{2}/\d{5}`
- **Code articles** — `(?:art\.|articles?)\s+(?:L\.|R\.|D\.)?\s*\d+(?:-\d+)?(?:\s+(?:du|de la|de l'))?\s+(?:Code\s+\w+|C\.\s*(?:civ|com|trav|pén|consom))`
- **Légifrance** — `LEGI[A-Z]{4}\d{12}` and `JURI[A-Z]{4}\d{12}`

Edges are **typed**: `cites | cites-overruled | cites-distinguished | cites-applied` (heuristic detection from surrounding text: "voir aussi", "contra", "comp."). Bidirectional, so we can answer both "what does this cite" and "what cites this".

**Storage:** LanceDB table `citations { src_doc_id, src_chunk_id, target_kind, target_ref, target_doc_id (if resolved locally), confidence }`. Bidirectional edges built at the end of ingest in a "resolution" pass that links targets to docs already in the index.

**Search expansion:** MCP tool `search` accepts `expand_citations: int` (0 by default; 1 = one hop). When >0, retrieved chunks pull in their citation neighbours and merge into results (with a decay factor to keep originals ranked higher).

### 6.3 Cross-encoder rerank (`store/ranking.rs::cross_encoder_rerank`)

**Problem:** Hybrid BM25+vector returns 50 candidates that are "related" but not necessarily "answer the query." For legal precision this matters.

**Solution:** Stage-2 rerank with **`antoinelouis/crossencoder-camembert-L6-mmarcoFR`** (68M params, ~140 MB, French-monolingual, ONNX-exported via `scripts/export_camembert_ce.py` following the existing anno export pattern).

**Pipeline:**
1. Hybrid search via LanceDB native `RRFReranker` k=60 → top-50 candidates with fused score
2. Privilege gate pass 1 (drop privileged before paying rerank cost)
3. Authority boost (LIFTED from hacienda) applied
4. Cross-encoder scores each (query, chunk) pair → reranker score in [0,1]
5. Final score = `0.7 * rerank_score + 0.3 * hybrid_score_normalized`
6. Return top-K (default K=10)

**Config:**
```toml
[ranking.cross_encoder]
enabled = true
model = "antoinelouis/crossencoder-camembert-L6-mmarcoFR"  # overridable
candidate_pool = 50
final_k = 10
weight_rerank = 0.7
weight_hybrid = 0.3
```

**Alternatives exposed for tuning (in `config.toml`):**
- Smaller: `crossencoder-camemberta-L4-mmarcoFR` (~115 MB)
- Higher-quality (still FR-monolingual, L8 layers updated 2025-04): `antoinelouis/crossencoder-camembert-L8-mmarcoFR` (~170 MB)
- Largest French monolingual: `crossencoder-camembert-base-mmarcoFR` (~440 MB)
- **Matched-pair with `bge-m3` embedder**: `BAAI/bge-reranker-v2-m3` (~570 MB). Recommended by BAAI when paired with bge-m3 — multilingual, may outperform camembert-L6 on FR/EN-mixed corpora. Trade-off: 4× the disk + RAM. Default off to honor "small footprint".
- **GPU-class option**: `BAAI/bge-reranker-v2.5-gemma2-lightweight` (~2.5 GB, Gemma2-based, Jul 2024). Reserve for power users with CUDA/Metal opt-in features enabled; not for CPU-default deployments.

**Caveat documented in user guide:** the mMARCO-FR training data is conversational Q→A, not legal. Quality is good but not optimal. Fine-tuning on French legal pairs is a v1.x improvement (see [Antoine Louis cross-encoder collection](https://huggingface.co/collections/antoinelouis/cross-encoder-rerankers) for current state).

### 6.4 Temporal versioning (`versioning.rs`) — LanceDB-native

**Problem:** Laws change. A query today should not return last year's version of an article unless the user explicitly asks.

**Solution:** LanceDB ≥ 0.27 has native **time-travel** (`list_versions`, `checkout`, `restore`, `tags`). We use it directly — **no custom `supersessions` table.**

**Two date concepts kept distinct:**
- **Index version** (`lance.tag()`) — when our index was updated; LanceDB MVCC handles this. We tag every ingest run: `tbl.tags().create(&format!("ingest-{}", iso8601), version)`.
- **Legal effectiveness** (`as_of_law_date: Option<Date>` column) — what the *source document* declares as its law version (e.g. "version au 1er janvier 2024"). Extracted during ingest from doc metadata or front-matter heuristic.

**Schema additions on the `chunks` table:**
- `valid_from: Date` — same as ingest date for the chunk; used for forensic "what did we have on day X"
- `valid_to: Option<Date>` — when superseded by a re-ingest of the same source; `None` = currently canonical
- `as_of_law_date: Option<Date>` — the legal effective date declared by the source

**Default query semantics:** `valid_to IS NULL OR valid_to > now()` plus, if user supplied `--as-of <date>`, also `as_of_law_date IS NULL OR as_of_law_date <= <date>`. The CLI flag scopes by `as_of_law_date` (legal effectiveness), not LanceDB time-travel — the user typically wants "what was the law on date X", not "what was in our index on date X". Both are exposed for forensic queries (CLI: `--at-version <tag>` for index time-travel, `--as-of <date>` for law-date).

**Re-ingest flow:** when a doc is re-ingested with the same `source_path`, the old chunks get `valid_to = now()` set; new chunks get a new `valid_from`. The vault is unchanged (tokens remain valid across versions). A new LanceDB tag is created.

**GDPR friction:** LanceDB-tagged versions resist pruning. `forget(subject)` must `tbl.tags().delete(...)` for affected versions before running `optimize(Prune)` — otherwise the tombstoned data lives forever in tagged versions. This is wired in `forget.rs` and documented.

**Audit log captures the effective query date** so an Art. 30 inspection can reconstruct "what did the search return on that date."

### 6.5 Verbatim fidelity + privilege gating (`privilege.rs`)

**Two distinct problems, one module:**

**(a) Verbatim:** Legal text demands byte-exact retrieval. The chunk a user sees must be byte-identical to what was stored at ingest time.

- Every chunk has `chunk_hash: [u8; 32]` — SHA-256 of the **stored (pseudonymized) chunk content**. The hash is computed once at ingest and stored alongside the chunk; on every read the content is re-hashed and compared.
- Cleartext provenance (line offsets in the *original* source) is stored separately so users can verify against the source document if they have access. The cleartext itself is never re-derived from the index — only from the vault + source document.
- Retrieval includes `chunk_hash` in the response so downstream callers can re-verify independently.
- Hash mismatch on read = corruption alert, audit-logged, search fails closed.
- No summarization, no truncation, no reformatting — chunks are immutable post-ingest.
- Source provenance: `{doc_id, source_path, page, char_start, char_end}` returned with every result.

**(b) Privilege gating:** Lawyers handle privileged communications (avocat-client correspondence) that must NOT leak to a remote LLM under any circumstance.

- During ingest, the active pack can classify a chunk as `privilege: avocat_client | judicial | none`
  - `legal-fr` pack rule: any doc whose path matches `**/correspondance/**` or `**/avocat-client/**` defaults to `avocat_client`
  - Operator can override per-doc via sidecar file `<doc>.privilege.toml`
- **MCP and HTTP API never return chunks where `privilege != none`** — unconditionally. The boundary is enforced in `privilege::gate()` between retrieval and response.
- CLI accepts `--include-privileged` for local review. Every use is audit-logged with `op = "privileged_access"`, actor, justification (free-text required if interactive).
- Privileged chunks remain searchable locally but are excluded from any output crossing the host.

### 6.6 Contextual prefixes + Summary-Augmented Chunking (NEW)

Added based on legal-RAG research (Anthropic Contextual Retrieval, arxiv 2510.06999 SAC, Isaacus Legal RAG Bench).

**(a) Contextual prefix per chunk** (`ingest/prefix.rs`):

Before embedding, prepend a small header to each chunk that gives the embedder the doc/section context the chunk alone lacks:

```
Title: Cass. civ. 1, 12 mars 2024, n° 21-12345
Section: Considérants > Sur le moyen unique
Valid: 2024-03-12..present

{raw_chunk_text}
```

**The prefix is for the embedder only — the chunk-as-stored (and as-returned) is the raw text.** Provenance + chunk_hash apply to the raw form. Anthropic reports −49% retrieval-failure with this technique combined with BM25; cost is one extra prompt-cached LLM call per doc for synthesizing the context (or pure-rule, using kreuzberg's structural tree).

**(b) Summary-Augmented Chunking + two-level index** (`ingest/summary.rs`, `store/lance.rs::summaries` table):

For long documents (>15 pages — typical 50-page Cass. arrêts and code excerpts), single-pass chunking + embedding is brittle. We add a second index level:

- At ingest, generate a **doc-level synthetic summary** (200 tokens, Anthropic prompt-cached) and a per-section summary
- Store summaries in a separate `summaries` LanceDB table with same embedder
- At query time, **Stage 1** retrieves at section-summary level (top-N=5 sections), **Stage 2** retrieves at chunk level within those candidate sections

This costs ~one cached LLM call per doc at ingest; gain on long-doc precision is well-documented in arxiv 2510.06999.

The "synthetic summary" generation is **the only place we leave the box at ingest time, by design** — and it is pseudonymized before sending. We document this clearly in the threat model: an external LLM (Cowork's configured model) sees pseudonyms of the doc; no cleartext PII crosses.

### 6.7 Quote-grammar verifier (NEW)

Added based on hallucination findings (Magesh et al. 2025 — 17-33% hallucination rate in Lexis/Westlaw legal RAG; Charlotin's 508-case tracker).

We expose a `verify_citations(text)` MCP tool that Cowork calls **after** Claude generates a response. It:

1. Regex-extracts every citation pattern (ECLI, Cass./CE/CA, code articles, internal refs `[doc_id:char_start-char_end]`)
2. Looks each up against the citations table and chunks table
3. Returns `{citation: str, status: Valid | NotFound | OffsetMismatch, evidence: Option<...>}` for each
4. Cowork's prompt template instructs Claude to use `[doc_id:char_start-char_end]` syntax for every factual claim; Cowork pipes the output back through `verify_citations` and surfaces any failures to the user

This is **defense-in-depth against citation hallucination** — the LLM still hallucinates, but we catch it before the lawyer reads it.

## 7. Pack format

A pack is a directory (also distributable as `.tar.zst`) dropped in `~/.anno-rag/packs/` or shipped in-repo at `packs/` (workspace root).

```
<pack_id>/
├── pack.toml             # manifest
├── patterns.toml         # FR-specific + vertical regexes
├── entity_types.toml     # entity type → description (drives GLiNER prompts)
├── citation_rules.toml   # (optional) citation extractors for the vertical
├── policy.toml           # retention, audit granularity, privilege rules
├── adapter/              # (optional) LoRA — base_model+adapter_config+safetensors
└── README.md
```

### 7.1 `pack.toml` schema

```toml
[pack]
id = "legal-fr-v1"
name = "Legal — France"
version = "1.0.0"
language = "fr"
vertical = "legal"
base_model = "fastino/gliner2-multi-v1"   # required if adapter/ present
license = "MIT OR Apache-2.0"
authors = ["Anno project"]

[pack.signature]
# Optional but recommended: ed25519 signature of the pack content hash
public_key = "..."
signature = "..."

[pack.compatibility]
anno_rag_min_version = "0.1.0"
requires_features = []
```

### 7.2 `policy.toml` schema

```toml
[retention]
default_days = 3650        # 10 years for legal-fr; 1825 (5y) for general-fr
purge_strategy = "shred"   # "shred" = vault entry + chunks; "anonymize" = vault entry only

[audit]
granularity = "verbose"    # "minimal" | "standard" | "verbose"
include_chunk_hashes = true
sign_daily = true

[privilege]
# patterns that auto-classify a doc as privileged at ingest time
auto_classify = [
  { glob = "**/correspondance/**", level = "avocat_client" },
  { glob = "**/notes-strategie/**", level = "avocat_client" },
]

[gdpr]
legal_basis_default = "art_6_1_f"  # legitimate interest
controller_required = true          # CLI refuses operations without controller identity set
```

### 7.3 Pack lifecycle

```sh
anno-rag pack install ./legal-fr-v1.tar.zst
anno-rag pack list
anno-rag pack info legal-fr-v1
anno-rag pack verify legal-fr-v1     # signature + manifest sanity
anno-rag profile activate legal-fr   # hot-swaps LoRA via anno::load_adapter (~100ms)
anno-rag profile current
```

Switching profiles flushes the in-memory detector and re-binds patterns. Vault is **profile-agnostic** — same vault across all profiles, but pseudonyms generated under one profile remain valid when querying under another.

## 8. CLI surface

```
anno-rag <COMMAND>

  ingest        Ingest a folder (or single file)
                -p, --profile <pack_id>          (required)
                -o, --output-dir <path>          (where anonymized copies go)
                --recursive
                --as-of <date>                    (override law-validity date)

  search        Query the index
                -q, --query <text>
                -k, --top-k <N>                   (default 10)
                --expand-citations <hops>         (default 0)
                --as-of <date>
                --include-privileged              (audit-logged)

  forget        Erase a subject (Art. 17)
                --subject <ref>                   (PER_42 | "name" | email | …)
                --reason <text>                   (required, audit-logged)
                                                  → emits proof-of-erasure receipt

  find-subject  Locate all chunks referencing a subject (Art. 15)
                --subject <ref>

  export        Export a subject's data (Art. 20)
                --subject <ref>
                --format json|csv

  register      Generate Art. 30 register
                --since <date> --until <date>
                --format pdf|csv

  pack          install | list | info | verify | uninstall
  profile       activate | current | list
  mcp           Run MCP server on stdio (for Cowork)
  serve         Run HTTP API on a TCP port
  audit         verify-chain | export | tail
  rehydrate     Restore originals in a piece of text (debug/admin)
```

## 9. MCP tool surface

Implemented via `rmcp`. Stdio transport for Cowork plugin model.

```
search(query, top_k, filters?, as_of?, expand_citations?, profile?)
   → [{chunk_id, doc_id, content, score, structural_path, citations[], chunk_hash, source}]
   (privilege-gated: chunks with privilege != none NEVER returned)

index_folder(path, recursive, output_dir?, profile)
   → {indexed_docs, indexed_chunks, skipped, errors[]}
   (idempotent on content hash)

get_document(doc_id, include_chunks?)
   → {metadata, chunks[]}

list_documents(filter?)
   → [{doc_id, source_path, ingested_at, profile, chunk_count}]

find_subject(subject_ref)            # Art. 15
   → {chunks_referencing[], docs[], total_count}

forget(subject_ref, reason)          # Art. 17
   → {erased_chunks, erased_docs, vault_entries_removed, proof_receipt}

export_subject(subject_ref, format)  # Art. 20
   → {data, format, generated_at, signature}

audit_query(actor?, since?, until?, op?)
   → [{ts, op, actor, subject_tokens, doc_id, ...}]
   (read access to audit log; privileged ops included)

verify_citations(text)               # NEW — post-generation citation check
   → [{citation, status: Valid | NotFound | OffsetMismatch, evidence?}]
   (defense-in-depth against LLM hallucinated citations)

list_versions(table?)                # NEW — LanceDB time-travel
   → [{version, tag?, timestamp, op}]

pseudonymize(text)                   # via cloakpipe-core (wrapper)
rehydrate(text)                      # via cloakpipe-core (wrapper)
detect(text)                         # combines cloakpipe + anno
vault_stats()                        # via cloakpipe-core
```

**Authorization:** the MCP server has a `roles` config. Three roles in v1:
- `reader` — search, get_document, list_documents
- `dpo` — adds find_subject, audit_query, register
- `admin` — adds forget, export_subject, pack/profile mgmt

Cowork's client identifier is mapped to a role in the server config.

## 10. HTTP API

Mirror of MCP tools, REST style. Listens on `127.0.0.1:<port>` by default. TLS strongly recommended if binding to non-loopback.

```
POST /v1/search
POST /v1/ingest
GET  /v1/documents
GET  /v1/documents/:id
POST /v1/subjects/find
POST /v1/subjects/:ref/forget
GET  /v1/subjects/:ref/export
GET  /v1/audit
GET  /v1/audit/register
POST /v1/pseudonymize
POST /v1/rehydrate
POST /v1/detect
```

Auth: bearer token (configurable, persisted in OS keyring). Same role map as MCP.

## 11. Storage layout on disk

```
~/.anno-rag/
├── config.toml                  # bound paths, ports, models, profile
├── .lock                        # fd-lock advisory lock; CLI and service mutually exclusive
├── vault.db                     # SqliteVault (AES-256-GCM per-row, WAL mode)
├── vault.key.ref                # OS keyring slot ref (key itself in keyring; 32 bytes
│                                # derived via Argon2id from keyring secret)
├── index.lance/                 # LanceDB columnar tables
│   ├── chunks.lance             # IVF_HNSW_SQ vector index (dim=1024 for bge-m3)
│   ├── summaries.lance          # SAC second-level index (per-section summaries)
│   ├── documents.lance
│   ├── citations.lance          # typed edges: cites / cites-overruled / …
│   ├── subjects.lance           # Art. 15 fast lookup
│   └── _versions/               # native time-travel; replaces our supersessions table
├── audit/
│   ├── audit.jsonl              # hash-chained (sha256 of prev line embedded)
│   ├── audit.jsonl.sig          # daily HMAC signature
│   └── register-<from>-<to>.{csv,pdf}
├── tessdata/                    # OCR language packs (only fra.traineddata
│   └── fra.traineddata          #   needs to be present; eng auto-downloaded
│                                #   by kreuzberg-tesseract on first use)
├── packs/                       # installed packs
│   ├── general-fr-v1/
│   └── legal-fr-v1/
├── models/                      # cached ONNX/safetensors weights
│   ├── gliner2-multi-v1/
│   ├── camembert-L6-mmarcoFR/   # default reranker (~140 MB)
│   └── bge-m3/                  # default embedder (~1.3 GB)
└── outputs/                     # pseudonymized copies (option C output)
    └── <doc_id>.anon.{md,txt}
```

**Encryption at rest options (config-driven):**
- Vault: always AES-256-GCM per-row (SqliteVault, mandatory)
- Audit: signed only (HMAC chain), not encrypted
- Index: not encrypted in v1 (LanceDB OSS lacks native encryption in May 2026; rely on filesystem-level encryption — BitLocker / FileVault — and document this prominently). v1.1 will add app-layer envelope encryption for the `text` column if needed before LanceDB ships native at-rest encryption.

**Multi-process safety:** LanceDB on local filesystem is NOT multi-process-safe by default (default commit handler can lose writes). `anno-rag` takes an exclusive advisory lock via `fd_lock::RwLock::try_write()` on `.lock` at startup. If CLI is running, the MCP server refuses to start, and vice versa. This is documented in the user guide.

**Windows-specific advisory:** the installer writes a README pointing operators to (a) exclude `~/.anno-rag/` from Windows Defender real-time scanning (fragment-rewrite-heavy workload), (b) never place `~/.anno-rag/` inside OneDrive / Dropbox / Google Drive (file locks + sync collisions break the WAL).

## 12. GDPR compliance mapping

| Article | What the product provides | What the operator still owes |
|---|---|---|
| **Art. 5(1)(e)** — storage limitation | Per-pack retention (`policy.retention.default_days`), purge job (`anno-rag audit purge`) | Set retention values appropriate to use case |
| **Art. 5(2)** — accountability | Hash-chained audit, signed daily; Art. 30 register exporter | DPO process, CNIL declaration |
| **Art. 15** — right of access | `find-subject` (CLI) and `find_subject` (MCP/API) — returns all chunks referencing a subject | Verify requestor identity; respond within 1 month |
| **Art. 17** — right to erasure | `forget` — drops chunks, removes vault entries, audit-logs with proof receipt | Verify identity; confirm to subject |
| **Art. 20** — portability | `export-subject` → CSV/JSON with rehydrated values for that subject only | Transmit securely to subject |
| **Art. 25** — privacy by design | Pseudonymization is mandatory in the pipeline; cleartext only exists during ingest and in the vault | — |
| **Art. 30** — register of processing | Auto-generated CSV/PDF register | Sign and store register |
| **Art. 32** — security | AES-256-GCM vault, OS keyring for key storage, role-gated APIs, audit chain integrity check on startup | Operate OS/keyring securely; backup vault.enc |
| **Art. 33** — breach notification | Startup integrity check; corruption-alert event on hash chain break | Actually notify CNIL within 72h |
| **Art. 35** — DPIA | DPIA-ready inventory: enabled entity types, retention, recipients, retention rationale per pack | Conduct DPIA, document residual risk |

**Sector overlays (loaded by pack):**
- `legal-fr` — CNB Référentiel Sécurité Avocats: privilege auto-classification; 10y retention; case-number subject linkage
- `tax-fr` (post-MVP) — Ordre des E.-C.: 10y retention (C. com. L.123-22); secret professionnel comptable
- `hr-fr` (post-MVP) — Code du travail: NIR special handling (always encrypted-only, never returned even pseudonymized in MCP responses); 5y retention
- `medical-fr` (post-MVP, gated) — HDS: refuses to operate unless `~/.anno-rag/config.toml` declares an HDS-certified storage path

## 13. Lifts inventory (attribution checklist)

Files lifted from `C:\OpenCode\hacienda`. **Narrowed from the v1 list** because kreuzberg covers more than originally assumed (chunking, structure detection) and LanceDB ≥ 0.27 ships native RRF + time-travel:

| Source (hacienda) | Destination (anno-rag) | Modification scope |
|---|---|---|
| `crates/hacienda-engine/src/lance/*.rs` (schema + open) | `crates/anno-rag-store/src/lance.rs` | Reshape schema entirely (chunks, summaries, citations); use `merge_insert` upsert; use native `RRFReranker`; switch index to `IVF_HNSW_SQ` |
| `crates/hacienda-engine/src/ranking.rs::authority_multiplier` | `crates/anno-rag-store/src/ranking.rs::authority_boost` | Keep boost logic for conclusions/operative; cross-encoder rerank via `Rerank` trait impl in `anno-rag-embed` (no direct candle dep in -store) |
| `crates/hacienda-engine/src/graph_rag.rs` | `crates/anno-rag-store/src/graph_rag.rs` | Extend with typed citation edges (cites/overruled/distinguished); add citation-graph hop expansion |
| ~~`crates/hacienda-engine/src/lance/search.rs::hybrid_search`~~ | (not lifted) | LanceDB 0.27 ships native hybrid + `RRFReranker` — use built-ins |
| ~~`crates/hacienda-core/src/ner/mod.rs::chunk_text*`~~ | (not lifted in MVP) | kreuzberg's `ChunkingConfig { chunker_type: MarkdownAware }` handles section-aware chunking; revisit if coref-aware chunking measurably wins |
| ~~`crates/hacienda-engine/src/lance/versioning`~~ | (not lifted) | LanceDB native time-travel (`list_versions`, `tag`) replaces custom supersessions |
| `crates/hacienda-core/src/gliner2_sidecar.rs` | (reference only — not lifted; we use anno in-process via gliner2_fastino_candle) | — |

Each destination file gets a header:
```rust
// Adapted from hacienda (https://github.com/<repo>) at commit <sha>,
// originally MIT OR Apache-2.0. Modifications: <summary>.
// See THIRD_PARTY/hacienda/PROVENANCE.md for the full provenance log.
```

`THIRD_PARTY/hacienda/PROVENANCE.md` lists every lifted file with source path, source commit SHA at lift time, destination path, and a short list of modifications. Updated when files diverge further.

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Kreuzberg is Elastic-2.0 (not OSI-OSS) since v4.8.0 | Operator on-prem use is fine; flagged in README + LICENSE-NOTICES; SaaS deployment would require swapping kreuzberg or commercial license. Documented prominently. |
| Kreuzberg API churn — RCs shipping on crates.io alongside stable (4.10.0-rc.15 + 4.9.7 both 2026-05) | Pin `=4.9.7`; abstraction layer in `ingest/kreuzberg.rs` isolates breakage; re-pin every minor bump and review CHANGELOG. Plan: monthly sync. |
| Kreuzberg field-name footgun (`max_chars` not `max_characters`) | Documented in `ingest/kreuzberg.rs` comments; explicit unit test asserts our `ChunkingConfig` builder maps to current names. |
| Cloakpipe-core not on crates.io, upstream may drift | Workspace path-dep vendored under `vendor/cloakpipe/`; **wrapper pipeline (no fork)** keeps drift cost low. `cloakpipe::Detector` is a struct not a trait — agent verified — so we compose, not inherit. |
| Cloakpipe Vault has no KDF — accepts raw 32 bytes | Derive key via Argon2id from a 32-byte OS-keyring secret; documented in `pseudonymize/vault.rs`. |
| Cloakpipe-audit is NOT hash-chained | We write our own `crates/anno-rag-audit/src/chain.rs` (sha256 chain + HMAC daily sig + Art. 30 register exporter). Don't use `cloakpipe-audit` for this product. |
| Cloakpipe token format `{PREFIX}_{N}` non-configurable | Acceptable for v1; if customization needed (e.g. emoji tokens), fork `vault.rs:151` + `rehydrator.rs:46`. Documented as known-limitation. |
| Reranker (camembert-L6-mmarcoFR) trained on mMARCO-FR conversational, not legal | Document caveat; expose model as overridable; v1.x roadmap: fine-tune on French legal pairs. Power users with bge-m3 embedder can pair with `bge-reranker-v2-m3`. |
| LoRA adapter for `legal-fr` not yet trained at MVP launch | MVP ships regex+heuristic only for `legal-fr`; LoRA slot in pack format; train + release as separate pack update |
| `fra.traineddata` Tesseract pack not bundled by kreuzberg | Ship `fra.traineddata` (~10 MB) in our installer at `~/.anno-rag/tessdata/`; set `TESSERACT_RS_CACHE_DIR`. Auto-download as fallback with operator consent. |
| Hash-chain audit can be tampered if attacker has FS access AND keyring access | Document threat model: audit chain protects against unprivileged tampering; full-trust admin compromise is out of scope for v1 (v1.x: signed external audit sink) |
| BGE-m3 embedder is ~1.3 GB on disk (vs ~470 MB for e5-small) | Trade-off accepted: legal-RAG quality improvements (MLDE-2024 benchmark) outweigh disk cost. Power-user config can swap to `multilingual-e5-small` for low-footprint. |
| LanceDB 0.x crate churn (Lance core 1.0 / lancedb 0.27 split) | Pin `=0.27.2`; arrow family must match (`arrow = "=58"`); re-pin every 2-4 weeks |
| LanceDB no native at-rest encryption (OSS) | Document FS-encryption requirement (BitLocker / FileVault) in install guide; v1.1: app-layer envelope encryption for `text` column. |
| LanceDB multi-process unsafe on local FS | Advisory lock `~/.anno-rag/.lock` via `fd-lock` — CLI and MCP server mutually exclusive |
| LanceDB delete = soft tombstone | `forget()` always runs `delete + Prune(older_than=0) + Compact(materialize_deletions=true) + drop affected tags` — documented in `forget.rs` |
| LanceDB index build is heavy (500k @ 1024-dim = minutes + GBs) | `store/index.rs` runs IVF_HNSW_SQ build in a `tokio::spawn_blocking` background task; CLI `ingest` returns after upsert; index rebuild on demand or scheduled |
| Windows Defender slows fragment-heavy writes | Installer adds README pointing operators to exclude `~/.anno-rag/` from real-time scan; refuses to run inside OneDrive/Dropbox paths |
| LanceDB schema migrations on version upgrades | Schema version column on every table; explicit migration commands; refusal to start on unrecognized schema unless `--migrate` |
| Synthetic summary generation requires LLM call at ingest | Documented in threat model: only pseudonymized text sent; Cowork-supplied LLM config inherits its trust; can be disabled (`[summary] enabled = false`) for fully air-gapped ingest at cost of long-doc precision |
| Citation regex coverage incomplete for old/non-standard FR refs | Pluggable via pack rules; `verify_citations` returns `NotFound` for unknown formats — operator sees the gap and can add a rule |
| Cross-platform code-signing burden | Document the steps; not in MVP scope but listed in v1.1 packaging story |
| **axum 0.7 → 0.8 migration** (path params syntax, native async-in-trait, `Sync` requirement on handlers) | Adopt 0.8 from day 1; document the path-param syntax change in `anno-rag-api` README. Old `/:id` panics at startup with clear message, won't silently fail. |
| **arrow 58 quarterly cadence** (next breaking ≈ 59 ETA Aug 2026) | Pin `=58`; abstract Arrow types behind `anno-rag-core::entity` types where reasonable; re-pin quarterly with explicit migration commits |
| **fs2 unmaintained** | Use `fd-lock = "4"` instead (cross-platform, RW lock, actively maintained) |
| **mold linker is Linux-only OSS** (macOS commercial-only as `sold`) | Use `lld` on macOS (brew install llvm); rust-lld bundled on Windows since 1.74 — docs branch by OS |
| **cargo-watch on life support** | Standardize on `bacon` for dev workflow; document in contributor guide |
| **ort 2.0 still RC after >12 months** | Pin exact RC (`=2.0.0-rc.12`); do NOT caret `"2"`. Plan re-evaluation when 2.0 stable ships |
| **Mono-crate sprawl** (5000+ LOC in one crate) | Mitigated by §5 decomposition (7 lib crates); CI build-time gate fails on > 25% regression from baseline |
| **Generic explosion** (monomorphization on Vec<Chunk> + Embed/Rerank trait bounds) | Use `&dyn Trait` for runtime-config seams (Embed, Rerank impls picked at startup); generics only where perf-critical and call-site type is fixed |

## 15. Decision log

| # | Question | Decision | Rationale |
|---|---|---|---|
| 1 | Workflow type | Option C — both anonymized copies on disk AND searchable index | User requirement |
| 2 | Topology | A — anno workspace owns, cloakpipe-core path-dep, hacienda surgical lifts | Minimum coupling, maximum control |
| 3 | LLM | Remote via Cowork (MCP client) — no local LLM, no bundled GUI | User requirement |
| 4 | Surfaces | CLI + MCP server + HTTP API, single Rust binary | User requirement |
| 5 | Ingestion | Kreuzberg Rust crate `=4.9.7` (Elastic-2.0 accepted for on-prem). Tesseract statically bundled via `kreuzberg-tesseract` feature. | Format coverage 97+; no system Tesseract install required |
| 6 | OCR | Default-on, Tesseract bundled. Ship `fra.traineddata` in installer. | User requirement; static-link removes prereq pain |
| 7 | Structure detection | **Kreuzberg's `include_document_structure: true` + `chunker_type: MarkdownAware` does it** — our `ingest/structure.rs` is a thin TYPING layer (~200 lines), not a parser | Massive simplification vs v1 spec; kreuzberg's hierarchy + RT-DETR layout already detect headings/sections/articles |
| 8 | Detection | Anno gliner2_fastino_candle + per-pack LoRA + FR regex patterns via cloakpipe `CustomConfig.patterns` | Phase 4 merged; LoRA hot-swap fits vertical packs |
| 9 | Cloakpipe integration | **Wrapper pipeline, no fork** — compose `cloakpipe::Detector` (struct, not trait) with `AnnoDetector`; merge entities; call `Replacer` | Agent verified Detector is a struct; wrapping is cleanest, survives upstream rebases |
| 10 | Vault | `SqliteVault` (not file `Vault`) for WAL thread-safety + `user_id` ready for v2 multi-tenant | `Vault` is `!Sync`; SqliteVault is what's actually used in cloakpipe-mcp |
| 11 | Vault key derivation | Argon2id from 32-byte OS-keyring secret (cloakpipe Vault takes raw 32 bytes — no KDF) | Cloakpipe doesn't ship a KDF; we add one |
| 12 | Audit | **Custom `anno-rag-audit` crate** (sha256 chain + daily HMAC sig + Art. 30 register). Do NOT use `cloakpipe-audit` (not hash-chained). | Cloakpipe-audit is append-only JSONL/SQLite without integrity layer |
| 13 | Vector store | LanceDB `=0.27.2`, arrow `=58` (verified May 2026 — arrow 58 is what lancedb 0.27.2 actually expects) | Rust-native, embedded; hacienda already uses it |
| 13a | Crate decomposition | **7 library crates + 3 binaries** (`anno-rag-core`/`-ingest`/`-detect`/`-embed`/`-store`/`-audit`/`-eval` + `-cli`/`-mcp`/`-api`) | Mirrors anno's existing pattern; isolates candle from store for fast incremental builds (~5s vs ~60s per edit) |
| 13b | Trait substitution | `Ingest`/`Detect`/`Embed`/`Rerank`/`Store`/`Audit`/`PrivilegeGate` traits in `anno-rag-core` | Testability + runtime model swap |
| 13c | Error model | `thiserror::Error` enum in core with fatal/recoverable distinction | Fail-closed for vault/audit corruption |
| 13d | Newtype IDs | `DocId`, `ChunkId`, `SubjectId`, `CitationId` distinct types | Compile-time disambiguation, non-negotiable for legal code |
| 13e | Lockfile crate | `fd-lock = "4"` (NOT `fs2` — unmaintained) | Cross-platform; advisory lock at `~/.anno-rag/.lock` |
| 13f | HTTP framework | `axum = "0.8"` (NOT 0.7 — verified current) | Native async-in-trait, `Sync` handlers enforced at compile; path params `/{id}` (breaking from 0.7) |
| 13g | Tokio LTS pin | `tokio = "1.51"` LTS until Mar 2027 | Predictability over latest features |
| 13h | Build perf | mold (Linux) / lld (Mac+Win) + cranelift codegen + sccache + `debug="line-tables-only"` + `[profile.dev.package."*"] opt-level = 1` | Target: cargo check < 2s per crate edit |
| 13i | MSRV + edition | Rust 1.81, edition 2024 | Native async-in-trait; all verified deps compile cleanly |
| 14 | Vector index | `IVF_HNSW_SQ` (scalar-quantized HNSW inside IVF partitions) | LanceDB sweet spot for 10k-500k vectors; HNSW pure is not top-level exposed |
| 15 | Search | LanceDB native `Index::FTS` (BM25) + native vector search + native `RRFReranker` k=60 | Drop our custom RRF — LanceDB ships it |
| 16 | Authority boost | Lifted from hacienda — conclusions+0.30, operative+0.20 | Plug-in to ranking pipeline |
| 17 | Cross-encoder rerank | In-process via candle (LanceDB has no native CE). Default `antoinelouis/crossencoder-camembert-L6-mmarcoFR` (~140 MB). Power-user pairing with bge-m3: `BAAI/bge-reranker-v2-m3` (~570 MB). | Honor "small reranker" constraint while documenting the pair-with-embedder upgrade path |
| 18 | Embedding model | **Switched: `BAAI/bge-m3` (568M, ~1.3 GB, 1024-dim multi-vector dense+sparse)** | Legal-RAG SOTA agent: bge-m3 beats e5-Mistral-7B on MLDE-2024 legal benchmark at 3.2× speed; 8k ctx handles long legal chunks. Power user can swap to multilingual-e5-small for footprint. |
| 19 | Temporal versioning | LanceDB **native time-travel** (`list_versions`, `tag`, `checkout`) + `as_of_law_date` column. **Drop custom supersessions table.** | Native is cleaner; only friction is tagged-version pruning for Art. 17 (handled in forget.rs) |
| 20 | GDPR Art. 17 erasure | `forget()` chains: delete + drop affected tags + Prune(older_than=0) + Compact(materialize_deletions=true) + remove vault entries + audit-log + receipt | LanceDB delete is soft tombstone; physical erasure needs the full chain |
| 21 | Process safety | Advisory lock `~/.anno-rag/.lock` via `fd-lock` (replaces unmaintained `fs2`) — CLI ↔ MCP server mutually exclusive | LanceDB multi-process on local FS not safe by default |
| 22 | Contextual prefixes | Added per Anthropic Contextual Retrieval (−49% retrieval-failure reported) | Cheap quality win |
| 23 | Summary-Augmented Chunking | Added per arxiv 2510.06999 — section-level secondary index for long docs | Required for 50-page Cass. arrêts |
| 24 | Quote-grammar verifier | New MCP tool `verify_citations(text)` for post-generation citation validation | Defense-in-depth against legal LLM hallucination (17-33% rate in published systems per Magesh et al. 2025) |
| 25 | Citation regex donors | BO-ECLI Parser (JURIX 2017) + pyJudilibre | Established, well-tested FR/EU legal citation parsers |
| 26 | Internal eval set | 100-Q FR-legal eval set in LegalBench-RAG format (char-span ground truth) | Required to evaluate any model swap |
| 27 | Privilege boundary | MCP/HTTP default-block privileged chunks; CLI `--include-privileged` is audit-logged | User requirement |
| 28 | Legal-RAG additions | All 7 (originally 5 + contextual prefixes + SAC + quote-grammar verifier) | User requirement + research-validated |
| 29 | Packs at MVP | `general-fr-v1`, `legal-fr-v1` (regex+heuristic; LoRA TBD) | User requirement; tax/HR/medical post-MVP |
| 30 | Watch mode | Deferred to v1.1 | "minimal" MVP |
| 31 | Index encryption at rest | Deferred to v1.1; FS encryption (BitLocker/FileVault) required at v1 | LanceDB OSS no native encryption |
| 32 | Cherry-pick from hacienda | Surgical lifts with attribution, narrowed since kreuzberg + LanceDB native cover more | See §13 |
| 33 | Hacienda anti-lifts | Plaintext SQLite doc store, double EntityCategory mapping, hardcoded FR descriptions, legacy VecStore, Python sidecar | Each conflicts with the design |

## 16. Provider privacy gateway roadmap

The original Northstar assumed Cowork as the UI and remote LLM access through
Cowork. The provider strategy is now more explicit: `anno` is the local trust
boundary, while TensorZero is optional LLMOps infrastructure behind that
boundary.

The global target is:

```text
Cowork / Claude Code / app metier
  -> anno privacy boundary
  -> local, sovereign, or global-anonymized provider
```

TensorZero remains useful for routing, observability, fallback, A/B testing,
and cost/latency tracking, but it must only receive pseudonymized payloads.
The same rule applies to documents: native Claude/Cowork uploads must be
processed locally or rejected until Anno can prove only sanitized derivatives
cross the provider boundary.

**v0.2 — Cowork tool rail**
- `anno-rag mcp` exposes local RAG tools to Cowork over stdio.
- Cowork can search pseudonymized chunks and rehydrate through the local vault.
- Spec: [2026-05-13-anno-rag-v0.2-cowork-minimum.md](./2026-05-13-anno-rag-v0.2-cowork-minimum.md)

**v0.3 — Cowork LLM privacy boundary**
- Add `anno-privacy-gateway` in front of Cowork's Anthropic-compatible LLM calls.
- Pseudonymize every text-bearing prompt field before any upstream call.
- Forward pseudonymized Anthropic requests to `anthropic-proxy-rs`, then TensorZero or another OpenAI-compatible provider.
- Rehydrate the final non-streaming answer automatically before returning it to Cowork.
- Reject streaming explicitly.
- Spec: [2026-05-13-anno-privacy-gateway-v0.3.md](./2026-05-13-anno-privacy-gateway-v0.3.md)

**v0.4 — Streaming and native provider adapter**
- Add streaming/SSE rehydration without leaking partial pseudonym tokens.
- Internalize the provider adapter path so `anthropic-proxy-rs` becomes a fallback, not a required sidecar.
- Route directly to TensorZero, local OpenAI-compatible models, sovereign providers, or global providers.
- Preserve the guarantee that TensorZero and external providers only see pseudonymized content unless an explicit local-cleartext policy is configured.
- Spec: [2026-05-13-anno-privacy-gateway-v0.4.md](./2026-05-13-anno-privacy-gateway-v0.4.md)

**v0.5 — Claude/Cowork document ingress**
- Keep v0.3/v0.4 fail-closed behavior for native `/v1/files` and `document` blocks until this is implemented.
- Preferred first path: local MCP/MCPB document ingest into `anno-rag`, using `kreuzberg` extraction and local vault pseudonymization before retrieval.
- Second path: intercept Anthropic-style `/v1/files` and `document` blocks, map local file IDs to sanitized derivatives, and forward only pseudonymized/sanitized content.
- TensorZero may observe sanitized files or pseudonymized text, never original document bytes by default.
- Spec: [2026-05-13-anno-document-ingress-v0.5.md](./2026-05-13-anno-document-ingress-v0.5.md)

**v1.0 — Regulated-profession RAG product**
- Combine the tool rail (`anno-rag mcp`) and LLM rail (`anno-privacy-gateway`) into one documented deployment profile.
- Default deployment supports local, sovereign, and global-anonymized model choices.
- All model observability is pseudonymized by construction.

## 17. Roadmap (post-MVP, informational)

**v1.1**
- Watch mode (folder reindex on file change/delete)
- Index encryption at rest (app-layer envelope on `text` column)
- `tax-fr` and `hr-fr` packs
- LoRA adapter training docs + release flow
- Reranker fine-tuned on French legal pairs
- Conditional HyDE query rewriting (skip for cite-bearing queries)
- Coref evaluation on FR legal corpus; possible swap to BookNLP-Fr if anno's multilingual coref underperforms

**v1.2**
- Multi-jurisdiction (EU + OHADA + QC) citation rules
- WebUI (optional Tauri shell)
- LanceDB native encryption (when upstream supports it) or sqlcipher sidecar
- Convex-combination hybrid fusion with tuned α (replace RRF default once we have ~50 labeled FR-legal queries)
- ColBERT-style late interaction via jina-colbert-v2 (multilingual)

**v2.0**
- Multi-tenant deployments (cloakpipe SessionManager-keyed)
- HDS-certified storage backend for `medical-fr`
- SaaS-mode (would require kreuzberg license decision)
- TEE deployment option (Nitro Enclave) lifted from cloakpipe-tee scope

## 18. Sources & references

**Kreuzberg:**
- [kreuzberg on lib.rs](https://lib.rs/crates/kreuzberg) / [docs.rs](https://docs.rs/crate/kreuzberg/latest)
- [Kreuzberg docs](https://docs.kreuzberg.dev/) (features, extraction guide, installation)
- [Rust API reference](https://docs.kreuzberg.dev/reference/api-rust/)
- [Document structure extraction (dev.to)](https://dev.to/kreuzberg/document-structure-extraction-with-kreuzberg-44cj)

**Cloakpipe:**
- Local source at `vendor/cloakpipe/` (upstream `rohansx/cloakpipe`, Apache-2.0)
- Agent survey output stored in design archive

**Claude/Cowork provider integration:**
- [Anthropic Files API](https://platform.claude.com/docs/en/build-with-claude/files)
- [Anthropic PDF support](https://platform.claude.com/docs/en/build-with-claude/pdf-support)
- [Build a desktop extension with MCPB](https://claude.com/docs/connectors/building/mcpb)
- [Cowork 3P extensions](https://claude.com/docs/cowork/3p/extensions)
- Local source: `C:\tmp\anno-research\anthropic-proxy-rs` (Files API unsupported; no `document` content block)
- Local source: `C:\tmp\anno-research\tensorzero` (native file blocks and provider serialization behind Anno boundary)

**LanceDB:**
- [lancedb docs.rs](https://docs.rs/lancedb/latest/lancedb/)
- [Table struct](https://docs.rs/lancedb/latest/lancedb/table/struct.Table.html)
- [Reranker trait](https://docs.rs/lancedb/latest/lancedb/rerankers/trait.Reranker.html)
- [Full-text search](https://docs.lancedb.com/search/full-text-search) / [Hybrid search](https://docs.lancedb.com/search/hybrid-search)
- [Vector indexes](https://docs.lancedb.com/indexing/vector-index)
- [Versioning](https://docs.lancedb.com/tables/versioning)
- [Announcing Lance SDK 1.0.0](https://lancedb.com/blog/announcing-lance-sdk/)

**Legal RAG state of the art:**
- [LegalBench-RAG (arxiv 2408.10343)](https://arxiv.org/abs/2408.10343) / [reference implementation](https://github.com/zeroentropy-ai/legalbenchrag)
- [Isaacus Legal RAG Bench](https://huggingface.co/blog/isaacus/legal-rag-bench)
- [Towards Reliable Retrieval for Large Legal Datasets (arxiv 2510.06999)](https://arxiv.org/abs/2510.06999) — Summary-Augmented Chunking
- [Anthropic Contextual Retrieval](https://www.anthropic.com/news/contextual-retrieval)
- [BSARD — Maastricht Law-Tech](https://github.com/maastrichtlawtech/bsard) — French statutory retrieval benchmark
- [MTEB-French (arxiv 2405.20468)](https://arxiv.org/html/2405.20468v2)
- [BAAI/bge-m3](https://huggingface.co/BAAI/bge-m3) / [bge-reranker-v2-m3](https://huggingface.co/BAAI/bge-reranker-v2-m3)
- [BGE-M3 vs E5-Mistral on multilingual legal](https://www.johal.in/finetuning-embedding-model-multilingual-legal-documents-bgem3-vs/)
- [Antoine Louis cross-encoder collection](https://huggingface.co/collections/antoinelouis/cross-encoder-rerankers)
- [BO-ECLI Parser (JURIX 2017)](https://ceur-ws.org/Vol-2143/paper4.pdf)
- [pyJudilibre](https://pypi.org/project/pyjudilibre/) / [pylegifrance docs](https://dassignies.law/blog/utilisation-de-lapi-legifrance) / [droit-francais-mcp](https://github.com/jmtanguy/droit-francais-mcp)
- [Magesh et al. — Hallucination-Free? (JELS 2025)](https://dho.stanford.edu/wp-content/uploads/Legal_RAG_Hallucinations.pdf) — 17–33% hallucination in legal RAG
- [Damien Charlotin AI Hallucination Cases Database](https://www.damiencharlotin.com/hallucinations/) — 508 published incidents Nov 2025
- [Harvard LIL — Open French Law RAG](https://lil.law.harvard.edu/blog/2025/01/21/open-french-law-rag/) / [pipeline repo](https://github.com/harvard-lil/open-french-law-rag-pipeline)
- [Hybrid retrieval RRF review (Chauzov 2025)](https://avchauzov.github.io/blog/2025/hybrid-retrieval-rrf-rank-fusion/) / [Hybrid search production (Tian Pan Apr 2026)](https://tianpan.co/blog/2026-04-12-hybrid-search-production-bm25-dense-embeddings)
- [JuDGE benchmark (NLLP 2025)](https://aclanthology.org/2025.nllp-1.3.pdf) — very-long-doc legal RAG
- [Jones Walker — AI Conversations Not Privileged](https://www.joneswalker.com/en/insights/blogs/ai-law-blog/your-ai-conversations-are-not-privileged-what-a-new-sdny-ruling-means-for-every.html?id=102mif8) — privilege threat model
- [CMS LawNow Feb 2026 — GenAI & Privilege (EU/FR)](https://cms-lawnow.com/en/ealerts/2026/02/generative-ai-llms-and-ai-notetakers-a-new-threat-to-legal-professional-privilege)

**French regulatory:**
- CNIL guidance on pseudonymisation and DPIA
- Code du travail, Code de commerce L.123-22 (retention)
- CNB Référentiel Sécurité Avocats
- HDS (Hébergement de Données de Santé) certification framework
