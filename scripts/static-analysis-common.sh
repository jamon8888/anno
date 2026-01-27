#!/usr/bin/env bash
#
# static-analysis-common.sh
#
# Single source of truth for static-analysis tool invocations that depend on repo layout.
#
# Why this exists:
# - `crates/anno` uses `#[path = "../..."]` to place many modules outside `src/`.
# - Tools that “scan the crate directory” need explicit paths to avoid false positives.
#
# Usage:
#   ./scripts/static-analysis-common.sh machete
#   ./scripts/static-analysis-common.sh machete-cmd   # prints command (shell-escaped)
#
set -euo pipefail

repo_root() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P
}

require_tool() {
  local tool="$1"
  local hint="$2"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing tool: $tool" >&2
    echo "hint: $hint" >&2
    return 1
  fi
}

anno_machete_paths() {
  # Keep this list in sync with `crates/anno/src/lib.rs` module roots.
  # The command is executed from `crates/anno/`.
  echo "src backends cli eval preprocess ingest joint linking types"
}

cmd_machete() {
  echo "cargo machete $(anno_machete_paths)"
}

run_machete() {
  require_tool cargo "install Rust toolchain"
  require_tool cargo-machete "cargo install cargo-machete"
  local root
  root="$(repo_root)"
  (cd "$root/crates/anno" && cargo machete $(anno_machete_paths))
}

case "${1:-}" in
  machete)
    run_machete
    ;;
  machete-cmd)
    cmd_machete
    ;;
  *)
    echo "usage: $0 {machete|machete-cmd}" >&2
    exit 2
    ;;
esac

