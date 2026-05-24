# GitHub Releases — OS binaries and Claude Desktop MCP distribution

**Date:** 2026-05-18  
**Status:** Design approved for planning  
**Scope:** Release/distribution workflow only. No runtime behavior change.

## 1. Goal

Ship Hacienda binaries through GitHub Releases so a Windows or macOS user can install `anno-rag` for Claude Desktop without building the whole Rust workspace.

The release must serve three audiences:

1. **Claude Desktop users** who need `anno-rag mcp` plus a copy-paste config.
2. **Rust/developer users** who still consume crates and source normally.
3. **Future extension users** who should eventually install a `.mcpb` Claude Desktop Extension.

## 2. Non-Goals

- Do not replace the existing crates.io `publish.yml` workflow.
- Do not package model weights in release assets.
- Do not build the `.mcpb` extension in the first implementation phase.
- Do not add installers, MSI/PKG/notarization, or auto-update in phase 1.
- Do not change `anno-rag` MCP protocol behavior.

## 3. Release Levels

### Level 1 — GitHub Release binaries

This is the first implementation target.

Publish archives for:

- Windows x64: `x86_64-pc-windows-msvc`
- macOS Intel: `x86_64-apple-darwin`
- macOS Apple Silicon: `aarch64-apple-darwin`

Each archive contains:

- `anno-rag` / `anno-rag.exe`
- `anno-privacy-gateway` / `anno-privacy-gateway.exe`
- `README.md`
- `LICENSE-MIT`
- `LICENSE-APACHE`
- `env.example`
- `examples/claude_desktop_config.windows.json`
- `examples/claude_desktop_config.macos.json`

Assets are named:

```text
hacienda-<tag>-x86_64-pc-windows-msvc.zip
hacienda-<tag>-x86_64-apple-darwin.tar.gz
hacienda-<tag>-aarch64-apple-darwin.tar.gz
SHA256SUMS.txt
```

`anno-rag` is the MCP binary. Claude Desktop runs it with `args: ["mcp"]`.

### Level 2 — Developer/crates distribution

The existing `.github/workflows/publish.yml` remains responsible for crates.io.

The release notes list crate versions separately because the workspace currently uses mixed versions:

- `anno = 0.10.0`
- `anno-cli = 0.10.0`
- `anno-rag = 0.2.0`
- `anno-rag-tabular = 0.1.0`
- `anno-privacy-gateway = 0.3.0`

The GitHub binary workflow and crates.io workflow may both run on `v*` tags, but they stay separate jobs/files so failures are isolated.

### Level 3 — Claude Desktop Extension

The `.mcpb` extension is deferred until the binary release pipeline is stable.

Future extension shape:

- Extension name: `Hacienda / anno-rag`
- Runtime command: `anno-rag mcp`
- Config fields:
  - `ANNO_RAG_VAULT_PASSPHRASE` as sensitive
  - `ANNO_NO_DOWNLOADS` as boolean
  - optional data directory once supported by the binary
  - optional `tesseract_path` for OCR

Preferred future packaging is one `.mcpb` per OS/arch. A lightweight extension that points to a manually installed binary remains possible, but gives a weaker user experience.

## 4. Workflow Design

Add `.github/workflows/release-binaries.yml`.

Trigger:

```yaml
on:
  push:
    tags:
      - "v*"
  workflow_dispatch:
```

Permissions:

```yaml
permissions:
  contents: write
```

Build matrix:

| OS | Target | Archive |
|---|---|---|
| `windows-latest` | `x86_64-pc-windows-msvc` | `.zip` |
| `macos-13` | `x86_64-apple-darwin` | `.tar.gz` |
| `macos-14` | `aarch64-apple-darwin` | `.tar.gz` |

Common setup:

- `actions/checkout@v4`
- Rust toolchain from `rust-toolchain.toml`
- `arduino/setup-protoc@v3`
- `Swatinem/rust-cache@v2`

Windows-specific setup mirrors existing CI:

```sh
RUSTFLAGS=-C target-feature=-crt-static
CFLAGS_x86_64-pc-windows-msvc=-MD
CXXFLAGS_x86_64-pc-windows-msvc=-MD
```

Build command:

```sh
cargo build --release -p anno-rag -p anno-privacy-gateway --target <target>
```

`anno-cli` is not part of phase 1 assets unless explicitly requested later. The release is focused on RAG/MCP and the HTTP privacy gateway.

## 5. Packaging

Packaging can be implemented inline in the workflow or via small scripts under `scripts/release/`.

Recommended files:

- `scripts/release/package-windows.ps1`
- `scripts/release/package-unix.sh`
- `scripts/release/checksums.sh`

The scripts should:

1. Create a clean `dist/<asset-name>/` folder.
2. Copy binaries and docs into it.
3. Copy Claude Desktop config examples from `docs/release/examples/`.
4. Compress the folder.
5. Emit SHA-256 checksums for every archive.

Checksums are uploaded as `SHA256SUMS.txt`.

## 6. Claude Desktop Examples

Add:

- `docs/release/examples/claude_desktop_config.windows.json`
- `docs/release/examples/claude_desktop_config.macos.json`

Windows example:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "C:\\\\path\\\\to\\\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_VAULT_PASSPHRASE": "change-me",
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

macOS example:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_RAG_VAULT_PASSPHRASE": "change-me",
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

The release README must state:

- Use absolute paths.
- Escape Windows backslashes in JSON.
- Restart Claude Desktop after editing config.
- Verify the server under Claude Desktop Connectors.
- Run `warmup_model` before offline use, or unset `ANNO_NO_DOWNLOADS` for first run.

## 7. Release Notes

Use GitHub generated release notes, then add a manual top block:

```text
## Install for Claude Desktop

1. Download the archive for your OS.
2. Extract it somewhere stable.
3. Copy the matching claude_desktop_config example.
4. Replace the binary path and passphrase.
5. Restart Claude Desktop.

## Assets

- hacienda-<tag>-x86_64-pc-windows-msvc.zip
- hacienda-<tag>-x86_64-apple-darwin.tar.gz
- hacienda-<tag>-aarch64-apple-darwin.tar.gz

## Checksums

See SHA256SUMS.txt.

## Known limitations

- Model weights are downloaded or loaded from local HuggingFace cache.
- Tesseract is optional and must be installed separately for OCR.
- `.mcpb` extension packaging is not included in this release.
```

## 8. Validation

Before tagging:

- `just pre-tag`
- `git diff --check`
- verify release example JSON parses
- verify `cargo metadata --no-deps` reports expected crate versions

In the release workflow:

- Build each target.
- Run each binary with `--help` when possible.
- Confirm each archive contains both binaries and Claude Desktop examples.
- Generate checksums.
- Upload all assets to the release.

No full model warmup is required in the release workflow. The release artifacts should stay small and not depend on HuggingFace availability.

## 9. Risks

| Risk | Mitigation |
|---|---|
| Windows MSVC CRT link mismatch | Reuse the existing CI `RUSTFLAGS` / `/MD` settings. |
| macOS Apple Silicon cross-build drift | Build natively on `macos-14`, not cross-compile. |
| Users edit invalid Claude Desktop JSON | Ship tested example JSON and document config paths. |
| Secrets copied into git | Examples use `change-me`; docs warn not to commit real passphrases. |
| Release and crates publish fail for different reasons | Keep binary release and crates publish as separate workflows. |
| First run downloads models unexpectedly | Document `warmup_model` and `ANNO_NO_DOWNLOADS`. |

## 10. Implementation Phases

### Phase A — Binary GitHub Releases

1. Add release example JSON files.
2. Add release packaging scripts.
3. Add `release-binaries.yml`.
4. Add release documentation.
5. Test workflow through `workflow_dispatch` on a prerelease tag or draft release.

### Phase B — Release Process Hardening

1. Add archive-content assertions.
2. Add checksum verification step.
3. Add release-note template.
4. Decide whether `anno-cli` belongs in the binary bundle.

### Phase C — Claude Desktop `.mcpb`

1. Design extension manifest.
2. Decide one extension per OS/arch vs external binary path.
3. Add sensitive config fields.
4. Package and test manual install through Claude Desktop Extensions.

## 11. Open Decisions

1. Should `anno-cli` be included in phase 1, or keep the bundle focused on `anno-rag` and `anno-privacy-gateway`?
2. Should GitHub Releases be created as draft first, or published immediately on tag push?
3. Should tags stay global `v*`, or move later to component tags like `anno-rag-v0.2.1`?

## 14. Vault Keyring Integration

### 14.1 Keyring lookup order

`anno-rag mcp` resolves the vault key in this order (matches `crates/anno-rag/src/vault.rs:derive_key`):

1. `ANNO_RAG_VAULT_PASSPHRASE` env var, if set and non-empty: Argon2id-derive a 32-byte key from the passphrase using a fixed app salt.
2. OS keyring lookup: service `anno-rag`, account `vault-key`. If present, hex-decode the stored value into a 32-byte key.
3. If neither produces a value: generate 32 random bytes via `OsRng`, hex-encode, store in the keyring under service `anno-rag` / account `vault-key`, and use that. (This is the default first-run behavior — Path A.)

Step 1 preserves the existing behavior so dev environments and CI keep working. See [ADR-0002](../../adrs/0002-encrypted-vault-aes-256-gcm-passphrase-or-keyring.md) for the underlying design rationale.

### 14.3 First-run passphrase population

Path A (auto-generate) is the existing default behavior — the engine generates 32 random bytes on first run, stores them in the keyring, and proceeds. The user never sees or types a passphrase. Best for paralegals.

Path B (user-supplied) is new in Phase E. The plugin's setup skill calls a new `anno_init_vault` MCP tool with `{passphrase: "..."}`. The engine derives the key via Argon2id, writes the derived key bytes to the keyring under service `anno-rag` / account `vault-key`, overwriting any auto-generated value. **The passphrase is never logged, never echoed in agent replies, never persisted outside the keyring entry.**

Both paths converge on the same keyring storage, so a user can start with Path A and switch to Path B (or vice versa via rotation in §14.4) without data loss.

