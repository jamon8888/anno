#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_PATH="$SCRIPT_DIR/setup-agent-harness.ps1"

to_windows_path() {
    local path="$1"
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -w "$path"
    elif command -v wslpath >/dev/null 2>&1; then
        wslpath -w "$path"
    else
        printf '%s\n' "$path"
    fi
}

if command -v powershell >/dev/null 2>&1; then
    powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_PATH" "$@"
elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(to_windows_path "$SCRIPT_PATH")" "$@"
elif command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -File "$SCRIPT_PATH" "$@"
elif command -v pwsh.exe >/dev/null 2>&1; then
    pwsh.exe -NoProfile -File "$(to_windows_path "$SCRIPT_PATH")" "$@"
else
    echo "error: powershell or pwsh is required" >&2
    exit 1
fi
