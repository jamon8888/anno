# Comprehensive Evaluation Results

Generated: 2025-12-22T01:41:39.346008+00:00


## Summary

| Metric | Value |
|--------|-------|
| Total runs | 35 |
| Successful | 28 |
| Skipped | 3 |
| Incompatible | 4 |
| Dataset errors | 0 |
| Errors | 0 |
| Avg F1 | 37.5% |
| Best F1 | 74.5% |
| Best | bert_onnx/WikiGold |


## By Backend

| Backend | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| bert_onnx | 48.1% | 74.5% | 5 |
| nuner | 40.8% | 68.1% | 5 |
| gliner2 | 39.0% | 50.3% | 5 |
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
| FewNERD | 25.8% | 35.6% | 5 |
| Wnut17 | 19.9% | 27.9% | 5 |


## Results Matrix

| Dataset | Backend | F1 | P | R | Time |
|---------|---------|-----|-----|-----|------|
| WikiGold | bert_onnx | 74.5 | 70.1 | 79.4 | 0.5s |
| WikiGold | nuner | 68.1 | 64.5 | 72.1 | 4.99s |
| CoNLL2003Sample | bert_onnx | 66.7 | 66.7 | 66.7 | 0.53s |
| MultiNERD | bert_onnx | 57.8 | 54.2 | 61.9 | 0.58s |
| CoNLL2003Sample | nuner | 54.8 | 53.1 | 56.7 | 4.75s |
| CoNLL2003Sample | gliner_onnx | 50.9 | 56.0 | 46.7 | 0.85s |
| WikiGold | gliner2 | 50.3 | 48.0 | 52.9 | 4.12s |
| WikiGold | stacked | 45.7 | 39.4 | 54.4 | 0.45s |
| WikiGold | gliner_onnx | 44.8 | 49.1 | 41.2 | 0.97s |
| MultiNERD | nuner | 44.7 | 40.4 | 50.0 | 4.78s |
| MultiNERD | gliner_onnx | 44.2 | 39.6 | 50.0 | 0.94s |
| MultiNERD | gliner2 | 43.9 | 33.3 | 64.3 | 4.17s |
| CoNLL2003Sample | stacked | 40.9 | 37.5 | 45.0 | 0.44s |
| CoNLL2003Sample | gliner2 | 40.7 | 43.4 | 38.3 | 4.19s |
| MultiNERD | stacked | 38.1 | 31.7 | 47.6 | 0.49s |
| FewNERD | stacked | 35.6 | 31.3 | 41.2 | 0.75s |
| FewNERD | gliner2 | 32.4 | 30.0 | 35.3 | 6.07s |
| Wnut17 | gliner2 | 27.9 | 23.1 | 35.3 | 4.76s |
| WikiGold | heuristic | 27.6 | 26.0 | 29.4 | 0.02s |
| CoNLL2003Sample | heuristic | 25.2 | 25.4 | 25.0 | 0.02s |
| FewNERD | gliner_onnx | 23.8 | 30.3 | 19.6 | 1.1s |
| FewNERD | bert_onnx | 22.4 | 20.0 | 25.5 | 0.91s |
| Wnut17 | nuner | 21.2 | 17.6 | 26.5 | 4.88s |
| Wnut17 | bert_onnx | 18.9 | 17.5 | 20.6 | 0.52s |
| Wnut17 | gliner_onnx | 16.1 | 17.9 | 14.7 | 0.84s |
| Wnut17 | stacked | 15.4 | 11.4 | 23.5 | 0.44s |
| FewNERD | nuner | 15.0 | 20.7 | 11.8 | 5.74s |
| CoNLL2003Sample | crf | 3.1 | 25.0 | 1.7 | 0.02s |


---
*Raw data: [eval-comprehensive.jsonl](eval-comprehensive.jsonl)*
