# Anno-RAG Tauri Setup Assistant + Unified CI Plan

> **Plan 2/3 — Installer & CI**
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Tauri-based installer that auto-configures Claude Desktop, downloads models, and inits the vault — with no terminal needed — plus a unified CI matrix that builds and packages all five platform variants (Windows CPU, Windows CUDA, macOS arm64 CPU, macOS arm64 Metal, macOS x86_64 CPU).

**Prerequisites:** Plan 1 merged. `anno-rag-bin` produces a working ONNX binary on all targets.

---

## Architecture

```
anno/
├── crates/
│   └── anno-rag-setup/          # NEW — Tauri app
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       ├── src/
│       │   ├── main.rs          # Tauri entry — setup wizard state machine
│       │   └── commands.rs      # tauri::command — patch_config, init_vault, download_models
│       └── ui/
│           └── index.html       # Simple progress wizard (vanilla JS, no framework)
└── .github/
    └── workflows/
        └── release-all.yml      # NEW — replaces release-binaries.yml + release-accelerated.yml
```

**Tauri role:** installer only — not the MCP runtime. The setup assistant runs once, configures Claude Desktop, then exits. The `anno-rag` binary is the MCP server; Tauri is the GUI wrapper for the first-run experience.

---

## File Map

| File | Change |
|------|--------|
| `crates/anno-rag-setup/Cargo.toml` | NEW — Tauri app crate |
| `crates/anno-rag-setup/tauri.conf.json` | NEW — Tauri config (bundle id, icons, targets) |
| `crates/anno-rag-setup/src/main.rs` | NEW — setup wizard entry point |
| `crates/anno-rag-setup/src/commands.rs` | NEW — `patch_claude_config`, `download_models_progress`, `init_vault_keyring` |
| `crates/anno-rag-setup/ui/index.html` | NEW — 4-step progress wizard UI |
| `.github/workflows/release-all.yml` | NEW — unified CI matrix (5 jobs, Tauri build + binary artifact) |
| `.github/workflows/release-binaries.yml` | REMOVE (replaced by release-all.yml) |
| `.github/workflows/release-accelerated.yml` | REMOVE (replaced by release-all.yml) |
| `Cargo.toml` (workspace) | Add `anno-rag-setup` to `members` |

---

### Task 1: Workspace — add anno-rag-setup crate

- [ ] **Step 1: Add to workspace members**

In root `Cargo.toml`, in `[workspace] members`, append `"crates/anno-rag-setup"`.

- [ ] **Step 2: Create crate directory structure**

```powershell
New-Item -ItemType Directory -Force crates/anno-rag-setup/src
New-Item -ItemType Directory -Force crates/anno-rag-setup/ui
```

- [ ] **Step 3: Write Cargo.toml**

```toml
[package]
name = "anno-rag-setup"
version.workspace = true
edition.workspace = true
rust-edition.workspace = true
description = "Anno-RAG Tauri setup assistant — one-click installer for lawyers"

[dependencies]
anno-rag = { path = "../anno-rag", default-features = false }
tauri = { version = "2", features = ["protocol-asset"] }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["full"] }
anyhow = { workspace = true }
dirs = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

[build-dependencies]
tauri-build = { version = "2", features = [] }

[[bin]]
name = "anno-rag-setup"
path = "src/main.rs"
```

- [ ] **Step 4: Verify workspace check**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo check -p anno-rag-setup 2>&1 | tail -10
```

Expected: `Finished` (no Tauri code yet — just an empty main.rs with `fn main() {}`).

- [ ] **Step 5: Commit**

```powershell
git add Cargo.toml crates/anno-rag-setup/
git commit -m "chore(workspace): add anno-rag-setup Tauri crate scaffold"
```

---

### Task 2: Tauri commands — patch_claude_config, download_models_progress, init_vault_keyring

**Files:**
- Write: `crates/anno-rag-setup/src/commands.rs`
- Write: `crates/anno-rag-setup/src/main.rs`

- [ ] **Step 1: Write `commands.rs`**

```rust
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

/// State returned to the UI after each wizard step.
#[derive(Debug, Serialize)]
pub struct StepResult {
    pub ok: bool,
    pub message: String,
}

/// Progress event sent over the download channel.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub pct: u8,
    pub current_file: String,
    pub downloaded_mb: f32,
    pub total_mb: f32,
}

/// Patch claude_desktop_config.json to register anno-rag MCP server.
///
/// Writes the anno-rag entry atomically, preserving existing mcpServers.
/// The binary path is the resolved path of the current executable (anno-rag-setup)
/// stripped to its directory, then `anno-rag` binary.
#[tauri::command]
pub async fn patch_claude_config() -> StepResult {
    match do_patch_claude_config() {
        Ok(path) => StepResult {
            ok: true,
            message: format!("Claude Desktop config updated: {}", path.display()),
        },
        Err(e) => StepResult {
            ok: false,
            message: format!("Failed to patch config: {e:#}"),
        },
    }
}

fn claude_config_path() -> Option<PathBuf> {
    // macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
    // Windows: %APPDATA%\Claude\claude_desktop_config.json
    dirs::config_dir().map(|p| p.join("Claude").join("claude_desktop_config.json"))
}

fn anno_rag_binary_path() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("current_exe")?;
    let dir = exe.parent().context("exe parent")?;
    let bin = if cfg!(target_os = "windows") {
        dir.join("anno-rag.exe")
    } else {
        dir.join("anno-rag")
    };
    Ok(bin)
}

fn do_patch_claude_config() -> anyhow::Result<PathBuf> {
    let config_path = claude_config_path().context("cannot locate Claude config dir")?;
    let binary = anno_rag_binary_path()?;

    // Read existing config or start fresh.
    let mut config: serde_json::Value = if config_path.exists() {
        let raw = std::fs::read_to_string(&config_path).context("read config")?;
        serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Inject anno-rag entry under mcpServers.
    config
        .as_object_mut()
        .context("config is not an object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("mcpServers is not an object")?
        .insert(
            "anno-rag".to_string(),
            serde_json::json!({
                "command": binary.to_string_lossy(),
                "args": ["mcp"]
            }),
        );

    // Atomic write: write to temp file then rename.
    let parent = config_path.parent().context("config parent")?;
    std::fs::create_dir_all(parent).context("create config dir")?;
    let tmp = config_path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&config)?).context("write tmp")?;
    std::fs::rename(&tmp, &config_path).context("rename")?;

    Ok(config_path)
}

/// Download the three anno-rag models with progress events sent to the UI.
#[tauri::command]
pub async fn download_models_progress(
    on_progress: Channel<DownloadProgress>,
) -> StepResult {
    // Emit a fake-start so the UI immediately shows something.
    let _ = on_progress.send(DownloadProgress {
        pct: 0,
        current_file: "Connecting…".to_string(),
        downloaded_mb: 0.0,
        total_mb: 545.0,
    });

    let cfg = anno_rag::config::AnnoRagConfig::default();
    match anno_rag::download_models::download(&cfg).await {
        Ok(()) => {
            let _ = on_progress.send(DownloadProgress {
                pct: 100,
                current_file: "Done".to_string(),
                downloaded_mb: 545.0,
                total_mb: 545.0,
            });
            StepResult { ok: true, message: "Models ready (~545 MB)".to_string() }
        }
        Err(e) => StepResult {
            ok: false,
            message: format!("Download failed: {e:#}"),
        },
    }
}

/// Initialise the vault keyring entry (generate key, store in OS keyring).
///
/// On Windows this uses DPAPI via the `keyring` crate.
/// On macOS this uses the Keychain.
#[tauri::command]
pub async fn init_vault_keyring() -> StepResult {
    let cfg = anno_rag::config::AnnoRagConfig::default();
    match anno_rag::vault::init_keyring(&cfg) {
        Ok(()) => StepResult {
            ok: true,
            message: "Vault key stored in OS keyring.".to_string(),
        },
        Err(e) => StepResult {
            ok: false,
            message: format!("Vault init failed: {e:#}"),
        },
    }
}
```

- [ ] **Step 2: Write `main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tracing_subscriber::fmt::init();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::patch_claude_config,
            commands::download_models_progress,
            commands::init_vault_keyring,
        ])
        .run(tauri::generate_context!())
        .expect("anno-rag-setup: tauri error");
}
```

- [ ] **Step 3: Write `tauri.conf.json`**

```json
{
  "productName": "Anno-RAG Setup",
  "version": "0.14.0",
  "identifier": "com.anno-rag.setup",
  "build": {
    "frontendDist": "ui"
  },
  "app": {
    "windows": [
      {
        "title": "Anno-RAG — Installation",
        "width": 600,
        "height": 480,
        "resizable": false,
        "center": true
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/icon.png"],
    "windows": {
      "wix": {
        "language": "fr-FR"
      }
    }
  }
}
```

- [ ] **Step 4: Write the setup wizard UI (`ui/index.html`)**

4-step minimal wizard: (1) Bienvenue, (2) Configuration Claude Desktop, (3) Téléchargement modèles, (4) Prêt.

```html
<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Anno-RAG — Installation</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: system-ui, sans-serif; background: #f8f9fa; color: #212529; display: flex; align-items: center; justify-content: center; height: 100vh; }
    .card { background: white; border-radius: 12px; box-shadow: 0 4px 24px rgba(0,0,0,.08); padding: 40px; width: 520px; }
    h1 { font-size: 1.5rem; margin-bottom: 8px; }
    .subtitle { color: #6c757d; margin-bottom: 32px; font-size: .95rem; }
    .step { display: none; }
    .step.active { display: block; }
    .btn { background: #1a56db; color: white; border: none; border-radius: 8px; padding: 12px 28px; font-size: 1rem; cursor: pointer; margin-top: 24px; }
    .btn:disabled { background: #adb5bd; cursor: not-allowed; }
    .progress-bar { background: #e9ecef; border-radius: 6px; height: 12px; margin: 16px 0; }
    .progress-fill { background: #1a56db; border-radius: 6px; height: 100%; transition: width .3s ease; }
    .status { color: #6c757d; font-size: .88rem; min-height: 20px; }
    .ok { color: #198754; font-weight: 600; }
    .err { color: #dc3545; font-weight: 600; }
    .steps-nav { display: flex; gap: 8px; margin-bottom: 32px; }
    .dot { width: 10px; height: 10px; border-radius: 50%; background: #dee2e6; }
    .dot.done { background: #1a56db; }
    .dot.current { background: #1a56db; box-shadow: 0 0 0 3px rgba(26,86,219,.2); }
  </style>
</head>
<body>
<div class="card">
  <div class="steps-nav" id="nav">
    <div class="dot current" id="d0"></div>
    <div class="dot" id="d1"></div>
    <div class="dot" id="d2"></div>
    <div class="dot" id="d3"></div>
  </div>

  <!-- Step 0: Welcome -->
  <div class="step active" id="s0">
    <h1>Bienvenue dans Anno-RAG</h1>
    <p class="subtitle">Cet assistant va configurer Anno-RAG pour Claude Desktop en 3 étapes rapides.<br><br>Durée estimée : 5–10 min (téléchargement inclus).</p>
    <button class="btn" onclick="gotoStep(1)">Commencer</button>
  </div>

  <!-- Step 1: Patch Claude config -->
  <div class="step" id="s1">
    <h1>Configuration Claude Desktop</h1>
    <p class="subtitle">Anno-RAG sera ajouté automatiquement à votre fichier de configuration Claude Desktop.</p>
    <div class="status" id="st1">En attente…</div>
    <button class="btn" id="btn1" onclick="runPatchConfig()">Configurer</button>
  </div>

  <!-- Step 2: Download models -->
  <div class="step" id="s2">
    <h1>Téléchargement des modèles IA</h1>
    <p class="subtitle">3 modèles (~545 MB) vont être téléchargés depuis Hugging Face.<br>Une connexion internet est requise.</p>
    <div class="progress-bar"><div class="progress-fill" id="pf" style="width:0%"></div></div>
    <div class="status" id="st2">En attente…</div>
    <button class="btn" id="btn2" onclick="runDownload()">Télécharger</button>
  </div>

  <!-- Step 3: Vault -->
  <div class="step" id="s3">
    <h1>Initialisation du coffre-fort</h1>
    <p class="subtitle">Une clé de chiffrement sécurisée est générée et stockée dans le trousseau de votre système d'exploitation.</p>
    <div class="status" id="st3">En attente…</div>
    <button class="btn" id="btn3" onclick="runVault()">Initialiser</button>
  </div>

  <!-- Step 4: Done -->
  <div class="step" id="s4">
    <h1 class="ok">✓ Anno-RAG est prêt</h1>
    <p class="subtitle" style="margin-top:16px">Redémarrez Claude Desktop pour activer le serveur MCP.<br><br>Vous pouvez maintenant fermer cette fenêtre.</p>
    <button class="btn" onclick="window.__TAURI__.window.getCurrentWindow().close()">Fermer</button>
  </div>
</div>

<script type="module">
import { invoke } from 'https://unpkg.com/@tauri-apps/api@2/core';
import { Channel } from 'https://unpkg.com/@tauri-apps/api@2/core';

let currentStep = 0;

window.gotoStep = function(n) {
  document.getElementById('s' + currentStep).classList.remove('active');
  document.getElementById('d' + currentStep).classList.replace('current', 'done');
  currentStep = n;
  document.getElementById('s' + n).classList.add('active');
  if (n < 4) {
    document.getElementById('d' + n).classList.add('current');
  }
};

window.runPatchConfig = async function() {
  const btn = document.getElementById('btn1');
  const st = document.getElementById('st1');
  btn.disabled = true;
  st.textContent = 'Configuration en cours…';
  const r = await invoke('patch_claude_config');
  if (r.ok) {
    st.innerHTML = '<span class="ok">✓ ' + r.message + '</span>';
    setTimeout(() => gotoStep(2), 1200);
  } else {
    st.innerHTML = '<span class="err">✗ ' + r.message + '</span>';
    btn.disabled = false;
  }
};

window.runDownload = async function() {
  const btn = document.getElementById('btn2');
  const st = document.getElementById('st2');
  const pf = document.getElementById('pf');
  btn.disabled = true;

  const ch = new Channel();
  ch.onmessage = (ev) => {
    pf.style.width = ev.pct + '%';
    st.textContent = ev.current_file + ' — ' + ev.downloaded_mb.toFixed(0) + ' / ' + ev.total_mb.toFixed(0) + ' MB';
  };

  const r = await invoke('download_models_progress', { onProgress: ch });
  if (r.ok) {
    st.innerHTML = '<span class="ok">✓ ' + r.message + '</span>';
    setTimeout(() => gotoStep(3), 1200);
  } else {
    st.innerHTML = '<span class="err">✗ ' + r.message + '</span>';
    btn.disabled = false;
  }
};

window.runVault = async function() {
  const btn = document.getElementById('btn3');
  const st = document.getElementById('st3');
  btn.disabled = true;
  st.textContent = 'Génération de la clé…';
  const r = await invoke('init_vault_keyring');
  if (r.ok) {
    st.innerHTML = '<span class="ok">✓ ' + r.message + '</span>';
    setTimeout(() => gotoStep(4), 1200);
  } else {
    st.innerHTML = '<span class="err">✗ ' + r.message + '</span>';
    btn.disabled = false;
  }
};
</script>
</body>
</html>
```

- [ ] **Step 5: Check compile**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo check -p anno-rag-setup 2>&1 | tail -15
```

Expected: `Finished`. Resolve any missing imports (vault::init_keyring, download_models::download).

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag-setup/
git commit -m "feat(setup): Tauri setup assistant — patch_claude_config, download_models, init_vault_keyring"
```

---

### Task 3: Unified CI workflow `release-all.yml`

**Files:**
- Write: `.github/workflows/release-all.yml`
- Remove: `.github/workflows/release-binaries.yml`
- Remove: `.github/workflows/release-accelerated.yml`

- [ ] **Step 1: Write `release-all.yml`**

```yaml
name: Release — All Targets

on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+'
  workflow_dispatch:
    inputs:
      tag:
        description: 'Release tag (e.g. v0.14.0)'
        required: true

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          # Windows CPU (ONNX only)
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            features: ""
            variant: cpu
            tauri_target: windows
            artifact_ext: msi

          # Windows CUDA (GPU, self-hosted runner required)
          - os: [self-hosted, windows, cuda]
            target: x86_64-pc-windows-msvc
            features: gpu-cuda
            variant: cuda
            tauri_target: windows
            artifact_ext: msi

          # macOS arm64 CPU
          - os: macos-14
            target: aarch64-apple-darwin
            features: ""
            variant: cpu
            tauri_target: macos
            artifact_ext: dmg

          # macOS arm64 Metal (GPU)
          - os: macos-14
            target: aarch64-apple-darwin
            features: gpu-metal
            variant: metal
            tauri_target: macos
            artifact_ext: dmg

          # macOS x86_64 CPU (Intel)
          - os: macos-13
            target: x86_64-apple-darwin
            features: ""
            variant: cpu
            tauri_target: macos
            artifact_ext: dmg

    runs-on: ${{ matrix.os }}
    name: ${{ matrix.target }}-${{ matrix.variant }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Setup sccache
        uses: mozilla-actions/sccache-action@v0.0.5

      - name: Setup Node.js (Tauri CLI)
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Install Tauri CLI
        run: npm install -g @tauri-apps/cli@^2

      - name: Cache Cargo registry
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Build anno-rag-bin
        env:
          RUSTC_WRAPPER: sccache
          CARGO_INCREMENTAL: "0"
          SCCACHE_GHA_ENABLED: "true"
        run: >
          cargo build --release
          -p anno-rag-bin
          --target ${{ matrix.target }}
          ${{ matrix.features != '' && format('--features {0}', matrix.features) || '' }}

      - name: Smoke test binary
        run: |
          ./target/${{ matrix.target }}/release/anno-rag${{ runner.os == 'Windows' && '.exe' || '' }} --help

      - name: Build Tauri installer
        env:
          RUSTC_WRAPPER: sccache
          CARGO_INCREMENTAL: "0"
          SCCACHE_GHA_ENABLED: "true"
        run: >
          tauri build
          --target ${{ matrix.target }}
          ${{ matrix.features != '' && format('--features {0}', matrix.features) || '' }}
          --config '{"version": "${{ github.ref_name || inputs.tag }}"}'
        working-directory: crates/anno-rag-setup

      - name: Rename artifact
        shell: bash
        run: |
          VERSION=${{ github.ref_name || inputs.tag }}
          SRC_DIR=crates/anno-rag-setup/target/${{ matrix.target }}/release/bundle/${{ matrix.tauri_target }}
          DEST=anno-rag-${VERSION}-${{ matrix.target }}-${{ matrix.variant }}.${{ matrix.artifact_ext }}
          mv ${SRC_DIR}/*.${artifact_ext} ${DEST}
          echo "ARTIFACT=${DEST}" >> $GITHUB_ENV
        env:
          artifact_ext: ${{ matrix.artifact_ext }}

      - name: Upload installer artifact
        uses: actions/upload-artifact@v4
        with:
          name: anno-rag-${{ matrix.target }}-${{ matrix.variant }}
          path: ${{ env.ARTIFACT }}
          retention-days: 30

      - name: Upload binary (zip/tar.gz)
        shell: bash
        run: |
          VERSION=${{ github.ref_name || inputs.tag }}
          BIN=./target/${{ matrix.target }}/release/anno-rag${{ runner.os == 'Windows' && '.exe' || '' }}
          if [[ "${{ runner.os }}" == "Windows" ]]; then
            zip anno-rag-${VERSION}-${{ matrix.target }}-${{ matrix.variant }}.zip ${BIN}
            echo "BIN_ARTIFACT=anno-rag-${VERSION}-${{ matrix.target }}-${{ matrix.variant }}.zip" >> $GITHUB_ENV
          else
            tar -czf anno-rag-${VERSION}-${{ matrix.target }}-${{ matrix.variant }}.tar.gz ${BIN}
            echo "BIN_ARTIFACT=anno-rag-${VERSION}-${{ matrix.target }}-${{ matrix.variant }}.tar.gz" >> $GITHUB_ENV
          fi

      - name: Upload binary artifact
        uses: actions/upload-artifact@v4
        with:
          name: anno-rag-bin-${{ matrix.target }}-${{ matrix.variant }}
          path: ${{ env.BIN_ARTIFACT }}
          retention-days: 30

  release:
    needs: build
    runs-on: ubuntu-latest
    if: startsWith(github.ref, 'refs/tags/')
    permissions:
      contents: write
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            anno-rag-*.msi
            anno-rag-*.dmg
            anno-rag-*.zip
            anno-rag-*.tar.gz
          generate_release_notes: true
```

- [ ] **Step 2: Remove old workflow files**

```powershell
Remove-Item .github/workflows/release-binaries.yml
Remove-Item .github/workflows/release-accelerated.yml
```

- [ ] **Step 3: Validate YAML syntax**

```powershell
python -c "import yaml; yaml.safe_load(open('.github/workflows/release-all.yml'))" 2>&1
```

Expected: no output (no errors).

- [ ] **Step 4: Commit**

```powershell
git add .github/workflows/release-all.yml
git rm .github/workflows/release-binaries.yml .github/workflows/release-accelerated.yml
git commit -m "ci: unified release-all.yml — 5-target matrix (Windows CPU/CUDA, macOS arm64 CPU/Metal, x86_64 CPU) + Tauri build"
```

---

### Task 4: Integration test + PR

- [ ] **Step 1: Verify workspace builds clean**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-setup -Mode check
```

Expected: `Finished`

- [ ] **Step 2: Manual smoke — patch config**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo run -p anno-rag-setup -- 2>&1 | head -5
```

Expected: Tauri window opens (or `Finished` if headless). Config patch can be tested with a one-off invocation if needed.

- [ ] **Step 3: PR**

```powershell
git push origin feat/tauri-ci
gh pr create --title "feat: Tauri setup assistant + unified CI matrix (5 targets)" --body "Plan 2/3 — Anno-RAG Installer & CI

## Changes
- New crate anno-rag-setup (Tauri): patches claude_desktop_config.json, downloads 3 models, inits vault keyring
- 4-step wizard UI (vanilla HTML, no framework)
- Unified release-all.yml: 5 jobs (Windows CPU/CUDA, macOS arm64 CPU/Metal, x86_64 CPU)
- Produces .msi + .zip on Windows, .dmg + .tar.gz on macOS
- Removes release-binaries.yml + release-accelerated.yml

## Test plan
- [ ] cargo check -p anno-rag-setup passes
- [ ] Tauri window opens on Windows, shows 4-step wizard
- [ ] patch_claude_config creates/updates claude_desktop_config.json
- [ ] init_vault_keyring stores key without ANNO_RAG_VAULT_PASSPHRASE
- [ ] CI YAML passes yaml lint
- [ ] CI matrix triggers on tag push (dry-run with workflow_dispatch)"
```

---

## Self-Review

- ✅ Tauri role: installer only, not runtime — anno-rag binary stays the MCP server
- ✅ `patch_claude_config`: atomic write (tmp + rename), preserves existing mcpServers, no overwrite of other entries
- ✅ Cross-platform: `dirs::config_dir()` resolves `%APPDATA%/Claude` on Windows, `~/Library/Application Support/Claude` on macOS
- ✅ 5 CI jobs: all platform/variant combinations from spec
- ✅ Artifact naming: `anno-rag-{version}-{target}-{variant}.{ext}`
- ✅ sccache + cargo cache in every CI job
- ✅ Old workflows removed to avoid duplicate triggers
- ⚠️ `vault::init_keyring` public function must exist in `anno-rag` crate — check before running Task 2 Step 5. If absent, expose it or call the internal equivalent.
- ⚠️ Tauri 2.x requires `build.rs` boilerplate (`tauri-build`) — add if `cargo check` fails with missing context macro.
