# ADR-003 — PII eval uses overlap-span matching with greedy left-to-right TP assignment

**Status:** Accepted (v0.7) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

Scoring a PII detector against an annotated corpus requires deciding when a detection counts as a true positive. Options:

1. **Exact-span equality** — detection's `(start, end)` matches a truth span byte-for-byte. Brittle: a detector that returns `"M. Dupont"` when the truth is `"Dupont"` would score zero, even though the practical outcome is the same (the name is redacted).
2. **Substring inclusion** — detection contains the truth span OR vice versa. Loose: a detection that returns the whole paragraph would count as a TP for every name in it.
3. **Overlap with greedy assignment** — any overlap counts as a candidate TP; assign greedy left-to-right so a single detection cannot be credited against multiple truths.

For the cabinet's purposes (protective redaction), option 2 over-counts. Option 1 punishes detectors that pick slightly wider spans for safety. Option 3 captures the "did the redaction cover the truth?" question correctly and avoids the option-2 inflation.

## Decision

**`pii_eval::score_detections` uses overlap-span matching with greedy left-to-right TP assignment.** Sort detections by start offset; for each detection, find the first un-matched truth span it overlaps; mark TP and consume the truth.

## Consequences

- A detection that swallows two truths only credits one TP (the leftmost). The second truth becomes an FN.
- Wider-than-truth detections still score as TPs — pragmatic for a protective regime.
- The 35-doc French legal corpus + v0.7 baselines (NIR/SIRET/IBAN_FR/Phone/Email at 1.0/1.0; Person/Org at 1.0/1.0; Location at 0.93/0.98) reflect this scoring. Changing the scoring would invalidate the locked baselines.
- The CI gate (98% tolerance on `eval_baseline.toml` recall) is meaningful only if the test and the baseline used the same scoring; both go through `score_detections`.

## Reference

`crates/anno-rag/src/pii_eval.rs::score_detections`, `crates/anno-rag/tests/fixtures/pii_baseline.toml`.
