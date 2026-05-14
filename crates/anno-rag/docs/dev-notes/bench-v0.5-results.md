# Bench v0.5 — performance budget results

> Run this template against your own corpus and replace the values.

**Date:** TODO
**Branch:** `feat/v0.5-perf`
**Host:** TODO (CPU, RAM, OS)

## Reproduction

```bash
cargo build --release -p anno-rag
./target/release/anno-rag bench --corpus /path/to/corpus
```

## Results

(paste the markdown table emitted by the CLI here)

## Diff vs v0.2 baseline

The v0.2 baseline (`bench-v0.2-piighost-test.md`) measured peak RSS 3.95 GB on
6 ingested docs. v0.5 targets are stricter; the CLI output checks each metric
against the SLO column.

## SLO compliance

- [ ] Cold-start <2s
- [ ] Idle RSS <200 MB
- [ ] Ingest <10s/doc
- [ ] Search p95 <200ms
- [ ] Peak RSS <1500 MB

If any box is unchecked, file a v0.5 follow-up issue with the gap.
