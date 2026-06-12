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

# Install Rust via rustup (repo pins 1.95 in rust-toolchain.toml)
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

# ── Stage 2: Model download ───────────────────────────────────────────────────
FROM ubuntu:24.04 AS model-downloader

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 libtesseract5 liblept5 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/ /usr/local/bin/
RUN ldconfig

# Download E5 (~470 MB) + GLiNER2 (~500 MB) into /models.
# --dir /models → data_dir=/ so models_cache()=/models.
ENV ANNO_RAG_VAULT_PASSPHRASE=docker-build-placeholder \
    LD_LIBRARY_PATH=/usr/local/bin
RUN anno-rag download-models --dir /models

# ── Stage 3: Runtime ──────────────────────────────────────────────────────────
FROM ubuntu:24.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        libtesseract5t64 \
        libleptonica6 \
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
