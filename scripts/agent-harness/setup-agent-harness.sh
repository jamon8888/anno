#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if command -v powershell >/dev/null 2>&1; then
    powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/setup-agent-harness.ps1" "$@"
elif command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -File "$SCRIPT_DIR/setup-agent-harness.ps1" "$@"
else
    echo "error: powershell or pwsh is required" >&2
    exit 1
fi
