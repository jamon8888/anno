# Evaluation Results

Generated: 2026-01-06 18:11:17 UTC

## Summary

| Metric | Value |
|--------|-------|
| Total runs | 300 |
| Successful | 36 |
| Best F1 | 64.8% |
| Best | nuner/WikiGold |
| Avg F1 | 31.2% |
| Total time | 2431.2s |
| Avg time | 8.1s |

## By Backend

| Backend | Avg F1 | Best F1 | Best Dataset | Runs | Avg Time |
|---------|--------|---------|--------------|------|----------|
| nuner | 58.8% | 64.8% | WikiGold | 55 | 21.6s |
| stacked | 30.4% | 53.1% | WikiGold | 60 | 30.6s |
| heuristic | 29.4% | 37.8% | CoNLL2003Sample | 60 | 0.0s |
| gliner2 | 22.8% | 50.2% | WikiGold | 60 | 201.7s |
| pattern | 0.4% | 0.4% | MitRestaurant | 5 | 0.0s |
| gliner | 0% | 0% | - | 60 | 0.0s |

## By Dataset

| Dataset | Best F1 | Best Backend | Backends Tested |
|---------|---------|--------------|-----------------|
| WikiGold | 64.8% | nuner | 6 |
| CoNLL2003Sample | 37.8% | heuristic | 6 |
| LitBank | 20.8% | stacked | 4 |
| Wnut17 | 20.5% | heuristic | 6 |
| MitRestaurant | 0.4% | pattern | 6 |
| BC5CDR | 0% | - | 5 |
| FewNERD | 0% | - | 5 |
| GAP | 0% | - | 5 |
| MitMovie | 0% | - | 6 |
| MultiNERD | 0% | - | 5 |
| NCBIDisease | 0% | - | 5 |
| PreCo | 0% | - | 5 |

## Backend × Dataset Matrix

| Backend | BC5CDR | CoNLL2003Sample | FewNERD | GAP | LitBank | MitMovie | MitRestaurant | MultiNERD | NCBIDisease | PreCo | WikiGold | Wnut17 |
|---------|------|------|------|------|------|------|------|------|------|------|------|------|
| nuner | - | - | - | - | - | - | - | - | - | - | 65 | - |
| stacked | - | 36 | - | - | 21 | - | 0 | - | - | - | 53 | 17 |
| heuristic | - | 38 | - | - | - | - | - | - | - | - | 34 | 20 |
| gliner2 | - | - | - | - | 3 | - | - | - | - | - | 50 | - |
| pattern | - | - | - | - | - | - | 0 | - | - | - | - | - |
| gliner | - | - | - | - | - | - | - | - | - | - | - | - |

---
*Raw data: [eval-results.jsonl](eval-results.jsonl)*