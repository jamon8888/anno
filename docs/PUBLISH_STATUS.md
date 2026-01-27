# Publish status

This repo publishes a single crate: `anno`.

## What is publishable?

- **Published / intended to be published**:
  - `anno` (the public facade crate)
- **Not intended to be published**:
  - legacy internal crates kept for history under `crates/` (they are not workspace members)

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

