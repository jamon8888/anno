# Anno Real Installer — Phase 2: cargo-dist + Model Bundling + Notarization

**Date:** 2026-05-22
**Status:** Design approved
**Scope:** Distribution pipeline only. No runtime behaviour change except `ANNO_MODELS_DIR` loader preference.

---

## 1. Goal

Replace the Phase 1 archive-only release pipeline with real native installers on all four target platforms, so a non-developer can install `anno-rag` for Claude Desktop in one step with no build environment and no first-run HuggingFace download.

### Audiences

1. **Claude Desktop users** (primary) — download a platform installer, run it, paste one config line, restart Claude Desktop. Done.
2. **Developer / CI users** — `curl | sh` or PowerShell one-liner that installs to `~/.cargo/bin` or `~/bin`.
3. **macOS Homebrew users** — `brew install jamon8888/hacienda/anno-rag`.
4. **Linux system users** — `.deb` for Debian/Ubuntu, AppImage as universal fallback.

---

## 2. Non-Goals (Phase 2)

- Do not build a GUI application — anno-rag remains a stdio MCP server + CLI.
- Do not change MCP protocol behaviour.
- Do not bundle optional models (reranker, OCR engine) — those are on-consent post-install.
- Do not replace `publish.yml` crates.io workflow.
- Do not implement auto-update (Sparkle/WinSparkle) — future phase.

---

## 3. Tool Decision: cargo-dist

**cargo-dist v0.32.0** (2026-05-22) is the primary tool.

| Criterion | cargo-dist | cargo-wix + hand-roll |
|---|---|---|
| All 4 platforms from one config | ✅ | ❌ each platform custom |
| macOS notarization built-in | ✅ | ❌ script it yourself |
| Shell + PowerShell install scripts | ✅ auto-generated | ❌ manual |
| Homebrew tap | ✅ auto-generated | ❌ manual |
| Managed workflow (regenerable) | ✅ `cargo dist generate` | ❌ hand-maintained |
| WiX v4 .msi on Windows | ✅ built-in | ✅ (cargo-wix is WiX v3/v4) |
| AppImage + .deb on Linux | ✅ | ❌ need cargo-deb + appimagetool separately |
| Maintained | ✅ Axo.dev active | ⚠️ cargo-wix slower cadence |

**Verdict:** cargo-dist handles everything. cargo-wix is a fallback only if .msi customization (per-user vs. machine, license dialog, registry writes beyond PATH) is needed in a future phase.

---

## 4. Target Matrix

| Platform | Rust target | Runner | Formats |
|---|---|---|---|
| Windows x64 | `x86_64-pc-windows-msvc` | `windows-latest` | `.msi` + `.zip` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `macos-14` | `.dmg` + `.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | `macos-13` | `.dmg` + `.tar.gz` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `ubuntu-22.04` | `AppImage` + `.deb` + `.tar.gz` |

Linux was absent from Phase 1 — it is added here.

---

## 5. Packages Distributed

| Binary | Source crate | Included in all targets |
|---|---|---|
| `anno-rag` | `anno-rag-bin` | ✅ |
| `anno-privacy-gateway` | `anno-privacy-gateway` | ✅ |

`anno-cli` is excluded from Phase 2 (developer tool, install via `cargo install`).

---

## 6. cargo-dist Configuration

Add to `Cargo.toml` `[workspace]` section:

```toml
[workspace.metadata.dist]
cargo-dist-version       = "0.32.0"
ci                       = "github"
targets                  = [
  "x86_64-pc-windows-msvc",
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
]
installers               = ["msi", "shell", "powershell", "homebrew"]
precise-builds           = true       # respects rust-toolchain.toml (1.95)
pr-run-mode              = "plan"     # dry-run diff on PRs, actual release on v* tags
create-release           = true
package-selection        = ["anno-rag-bin", "anno-privacy-gateway"]
```

The `precise-builds = true` key tells cargo-dist to pass `--locked` and to honour `rust-toolchain.toml`, which pins Rust 1.95.

### MSVC CRT injection

cargo-dist generates `.github/workflows/release.yml`. After generation, patch the Windows build step to inject the CRT env vars documented in the gliner2-fastino finalization plan:

```yaml
# injected into the Windows build step under env:
RUSTFLAGS: "-C target-feature=-crt-static"
CFLAGS_x86_64-pc-windows-msvc: "-MD"
CXXFLAGS_x86_64-pc-windows-msvc: "-MD"
```

This must survive `cargo dist generate` regeneration — document in a `# ANNO-PATCH:` comment block so it is re-applied when the workflow is regenerated.

### Release features

Release builds should enable both opt-in features:

```toml
[workspace.metadata.dist.builds.global]
features = ["rerank", "embedded-ocr"]
```

If `embedded-ocr` requires Tesseract headers on the runner, gate it behind a separate build variant or document the apt/brew dependency.

---

## 7. Model Bundling (Phase 2B)

### 7.1 License gate

Before bundling, confirm redistribution is permitted:

| Model | HF repo | License | Verdict |
|---|---|---|---|
| NER | `SemplificaAI/gliner2-multi-v1-onnx` | Apache-2.0 | ✅ redistributable |
| Embedder | `intfloat/multilingual-e5-small` | MIT | ✅ redistributable |
| Reranker (optional) | `onnx-community/bge-reranker-v2-m3-ONNX` | Apache-2.0 | ✅ (not bundled in Phase 2B) |

### 7.2 Loader change — `ANNO_MODELS_DIR`

Add an `ANNO_MODELS_DIR` environment variable to both model loaders. When set and the expected files are present at that path, the loaders skip the HuggingFace download entirely.

**`crates/anno-rag/src/embed.rs`** — `Embedder::load`:
```rust
// Before calling hf_hub, check ANNO_MODELS_DIR.
if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
    let base = PathBuf::from(models_dir).join("multilingual-e5-small");
    let config   = base.join("config.json");
    let tokenizer = base.join("tokenizer.json");
    let weights  = base.join("model.safetensors");
    if config.exists() && tokenizer.exists() && weights.exists() {
        // load from local path, skip hf_hub entirely
        return Self::load_from_paths(cfg, &config, &tokenizer, &weights).await;
    }
}
// fall through to existing hf_hub path
```

**`crates/anno-rag/src/detect.rs`** — `Detector::get_or_init`:
```rust
if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
    let model_path = PathBuf::from(models_dir).join("gliner2-multi-v1-onnx");
    if model_path.exists() {
        return GLiNER2Fastino::from_local(&model_path)...;
    }
}
// fall through to from_pretrained HF path
```

`GLiNER2Fastino::from_local` already exists (used in tests); this is wiring, not new inference code.

### 7.3 CI model pre-download step

In the cargo-dist workflow, before packaging:

```yaml
- name: Pre-download models (bundle)
  env:
    HF_HUB_CACHE: ${{ runner.temp }}/hf-cache
  run: cargo run -p anno-rag-bin -- warmup_model
```

Then copy `${{ runner.temp }}/hf-cache` into the staging area. cargo-dist `extra-artifacts` can attach the `models/` directory to each platform's archive.

### 7.4 Installer sets ANNO_MODELS_DIR

Each installer type sets `ANNO_MODELS_DIR` at install time:
- **Windows .msi**: `<SetEnvironmentProperty>` in WiX (cargo-dist supports custom WiX fragments)
- **macOS .dmg**: post-install shell script writes `~/.anno-rag/env`
- **Linux .deb**: `/etc/environment.d/anno-rag.conf` or `~/.config/anno-rag/env`
- **Shell installer**: appends export to `~/.bashrc` / `~/.zshrc`

### 7.5 Size budget

| Model | Approx size |
|---|---|
| gliner2-multi-v1-onnx | ~320 MB |
| multilingual-e5-small (safetensors) | ~470 MB |
| Total bundled payload | ~790 MB |

This is large for GitHub Release assets as separate downloads but acceptable for .msi/.dmg installer payloads. The installer download will be ~790 MB — document this prominently.

Alternative: ship a "slim" installer (no models, ~30 MB) + a separate "models pack" archive. Users run `anno-rag download-models` post-install. This avoids the 790 MB first download for users who already have HF models cached.

**Recommended for Phase 2B**: slim installer by default, `anno-rag download-models` for offline bundling. Full bundled installer as an optional "offline" variant once the slim path is validated.

---

## 8. macOS Notarization (Phase 2C)

Without notarization, macOS Gatekeeper quarantines the binary on first launch. Users must right-click → Open → Trust. Acceptable for a beta, unacceptable for a GA release.

### 8.1 Requirements

- Apple Developer Program membership ($99/year)
- Developer ID Application certificate (codesigning)
- App-specific password or API key for notarytool

### 8.2 GitHub Secrets needed

| Secret | Purpose |
|---|---|
| `APPLE_CERTIFICATE` | Developer ID Application cert, base64-encoded p12 |
| `APPLE_CERTIFICATE_PASSWORD` | p12 passphrase |
| `APPLE_TEAM_ID` | 10-character team identifier |
| `APPLE_NOTARIZATION_KEY` | App Store Connect API key (.p8, base64) |
| `APPLE_NOTARIZATION_KEY_ID` | API key ID |
| `APPLE_NOTARIZATION_KEY_ISSUER_ID` | API key issuer UUID |

### 8.3 cargo-dist config

```toml
[workspace.metadata.dist]
macos-sign = true
```

cargo-dist injects `codesign` + `xcrun notarytool` steps automatically when the Apple secrets are present. If secrets are absent (e.g. fork CI), notarization is skipped and the binary is unsigned.

### 8.4 Phase gate

Phase 2C is gated on:
1. Apple Developer account created
2. Developer ID Application certificate issued and exported
3. GitHub secrets populated

Phase 2A and 2B ship without notarization, with a README note about right-click-Open.

---

## 9. Workflow Design

`cargo dist generate` replaces `release-binaries.yml` with a managed `.github/workflows/release.yml`. The old file is deleted.

Trigger: unchanged (`push: tags: v*` + `workflow_dispatch`).

Post-generation patches (must be re-applied after each `cargo dist generate`):
1. MSVC CRT env vars on the Windows build step (§6)
2. `HF_HUB_CACHE` model pre-download step before packaging (§7.3)
3. Gateway boot smoke using `scripts/release/smoke-gateway.{ps1,sh}` replacing any `--help` smoke

---

## 10. Shell / PowerShell Installers

cargo-dist auto-generates:
- `https://github.com/jamon8888/anno/releases/download/v<TAG>/anno-rag-installer.sh` — `curl | sh` for macOS/Linux, installs to `~/.cargo/bin` or `~/bin`
- `https://github.com/jamon8888/anno/releases/download/v<TAG>/anno-rag-installer.ps1` — `irm | iex` for Windows, installs to `%LOCALAPPDATA%\Programs\anno-rag`

These are the primary "developer install" path and should be the first option in the README install section.

---

## 11. Homebrew Tap

cargo-dist generates a Homebrew formula committed to a tap repository. Recommend creating `jamon8888/homebrew-hacienda` (or `jamon8888/homebrew-anno`) before Phase 2A ships.

Config addition:
```toml
[workspace.metadata.dist]
tap = "jamon8888/homebrew-hacienda"
```

Users install with: `brew install jamon8888/hacienda/anno-rag`.

---

## 12. Claude Desktop Config Examples (updated)

The Phase 1 examples in `docs/release/examples/` remain valid. Update them to reference the installer-installed paths:

**Windows (after .msi install):**
```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "%LOCALAPPDATA%\\Programs\\anno-rag\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_VAULT_PASSPHRASE": "change-me"
      }
    }
  }
}
```

(No `ANNO_NO_DOWNLOADS` needed after Phase 2B because models are bundled.)

---

## 13. Validation

Before tagging:
- `cargo dist plan` — dry-run, confirms matrix and asset names
- `cargo dist build --artifacts=local` — local test build
- Verify archive contains both binaries, Claude Desktop examples, and `models/` directory (Phase 2B+)
- Verify `.msi` installs and uninstalls cleanly on Windows (manual)
- Verify `.dmg` mounts and runs `anno-rag --help` on macOS (manual)
- Verify AppImage runs on Ubuntu (manual or Docker)

---

## 14. Implementation Phases

### Phase 2A — cargo-dist migration + Linux
- Add `[workspace.metadata.dist]` block to `Cargo.toml`
- Run `cargo dist init` and `cargo dist generate`
- Delete old `release-binaries.yml`
- Apply MSVC CRT patch and gateway smoke patch
- Add Linux x64 target
- Validate with `cargo dist plan`

### Phase 2B — Slim installer + `anno-rag download-models`
- Add `ANNO_MODELS_DIR` env var support to `embed.rs` and `detect.rs`
- Add `anno-rag download-models` CLI subcommand (wraps existing warmup + sets local dir)
- Shell/PowerShell installer appends `ANNO_MODELS_DIR` env var post-install
- Update Claude Desktop README to remove `ANNO_NO_DOWNLOADS` requirement

### Phase 2C — macOS notarization
- Gated on Apple Developer account
- Populate GitHub secrets
- Add `macos-sign = true` to cargo-dist config
- Verify notarization via `spctl -a -v ./anno-rag`

---

## 15. Risks

| Risk | Mitigation |
|---|---|
| `cargo dist generate` overwrites MSVC CRT patch | Document patch; mark with `# ANNO-PATCH:` comment; script re-application |
| Model pre-download step adds 5–10 min to CI | Cache `HF_HUB_CACHE` with `actions/cache` keyed on model IDs |
| 790 MB installer download size | Ship slim installer by default; full bundled as optional variant |
| Apple Developer account not available | Phase 2C gated; ship Phase 2A/B with unsigned .dmg + right-click note |
| `embedded-ocr` feature needs Tesseract on runner | Document apt/brew dep; gate behind a separate CI matrix row if needed |
| `ort` crate links against `onnxruntime.dll` on Windows | No extra work — `ort` defaults to static linking when no `load-dynamic` feature is set; confirmed in Cargo.toml |
| Homebrew tap repo doesn't exist | Create `jamon8888/homebrew-hacienda` before Phase 2A tag |
