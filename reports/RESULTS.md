# Comprehensive Evaluation Results

Generated: 2025-12-22T02:17:14.396078+00:00


## Summary

| Metric | Value |
|--------|-------|
| Total runs | 35 |
| Successful | 28 |
| Skipped | 3 |
| Incompatible | 4 |
| Dataset errors | 0 |
| Errors | 0 |
| Avg F1 | 38.2% |
| Best F1 | 74.5% |
| Best | bert_onnx/WikiGold |


## By Backend

| Backend | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| bert_onnx | 49.3% | 74.5% | 5 |
| nuner | 41.2% | 68.1% | 5 |
| gliner2 | 40.9% | 50.3% | 5 |
| gliner_onnx | 36.0% | 50.9% | 5 |
| stacked | 35.1% | 45.7% | 5 |
| heuristic | 26.4% | 27.6% | 2 |
| crf | 3.1% | 3.1% | 1 |


## By Dataset

| Dataset | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| WikiGold | 51.8% | 74.5% | 6 |
| MultiNERD | 45.7% | 57.8% | 5 |
| CoNLL2003Sample | 40.3% | 66.7% | 7 |
| FewNERD | 28.0% | 39.6% | 5 |
| Wnut17 | 21.4% | 30.2% | 5 |


## Results Matrix

| Dataset | Backend | F1 | P | R | Time |
|---------|---------|-----|-----|-----|------|
| WikiGold | bert_onnx | 74.5 | 70.1 | 79.4 | 1.08s |
| WikiGold | nuner | 68.1 | 64.5 | 72.1 | 10.23s |
| CoNLL2003Sample | bert_onnx | 66.7 | 66.7 | 66.7 | 1.03s |
| MultiNERD | bert_onnx | 57.8 | 54.2 | 61.9 | 1.3s |
| CoNLL2003Sample | nuner | 54.8 | 53.1 | 56.7 | 12.48s |
| CoNLL2003Sample | gliner_onnx | 50.9 | 56.0 | 46.7 | 1.71s |
| WikiGold | gliner2 | 50.3 | 48.0 | 52.9 | 7.93s |
| WikiGold | stacked | 45.7 | 39.4 | 54.4 | 0.91s |
| WikiGold | gliner_onnx | 44.8 | 49.1 | 41.2 | 1.39s |
| MultiNERD | nuner | 44.7 | 40.4 | 50.0 | 15.55s |
| MultiNERD | gliner_onnx | 44.2 | 39.6 | 50.0 | 1.59s |
| MultiNERD | gliner2 | 43.9 | 33.3 | 64.3 | 7.23s |
| CoNLL2003Sample | stacked | 40.9 | 37.5 | 45.0 | 0.92s |
| CoNLL2003Sample | gliner2 | 40.7 | 43.4 | 38.3 | 7.76s |
| FewNERD | gliner2 | 39.6 | 36.7 | 43.1 | 10.71s |
| MultiNERD | stacked | 38.1 | 31.7 | 47.6 | 1.01s |
| FewNERD | stacked | 35.6 | 31.3 | 41.2 | 1.32s |
| Wnut17 | gliner2 | 30.2 | 25.0 | 38.2 | 7.14s |
| WikiGold | heuristic | 27.6 | 26.0 | 29.4 | 0.04s |
| FewNERD | bert_onnx | 25.9 | 23.1 | 29.4 | 0.87s |
| CoNLL2003Sample | heuristic | 25.2 | 25.4 | 25.0 | 0.08s |
| FewNERD | gliner_onnx | 23.8 | 30.3 | 19.6 | 1.47s |
| Wnut17 | nuner | 23.5 | 19.6 | 29.4 | 10.02s |
| Wnut17 | bert_onnx | 21.6 | 20.0 | 23.5 | 1.34s |
| Wnut17 | gliner_onnx | 16.1 | 17.9 | 14.7 | 2.05s |
| Wnut17 | stacked | 15.4 | 11.4 | 23.5 | 0.9s |
| FewNERD | nuner | 15.0 | 20.7 | 11.8 | 9.27s |
| CoNLL2003Sample | crf | 3.1 | 25.0 | 1.7 | 0.03s |


---
*Raw data: [eval-comprehensive.jsonl](eval-comprehensive.jsonl)*
