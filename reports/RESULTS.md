# Evaluation Results

Generated: 2025-12-21 03:37:30 UTC

## Summary

| Metric | Value |
|--------|-------|
| Total runs | 27 |
| Successful | 10 |
| Best F1 | 59.7% |
| Best | nuner/WikiGold |
| Avg F1 | 29.8% |
| Total time | 132.8s |
| Avg time | 4.9s |

## By Backend

| Backend | Avg F1 | Best F1 | Best Dataset | Runs | Avg Time |
|---------|--------|---------|--------------|------|----------|
| nuner | 59.7% | 59.7% | WikiGold | 4 | 73.0s |
| gliner2 | 46.1% | 46.1% | WikiGold | 4 | 55.4s |
| heuristic | 30.6% | 37.8% | CoNLL2003Sample | 5 | 0.0s |
| stacked | 25.0% | 46.3% | WikiGold | 5 | 4.4s |
| pattern | 0.4% | 0.4% | MitRestaurant | 5 | 0.0s |
| gliner | 0% | 0% | - | 4 | 0.0s |

## By Dataset

| Dataset | Best F1 | Best Backend | Backends Tested |
|---------|---------|--------------|-----------------|
| WikiGold | 59.7% | nuner | 6 |
| CoNLL2003Sample | 37.8% | heuristic | 6 |
| Wnut17 | 20.5% | heuristic | 6 |
| MitRestaurant | 0.4% | pattern | 3 |
| MitMovie | 0% | - | 6 |

## Backend × Dataset Matrix

| Backend | CoNLL2003Sample | MitMovie | MitRestaurant | WikiGold | Wnut17 |
|---------|------|------|------|------|------|
| nuner | - | - | - | 60 | - |
| gliner2 | - | - | - | 46 | - |
| heuristic | 38 | - | - | 34 | 20 |
| stacked | 36 | - | 0 | 46 | 17 |
| pattern | - | - | 0 | - | - |
| gliner | - | - | - | - | - |

---
*Raw data: [eval-results.jsonl](eval-results.jsonl)*