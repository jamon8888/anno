# Backend Testing Plan for Document Understanding

## Decision: Focus on GLiNER2 (Multi-Task) for Document Understanding

**Rationale**: Based on research and available backends, GLiNER2 provides the best balance for cross-document understanding:
- **Multi-task**: NER + classification + structure in one pass
- **Zero-shot**: Custom entity types without retraining
- **Efficient**: 130-200ms on CPU, smaller than LLMs
- **Cross-document ready**: Better entity representations for linking

## Available Backends (Priority Order)

### Tier 1: Best for Document Understanding
1. **GLiNER2** (Multi-task) - `gliner2.rs`
   - NER + classification + hierarchical extraction
   - Best for understanding document structure and relationships
   - **Test next**: This is the priority

2. **GLiNER** (ONNX) - Already tested ✅
   - Zero-shot NER, good entity types
   - Works well, but GLiNER2 is better

3. **NuNER** - `nuner.rs`
   - Arbitrary-length entities
   - Good for long entity names across documents
   - **Test after GLiNER2**

### Tier 2: Specialized Use Cases
4. **W2NER** - `w2ner.rs`
   - Nested/discontinuous entities
   - Useful for complex medical/legal documents
   - Lower priority for general document understanding

5. **BERT ONNX** - `onnx.rs`
   - Fast, fixed types (PER/ORG/LOC/MISC)
   - Good baseline, but less flexible than GLiNER

### Tier 3: Not Priority for Document Understanding
- **TPLinker**: Relation extraction (not NER focus)
- **DeBERTa/ALBERT**: Similar to BERT, less flexible
- **Candle backends**: GPU acceleration (nice-to-have, not essential)

## Kodama: Hierarchical Clustering

**Decision: Not needed for current CDCR implementation**

**Why:**
- Current union-find approach is more efficient for large-scale CDCR
- Union-find: O(n) merge operations, excellent for 1000+ documents
- Hierarchical clustering: O(n²) distance matrix, better for analysis but slower
- **Trade-off**: Union-find prioritizes speed (critical for cross-doc), HAC prioritizes interpretability

**When kodama would be useful:**
- If we need dendrogram visualization of entity relationships
- For analyzing clustering decisions at different thresholds
- For research/analysis purposes (not production CDCR)

**Recommendation**: Skip kodama for now, focus on better entity extraction (GLiNER2)

## Testing Plan

### Phase 1: GLiNER2 Multi-Task (Priority)
```bash
# Test GLiNER2 for document understanding
cargo build --features "cli,onnx,eval-advanced" --bin anno
./target/debug/anno cross-doc hack/real_data/news --model gliner2 --threshold 0.35
```

**Why GLiNER2 first:**
- Multi-task extraction (entities + classification + structure)
- Better entity representations for cross-doc linking
- More informative output for document understanding

### Phase 2: NuNER (Arbitrary Length)
```bash
# Test NuNER for long entity names
./target/debug/anno cross-doc hack/real_data/news --model nuner --threshold 0.35
```

**Why NuNER second:**
- Handles long entity names better (e.g., "Taiwan Semiconductor Manufacturing Company")
- Important for cross-doc linking where full names matter

### Phase 3: Comparison
- Compare GLiNER2 vs GLiNER vs NuNER on same dataset
- Measure: entity quality, cross-doc linking accuracy, performance

## What's Most Useful for Document Understanding

Based on research:

1. **Entity Extraction Quality** (GLiNER2 > GLiNER > NuNER > BERT)
   - Better entities = better cross-doc linking
   - Zero-shot capability = handle domain-specific entities

2. **Contextual Understanding** (All transformer-based models)
   - BERT/GLiNER understand context, not just patterns
   - Critical for disambiguating "John Smith" across documents

3. **Multi-task Capabilities** (GLiNER2 only)
   - Extract entities + classify documents + understand structure
   - Most comprehensive for document understanding

4. **Scalability** (Current union-find is fine)
   - LSH blocking already handles large datasets
   - No need for hierarchical clustering overhead

## Next Steps

1. ✅ Test GLiNER (done)
2. **Next**: Test GLiNER2 multi-task model
3. Then: Test NuNER for long entities
4. Compare: Quality vs performance trade-offs

