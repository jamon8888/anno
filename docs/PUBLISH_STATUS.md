# Publish status

## Decision (updated 2026-02-19)

Publishing to crates.io is **active**.  We publish the minimum set of
crates needed to make `anno` usable as an external dependency:

| Crate | Package name | Publish | Notes |
|-------|-------------|---------|-------|
| `crates/anno-core` | `anno-core` | ✅ | No anno-* deps; publish first |
| `crates/anno-metrics` | `anno-metrics` | ✅ | Depends on anno-core |
| `crates/anno` | `anno-lib` → crate name `anno` | ✅ | Depends on anno-core + anno-metrics |
| `crates/anno-eval` | `anno-eval` | ❌ | Internal eval harness |
| `crates/anno-cli` | `anno-cli` | ❌ | Internal CLI tooling |
| `crates/anno-lattix` | `anno-lattix` | ❌ | Internal graph substrate adapters |

## Publish order

```
anno-core → anno-metrics → anno-lib (published as crate `anno`)
```

## Local publish command

```bash
# From repo root
cargo publish -p anno-core
# (wait for crates.io to index)
cargo publish -p anno-metrics
# (wait for crates.io to index)
cargo publish -p anno-lib
```

## CI checks

The CI workflow runs a best-effort "publish validation" job and uploads a
short markdown report as `publish-validation-report`.

## Local checks

If you want to create a release tag, follow `docs/TAGS.md`.
