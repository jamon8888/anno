# Crates

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator
Language: EN

Hacienda is a Rust workspace. The user-facing product name is Hacienda, while
the crates keep the historical `anno` names for compatibility.

## Product Crates

| Crate | Role |
|---|---|
| `anno` | Core library for text preparation, NER, PII detection, relation extraction, and coreference primitives. |
| `anno-cli` | Historical command-line surface for core `anno` workflows and development automation. |
| `anno-rag` | Library crate for local vaults, pseudonymization, LanceDB storage, embeddings, search, memory, and legal helpers. |
| `anno-rag-bin` | Binary crate that builds the installed `anno-rag` command, including CLI and MCP server entrypoints. |
| `anno-rag-mcp` | MCP server surface for `anno-rag` tools, including retrieval, memory, legal, and review workflows. |
| `anno-privacy-gateway` | Anthropic-compatible HTTP gateway that tokenizes outbound prompts before optional upstream LLM calls. |
| `anno-rag-tabular` | Library crate for schema-driven legal review tables, extraction, citations, verification, and exports. |

## Source Builds

Build the main local RAG/MCP binary:

```bash
cargo build --release -p anno-rag-bin
```

Build the HTTP privacy gateway:

```bash
cargo build --release -p anno-privacy-gateway
```

The resulting binaries are under `target/release/`:

| Package | Binary |
|---|---|
| `anno-rag-bin` | `anno-rag` or `anno-rag.exe` |
| `anno-privacy-gateway` | `anno-privacy-gateway` or `anno-privacy-gateway.exe` |

Release archives already include the supported binaries for the release
candidate. Source builds are mainly for developers, integrators, and operators
who need a local build with custom features.

## Related Docs

- [CLI](cli.md)
- [MCP Tools](mcp-tools.md)
- [Configuration](configuration.md)
