# Real ML Models Testing

## Models Tested

### GLiNER (via ONNX)
- **Feature**: `onnx`
- **Model**: `onnx-community/gliner_small-v2.1` (default)
- **Capabilities**: Zero-shot NER, any entity type
- **Performance**: ~1.3s for 33 documents

## Test Results

### GLiNER vs Stacked Comparison

**GLiNER Results (19 news documents):**
- **Entities**: 49
- **Clusters**: 8 (2 cross-doc, 6 singleton)
- **Entity Types**: ORG (4), facility (1), PER (1), event (1), MONEY (1)
- **Notable**: Correctly identified "COP29" as "event" type (zero-shot)
- **Notable**: Found "semiconductor fabrication facility" as "facility" entity

**Stacked Results (19 news documents):**
- **Entities**: 63
- **Clusters**: 15 (4 cross-doc, 11 singleton)
- **Entity Types**: PER (7), LOC (6), ORG (2)
- **Note**: More entities but some false positives (e.g., "AI" as PER, dates as PER)

### Key Differences

1. **Entity Type Quality**: GLiNER provides more accurate entity types
   - Zero-shot capability allows custom types (event, facility, product)
   - Better disambiguation (e.g., "COP29" as event, not LOC)

2. **Entity Count**: GLiNER finds fewer but higher-quality entities
   - Stacked: 63 entities (more false positives)
   - GLiNER: 49 entities (better precision)

3. **Cross-Doc Clustering**: Both models find valid cross-doc links
   - GLiNER: "Nvidia's" across 3 docs, "COP29" across 2 docs
   - Stacked: Similar clusters but with more noise

4. **Performance**: 
   - Stacked: ~0.1s for 113 documents
   - GLiNER: ~1.3s for 33 documents (slower but acceptable)

## Usage

```bash
# Build with ONNX support
cargo build --features "cli,onnx,eval-advanced" --bin anno

# Use GLiNER model
./target/debug/anno cross-doc hack/real_data/news --model gliner --threshold 0.35

# Compare with stacked
./target/debug/anno cross-doc hack/real_data/news --model stacked --threshold 0.35
```

## Recommendations

1. **For Production**: Use GLiNER for better entity type accuracy
2. **For Speed**: Use Stacked for faster processing
3. **For Zero-Shot**: GLiNER supports custom entity types (event, facility, product, etc.)
4. **For Large Datasets**: Consider LSH blocking (auto-enabled for >100 docs)

## Future Testing

- [ ] Test with BERT ONNX model (when available in CLI)
- [ ] Test with GLiNER large model (better accuracy, slower)
- [ ] Test with Candle backend (GPU acceleration)
- [ ] Compare entity extraction quality metrics

