# `scripts/` (development + ops helpers)

This directory contains **repo-local helper scripts** used by `justfile`, CI, and ad-hoc debugging.
Treat these as **tools**, not as a stable public interface.

## Prefer `just`

Most common entrypoints are wrapped in `justfile` so you don’t need to remember flags/paths:

- **Health**: `just check`, `just ci`, `just ci-eval`
- **Eval**: `just matrix`, `just eval-quick`, `just eval-wide`
- **Docs**: `just docs-audit`, `just serve-readme`, `just e2e-readme-test`
- **Static analysis**: `just static-analysis`, `just validate-static-analysis-setup`

## Fast Rust debug loop

For local Rust work, prefer the targeted PowerShell loop before broad workspace
builds:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -PrintOnly
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1
```

Useful variants:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -AllAffected
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode build
```

`scripts\loop.ps1` wraps repeated dev-fast runs for D-loop debugging.

## Release helpers

The current GitHub `Release Binaries` workflow uses the scripts under
`scripts/release/` to build, smoke-test, package and checksum release archives:

- `local-pipeline-gate.ps1` — local pre-release gate with `fast`, `release`
  and `deep` profiles.
- `package-windows.ps1` / `package-unix.sh` — package `anno-rag`,
  `anno-privacy-gateway`, licenses, env example and Claude Desktop config
  examples.
- `smoke-gateway.ps1` / `smoke-gateway.sh` — boot smoke for
  `anno-privacy-gateway`.
- `verify-release-binary.ps1` and `checksums.sh` — artifact validation.

## Python scripts

These scripts are not part of a Python package. Many are intended to be run with `uv`:

- Example: `uv run scripts/eval_comprehensive.py --max-examples 20`

When a script needs extra dependencies, prefer using `uv`’s inline metadata format (see
the generated `convert_pytorch_to_safetensors.py` written by `sync_datasets_s3.sh`).

## Shell scripts

Most bash scripts are standalone entrypoints and should use strict mode:

```bash
set -euo pipefail
```

If a script is meant to be sourced (rare here), it should avoid setting shell options.

## Organization (informal)

- **Eval + regression hunting**: `eval-*.sh`, `check-*-patterns.sh`, `summarize-failures.sh`
- **Dataset registry + generation**: `generate_*`, `*_registry*`, `verify_dataset_urls.py`
- **S3 cache + artifacts**: `sync_datasets_s3.sh`, `prepare_datasets_s3.py`, `upload_*_s3.sh`
- **CI helpers**: `benchmark-static-analysis.sh`, `validate-static-analysis-setup.sh`
- **Fast local Rust checks**: `dev-fast.ps1`, `loop.ps1`
- **Release packaging/gates**: `release/`
- **Spot orchestration**: `spot/` (AWS spot eval harness tooling)

If you add a new script, prefer:

- one clear entrypoint (`main()` for python; `case "$1"` for bash)
- being runnable from the repo root
- printing a short `--help`/usage string
