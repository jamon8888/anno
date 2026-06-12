# syntax=docker/dockerfile:1.7
# ─────────────────────────────────────────────────────────────────────────────
# anno-rag — self-contained Docker image with models baked in (~2 GB)
#
# Build:  docker build -t anno-rag .
# Run MCP (stdio, used by Claude Desktop):
#   docker run --rm -i -v anno-data:/data \
#     -e ANNO_RAG_VAULT_PASSPHRASE=<your-passphrase> anno-rag
# ─────────────────────────────────────────────────────────────────────────────

# ── Stage 1: Build ────────────────────────────────────────────────────────────
FROM rust:1.85-bookworm AS builder

# System deps:
#   protobuf-compiler + libprotobuf-dev — protoc + well-known protos (lance-encoding)
#   tesseract-ocr + libtesseract-dev + libleptonica-dev — kreuzberg-tesseract build.rs
#     (build.rs executes the tesseract binary + links against libtesseract)
#   libclang-dev — bindgen (used by some -sys crates)
#   cmake — some C++ build scripts require it
#   pkg-config libssl-dev ca-certificates — TLS + cargo https
RUN apt-get update && apt-get install -y --no-install-recommends \
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

WORKDIR /build

# Copy workspace manifests first so the dependency layer is cached
COPY Cargo.toml Cargo.lock ./
COPY workspace-hack/Cargo.toml workspace-hack/Cargo.toml
COPY crates/anno-rag/Cargo.toml           crates/anno-rag/Cargo.toml
COPY crates/anno-rag-mcp/Cargo.toml       crates/anno-rag-mcp/Cargo.toml
COPY crates/anno-rag-bin/Cargo.toml       crates/anno-rag-bin/Cargo.toml
COPY crates/anno-rag-tabular/Cargo.toml   crates/anno-rag-tabular/Cargo.toml
COPY crates/anno-config-meta/Cargo.toml   crates/anno-config-meta/Cargo.toml
COPY crates/anno-corpus-core/Cargo.toml   crates/anno-corpus-core/Cargo.toml
COPY crates/anno-corpus-store/Cargo.toml  crates/anno-corpus-store/Cargo.toml
COPY crates/anno-knowledge-core/Cargo.toml  crates/anno-knowledge-core/Cargo.toml
COPY crates/anno-knowledge-store/Cargo.toml crates/anno-knowledge-store/Cargo.toml
COPY crates/anno-source-local/Cargo.toml  crates/anno-source-local/Cargo.toml
COPY crates/anno-privacy-gateway/Cargo.toml crates/anno-privacy-gateway/Cargo.toml
COPY crates/anno-cli/Cargo.toml           crates/anno-cli/Cargo.toml
COPY crates/anno-eval/Cargo.toml          crates/anno-eval/Cargo.toml
COPY crates/anno/Cargo.toml               crates/anno/Cargo.toml

# Copy full source
COPY . .

# Build release binary.
# ORT's download-binaries feature fetches libonnxruntime.so at build time;
# copy-dylibs places it in target/release/ so we can find it below.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p anno-rag-bin \
    && mkdir -p /out \
    && cp target/release/anno-rag /out/anno-rag \
    && (find target/release -maxdepth 1 -name "libonnxruntime*.so*" -exec cp {} /out/ \; || true) \
    && (find target/release/build -name "libonnxruntime*.so*" -exec cp {} /out/ \; || true)

# ── Stage 2: Model download ───────────────────────────────────────────────────
FROM debian:bookworm-slim AS model-downloader

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/ /usr/local/bin/
RUN ldconfig /usr/local/lib 2>/dev/null || true

# Download E5 (~470 MB) + GLiNER2 (~500 MB) into /models.
# --dir /models makes anno-rag set data_dir=/ so models_cache()=/models.
ENV ANNO_RAG_VAULT_PASSPHRASE=docker-build-placeholder \
    LD_LIBRARY_PATH=/usr/local/bin
RUN anno-rag download-models --dir /models

# ── Stage 3: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        libtesseract5 \
        liblept5 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -ms /bin/sh -u 1000 anno

# Binary + ORT shared library
COPY --from=builder /out/ /usr/local/bin/
RUN ldconfig

# Baked models (~970 MB layer)
COPY --from=model-downloader /models /models

# Data volume: LanceDB index, vault.enc, processed outputs
VOLUME ["/data"]

# ── Environment ───────────────────────────────────────────────────────────────
# ANNO_RAG_VAULT_PASSPHRASE  REQUIRED at runtime — set in docker-compose.yml
# ANNO_DATA_DIR              Persistent data (index + vault)
# ANNO_MODELS_DIR            Points to baked models; no download at startup
ENV ANNO_DATA_DIR=/data \
    ANNO_MODELS_DIR=/models \
    LD_LIBRARY_PATH=/usr/local/bin

USER anno
WORKDIR /data

ENTRYPOINT ["anno-rag"]
CMD ["mcp"]
