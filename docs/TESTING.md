# Testing

This doc explains how to run tests and where to add new ones. It intentionally avoids being a “testing report” (those drift quickly).

For known bugs and workarounds, see `docs/guides/BUGS.md`.

## Fast commands (nextest)

Profiles are defined in `.config/nextest.toml`.

```bash
# Fast dev loop (filters out slow/property/e2e)
cargo nextest run --profile quick -p anno

# CI-ish run (more complete, stricter timeouts)
cargo nextest run --profile ci --workspace
```

## Feature-gated testing

Many tests are feature-gated (e.g. `onnx`, `candle`, `eval-advanced`). For broad coverage:

```bash
cargo nextest run --profile ci --workspace --features "eval-full discourse onnx candle"
```

## Where tests live

- `anno/tests/` and `tests/` contain most integration and regression tests.
- Property tests use `proptest` (see `**/*proptest*` test binaries and modules).

## What we try to enforce (invariants)

- **Unicode-safe spans**: character offsets, never byte slicing (see `docs/guides/UNICODE_OFFSETS.md`).
- **Bounded confidence**: confidence values stay in `[0, 1]`.
- **Core hierarchy consistency**: Signals, Tracks, Identities remain internally consistent (`Signal → Track → Identity`).

## Adding a new test

Prefer:

- a small unit test near the code if it’s local behavior
- an integration test if it crosses modules/features
- a property test if it’s an invariant that should hold for many inputs

## Related docs

- `docs/EVALUATION.md` for evaluation harness usage
- `docs/guides/TESTDATA_REAL_WORLD.md` for local-only real-world testing approach