# Running anno-rag with Docker

anno-rag ships as a lightweight Docker image (~500 MB — binary only, no baked models).
Models are downloaded once to a host directory and mounted as a volume, so the image
stays small and rebuilds quickly when only source changes.

## Prerequisites

- Docker with BuildKit support (Docker 23+)
- ~1 GB free disk space for models

## Quick start (three steps)

### 1. Pull or build the image

**Pull from GHCR (recommended):**

```bash
docker pull ghcr.io/jamon8888/anno:latest
docker tag ghcr.io/jamon8888/anno:latest anno-rag
```

**Or build locally:**

```bash
docker build -t anno-rag .
```

### 2. Download models once

Models are stored on the host and mounted read-only at runtime.
Run this once — it creates a `./models` directory next to `docker-compose.yml`:

```bash
# PowerShell
docker run --rm -v "${PWD}\models:/models" `
  -e ANNO_RAG_VAULT_PASSPHRASE=placeholder `
  -e ANNO_RAG_EMBED_MODEL=OrdalieTech/Solon-embeddings-large-0.1 `
  -e ANNO_RAG_EMBED_DIM=1024 `
  anno-rag download-models --dir /models

# bash / Linux / macOS
docker run --rm -v "$(pwd)/models:/models" \
  -e ANNO_RAG_VAULT_PASSPHRASE=placeholder \
  -e ANNO_RAG_EMBED_MODEL=OrdalieTech/Solon-embeddings-large-0.1 \
  -e ANNO_RAG_EMBED_DIM=1024 \
  anno-rag download-models --dir /models
```

### 3. Run the MCP server

```bash
# PowerShell
docker run --rm -i `
  -v anno-data:/data `
  -v "${PWD}\models:/models:ro" `
  -e ANNO_RAG_VAULT_PASSPHRASE="your-strong-passphrase" `
  anno-rag

# bash / Linux / macOS
docker run --rm -i \
  -v anno-data:/data \
  -v "$(pwd)/models:/models:ro" \
  -e ANNO_RAG_VAULT_PASSPHRASE="your-strong-passphrase" \
  anno-rag
```

## docker-compose

The compose file includes an `anno-rag` service and a one-shot `model-downloader` service.

```bash
# 1. Set your passphrase
cp .env.example .env       # then edit ANNO_RAG_VAULT_PASSPHRASE in .env

# 2. Download models (one-time)
docker compose --profile tools run --rm model-downloader

# 3. Start the MCP server
docker compose up
```

> **Re-running model-downloader** is safe — it skips files that already exist.

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ANNO_RAG_VAULT_PASSPHRASE` | **yes** | — | Passphrase used to derive the AES-256 vault key. |
| `ANNO_RAG_EMBED_MODEL` | no | `OrdalieTech/Solon-embeddings-large-0.1` | HuggingFace model ID for the embedder. |
| `ANNO_RAG_EMBED_DIM` | no | `1024` | Must match the embedder output dimension. |
| `ANNO_RAG_DATA_DIR` | no | `/data` | Path inside the container for the LanceDB index, `vault.enc`, and outputs. |
| `ANNO_MODELS_DIR` | no | `/models` | Path to pre-downloaded model weights. |

> **Changing the embedder** invalidates the existing LanceDB index. Drop and recreate the
> `anno-data` volume before restarting: `docker compose down -v && docker compose up`.

## Volumes

| Mount | Purpose |
|-------|---------|
| `/data` | Persistent data: vector index, vault, processed documents. Use a named volume (`anno-data`) or a host path. |
| `/models` | Pre-downloaded model weights. Mount read-only (`ro`) at runtime. |

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
        "-v", "/absolute/path/to/models:/models:ro",
        "-e", "ANNO_RAG_VAULT_PASSPHRASE=your-strong-passphrase",
        "anno-rag"
      ]
    }
  }
}
```

> **Windows path**: `%APPDATA%\Claude\claude_desktop_config.json`
>
> **macOS path**: `~/Library/Application Support/Claude/claude_desktop_config.json`
>
> Use an absolute host path for the models mount — relative paths are not resolved by Claude Desktop.

## First run

On first use, initialize the vault (only needed once per `/data` volume):

```
anno_init_vault   ← call this MCP tool from Claude
```

The vault passphrase must match `ANNO_RAG_VAULT_PASSPHRASE`. After initialization, all PII
in indexed documents is pseudonymized via the local vault.

## Image layout

```
/usr/local/bin/anno-rag              ← compiled binary
/usr/local/bin/libonnxruntime*.so    ← ORT 1.24.4 runtime (glibc 2.38+)
/data/                               ← volume (empty until first use)
/models/                             ← volume (populated by model-downloader)
  OrdalieTech/
    Solon-embeddings-large-0.1/      ← default embedder (1024d, French-optimized)
```

## Build details

The image uses a 2-stage build on `ubuntu:24.04` (required for glibc 2.38+, which ORT 1.24.4's
pre-built static library depends on):

1. **builder** — compiles the Rust binary; ORT 1.24.4 is fetched via `download-binaries`.
2. **runtime** — copies the binary and ORT shared library; final image is ~500 MB.

`--mount=type=cache` on the Cargo registry and build target keeps rebuilds fast when only
source files change.
