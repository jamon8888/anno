# anno-rag

Local, GDPR-oriented document anonymization and RAG for French legal work.

`anno-rag` is the library crate. The user-facing binary is built by
`anno-rag-bin` and is still named `anno-rag`.

## Current Surface

- Folder ingest for PDF, Office, HTML, email, text, Excel, XML and archives via
  Kreuzberg.
- French PII detection with structured regexes plus `anno` NER.
- Reversible pseudonymization through a local AES-256-GCM vault backed by
  `cloakpipe-core`.
- LanceDB chunk storage with hybrid vector/full-text search and optional
  cross-encoder reranking behind `--features rerank`.
- Long-term memory tables with bi-temporal validity, entity references,
  graph recall, invalidation and forget cascades.
- Legal RAG helpers for graph-ready ingestion, citation rehydration, timelines,
  risk review, clause audits, prescription checks and human validation.
- Tabular review support through `anno-rag-tabular` and the `anno-rag review`
  CLI.
- Stdio MCP server through `anno-rag-mcp` for Claude Desktop/Cowork.

All normal data paths are local. Raw PII is kept in the encrypted vault; indexed
chunks, tabular cells and MCP payloads use pseudonymized text unless explicitly
rehydrated on the local machine.

## Quick Start From Source

```sh
# Build the binary package.
cargo build --release -p anno-rag-bin

# Download end-user models once (~970 MiB: embedder + NER).
./target/release/anno-rag download-models
export ANNO_MODELS_DIR="$HOME/.anno-rag/models"

# Ingest documents.
./target/release/anno-rag ingest ~/cabinet/dossier-acme --recursive

# Search the local pseudonymized index.
./target/release/anno-rag search "résiliation pour cause"

# Run the MCP server for Claude Desktop/Cowork.
./target/release/anno-rag mcp
```

For developer cache warmup, the older example still works:

```sh
cargo run --release --example warmup_model -p anno-rag
```

## CLI

| Command | Purpose |
|---|---|
| `anno-rag ingest <folder> [--recursive] [--output <dir>]` | Extract, pseudonymize and index a folder. |
| `anno-rag ingest --advanced-pdf-native ...` | Use structured native PDF extraction knobs for text-layer PDFs. |
| `anno-rag ingest --enable-ocr ...` | Enable embedded OCR when the binary was built with `embedded-ocr`. |
| `anno-rag search <query> [--top-k N]` | Search the pseudonymized local corpus. |
| `anno-rag mcp` | Start the stdio MCP server. |
| `anno-rag bench --corpus <dir>` | Reproduce ingest/search SLO measurements on a corpus. |
| `anno-rag download-models [--dir <dir>]` | Download model weights and print the models directory. |
| `anno-rag vault status` | Report whether the OS keyring holds a vault key. |
| `anno-rag vault rotate` | Rotate the keyring vault key. |
| `anno-rag review ...` | Manage tabular legal review grids. |

## MCP Tools

Core privacy/RAG:

| Tool | Purpose |
|---|---|
| `search(query, top_k, rerank?)` | Pseudonymized top-K chunks; optional cross-encoder rerank when built with `rerank`. |
| `rehydrate(text)` | Restore local pseudo-tokens from the vault. |
| `detect(text)` | Dry-run PII scan with offsets and confidence. |
| `vault_stats()` | Token mapping diagnostics. |
| `anno_init_vault(passphrase)` | Initialize the keyring vault entry with a user passphrase. |
| `anno_health()` | Version, build target, vault state and advertised tools. |
| `download_models()` | Download the expected model layout for offline startup. |

Memory:

| Tool | Purpose |
|---|---|
| `memory_save` | Store a memory; default mode writes immediately and enriches NER refs asynchronously. |
| `memory_recall` | Hybrid vector/full-text recall, optional `as_of`, graph expansion and rerank. |
| `memory_graph_recall` | Entity-reference graph recall up to bounded hops. |
| `memory_invalidate` | Set `valid_to` on a memory. |
| `memory_forget` | Forget by id or query and cascade orphaned vault tokens. |
| `memory_list` | Cursor-paginated memory listing. |

Legal and tabular review tools are exposed in the same MCP server. Legal tools
cover legal ingest/search, graph queries, citations, contract/case extraction,
timelines, risk review, mandatory clause audits, prescription checks and human
validation. Review tools cover creating reviews, adding rows, reading the grid,
refining or overriding cells, locking cells, and exporting CSV/Markdown/XLSX.

## Tabular Review CLI

```sh
anno-rag review create --name "Acme NDA batch" --template nda-v1
anno-rag review add-rows --review <uuid> --folder-path Deal_Acme --doc-ids <doc_uuid>
anno-rag review extract --review <uuid>
anno-rag review export --review <uuid> --format xlsx --output C:\tmp\review.xlsx
```

The tabular tables live next to the RAG corpus tables in
`ANNO_RAG_DATA_DIR/index.lance` (or `~/.anno-rag/index.lance` by default).

## Configuration

Important environment variables:

| Variable | Purpose |
|---|---|
| `ANNO_RAG_DATA_DIR` | Override the local data root. Defaults to `~/.anno-rag`. |
| `ANNO_MODELS_DIR` | Use an already-downloaded model directory. |
| `ANNO_NO_DOWNLOADS=1` | Prevent network model downloads; cache/model dir must already exist. |
| `ANNO_RAG_VAULT_PASSPHRASE` | Optional passphrase-derived vault key. If absent, the OS keyring is used. |
| `ANNO_RAG_MEMORY_NER_MODE` | `async`, `sync`, or `disabled` memory NER enrichment. |

Derived paths:

- Vault: `~/.anno-rag/vault.enc`
- LanceDB index: `~/.anno-rag/index.lance`
- Model cache: `~/.anno-rag/models`
- Pseudonymized outputs: `~/.anno-rag/outputs`

## Optional OCR

OCR is off by default. For scanned PDFs, install Tesseract and build with
`embedded-ocr`; `--enable-ocr` then maps runtime OCR mode to
`auto_embedded`.

- Linux: `sudo apt install tesseract-ocr tesseract-ocr-fra`
- macOS: `brew install tesseract tesseract-lang`
- Windows: `winget install --id UB-Mannheim.TesseractOCR`

Then run:

```sh
cargo build --release -p anno-rag-bin --features embedded-ocr
anno-rag ingest <folder> --enable-ocr
```

## Release Install

For end users, prefer the GitHub Release archives documented in
`docs/release/README-release.md`. They contain:

- `anno-rag` / `anno-rag.exe`
- `anno-privacy-gateway` / `anno-privacy-gateway.exe`
- Claude Desktop config examples
- checksums in the release asset `SHA256SUMS.txt`

## License

Dual MIT OR Apache-2.0. Vendored `cloakpipe-core` is Apache-2.0. Kreuzberg is
Elastic License 2.0; on-prem usage is the intended release path, and SaaS
distribution should review Kreuzberg's terms.
