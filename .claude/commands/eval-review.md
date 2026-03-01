# /eval-review -- Run evaluation and interpret results

Run anno's evaluation benchmarks, read the output, compare against previous results, and produce an assessment of what improved, what regressed, and what to investigate.

## Procedure

### 1. Build

```bash
cd <repo-root>
cargo build --release -p anno-cli --features "eval onnx"
```

### 2. Run a bounded eval

Default to `eval-quick` unless the user asks for broader coverage:

```bash
# Fast (5 datasets, 4 backends, 20 examples each)
just eval-quick

# Wider (7 datasets, 5 backends, 50 examples each)
just eval-wide 50

# Specific profile
just eval-profile ner-standard 20
```

Read the **full output** of the eval run. Do not truncate.

### 3. Read the report

Reports are written to `reports/`. Read the generated markdown and JSON:

```bash
ls -1 reports/eval-*.md reports/eval-*.json 2>/dev/null
```

Read both the markdown (human summary) and JSON (machine-readable scores). Focus on:

- **F1 scores by backend**: which backends score highest? which are surprisingly low?
- **Per-dataset variation**: does a backend do well on CoNLL but poorly on Wnut17? That suggests domain sensitivity.
- **Precision vs recall tradeoffs**: high precision + low recall = conservative; low precision + high recall = noisy.

### 4. Compare against previous results

Check for previous eval reports and muxer history:

```bash
ls -1 reports/ 2>/dev/null
ls -1 ~/.anno_cache/eval-results.jsonl 2>/dev/null
```

If previous reports exist, compare:
- **Regressions**: any backend-dataset pair where F1 dropped by >2 points?
- **Improvements**: any pair where F1 increased by >2 points?
- **New coverage**: any backend or dataset not present in the previous run?

If muxer history exists, also run:

```bash
just check-regressions
```

Read the full output -- it flags F1 drops with specific (backend, dataset) pairs.

### 5. Write the assessment

Structure:

1. **Test conditions**: date, eval profile used, number of examples, backends tested
2. **Headline numbers**: best backend overall, worst backend overall, biggest surprise
3. **Regression table**: backend-dataset pairs where performance dropped (with scores)
4. **Improvement table**: backend-dataset pairs where performance improved
5. **Per-backend notes**: any backend with pathological behavior (crashes, empty output, extremely slow)
6. **Recommendations**: what to investigate, what to fix, what to run next

### 6. Optionally run targeted follow-ups

If a regression is found, drill down:

```bash
# Run specific backend on specific dataset with more examples
just eval-profile ner-standard 100

# Run matrix with specific strategy
just matrix worst-first 42

# Check if the regression is seed-dependent
just eval-seed 42 50
just eval-seed 123 50
```

## What this is NOT

- Not a real-world quality audit (that's `/qa`)
- Not a code quality check (that's `/check-fix`)
- Not a publication readiness check (that's `/release-check`)

This answers: "are anno's NER scores stable, improving, or regressing?"
