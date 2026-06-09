#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"
dist_dir="${repo_root}/dist"
checksum_path="${dist_dir}/SHA256SUMS.txt"

if [[ ! -d "${dist_dir}" ]]; then
  echo "Cannot write checksums. dist directory does not exist: ${dist_dir}" >&2
  exit 1
fi

shopt -s nullglob
archives=("${dist_dir}"/*.zip "${dist_dir}"/*.tar.gz "${dist_dir}"/*.dmg)
shopt -u nullglob

if (( ${#archives[@]} == 0 )); then
  echo "Cannot write checksums. No .zip, .tar.gz, or .dmg archives found in ${dist_dir}" >&2
  exit 1
fi

IFS=$'\n' archives=($(printf '%s\n' "${archives[@]}" | sort))
unset IFS

if command -v sha256sum >/dev/null 2>&1; then
  (
    cd "${dist_dir}"
    sha256sum -- "${archives[@]##*/}" > "${checksum_path}"
  )
elif command -v shasum >/dev/null 2>&1; then
  (
    cd "${dist_dir}"
    shasum -a 256 -- "${archives[@]##*/}" > "${checksum_path}"
  )
else
  echo "Cannot write checksums. Neither sha256sum nor shasum is available." >&2
  exit 1
fi

cat -- "${checksum_path}"
