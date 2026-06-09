#!/usr/bin/env bash
# install-mcp.sh — Register anno-rag as an MCP server after extracting the archive.
#
# Usage (run from the directory containing this script and the anno-rag binary):
#   ./install-mcp.sh
#   ./install-mcp.sh --dry-run       # Preview what would change
#   ./install-mcp.sh --skip-models   # Skip model download (already done)
#
# Registers in:
#   Claude Desktop  → ~/Library/Application Support/Claude/claude_desktop_config.json (macOS)
#                   → %APPDATA%\Claude\claude_desktop_config.json (Windows)
#   Claude Code     → runs `claude mcp add` if the CLI is on PATH
#
# Restart Claude Desktop / Claude Code after running this script.
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
binary="${script_dir}/anno-rag"

if [[ ! -f "${binary}" ]]; then
  echo "anno-rag binary not found at ${binary}" >&2
  echo "Run this script from the directory containing the anno-rag binary." >&2
  exit 1
fi

if [[ ! -x "${binary}" ]]; then
  chmod +x "${binary}"
fi

args=(setup-mcp --target all)
for arg in "$@"; do
  args+=("${arg}")
done

echo "Registering anno-rag as MCP server..."
"${binary}" "${args[@]}"
echo ""
echo "Done. Restart Claude Desktop or Claude Code to load the server."
