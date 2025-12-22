# Comprehensive Evaluation Results

Generated: 2025-12-22T01:38:54.217923+00:00


## Summary

| Metric | Value |
|--------|-------|
| Total runs | 4 |
| Successful | 3 |
| Skipped | 1 |
| Incompatible | 0 |
| Dataset errors | 0 |
| Errors | 0 |
| Avg F1 | 38.6% |
| Best F1 | 70.3% |
| Best | nuner/WikiGold |


## By Backend

| Backend | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| nuner | 43.3% | 70.3% | 2 |
| heuristic | 29.3% | 29.3% | 1 |


## By Dataset

| Dataset | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| WikiGold | 49.8% | 70.3% | 2 |
| FewNERD | 16.3% | 16.3% | 1 |


## Results Matrix

| Dataset | Backend | F1 | P | R | Time |
|---------|---------|-----|-----|-----|------|
| WikiGold | nuner | 70.3 | 70.9 | 69.6 | 5.53s |
| WikiGold | heuristic | 29.3 | 28.3 | 30.4 | 0.02s |
| FewNERD | nuner | 16.3 | 22.2 | 12.9 | 6.19s |


---
*Raw data: [eval-comprehensive.jsonl](eval-comprehensive.jsonl)*
