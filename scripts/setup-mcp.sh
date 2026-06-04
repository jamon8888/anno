#!/usr/bin/env bash
set -euo pipefail

target="all"
source="release"
tag="latest"
binary=""
install_dir="${HOME}/Tools/hacienda"
models_dir="${HOME}/.anno-rag/models"
skip_models=0
dry_run=0
force=0

usage() {
  cat >&2 <<'EOF'
Usage: setup-mcp.sh [--target desktop|claude-code|all|manual] [--source release|local-build|path]
                    [--tag TAG|latest] [--binary PATH] [--install-dir DIR]
                    [--models-dir DIR] [--skip-models] [--dry-run] [--force]
EOF
}

require_value() {
  local name="${1:-}"
  local value="${2:-}"
  if [[ -z "${value}" || "${value}" == --* ]]; then
    echo "${name} requires a value" >&2
    usage
    exit 2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      require_value "$1" "${2:-}"
      target="$2"
      shift 2
      ;;
    --source)
      require_value "$1" "${2:-}"
      source="$2"
      shift 2
      ;;
    --tag)
      require_value "$1" "${2:-}"
      tag="$2"
      shift 2
      ;;
    --binary)
      require_value "$1" "${2:-}"
      binary="$2"
      shift 2
      ;;
    --install-dir)
      require_value "$1" "${2:-}"
      install_dir="$2"
      shift 2
      ;;
    --models-dir)
      require_value "$1" "${2:-}"
      models_dir="$2"
      shift 2
      ;;
    --skip-models)
      skip_models=1
      shift
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    --force)
      force=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

case "${target}" in
  desktop|claude-code|all|manual) ;;
  *) echo "invalid --target: ${target}" >&2; exit 2 ;;
esac

case "${source}" in
  release|local-build|path) ;;
  *) echo "invalid --source: ${source}" >&2; exit 2 ;;
esac

resolve_latest_tag() {
  if [[ "${tag}" != "latest" ]]; then
    printf '%s\n' "${tag}"
    return
  fi

  curl -fsSL https://api.github.com/repos/jamon8888/anno/releases/latest |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -n 1
}

detect_target() {
  local uname_s uname_m
  uname_s="$(uname -s)"
  uname_m="$(uname -m)"
  if [[ "${uname_s}" == "Darwin" && "${uname_m}" == "arm64" ]]; then
    printf 'aarch64-apple-darwin\n'
  elif [[ "${uname_s}" == "Darwin" && "${uname_m}" == "x86_64" ]]; then
    printf 'x86_64-apple-darwin\n'
  else
    echo "release install is currently supported by published macOS assets only; use --source path or --source local-build on ${uname_s}/${uname_m}" >&2
    exit 2
  fi
}

install_release_binary() {
  local resolved_tag target_triple asset base download_dir archive sums expected actual extract_dir exe
  resolved_tag="$(resolve_latest_tag)"
  if [[ -z "${resolved_tag}" ]]; then
    echo "could not resolve latest release tag" >&2
    exit 2
  fi

  target_triple="$(detect_target)"
  asset="hacienda-${resolved_tag}-${target_triple}.tar.gz"
  base="https://github.com/jamon8888/anno/releases/download/${resolved_tag}"
  download_dir="${TMPDIR:-/tmp}/anno-rag-${resolved_tag}"
  mkdir -p "${download_dir}" "${install_dir}"
  archive="${download_dir}/${asset}"
  sums="${download_dir}/SHA256SUMS.txt"

  curl -fL "${base}/${asset}" -o "${archive}"
  curl -fL "${base}/SHA256SUMS.txt" -o "${sums}"
  expected="$(grep -F "${asset}" "${sums}" | awk '{print $1}' | head -n 1)"
  actual="$(shasum -a 256 "${archive}" | awk '{print $1}')"
  if [[ -z "${expected}" || "${expected}" != "${actual}" ]]; then
    echo "checksum mismatch for ${asset}" >&2
    exit 1
  fi

  extract_dir="${install_dir}/${resolved_tag}-${target_triple}"
  rm -rf "${extract_dir}"
  mkdir -p "${extract_dir}"
  tar -xzf "${archive}" -C "${extract_dir}"
  exe="$(find "${extract_dir}" -type f -name anno-rag -perm -111 | head -n 1)"
  if [[ -z "${exe}" ]]; then
    echo "anno-rag not found after extract" >&2
    exit 1
  fi

  printf '%s\n' "${exe}"
}

if [[ "${source}" == "path" ]]; then
  if [[ -z "${binary}" ]]; then
    echo "--binary is required with --source path" >&2
    exit 2
  fi
  if [[ ! -f "${binary}" ]]; then
    echo "binary not found: ${binary}" >&2
    exit 1
  fi
  resolved_binary="$(cd "$(dirname "${binary}")" && pwd)/$(basename "${binary}")"
elif [[ "${source}" == "local-build" ]]; then
  repo_root="$(git rev-parse --show-toplevel)"
  cargo build -p anno-rag-bin --bin anno-rag
  mkdir -p "${install_dir}"
  cp "${repo_root}/target/debug/anno-rag" "${install_dir}/anno-rag"
  chmod +x "${install_dir}/anno-rag"
  resolved_binary="$(cd "${install_dir}" && pwd)/anno-rag"
else
  resolved_binary="$(install_release_binary)"
fi

args=(setup-mcp --target "${target}" --binary "${resolved_binary}" --models-dir "${models_dir}")
if [[ "${skip_models}" == "1" ]]; then
  args+=(--skip-models)
fi
if [[ "${dry_run}" == "1" ]]; then
  args+=(--dry-run)
fi
if [[ "${force}" == "1" ]]; then
  args+=(--force)
fi

"${resolved_binary}" "${args[@]}"
