# Comprehensive Evaluation Results

Generated: 2025-12-22T01:01:12.444064+00:00


## Summary

| Metric | Value |
|--------|-------|
| Total runs | 6 |
| Successful | 5 |
| Skipped | 1 |
| Incompatible | 0 |
| Dataset errors | 0 |
| Errors | 0 |
| Avg F1 | 34.2% |
| Best F1 | 79.1% |
| Best | nuner/WikiGold |


## By Backend

| Backend | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| nuner | 44.1% | 79.1% | 2 |
| stacked | 31.4% | 56.0% | 2 |
| heuristic | 20.0% | 20.0% | 1 |


## By Dataset

| Dataset | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| WikiGold | 51.7% | 79.1% | 3 |
| Wnut17 | 7.9% | 9.1% | 2 |


## Results Matrix

| Dataset | Backend | F1 | P | R | Time |
|---------|---------|-----|-----|-----|------|
| WikiGold | nuner | 79.1 | 77.3 | 81.0 | 7.97s |
| WikiGold | stacked | 56.0 | 48.3 | 66.7 | 0.89s |
| WikiGold | heuristic | 20.0 | 21.1 | 19.0 | 0.03s |
| Wnut17 | nuner | 9.1 | 7.7 | 11.1 | 7.38s |
| Wnut17 | stacked | 6.7 | 4.8 | 11.1 | 0.69s |


---
*Raw data: [eval-comprehensive.jsonl](eval-comprehensive.jsonl)*
