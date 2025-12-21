# Real Data Testing Notes

## Data Sources

### News Articles
- **Tech/Semiconductors**: TSMC, Intel, AMD, Nvidia articles from Reuters, TechCrunch
- **Climate**: COP29, renewable energy articles
- **AI/Tech**: OpenAI, DeepSeek, AWS re:Invent, various AI news

### Datasets (HuggingFace)
- **WikiGold**: 20 Wikipedia-based NER examples (PER, LOC, ORG, MISC)
- **CoNLL-2003**: 20 news article examples (classic NER benchmark)
- **WNUT-17**: 20 social media NER examples (emerging entities)

## Test Results

### Summary Statistics (113 documents)
- **Total entities**: 324
- **Clusters**: 158 (24 cross-doc, 15.2%)
- **Entity types**: PER (49.4%), LOC (22.2%), ORG (16.5%)
- **Largest cluster**: 19 mentions across 9 documents

### Findings

#### Successful Cross-Doc Clusters
- **Nvidia**: Correctly linked across 4 documents (tech articles)
- **COP29**: Correctly linked across 2 climate documents
- **China**: Correctly linked across 2 documents
- **Historical entities**: "Second Battle of Bull Run", "IV Corps", "Potomac" correctly clustered across WikiGold documents

#### Issues Observed
1. **Entity Type Misclassification**:
   - "AI" classified as PER (should be ORG/MISC)
   - "Tuesday", "Sunday" classified as PER (should be DATE)
   - "NY" classified as PER (should be LOC)
   - "July" classified as LOC (should be DATE)
   - "Siemens Healthineers" classified as PER (should be ORG)

2. **Over-clustering**:
   - "Taiwan-based" cluster includes "AI" mentions (20 mentions, 13 docs) - likely false merge
   - Some generic terms being clustered together

3. **Under-clustering**:
   - "Nvidia" and "Nvidia's" correctly linked, but some variations might be missed

### Threshold Testing
- **0.2**: Too permissive, many false positives
- **0.3**: Good balance for news articles
- **0.35**: Better precision, fewer false merges
- **0.4-0.5**: Too strict, misses valid cross-doc links

### Performance
- **113 documents**: ~2-3 seconds processing time
- **LSH blocking**: Automatically enabled for >100 documents
- **Memory**: Efficient, no issues with current dataset sizes

## Recommendations

1. **Entity Type Disambiguation**: Improve heuristics for DATE, LOC, ORG vs PER
2. **Similarity Metrics**: Consider context-aware similarity (not just string similarity)
3. **Type Matching**: `--require-type-match` helps but may be too strict
4. **Canonical Name Generation**: Could be improved to handle variations better

## Next Steps

- [ ] Test with larger Common Crawl samples
- [ ] Test with domain-specific corpora (legal, medical, etc.)
- [ ] Experiment with different similarity thresholds per entity type
- [ ] Add entity type confidence scores to output
- [ ] Test with multilingual content

