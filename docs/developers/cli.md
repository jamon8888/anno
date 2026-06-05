# CLI

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, Admin
Language: EN

The installed `anno-rag --help` output is the authoritative command reference
for the exact binary on a machine. This page summarizes the stable developer
surface and calls out places where options depend on the installed build.

## Inspect The Installed Build

```bash
anno-rag --help
anno-rag mcp --help
anno-rag review --help
```

Use these commands before writing automation. Release candidates may expose
additional options when built with optional features.

## Common Commands

| Command | Purpose | Notes |
|---|---|---|
| `anno-rag mcp` | Start the stdio MCP server for Claude Desktop, Cowork, or another MCP client. | Configure clients with an absolute binary path and `args: ["mcp"]`. |
| `anno-rag download-models` | Download local embedder and NER weights to the model cache. | Prints the model path; set `ANNO_MODELS_DIR` when using a non-default location. |
| `anno-rag ingest <folder>` | Ingest documents, pseudonymize text, write pseudonymized outputs, and index chunks. | Options such as recursive ingest, OCR, and advanced PDF extraction depend on the installed build. |
| `anno-rag search "<query>"` | Search the local indexed corpus and return pseudonymized chunks. | Queries may contain PII; the vault tokenizes them locally before search. |
| `anno-rag review list` | List local tabular reviews. | Uses the same local LanceDB data directory as RAG storage. |
| `anno-rag review create` | Create a review, optionally from a built-in template. | Template names are validated by the installed build. |
| `anno-rag review add-rows` | Attach ingested document UUIDs to a review. | Requires document IDs from MCP/search or another installed-build output path. |
| `anno-rag review extract` | Run review extraction for existing rows and columns. | Requires configured model/provider support; use `--help` for exact flags. |
| `anno-rag review export` | Export a review. | Current CLI builds reliably document `xlsx` and `md`; use installed `--help` for available formats. |

## MCP Corpus Sync

Corpus catch-up is exposed through the MCP server rather than a standalone CLI
command. Use `sync_corpus` from the connected MCP client to refresh a selected
corpus after adding files. The default output is `knowledge_fast`; request
`legal_semantic` explicitly when legal vectors, enrichment, graph rows, and
legal document bindings should be refreshed.

## Verification

Check the binary version:

```bash
anno-rag --version
```

Check model download support without starting the RAG pipeline:

```bash
anno-rag download-models --help
```

For MCP startup issues, run:

```bash
anno-rag mcp --help
```

If the command is not found, verify the release archive was extracted and that
the binary directory is on `PATH`, or use an absolute path in the client config.

## Related Docs

- [Commands Reference](../reference/commands.md)
- [MCP Tools](mcp-tools.md)
- [Configuration](configuration.md)
