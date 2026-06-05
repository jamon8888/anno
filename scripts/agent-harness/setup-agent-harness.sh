#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_PATH="$SCRIPT_DIR/setup-agent-harness.ps1"
ARGS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target)
            ARGS+=("-Target")
            if [[ $# -lt 2 ]]; then
                echo "error: --target requires a value" >&2
                exit 1
            fi
            shift
            ARGS+=("$1")
            ;;
        --target=*)
            ARGS+=("-Target" "${1#--target=}")
            ;;
        --dry-run)
            ARGS+=("-DryRun")
            ;;
        *)
            ARGS+=("$1")
            ;;
    esac
    shift
done

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
    powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_PATH" "${ARGS[@]}"
elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(to_windows_path "$SCRIPT_PATH")" "${ARGS[@]}"
elif command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -File "$SCRIPT_PATH" "${ARGS[@]}"
elif command -v pwsh.exe >/dev/null 2>&1; then
    pwsh.exe -NoProfile -File "$(to_windows_path "$SCRIPT_PATH")" "${ARGS[@]}"
else
    echo "error: powershell or pwsh is required" >&2
    exit 1
fi
