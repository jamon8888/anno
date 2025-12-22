# Comprehensive Evaluation Results

Generated: 2025-12-22T02:47:35.175728+00:00


## Summary

| Metric | Value |
|--------|-------|
| Total runs | 36 |
| Successful | 29 |
| Skipped | 3 |
| Incompatible | 4 |
| Dataset errors | 0 |
| Errors | 0 |
| Avg F1 | 39.1% |
| Best F1 | 80.0% |
| Best | bert_onnx/MultiNERD |


## By Backend

| Backend | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| bert_onnx | 54.9% | 80.0% | 5 |
| nuner | 41.4% | 70.3% | 5 |
| gliner2 | 39.1% | 46.3% | 5 |
| stacked | 36.7% | 49.6% | 5 |
| gliner_onnx | 35.9% | 53.1% | 5 |
| mention_ranking | 34.6% | 34.6% | 1 |
| heuristic | 28.1% | 29.3% | 2 |
| crf | 4.9% | 4.9% | 1 |


## By Dataset

| Dataset | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| MultiNERD | 54.7% | 80.0% | 5 |
| WikiGold | 52.9% | 75.2% | 6 |
| CoNLL2003Sample | 41.6% | 70.1% | 7 |
| GAP | 34.6% | 34.6% | 1 |
| FewNERD | 28.1% | 37.3% | 5 |
| Wnut17 | 15.5% | 26.4% | 5 |


## Results Matrix

| Dataset | Backend | F1 | P | R | Time |
|---------|---------|-----|-----|-----|------|
| MultiNERD | bert_onnx | 80.0 | 84.6 | 75.9 | 0.53s |
| WikiGold | bert_onnx | 75.2 | 72.1 | 78.6 | 0.58s |
| WikiGold | nuner | 70.3 | 70.9 | 69.6 | 5.0s |
| CoNLL2003Sample | bert_onnx | 70.1 | 69.2 | 71.1 | 0.46s |
| CoNLL2003Sample | nuner | 54.5 | 53.8 | 55.3 | 4.96s |
| MultiNERD | gliner_onnx | 53.1 | 48.6 | 58.6 | 1.15s |
| MultiNERD | nuner | 50.0 | 43.6 | 58.6 | 7.09s |
| WikiGold | stacked | 49.6 | 44.9 | 55.4 | 0.57s |
| CoNLL2003Sample | gliner_onnx | 49.3 | 54.8 | 44.7 | 0.92s |
| MultiNERD | stacked | 48.6 | 41.5 | 58.6 | 0.51s |
| WikiGold | gliner_onnx | 46.9 | 54.8 | 41.1 | 0.91s |
| WikiGold | gliner2 | 46.3 | 48.1 | 44.6 | 4.86s |
| CoNLL2003Sample | gliner2 | 44.1 | 50.0 | 39.5 | 4.62s |
| MultiNERD | gliner2 | 41.9 | 31.6 | 62.1 | 4.54s |
| CoNLL2003Sample | stacked | 41.5 | 38.6 | 44.7 | 0.49s |
| FewNERD | stacked | 37.3 | 31.8 | 45.2 | 0.52s |
| FewNERD | gliner2 | 36.6 | 32.5 | 41.9 | 4.72s |
| GAP | mention_ranking | 34.6 | 0.0 | 80.3 | 8.06s |
| FewNERD | bert_onnx | 31.0 | 27.5 | 35.5 | 0.51s |
| WikiGold | heuristic | 29.3 | 28.3 | 30.4 | 0.02s |
| CoNLL2003Sample | heuristic | 27.0 | 27.8 | 26.3 | 0.02s |
| Wnut17 | gliner2 | 26.4 | 21.2 | 35.0 | 4.32s |
| FewNERD | gliner_onnx | 19.2 | 23.8 | 16.1 | 1.0s |
| Wnut17 | bert_onnx | 18.2 | 16.7 | 20.0 | 0.53s |
| FewNERD | nuner | 16.3 | 22.2 | 12.9 | 4.99s |
| Wnut17 | nuner | 15.7 | 12.9 | 20.0 | 5.06s |
| Wnut17 | gliner_onnx | 11.1 | 12.5 | 10.0 | 1.03s |
| Wnut17 | stacked | 6.3 | 4.5 | 10.0 | 0.46s |
| CoNLL2003Sample | crf | 4.9 | 33.3 | 2.6 | 0.02s |


---
*Raw data: [eval-comprehensive.jsonl](eval-comprehensive.jsonl)*
