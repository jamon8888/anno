#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 TAG TARGET" >&2
  exit 2
fi

tag="$1"
target="$2"

validate_asset_component() {
  local name="$1"
  local value="$2"

  if [[ ! "${value}" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "Invalid ${name}: must match ^[A-Za-z0-9._-]+$" >&2
    exit 2
  fi

  if [[ ! "${value}" =~ [A-Za-z0-9] ]]; then
    echo "Invalid ${name}: must contain at least one ASCII alphanumeric character" >&2
    exit 2
  fi
}

validate_asset_component "TAG" "${tag}"
validate_asset_component "TARGET" "${target}"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"

package_name="hacienda-${tag}-${target}"
dist_dir="${repo_root}/dist"
staging_dir="${dist_dir}/${package_name}"
tarball_path="${dist_dir}/${package_name}.tar.gz"

executables=(
  "target/${target}/release/anno-rag"
  "target/${target}/release/anno-privacy-gateway"
)

required_files=(
  "README.md"
  "LICENSE-MIT"
  "LICENSE-APACHE"
  "env.example"
  "docs/release/examples/claude_desktop_config.windows.json"
  "docs/release/examples/claude_desktop_config.macos.json"
  "scripts/setup-mcp.ps1"
  "scripts/setup-mcp.sh"
)

missing=()
not_executable=()

for relative_path in "${executables[@]}"; do
  full_path="${repo_root}/${relative_path}"
  if [[ ! -f "${full_path}" ]]; then
    missing+=("${relative_path}")
  elif [[ ! -x "${full_path}" ]]; then
    not_executable+=("${relative_path}")
  fi
done

for relative_path in "${required_files[@]}"; do
  if [[ ! -f "${repo_root}/${relative_path}" ]]; then
    missing+=("${relative_path}")
  fi
done

if (( ${#missing[@]} > 0 )); then
  {
    echo "Cannot create Unix package. Missing required file(s):"
    printf '  %s\n' "${missing[@]}"
  } >&2
  exit 1
fi

if (( ${#not_executable[@]} > 0 )); then
  {
    echo "Cannot create Unix package. Required executable(s) are not executable:"
    printf '  %s\n' "${not_executable[@]}"
  } >&2
  exit 1
fi

mkdir -p "${dist_dir}"
rm -rf -- "${staging_dir}"
rm -f -- "${tarball_path}"

mkdir -p "${staging_dir}/examples" "${staging_dir}/scripts"

cp -- "${repo_root}/target/${target}/release/anno-rag" "${staging_dir}/"
cp -- "${repo_root}/target/${target}/release/anno-privacy-gateway" "${staging_dir}/"
cp -- "${repo_root}/README.md" "${staging_dir}/"
cp -- "${repo_root}/LICENSE-MIT" "${staging_dir}/"
cp -- "${repo_root}/LICENSE-APACHE" "${staging_dir}/"
cp -- "${repo_root}/env.example" "${staging_dir}/"
cp -- "${repo_root}/docs/release/examples/claude_desktop_config.windows.json" "${staging_dir}/examples/"
cp -- "${repo_root}/docs/release/examples/claude_desktop_config.macos.json" "${staging_dir}/examples/"
cp -- "${repo_root}/scripts/setup-mcp.ps1" "${staging_dir}/scripts/"
cp -- "${repo_root}/scripts/setup-mcp.sh" "${staging_dir}/scripts/"

tar -C "${dist_dir}" -czf "${tarball_path}" "${package_name}"

echo "${tarball_path}"
