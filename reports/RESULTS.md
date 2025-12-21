# Comprehensive Evaluation Results

Generated: 2025-12-21T23:53:17.337406+00:00


## Summary

| Metric | Value |
|--------|-------|
| Total runs | 49 |
| Successful | 44 |
| Skipped | 5 |
| Errors | 0 |
| Avg F1 | 21.4% |
| Best F1 | 74.5% |
| Best | bert_onnx/WikiGold |


## By Backend

| Backend | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| bert_onnx | 31.1% | 74.5% | 7 |
| nuner | 27.0% | 68.1% | 7 |
| heuristic | 26.4% | 27.6% | 2 |
| gliner2 | 26.0% | 50.3% | 7 |
| gliner_onnx | 22.3% | 50.9% | 7 |
| stacked | 20.0% | 45.7% | 7 |
| crf | 0.4% | 3.1% | 7 |


## By Dataset

| Dataset | Avg F1 | Best F1 | Runs |
|---------|--------|---------|------|
| WikiGold | 44.4% | 74.5% | 7 |
| CoNLL2003Sample | 40.3% | 66.7% | 7 |
| MultiNERD | 37.2% | 57.8% | 6 |
| Wnut17 | 16.6% | 27.9% | 6 |
| MitMovie | 3.0% | 15.9% | 6 |
| MitRestaurant | 1.1% | 6.4% | 6 |
| FewNERD | 0.0% | 0.0% | 6 |


## Results Matrix

| Dataset | Backend | F1 | P | R | Time |
|---------|---------|-----|-----|-----|------|
| WikiGold | bert_onnx | 74.5 | 70.1 | 79.4 | 0.73s |
| WikiGold | nuner | 68.1 | 64.5 | 72.1 | 4.61s |
| CoNLL2003Sample | bert_onnx | 66.7 | 66.7 | 66.7 | 0.71s |
| MultiNERD | bert_onnx | 57.8 | 54.2 | 61.9 | 1.46s |
| CoNLL2003Sample | nuner | 54.8 | 53.1 | 56.7 | 4.6s |
| CoNLL2003Sample | gliner_onnx | 50.9 | 56.0 | 46.7 | 0.85s |
| WikiGold | gliner2 | 50.3 | 48.0 | 52.9 | 4.1s |
| WikiGold | stacked | 45.7 | 39.4 | 54.4 | 0.45s |
| WikiGold | gliner_onnx | 44.8 | 49.1 | 41.2 | 0.88s |
| MultiNERD | nuner | 44.7 | 40.4 | 50.0 | 6.6s |
| MultiNERD | gliner_onnx | 42.1 | 37.7 | 47.6 | 1.29s |
| CoNLL2003Sample | stacked | 40.9 | 37.5 | 45.0 | 0.45s |
| CoNLL2003Sample | gliner2 | 40.7 | 43.4 | 38.3 | 4.07s |
| MultiNERD | gliner2 | 40.7 | 30.9 | 59.5 | 7.07s |
| MultiNERD | stacked | 38.1 | 31.7 | 47.6 | 6.69s |
| Wnut17 | gliner2 | 27.9 | 23.1 | 35.3 | 4.06s |
| WikiGold | heuristic | 27.6 | 26.0 | 29.4 | 64.64s |
| CoNLL2003Sample | heuristic | 25.2 | 25.4 | 25.0 | 0.02s |
| Wnut17 | nuner | 21.2 | 17.6 | 26.5 | 5.14s |
| Wnut17 | bert_onnx | 18.9 | 17.5 | 20.6 | 0.61s |
| Wnut17 | gliner_onnx | 16.1 | 17.9 | 14.7 | 0.88s |
| MitMovie | gliner2 | 15.9 | 17.5 | 14.7 | 4.07s |
| Wnut17 | stacked | 15.4 | 11.4 | 23.5 | 0.45s |
| MitRestaurant | gliner2 | 6.4 | 7.1 | 5.8 | 4.21s |
| CoNLL2003Sample | crf | 3.1 | 25.0 | 1.7 | 0.02s |
| MitMovie | gliner_onnx | 2.3 | 7.7 | 1.3 | 0.86s |
| FewNERD | stacked | 0.0 | 1.0 | 0.0 | 1.85s |
| FewNERD | crf | 0.0 | 1.0 | 0.0 | 1.65s |
| FewNERD | nuner | 0.0 | 1.0 | 0.0 | 1.37s |
| FewNERD | gliner_onnx | 0.0 | 1.0 | 0.0 | 1.45s |
| FewNERD | gliner2 | 0.0 | 1.0 | 0.0 | 1.9s |
| FewNERD | bert_onnx | 0.0 | 1.0 | 0.0 | 1.52s |
| MitMovie | stacked | 0.0 | 0.0 | 0.0 | 0.44s |
| MitMovie | crf | 0.0 | 0.0 | 0.0 | 0.01s |
| MitMovie | nuner | 0.0 | 0.0 | 0.0 | 4.97s |
| MitMovie | bert_onnx | 0.0 | 0.0 | 0.0 | 0.63s |
| MitRestaurant | stacked | 0.0 | 0.0 | 0.0 | 0.44s |
| MitRestaurant | crf | 0.0 | 0.0 | 0.0 | 0.01s |
| MitRestaurant | nuner | 0.0 | 0.0 | 0.0 | 4.61s |
| MitRestaurant | gliner_onnx | 0.0 | 0.0 | 0.0 | 0.86s |
| MitRestaurant | bert_onnx | 0.0 | 0.0 | 0.0 | 0.62s |
| MultiNERD | crf | 0.0 | 0.0 | 0.0 | 0.24s |
| WikiGold | crf | 0.0 | 0.0 | 0.0 | 0.02s |
| Wnut17 | crf | 0.0 | 0.0 | 0.0 | 0.02s |


---
*Raw data: [eval-comprehensive.jsonl](eval-comprehensive.jsonl)*
