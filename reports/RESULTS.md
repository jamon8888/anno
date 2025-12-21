# Evaluation Results

Generated: 2025-12-21 03:13:50 UTC

## Summary

| Metric | Value |
|--------|-------|
| Total runs | 25 |
| Successful | 7 |
| Best F1 | 59.7% |
| Best | nuner/WikiGold |
| Avg F1 | 36.7% |
| Total time | 132.8s |
| Avg time | 5.3s |

## By Backend

| Backend | Avg F1 | Best F1 | Best Dataset | Runs | Avg Time |
|---------|--------|---------|--------------|------|----------|
| nuner | 59.7% | 59.7% | WikiGold | 4 | 36.5s |
| gliner2 | 46.1% | 46.1% | WikiGold | 4 | 27.7s |
| stacked | 36.0% | 46.3% | WikiGold | 6 | 2.2s |
| heuristic | 26.4% | 26.4% | WikiGold | 7 | 0.0s |
| gliner | 0% | 0% | - | 4 | 0.0s |

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