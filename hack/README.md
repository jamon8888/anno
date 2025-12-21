# Hack Folder - Local Data Testing

This folder is for **local experiments** (scratch scripts, one-off runs, and optional local data).
The repo includes a small, tracked fixture set under `testdata/fixtures/` for reproducible examples.

## Notes

Design/UX/testing notes that used to live in `hack/` have been moved to `docs/notes/hack/`.

## Fixtures (tracked)

Use these for docs/tests without licensing ambiguity:

- `testdata/fixtures/cross_doc/` (3 small `.txt` files)

## Usage

```bash
# Batch extraction over a small directory
./target/debug/anno batch --dir testdata/fixtures/cross_doc --format human

# Cross-document clustering (requires `eval-advanced`)
./target/debug/anno cross-doc testdata/fixtures/cross_doc --format tree --threshold 0.3
```

## Extraction Script

To extract dataset texts into a local directory (not tracked):

```bash
cargo run --example extract_dataset_texts --features eval-advanced
```

Note: some popular datasets/news sources have redistribution restrictions. Keep any “real data”
you download/scrape under `hack/real_data/` as **local-only** unless you’ve verified licensing.

