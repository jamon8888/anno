# Evaluation Results

Generated: 2025-12-21 03:09:35 UTC

## Summary

| Metric | Value |
|--------|-------|
| Total runs | 25 |
| Successful | 7 |
| Best F1 | 59.7% |
| Best | nuner/WikiGold |
| Avg F1 | 36.7% |

## By Backend

| Backend | Avg F1 | Best F1 | Best Dataset | Runs |
|---------|--------|---------|--------------|------|
| nuner | 59.7% | 59.7% | WikiGold | 4 |
| gliner2 | 46.1% | 46.1% | WikiGold | 4 |
| stacked | 36.0% | 46.3% | WikiGold | 6 |
| heuristic | 26.4% | 26.4% | WikiGold | 7 |
| gliner | 0% | 0% | - | 4 |

## By Dataset

| Dataset | Best F1 | Best Backend | Backends Tested |
|---------|---------|--------------|-----------------|
| WikiGold | 59.7% | nuner | 5 |
| CoNLL2003Sample | 0% | - | 5 |
| MitMovie | 0% | - | 5 |
| Wnut17 | 0% | - | 5 |

## Backend × Dataset Matrix

| Backend | CoNLL2003Sample | MitMovie | WikiGold | Wnut17 |
|---------|------|------|------|------|
| nuner | - | - | 60 | - |
| gliner2 | - | - | 46 | - |
| stacked | - | - | 46 | - |
| heuristic | - | - | 26 | - |
| gliner | - | - | - | - |

---
*Raw data: [eval-results.jsonl](eval-results.jsonl)*