# Quickstart

This is a short, practical starting point for using `anno` via the CLI and as a Rust library.

## Install

From crates.io:

```bash
cargo install anno
```

From source (recommended if you want optional backends like ONNX/Candle, and required for some subcommands like `cross-doc`):

```bash
git clone https://github.com/arclabs561/anno
cd anno
cargo build --release -p anno --bin anno --features "cli eval-advanced"
```

## CLI: extract entities

Prefer `--text` / `--file` over positional text for now (it avoids known arg-order/input-parsing pitfalls; see `docs/guides/BUGS.md`).

```bash
anno extract --text "Marie Curie was born in Paris."
```

Pattern-only extraction (emails/dates/money/urls, etc.):

```bash
anno extract --model pattern --text "Email bob@acme.com on 2024-01-15 for $100."
```

Machine-readable output:

```bash
anno extract --model pattern --format tsv --text "Email bob@acme.com on 2024-01-15 for $100."
```

## CLI: within-document coreference

```bash
anno pipeline --coref --text "Marie Curie was born in Paris. She moved to Paris."
```

## CLI: cross-document clustering

Cross-document clustering requires the `eval-advanced` feature (see `docs/BACKENDS.md`).

```bash
anno cross-doc ./docs --threshold 0.6 --format tree
```

## Offsets (Unicode)

Offsets are **character offsets**, not byte offsets. See `docs/guides/UNICODE_OFFSETS.md`.

## Next docs

- `docs/SCOPE.md` — what’s in/out of scope
- `docs/BACKENDS.md` — backend selection and feature flags
- `docs/PIPELINE.md` — end-to-end workflows
- `docs/EVALUATION.md` — how to measure quality

