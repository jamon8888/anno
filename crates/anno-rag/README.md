# anno-rag

Local, GDPR-compliant document anonymizer + RAG service for French legal documents.

## What it does

Ingest a folder of legal documents → detect French PII (names, NIR, SIRET, IBAN-FR, phone) → pseudonymize through an AES-256-GCM vault → index in LanceDB → expose to Claude Cowork via MCP for retrieval. All local, no cloud.

## Quick start

```sh
# Build
cargo build --release -p anno-rag

# 1. Warm the model cache once (~600 MiB total: embedder + NER)
cargo run --release --example warmup_model -p anno-rag

# 2. Ingest a folder of French legal documents
./target/release/anno-rag ingest ~/cabinet/dossier-acme --recursive

# 3. Query from the CLI
./target/release/anno-rag search "résiliation pour cause"

# 4. Or hand off to Cowork via MCP
./target/release/anno-rag mcp
```

## Cowork plugin configuration

Add to your Cowork plugins config:

```json
{
  "anno-rag": {
    "command": "/absolute/path/to/anno-rag",
    "args": ["mcp"],
    "env": {
      "ANNO_RAG_VAULT_PASSPHRASE": "your-passphrase-here"
    }
  }
}
```

Cowork connects to anno-rag over stdio and gets these tools:

| Tool | Purpose |
|---|---|
| `search(query, top_k)` | Pseudonymized top-K chunks — the data Claude sees. Query is pseudonymized before embed + vector search. |
| `rehydrate(text)` | Restore originals from the local vault. Only your machine ever has both raw and pseudonymized. |
| `detect(text)` | Dry-run PII scan — entity list with offsets, no replacement. Useful for UI previews. |
| `vault_stats()` | Token-mapping counts (total + per category). |

## Architecture

```
ingest:  folder → kreuzberg → detect (regex + anno NER) → cloakpipe Vault
                            → embed (BGE-multilingual-e5-small) → LanceDB → outputs/*.anon.md

search:  query → pseudonymize → embed → LanceDB top-K → pseudonymized chunks
                                                          ↓
                                                       Cowork → Claude
                                                          ↓
                              (optional) Claude's answer with tokens → rehydrate → user
```

Cleartext PII lives only in `~/.anno-rag/vault.enc`. Pseudonymized copies (`outputs/*.anon.md`) and the LanceDB index never see raw PII.

## v0.2 additions over v0.1

- `mcp` subcommand: stdio MCP server (4 tools above)
- `warmup_model` example also downloads the anno NER model so names get scrubbed reliably (not just structured PII)
- LanceDB vector index built automatically once the chunks table crosses 1000 rows (configurable via `vector_index_threshold`)
- Crate version bumped to 0.2.0

## v0.2 deliberate non-goals

Tracked in the v1 design (`docs/superpowers/specs/2026-05-12-anno-rag-design.md`) for later releases:

- HTTP API (axum)
- GDPR Art. 30 audit / Art. 17 forget / Art. 15 find-subject
- Hierarchical legal structure typing
- Citation graph (ECLI, Cass./CE/CA)
- Privilege gating (avocat-client)
- Cross-encoder rerank
- Tabular review (Harvey/Legora pattern — v1.1 plan exists)
- Watch mode / index encryption at rest / multi-tenant
- 7-crate decomposition

## Operational notes

- **Vault key:** sourced from `ANNO_RAG_VAULT_PASSPHRASE` env (Argon2id) or OS keyring (random 32 bytes hex). Setting the env in `~/.profile` is fine for single-user; CI uses the env path.
- **Output dir:** defaults to `~/.anno-rag/outputs`. Override per ingest with `--output <dir>`.
- **Data dir:** `~/.anno-rag/` is hardcoded for v0.2; `--data-dir` flag lands in v0.3.
- **Build dep on WSL/Linux:** lance-encoding's build script wants `google/protobuf/empty.proto`. On Ubuntu/Debian: `apt install libprotobuf-dev` then build with `PROTOC_INCLUDE=/usr/include`.

## License

Dual MIT OR Apache-2.0.
Vendored `cloakpipe-core` is Apache-2.0 (upstream `rohansx/cloakpipe`).
Kreuzberg is Elastic License 2.0 — fine for on-prem use; if you ever ship as SaaS, evaluate kreuzberg's terms.

## Status

v0.2 — Cowork-minimum. See `docs/superpowers/specs/2026-05-13-anno-rag-v0.2-cowork-minimum.md` for the in/out-of-scope list.

The Northstar is `docs/superpowers/specs/2026-05-12-anno-rag-design.md` (v1 design).
