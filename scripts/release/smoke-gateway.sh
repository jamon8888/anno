#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 BINARY_PATH [SECONDS]" >&2
  exit 2
fi

binary_path="$1"
seconds="${2:-3}"

if [[ ! -x "${binary_path}" ]]; then
  echo "Gateway binary is not executable: ${binary_path}" >&2
  exit 1
fi

stdout_path="$(mktemp "${TMPDIR:-/tmp}/anno-gateway-smoke.XXXXXX.stdout.log")"
stderr_path="$(mktemp "${TMPDIR:-/tmp}/anno-gateway-smoke.XXXXXX.stderr.log")"

ANNO_GATEWAY_LISTEN="127.0.0.1:0" "${binary_path}" >"${stdout_path}" 2>"${stderr_path}" &
pid="$!"

cleanup() {
  if kill -0 "${pid}" >/dev/null 2>&1; then
    kill "${pid}" >/dev/null 2>&1 || true
    wait "${pid}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

sleep "${seconds}"

if kill -0 "${pid}" >/dev/null 2>&1; then
  cleanup
  trap - EXIT
  echo "Gateway stayed alive for ${seconds}s on ANNO_GATEWAY_LISTEN=127.0.0.1:0; smoke passed."
  exit 0
fi

set +e
wait "${pid}"
exit_code="$?"
set -e
{
  echo "Gateway exited early with code ${exit_code}."
  echo "stdout: ${stdout_path}"
  echo "stderr: ${stderr_path}"
} >&2
exit 1
