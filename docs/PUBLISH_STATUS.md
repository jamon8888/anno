# Publish status

This repo is a Rust workspace with multiple crates. Some are intended to be published to crates.io, and some are intended to remain internal to this repo.

## What is publishable?

- **Published / intended to be published**:
  - `anno`
  - `anno-core`
  - `anno-coalesce`
- **Not intended to be published**:
  - `anno-tier` (the crate has `publish = false` in its `Cargo.toml`)

## CI checks

The CI workflow runs `cargo publish --dry-run` for the publishable crates and uploads a short markdown report as an artifact named `publish-validation-report`.

## Local checks

From the repo root:

```bash
just validate-publish
```

If you get an unexpected failure, start by running:

```bash
cargo publish --dry-run -p anno
```

