# syntax=docker/dockerfile:1.7
# ─────────────────────────────────────────────────────────────────────────────
# anno-rag — lightweight image (~500 MB binary only, no baked models)
#
# Models are downloaded to a host directory on first run via `download-models`.
# This avoids a multi-GB COPY between Docker stages (extremely slow on
# Windows/WSL2) and keeps the image fast to build and rebuild.
#
# Build:
#   docker build -t anno-rag .
#
# Download models once (creates ./models/ on the host):
#   Windows CMD:  docker run --rm -v "%cd%\models:/models" ...
#   PowerShell:   docker run --rm -v "${PWD}\models:/models" ...
#   bash/Linux:   docker run --rm -v "$(pwd)/models:/models" ...
#     -e ANNO_RAG_VAULT_PASSPHRASE=placeholder \
#     -e ANNO_RAG_EMBED_MODEL=OrdalieTech/Solon-embeddings-large-0.1 \
#     -e ANNO_RAG_EMBED_DIM=1024 \
#     anno-rag download-models --dir /models
#
# Run MCP (stdio, used by Claude Desktop):
#   Windows CMD:  docker run --rm -i -v anno-data:/data -v "%cd%\models:/models" ...
#   PowerShell:   docker run --rm -i -v anno-data:/data -v "${PWD}\models:/models" ...
#   bash/Linux:   docker run --rm -i -v anno-data:/data -v "$(pwd)/models:/models" ...
#     -e ANNO_RAG_VAULT_PASSPHRASE=<your-passphrase> anno-rag
# ─────────────────────────────────────────────────────────────────────────────

# ── Stage 1: Build ────────────────────────────────────────────────────────────
# Ubuntu 24.04 (glibc 2.39) is required: ORT 1.24.4's pre-built static library
# (downloaded via ort-sys download-binaries) was compiled against glibc 2.38+,
# so Debian bookworm (glibc 2.36) cannot link it.
FROM ubuntu:24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

# System deps:
#   build-essential curl — toolchain + rustup
#   protobuf-compiler + libprotobuf-dev — protoc + well-known protos (lance-encoding)
#   tesseract-ocr + libtesseract-dev + libleptonica-dev — kreuzberg-tesseract build.rs
#   libclang-dev — bindgen (-sys crates)
#   cmake — C++ build scripts
#   pkg-config libssl-dev ca-certificates — TLS + cargo https
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        curl \
        protobuf-compiler \
        libprotobuf-dev \
        tesseract-ocr \
        libtesseract-dev \
        libleptonica-dev \
        libclang-dev \
        cmake \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust via rustup (repo pins version in rust-toolchain.toml)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH="/root/.cargo/bin:$PATH"

WORKDIR /build

# Copy workspace manifests first so the dependency layer is cached
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY workspace-hack/Cargo.toml workspace-hack/Cargo.toml
COPY crates/anno-rag/Cargo.toml             crates/anno-rag/Cargo.toml
COPY crates/anno-rag-mcp/Cargo.toml         crates/anno-rag-mcp/Cargo.toml
COPY crates/anno-rag-bin/Cargo.toml         crates/anno-rag-bin/Cargo.toml
COPY crates/anno-rag-tabular/Cargo.toml     crates/anno-rag-tabular/Cargo.toml
COPY crates/anno-config-meta/Cargo.toml     crates/anno-config-meta/Cargo.toml
COPY crates/anno-corpus-core/Cargo.toml     crates/anno-corpus-core/Cargo.toml
COPY crates/anno-corpus-store/Cargo.toml    crates/anno-corpus-store/Cargo.toml
COPY crates/anno-knowledge-core/Cargo.toml  crates/anno-knowledge-core/Cargo.toml
COPY crates/anno-knowledge-store/Cargo.toml crates/anno-knowledge-store/Cargo.toml
COPY crates/anno-source-local/Cargo.toml    crates/anno-source-local/Cargo.toml
COPY crates/anno-privacy-gateway/Cargo.toml crates/anno-privacy-gateway/Cargo.toml
COPY crates/anno-cli/Cargo.toml             crates/anno-cli/Cargo.toml
COPY crates/anno-eval/Cargo.toml            crates/anno-eval/Cargo.toml
COPY crates/anno/Cargo.toml                 crates/anno/Cargo.toml

# Copy full source
COPY . .

# Build release binary.
# ORT's download-binaries fetches a pre-built static libonnxruntime for the
# target; copy-dylibs also places libonnxruntime.so next to the binary.
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p anno-rag-bin \
    && mkdir -p /out \
    && cp target/release/anno-rag /out/anno-rag \
    && (find target/release -maxdepth 1 -name "libonnxruntime*.so*" -exec cp {} /out/ \; || true) \
    && (find target/release/build -name "libonnxruntime*.so*" -exec cp {} /out/ \; || true) \
    && (find /root/.cache/ort.pyke.io -name "libonnxruntime*.so*" -exec cp {} /out/ \; || true)

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM ubuntu:24.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        libtesseract5 \
        liblept5 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -ms /bin/sh anno

# Binary + ORT shared library
COPY --from=builder /out/ /usr/local/bin/
RUN ldconfig

# Data volume: LanceDB index, vault.enc, processed outputs
VOLUME ["/data"]

# Models volume: mounted from host after running download-models once.
# The /models directory MUST be pre-populated via the download-models service
# before starting anno-rag — the compose service mounts it read-only.
# Without models, startup will fail. Run download-models first:
#   docker compose --profile tools run --rm model-downloader
#
# NOTE: if you change ANNO_RAG_EMBED_MODEL or ANNO_RAG_EMBED_DIM, the
# existing LanceDB index at /data is incompatible (different vector dimension).
# Drop and recreate the anno-data volume:
#   docker compose down -v && docker compose up -d
VOLUME ["/models"]

# ── Environment ───────────────────────────────────────────────────────────────
# ANNO_RAG_VAULT_PASSPHRASE  REQUIRED at runtime — set in docker-compose.yml
# ANNO_RAG_DATA_DIR          Persistent data (index + vault)
# ANNO_MODELS_DIR            Host-mounted model weights directory
# ANNO_RAG_EMBED_MODEL       Ordalie embedder (1024d, French-optimized)
# ANNO_RAG_EMBED_DIM         Must match embedder output dimension
ENV ANNO_RAG_DATA_DIR=/data \
    ANNO_MODELS_DIR=/models \
    ANNO_RAG_EMBED_MODEL=OrdalieTech/Solon-embeddings-large-0.1 \
    ANNO_RAG_EMBED_DIM=1024 \
    LD_LIBRARY_PATH=/usr/local/bin

USER anno
WORKDIR /data

ENTRYPOINT ["anno-rag"]
CMD ["mcp"]
