# Commands Reference

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, Admin
Language: EN

The installed release binary is authoritative. Run `--help` on the exact
`anno-rag` or `anno-privacy-gateway` binary deployed on the machine before
writing scripts or support runbooks.

## Common Commands

| Command | Purpose | Notes |
|---|---|---|
| `anno-rag --help` | Show the top-level CLI help. | Use this as the first source for the installed command surface. |
| `anno-rag --version` | Print the installed binary version. | Useful when matching support reports to a release candidate. |
| `anno-rag mcp` | Start the stdio MCP server. | Configure Claude Desktop, Cowork, or another MCP client with an absolute binary path and `args: ["mcp"]`. |
| `anno-rag download-models` | Populate the local model cache. | Downloads the embedder and NER model files and prints the models directory. |
| `anno-rag review --help` | Show tabular review subcommands. | Review workflows store state in the local LanceDB directory. |
| `anno-rag review list` | List local reviews. | Does not start the full RAG pipeline. |
| `anno-rag review create` | Create a review, optionally from a built-in template. | Use installed `--help` for template and flag details. |
| `anno-rag review add-rows` | Attach ingested document UUIDs to a review. | Requires `--review`, `--folder-path`, and one or more `--doc-ids`. |
| `anno-rag review extract` | Run extraction for existing review rows and columns. | Requires model/provider configuration and boots the full RAG pipeline. |
| `anno-rag review export` | Export a review. | Current reliable CLI formats are `xlsx` and `md`/`markdown`; do not assume CLI CSV export works in `v0.11.0-rc.11`. |
| `anno-privacy-gateway` | Start the Anthropic-compatible privacy gateway. | Runtime behavior is controlled by `ANNO_GATEWAY_*` environment variables. |

## Script Safety

- Pin automation to a verified binary path, not only `PATH`.
- Check `anno-rag --version` before running release-sensitive workflows.
- Use `anno-rag review export --review <id> --format xlsx --output <file>` for
  spreadsheet output.
- Use `anno-rag review export --review <id> --format md` for Markdown output.
- Treat other formats as build-dependent unless the installed `--help` and a
  test command confirm support.

## Related Links

- [CLI](../developers/cli.md)
- [Release Management](../admins/release-management.md)
