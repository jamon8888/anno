# Installation

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin
Language: Bilingual

Install Hacienda from the GitHub release when you want a ready binary, or build
from source when you are developing the Rust workspace.

Installer Hacienda depuis la release GitHub est le chemin recommande pour les
tests utilisateur. Compiler depuis les sources est reserve aux developpeurs et
integrateurs.

## Release Install

Release URL:
[v0.11.0-rc.11](https://github.com/jamon8888/anno/releases/tag/v0.11.0-rc.11)

Download the archive for your platform and `SHA256SUMS.txt`:

| Platform | Asset |
|---|---|
| Windows x64 | `hacienda-v0.11.0-rc.11-x86_64-pc-windows-msvc.zip` |
| macOS Apple Silicon | `hacienda-v0.11.0-rc.11-aarch64-apple-darwin.tar.gz` |
| Checksums | `SHA256SUMS.txt` |

### Windows Checksum

Run this from the folder that contains the downloaded files:

```powershell
$asset = "hacienda-v0.11.0-rc.11-x86_64-pc-windows-msvc.zip"
$expected = (Select-String -Path .\SHA256SUMS.txt -SimpleMatch $asset).Line.Split()[0]
$actual = (Get-FileHash -Algorithm SHA256 .\$asset).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Checksum mismatch for $asset" }
Expand-Archive -Path .\$asset -DestinationPath "$HOME\Tools\hacienda-v0.11.0-rc.11" -Force
```

### macOS Checksum

Run this from the folder that contains the downloaded files:

```bash
asset="hacienda-v0.11.0-rc.11-aarch64-apple-darwin.tar.gz"
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
mkdir -p "$HOME/Tools/hacienda-v0.11.0-rc.11"
tar -xzf "$asset" -C "$HOME/Tools/hacienda-v0.11.0-rc.11"
```

If your macOS `shasum` does not support `--ignore-missing`, compare manually:

```bash
expected="$(grep "$asset" SHA256SUMS.txt | awk '{print $1}')"
actual="$(shasum -a 256 "$asset" | awk '{print $1}')"
test "$actual" = "$expected"
```

## Source Build

Use the source build path when editing the repo or testing an unreleased change.

```powershell
cargo build --release -p anno-rag-bin
```

The binary is produced as `anno-rag` (`anno-rag.exe` on Windows). Confirm the
exact command surface with:

```powershell
.\target\release\anno-rag.exe --help
```

On macOS or Linux:

```bash
cargo build --release -p anno-rag-bin
./target/release/anno-rag --help
```

## Next Step

Configure Claude Desktop or Cowork through the local MCP server:
[Claude Desktop And Cowork Setup](claude-desktop-cowork.md).
