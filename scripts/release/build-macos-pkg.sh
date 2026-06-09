#!/usr/bin/env bash
# build-macos-pkg.sh — Build a macOS PKG installer and wrap it in a DMG.
#
# Usage: ./scripts/release/build-macos-pkg.sh TAG TARGET [OPTIONS]
#
# Required:
#   TAG     Release tag, e.g. v0.12.0
#   TARGET  Cargo target triple, e.g. aarch64-apple-darwin
#
# Optional env vars (all optional — skip signing/notarization if unset):
#   APPLE_INSTALLER_SIGNING_IDENTITY   productsign identity, e.g.
#                                      "Developer ID Installer: Acme Corp (TEAMID)"
#   APPLE_CODESIGN_IDENTITY            codesign identity, e.g.
#                                      "Developer ID Application: Acme Corp (TEAMID)"
#   APPLE_ID                           Apple ID email for notarization
#   APP_SPECIFIC_PASSWORD              App-specific password for notarization
#   APPLE_TEAM_ID                      Team ID for notarization
#
# Outputs:
#   dist/anno-rag-TAG-TARGET.pkg   (component pkg, kept for debugging)
#   dist/anno-rag-TAG-TARGET.dmg   (final distributable)
set -euo pipefail

# ---------------------------------------------------------------------------
# Argument validation
# ---------------------------------------------------------------------------
if [[ $# -lt 2 ]]; then
  echo "Usage: $0 TAG TARGET [--skip-notarize]" >&2
  exit 2
fi

tag="$1"
target="$2"
skip_notarize="${3:-}"

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

# Strip leading "v" for the PKG CFBundleShortVersionString (must be X.Y.Z)
pkg_version="${tag#v}"

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"
dist_dir="${repo_root}/dist"
work_dir="${dist_dir}/.pkg-build-$$"   # temp work dir, cleaned on exit

pkg_name="anno-rag-${tag}-${target}"
pkg_path="${dist_dir}/${pkg_name}.pkg"
dmg_path="${dist_dir}/${pkg_name}.dmg"

# Binary sources
anno_rag_bin="${repo_root}/target/${target}/release/anno-rag"
anno_gw_bin="${repo_root}/target/${target}/release/anno-privacy-gateway"
postinstall_src="${script_dir}/postinstall.sh"

# ---------------------------------------------------------------------------
# Cleanup on exit
# ---------------------------------------------------------------------------
cleanup() {
  rm -rf -- "${work_dir}"
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
for tool in pkgbuild hdiutil; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "Required tool not found: ${tool}" >&2
    exit 1
  fi
done

missing=()
for f in "${anno_rag_bin}" "${anno_gw_bin}" "${postinstall_src}"; do
  [[ -f "${f}" ]] || missing+=("${f}")
done
if (( ${#missing[@]} > 0 )); then
  echo "Cannot build PKG. Missing required file(s):" >&2
  printf '  %s\n' "${missing[@]}" >&2
  exit 1
fi

mkdir -p "${dist_dir}"
rm -f -- "${pkg_path}" "${dmg_path}"

# ---------------------------------------------------------------------------
# Optional: sign binaries
# ---------------------------------------------------------------------------
codesign_identity="${APPLE_CODESIGN_IDENTITY:-}"

sign_binary() {
  local bin="$1"
  if [[ -n "${codesign_identity}" ]]; then
    echo "  Signing: $(basename "${bin}")..."
    codesign --force --options runtime \
             --sign "${codesign_identity}" \
             --timestamp \
             "${bin}"
  fi
}

# ---------------------------------------------------------------------------
# Build PKG root — mirrors install destination /usr/local/bin/
# ---------------------------------------------------------------------------
pkg_root="${work_dir}/pkg-root"
pkg_scripts="${work_dir}/pkg-scripts"
mkdir -p "${pkg_root}/usr/local/bin" "${pkg_scripts}"

echo "Copying binaries..."
cp -- "${anno_rag_bin}" "${pkg_root}/usr/local/bin/anno-rag"
cp -- "${anno_gw_bin}"  "${pkg_root}/usr/local/bin/anno-privacy-gateway"
chmod 755 "${pkg_root}/usr/local/bin/anno-rag" \
          "${pkg_root}/usr/local/bin/anno-privacy-gateway"

sign_binary "${pkg_root}/usr/local/bin/anno-rag"
sign_binary "${pkg_root}/usr/local/bin/anno-privacy-gateway"

echo "Copying postinstall script..."
cp -- "${postinstall_src}" "${pkg_scripts}/postinstall"
chmod 755 "${pkg_scripts}/postinstall"

# ---------------------------------------------------------------------------
# Build PKG with pkgbuild
# ---------------------------------------------------------------------------
echo "Building PKG (pkgbuild)..."
pkgbuild \
  --root "${pkg_root}" \
  --scripts "${pkg_scripts}" \
  --identifier "io.arclabs.anno-rag" \
  --version "${pkg_version}" \
  --install-location "/" \
  "${pkg_path}"

# ---------------------------------------------------------------------------
# Optional: sign PKG
# ---------------------------------------------------------------------------
installer_identity="${APPLE_INSTALLER_SIGNING_IDENTITY:-}"
if [[ -n "${installer_identity}" ]]; then
  echo "Signing PKG (productsign)..."
  signed_pkg="${work_dir}/anno-rag-signed.pkg"
  productsign --sign "${installer_identity}" --timestamp "${pkg_path}" "${signed_pkg}"
  mv -- "${signed_pkg}" "${pkg_path}"
fi

# ---------------------------------------------------------------------------
# Build DMG
# ---------------------------------------------------------------------------
dmg_staging="${work_dir}/dmg-staging"
mkdir -p "${dmg_staging}"

cp -- "${pkg_path}" "${dmg_staging}/"

echo "Building DMG (hdiutil)..."
hdiutil create \
  -volname "Anno RAG ${tag}" \
  -srcfolder "${dmg_staging}" \
  -ov \
  -format UDZO \
  "${dmg_path}"

# ---------------------------------------------------------------------------
# Optional: notarize DMG
# ---------------------------------------------------------------------------
apple_id="${APPLE_ID:-}"
app_password="${APP_SPECIFIC_PASSWORD:-}"
team_id="${APPLE_TEAM_ID:-}"

if [[ -n "${apple_id}" && -n "${app_password}" && -n "${team_id}" && "${skip_notarize}" != "--skip-notarize" ]]; then
  echo "Submitting DMG for notarization..."
  xcrun notarytool submit "${dmg_path}" \
    --apple-id "${apple_id}" \
    --password "${app_password}" \
    --team-id "${team_id}" \
    --wait
  echo "Stapling notarization ticket..."
  xcrun stapler staple "${dmg_path}"
else
  echo "Skipping notarization (credentials not set or --skip-notarize passed)."
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
echo ""
echo "PKG: ${pkg_path}"
echo "DMG: ${dmg_path}"
echo "${dmg_path}"
