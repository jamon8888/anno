# Running anno-rag with Docker

anno-rag ships as a self-contained Docker image with models baked in (~2 GB total).
No internet access is needed at runtime.

## Quick start

```bash
# Build the image (one-time, ~10–20 min depending on network)
docker build -t anno-rag .

# Run the MCP server (interactive stdio, used by Claude Desktop)
docker run --rm -i \
  -v anno-data:/data \
  -e ANNO_RAG_VAULT_PASSPHRASE="your-strong-passphrase" \
  anno-rag
```

## docker-compose

```bash
# Copy the template and set your passphrase
cp .env.example .env          # or just export the variable
export ANNO_RAG_VAULT_PASSPHRASE="your-strong-passphrase"

docker compose up --build
```

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ANNO_RAG_VAULT_PASSPHRASE` | **yes** | — | Passphrase used to derive the AES-256 vault key. Keep this secret. |
| `ANNO_DATA_DIR` | no | `/data` | Path inside the container for the LanceDB index, `vault.enc`, and outputs. |
| `ANNO_MODELS_DIR` | no | `/models` | Path to pre-downloaded models. Already set in the image — change only if you mount your own models. |

## Volumes

| Volume | Purpose |
|--------|---------|
| `/data` | Persistent data: vector index, vault, processed documents. Mount a named volume or host path. |
| `/models` | Read-only baked-in models. Overridable with `-v /host/models:/models:ro` to use locally cached models instead of rebuilding the image. |

## Claude Desktop integration (MCP over stdio)

Add this block to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "docker",
      "args": [
        "run", "--rm", "-i",
        "-v", "anno-data:/data",
        "-e", "ANNO_RAG_VAULT_PASSPHRASE=your-strong-passphrase",
        "anno-rag"
      ]
    }
  }
}
```

Claude Desktop calls `docker run` which starts the container on stdio — anno-rag responds to MCP tool calls directly.

> **Windows path**: `claude_desktop_config.json` lives at
> `%APPDATA%\Claude\claude_desktop_config.json`
>
> **macOS path**: `~/Library/Application Support/Claude/claude_desktop_config.json`

## First run

On first use, initialize the vault (only needed once per `/data` volume):

```
anno_init_vault   ← call this MCP tool from Claude
```

The vault passphrase must match `ANNO_RAG_VAULT_PASSPHRASE`. After initialization, all PII in indexed documents is pseudonymized via the local vault.

## Updating models without rebuilding

If you want to store models outside the image (to avoid re-downloading on every `docker build`):

```bash
# Download models to a host directory once
docker run --rm \
  -v /your/host/models:/models \
  -e ANNO_RAG_VAULT_PASSPHRASE=placeholder \
  anno-rag download-models --dir /models

# Then mount at runtime
docker run --rm -i \
  -v anno-data:/data \
  -v /your/host/models:/models:ro \
  -e ANNO_RAG_VAULT_PASSPHRASE="your-passphrase" \
  anno-rag
```

## Image layout

```
/usr/local/bin/anno-rag        ← compiled binary
/usr/local/bin/libonnxruntime*.so  ← ORT 1.24.4 runtime
/models/                       ← baked models (~970 MB)
  multilingual-e5-small-onnx/  ← embedder (~470 MB)
  gliner2-multi-v1-onnx/       ← NER / GLiNER2 (~500 MB)
/data/                         ← volume (empty until first use)
```

## Build details

The image uses a 3-stage build:

1. **builder** (`rust:1.85-bookworm`) — compiles the Rust binary; ORT 1.24.4 is auto-downloaded via the `download-binaries` feature.
2. **model-downloader** (`debian:bookworm-slim`) — runs `anno-rag download-models` to pull ~970 MB from HuggingFace Hub into `/models`.
3. **runtime** (`debian:bookworm-slim`) — copies binary + ORT lib + baked models; final image is ~2 GB.

`--mount=type=cache` on the cargo registry and build target keeps rebuilds fast when only source files change.
