# Phase C — `.mcpb` Claude Desktop Extension Design

**Date:** 2026-05-22
**Status:** Design approved
**Scope:** Claude Desktop Extension packaging and lazy MCP startup. No change to anno-rag's
retrieval, vault, or memory logic.

---

## 1. Goal

Ship `anno-rag` as a one-click Claude Desktop Extension (`.mcpb`) so users never need to edit
`claude_desktop_config.json` or open a terminal to get started. The install flow should work
without model weights pre-downloaded.

The extension must serve one audience: **Claude Desktop users** who want `anno-rag mcp` and
care nothing about Rust, cargo, or config files.

---

## 2. Non-Goals

- Do not change `anno-rag`'s MCP protocol, tools, or output format (except adding
  `download_models`).
- Do not bundle model weights in the `.mcpb` archive.
- Do not bundle `anno-privacy-gateway` (users who need it use the `.tar.gz` release).
- Do not add automatic model download at MCP server startup (would block past the
  60-second Claude Desktop timeout).
- Do not add code-signing, notarization, or an app store listing.

---

## 3. Architecture

### 3.1 Three platform-specific `.mcpb` files

The `.mcpb` format does not support architecture-level `platform_overrides` — only OS-level
(`darwin`, `win32`, `linux`). Because `x86_64-apple-darwin` and `aarch64-apple-darwin` are
both `"darwin"`, a single universal bundle cannot select the right binary at install time.

Three files are published per release:

| File | Platform target | Claude Desktop platform |
|---|---|---|
| `hacienda-<tag>-x86_64-pc-windows-msvc.mcpb` | `windows-latest` matrix job | `win32` |
| `hacienda-<tag>-x86_64-apple-darwin.mcpb` | `macos-13` matrix job | `darwin` |
| `hacienda-<tag>-aarch64-apple-darwin.mcpb` | `macos-14` matrix job | `darwin` |

Each `.mcpb` is a ZIP archive containing:

```
hacienda-<tag>-<target>.mcpb  (ZIP)
├── manifest.json
└── server/
    └── anno-rag[.exe]
```

No model weights, no gateway binary, no README.

### 3.2 `manifest.json`

One manifest template; per-platform differences are `server.entry_point`, `mcp_config.command`,
`mcp_config.platform_overrides`, and `compatibility.platforms`. The `${__dirname}` variable is
resolved by Claude Desktop to the extension's unpacked directory.

```json
{
  "$schema": "https://raw.githubusercontent.com/modelcontextprotocol/mcpb/main/manifest-schema.json",
  "manifest_version": "0.3",
  "name": "hacienda-anno-rag",
  "display_name": "Hacienda / anno-rag",
  "version": "<crate-version>",
  "description": "Local RAG memory for Claude. Ingest documents, search offline. First use: click 'Set up anno-rag' to download models (~970 MB).",
  "author": { "name": "Hacienda" },
  "license": "MIT OR Apache-2.0",
  "server": {
    "type": "binary",
    "entry_point": "server/anno-rag"
  },
  "mcp_config": {
    "command": "${__dirname}/server/anno-rag",
    "args": ["mcp"],
    "platform_overrides": {
      "win32": {
        "command": "${__dirname}/server/anno-rag.exe"
      }
    }
  },
  "compatibility": {
    "platforms": ["darwin"]
  },
  "user_config": [
    {
      "id": "ANNO_MODELS_DIR",
      "name": "Models directory",
      "description": "Path to downloaded model files. Leave blank and use the 'Set up anno-rag' prompt to download automatically.",
      "type": "directory",
      "required": false
    },
    {
      "id": "ANNO_RAG_VAULT_PASSPHRASE",
      "name": "Vault passphrase",
      "description": "Leave blank to use the OS keyring (recommended). Only set for headless or scripted use.",
      "type": "string",
      "sensitive": true,
      "required": false
    },
    {
      "id": "ANNO_NO_DOWNLOADS",
      "name": "Block model downloads",
      "description": "Prevent all network model downloads. Enable after models are downloaded.",
      "type": "boolean",
      "required": false,
      "default": false
    },
    {
      "id": "TESSERACT_PATH",
      "name": "Tesseract path (optional)",
      "description": "Path to the tesseract executable for OCR support. Leave blank to disable OCR.",
      "type": "file",
      "required": false
    }
  ]
}
```

Windows `.mcpb`: `compatibility.platforms` is `["win32"]`; the `platform_overrides` block is
retained for forward-compatibility but the root `command` is already the `.exe` path.

macOS Intel: `compatibility.platforms` is `["darwin"]`, `entry_point` and `command` use
`server/anno-rag` (no `.exe`).

macOS ARM: identical to Intel except the binary inside `server/` is the `aarch64-apple-darwin`
build.

### 3.3 Lazy MCP startup (runtime change)

**Problem:** `anno-rag mcp` currently calls `Pipeline::new` before starting the MCP server.
`Pipeline::new` downloads models from HuggingFace if they are not cached, which takes
2–15 minutes on a first run. Claude Desktop hard-times-out MCP server startup at 60 seconds,
so the server is marked failed before models finish downloading.

**Solution:** Defer `Pipeline::new` until the first tool call that needs it. The server starts
in under one second. If models are not available when the first tool is called, return a clear
error with instructions instead of timing out.

**Changes to `crates/anno-rag-mcp/src/lib.rs`:**

`AnnoRagServer` currently holds `pipeline: Arc<Pipeline>`. Change it to hold a lazy cell:

```rust
use tokio::sync::OnceCell;

pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Pipeline>>,
    cfg:      Arc<AnnoRagConfig>,
    key:      Arc<[u8; 32]>,          // derive_key() result, passed in at construction
    tool_router: ToolRouter<Self>,
}
```

**Changes to `crates/anno-rag-bin/src/main.rs`:**

`serve_stdio` currently receives a fully-built `Pipeline`. Change the `Mcp` branch to:

1. Auto-detect the default models path **before** spawning async tasks (still single-threaded
   at this point in `main()`), so `std::env::set_var` is safe:

```rust
Cmd::Mcp => {
    // Auto-detect default models path before any async threads start.
    // set_var is safe: called before serve_stdio_lazy spawns worker threads.
    if std::env::var("ANNO_MODELS_DIR").is_err() {
        let default_models = cfg.models_cache(); // ~/.anno-rag/models
        if default_models.join("e5-small").exists()
            && default_models.join("gliner2").exists()
        {
            std::env::set_var("ANNO_MODELS_DIR", &default_models);
        }
    }
    anno_rag_mcp::serve_stdio_lazy(cfg, key).await?;
}
```

2. Add `serve_stdio_lazy(cfg: AnnoRagConfig, key: [u8; 32])` to `anno-rag-mcp`. Keep
   the existing `serve_stdio(pipeline: Pipeline, cfg: AnnoRagConfig)` for test use.

Add a private helper `pipeline(&self) -> Result<&Pipeline>` that drives the `OnceCell`:

```rust
async fn pipeline(&self) -> anno_rag::error::Result<&Pipeline> {
    self.pipeline
        .get_or_try_init(|| async {
            // ANNO_MODELS_DIR is either user-supplied or set by the auto-detect
            // in main.rs before this point. If neither, refuse to download
            // (would block past the Claude Desktop 60 s timeout).
            if std::env::var("ANNO_MODELS_DIR").is_err() {
                return Err(anno_rag::error::Error::Other(
                    "Models not downloaded. Ask me to 'Set up anno-rag' \
                     or run 'anno-rag download-models' in a terminal, \
                     then restart the extension.".into(),
                ));
            }
            Pipeline::new((*self.cfg).clone(), *self.key).await
        })
        .await
}
```

Every existing tool handler replaces its `self.pipeline.operation()` calls with
`self.pipeline().await?.operation()`.

### 3.4 `download_models` MCP tool

Add to `AnnoRagServer` alongside the existing tools. This tool does not need the pipeline and
must not call `self.pipeline().await?`.

Behaviour:

1. Check if `cfg.models_cache()` already contains both model subdirectories
   (`e5-small/` and `gliner2/`). If yes, return:
   ```
   Models already present at <path>. Set ANNO_MODELS_DIR=<path> in extension
   settings (or leave blank — the default path is detected automatically).
   ```

2. If a download appears to be in progress (a `.download-lock` sentinel file exists in
   `cfg.models_cache()`), return:
   ```
   Download in progress. Retry in a few minutes.
   ```

3. Otherwise, write the sentinel file, then `tokio::task::spawn` the download:
   ```rust
   let cfg2 = self.cfg.clone();
   tokio::task::spawn(async move {
       let _ = anno_rag::download_models::download(&cfg2).await;
       let _ = std::fs::remove_file(cfg2.models_cache().join(".download-lock"));
   });
   ```
   Return immediately:
   ```
   Downloading anno-rag models to <path> (~970 MB).
   This runs in the background and takes 2–15 minutes depending on your
   connection. Ask me again in a few minutes — I will confirm when ready.
   ```

4. On the next call while the sentinel is present: return the "in progress" message.
5. On the next call after the sentinel is gone: the pipeline helper auto-detects models
   at the default path. Subsequent tool calls work without any config change.

No parameters needed. The tool always downloads to `cfg.models_cache()` (same path used
by `anno-rag download-models` CLI).

### 3.5 `.mcpb` packaging in `release.yml`

The cargo-dist `release.yml` has a `build-local-artifacts` job with a dynamic matrix
(3 platform entries: `windows-latest`, `macos-13`, `macos-14`). The `.mcpb` files are built
inline, at the end of each matrix job, after `dist build` completes.

**Steps added to `build-local-artifacts`:**

```yaml
- name: Package .mcpb extension
  shell: bash
  env:
    RELEASE_TAG: ${{ env.RELEASE_TAG }}
  run: |
    TARGET="${{ join(matrix.targets, '-') }}"
    VERSION="${RELEASE_TAG#v}"
    MCPB_NAME="hacienda-${RELEASE_TAG}-${TARGET}.mcpb"
    if [ "${{ runner.os }}" = "Windows" ]; then
      BIN="anno-rag.exe"
      PLATFORM="win32"
    else
      BIN="anno-rag"
      PLATFORM="darwin"
    fi
    mkdir -p mcpb-staging/server
    cp "target/${TARGET}/release/${BIN}" mcpb-staging/server/
    python3 -c "
    import json, sys
    m = json.load(open('scripts/release/mcpb-manifest-template.json'))
    m['version'] = sys.argv[1]
    m['compatibility']['platforms'] = [sys.argv[2]]
    m['server']['entry_point'] = 'server/' + sys.argv[3]
    m['mcp_config']['command'] = '\${__dirname}/server/' + sys.argv[3]
    json.dump(m, open('mcpb-staging/manifest.json', 'w'), indent=2)
    " "$VERSION" "$PLATFORM" "$BIN"
    cd mcpb-staging && zip -r "../${MCPB_NAME}" . && cd ..
    mkdir -p target/distrib
    cp "${MCPB_NAME}" target/distrib/
    echo "MCPB_NAME=${MCPB_NAME}" >> "$GITHUB_ENV"
```

The `scripts/release/mcpb-manifest-template.json` file is the manifest JSON from §3.2 with
`version`, `compatibility.platforms`, `server.entry_point`, and `mcp_config.command` left as
placeholders (the Python one-liner fills them in).

The `.mcpb` file is copied into `target/distrib/` so the existing
`actions/upload-artifact` step (which uploads `target/distrib/`) picks it up.

**Step added to `host` job** (after `dist host` completes, using the fetched artifacts):

```yaml
- name: Upload .mcpb files to GitHub Release
  shell: bash
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: |
    for f in target/distrib/*.mcpb; do
      [ -f "$f" ] && gh release upload "${RELEASE_TAG}" "$f" --clobber
    done
```

---

## 4. User install flow

```
1. User downloads hacienda-<tag>-aarch64-apple-darwin.mcpb from GitHub Releases.
2. Opens Claude Desktop → Settings → Extensions → drag .mcpb or "Install Extension…".
3. Claude Desktop shows: name, description, 4 config fields (all optional).
4. User clicks Install. Server starts in <1 second. Appears in Connectors.
5. User asks: "Set up anno-rag."
6. Claude calls download_models → spawns background download, returns immediately.
7. User waits 2–15 min, asks again.
8. Claude calls any anno-rag tool → OnceCell inits pipeline from auto-detected path → works.
```

No terminal. No config editing. No `.tar.gz` download required.

**Power-user shortcut** (users who already have the binary):

```sh
anno-rag download-models   # prints path, e.g. ~/.anno-rag/models
```

Then in extension settings: set **Models directory** to that path → pipeline loads from
disk on first tool call without touching the network.

---

## 5. Implementation tasks

| # | Task | Files |
|---|---|---|
| C1 | Add `mcpb-manifest-template.json` | `scripts/release/mcpb-manifest-template.json` |
| C2 | Lazy `OnceCell<Pipeline>` in MCP server | `crates/anno-rag-mcp/src/lib.rs` |
| C3 | `serve_stdio_lazy` entry point | `crates/anno-rag-mcp/src/lib.rs`, `crates/anno-rag-bin/src/main.rs` |
| C4 | `download_models` MCP tool | `crates/anno-rag-mcp/src/lib.rs` |
| C5 | `.mcpb` packaging steps in `release.yml` | `.github/workflows/release.yml` |
| C6 | Update `README-release.md` with `.mcpb` install path | `docs/release/README-release.md` |

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| `set_var` race in lazy init | `OnceCell` guarantees single initialization; the env var write happens before `Pipeline::new` and is never unset. |
| Background download silently fails | `.download-lock` sentinel is removed on both success and failure; on next `download_models` call the tool retries. |
| `target/distrib/` glob misses `.mcpb` | The `cp` step runs before the existing `upload-artifact` step; verified locally with `ls target/distrib/*.mcpb`. |
| `dist host` sees extra files and errors | `dist host` uploads files from its own manifest; extra files in `target/distrib/` are ignored by dist and uploaded separately by the new `gh release upload` step. |
| Platform mismatch (user downloads wrong arch) | Release notes list all three files with platform labels; `.mcpb` `compatibility.platforms` causes Claude Desktop to warn if the wrong file is installed on an incompatible OS. |

---

## 7. Validation

Before tagging:
- Locally pack the staging dir with `zip -r test.mcpb mcpb-staging/.` and drag to Claude Desktop.
- Verify all 4 config fields appear.
- Verify the server registers without `ANNO_MODELS_DIR` set.
- Verify `download_models` tool appears and returns without error.
- Verify the pipeline loads after `download-models` completes and `ANNO_MODELS_DIR` is set.

In CI (`workflow_dispatch` on a pre-release tag):
- Confirm three `.mcpb` files appear in the GitHub Release alongside the existing archives.
- Confirm `SHA256SUMS.txt` is generated for `.mcpb` files. Note: `dist build-global-artifacts`
  only checksums files from its own manifest. The `host` job's `gh release upload` step
  must also produce a supplemental checksum entry, or a separate `mcpb-checksums.txt` is
  uploaded alongside. The implementation plan should decide which.
