# Testing Decisions

## Kodama (Hierarchical Clustering)

**Decision: Skip for now**

**Reasoning:**
- Current union-find approach is more efficient for large-scale CDCR
- Union-find: O(n) merge operations, excellent for 1000+ documents  
- Hierarchical clustering: O(n²) distance matrix, better for analysis but slower
- **Trade-off**: Union-find prioritizes speed (critical for cross-doc), HAC prioritizes interpretability

**When kodama would be useful:**
- If we need dendrogram visualization of entity relationships
- For analyzing clustering decisions at different thresholds
- For research/analysis purposes (not production CDCR)

**Recommendation**: Focus on better entity extraction first (GLiNER2), revisit kodama if we need visualization/analysis features.

## Backend Testing Priority

### Tier 1: Document Understanding (Test These)
1. **GLiNER2** (Multi-task) - **PRIORITY**
   - NER + classification + hierarchical extraction
   - Best for understanding document structure and relationships
   - Status: Added to CLI, testing model path

2. **GLiNER** (ONNX) - ✅ Tested
   - Zero-shot NER, good entity types
   - Works well, but GLiNER2 is better

3. **NuNER** - Lower priority
   - Arbitrary-length entities
   - Good for long entity names
   - Not yet implemented in CLI

### Tier 2: Specialized (Skip for now)
- **W2NER**: Nested entities (medical/legal docs)
- **BERT ONNX**: Fast baseline, less flexible
- **Candle backends**: GPU acceleration (nice-to-have)

## What's Most Useful for Document Understanding

Based on research:

1. **Multi-task Extraction** (GLiNER2)
   - Extract entities + classify documents + understand structure
   - Most comprehensive for document understanding

2. **Entity Quality** (GLiNER2 > GLiNER > NuNER > BERT)
   - Better entities = better cross-doc linking
   - Zero-shot capability = handle domain-specific entities

3. **Contextual Understanding** (All transformer-based)
   - BERT/GLiNER understand context, not just patterns
   - Critical for disambiguating "John Smith" across documents

4. **Scalability** (Current approach is fine)
   - LSH blocking already handles large datasets
   - Union-find clustering is efficient

## Next Steps

1. ✅ Test GLiNER (done)
2. **Current**: Fix GLiNER2 model path and test
3. Then: Compare GLiNER vs GLiNER2 quality
4. Future: Consider NuNER if long entity names become an issue

