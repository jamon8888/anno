# Publish status

## Decision (updated 2026-02-19)

**Only the root crate `anno` is published to crates.io.** No other anno workspace crate should ever be published.

| Crate | Package name | Publish | Notes |
|-------|-------------|---------|-------|
| (root) | `anno` | yes | **Only** published crate; facade for the library |
| `crates/anno` | `anno-lib` | no | Yanked on crates.io; `publish = false` |
| `crates/anno-core` | `anno-core` | no | Yanked on crates.io; `publish = false` |
| `crates/anno-metrics` | `anno-metrics` | no | Yanked on crates.io; `publish = false` |
| `crates/anno-eval` | `anno-eval` | no | Internal eval harness |
| `crates/anno-cli` | `anno-cli` | no | Internal CLI tooling |
| `crates/anno-lattix` | `anno-lattix` | no | Internal graph substrate adapters |

`anno-core`, `anno-metrics`, and `anno-lib` were published once (v0.3.0) and have been **yanked**. They remain on crates.io only so that the published `anno` crate can resolve its dependencies; they must not be published again.

## Publish command (root only)

```bash
# From repo root
cargo publish -p anno
```

## CI checks

The CI workflow runs a best-effort "publish validation" job and uploads a short markdown report as `publish-validation-report`.

## Local checks

If you want to create a release tag, follow `docs/TAGS.md`.
