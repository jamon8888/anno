# Type System Critique: Anno vs. Research State-of-Art

## Executive Summary

Anno's type system is **well-designed for production NER** but has some gaps compared to bleeding-edge research. Most gaps are intentional trade-offs favoring simplicity and determinism over flexibility.

---

## Strengths (Research-Aligned)

### 1. Signal/Track/Identity 3-Level Hierarchy ✓
**Research**: X-AMR (ACL 2024) proposes graphical event representations for linear-complexity cross-document coreference.

**Anno**: The Signal → Track → Identity hierarchy mirrors this:
- Signal: individual mention (like X-AMR event mention)
- Track: within-document cluster (local coreference)  
- Identity: cross-document entity with KB linking

This is **exactly right** per current research.

### 2. Multimodal Location Types ✓
**Research**: GMNER (Grounded Multimodal NER) requires both text spans AND visual bounding boxes.

**Anno**: \`Location\` enum handles this elegantly:
- \`Text { start, end }\` - character offsets
- \`BoundingBox { x, y, width, height, page }\` - visual grounding
- \`Temporal { start_sec, end_sec, frame }\` - audio/video
- \`Cuboid { center, dimensions, rotation }\` - 3D/LiDAR
- \`Genomic { contig, start, end, strand }\` - bioinformatics

This is **more comprehensive** than most research systems.

### 3. Hierarchical Confidence ✓
**Research**: Decomposed uncertainty (boundary vs type vs linking confidence) matters for calibration.

**Anno**: \`HierarchicalConfidence\` provides exactly this:
\`\`\`rust
pub struct HierarchicalConfidence {
    pub linkage: f32,   // entity detection confidence
    pub type_conf: f32, // classification confidence
    pub boundary: f32,  // span boundary confidence
}
\`\`\`

### 4. Knowledge Base Linking ✓
**Research**: FELA (2024) and LLM-based EL emphasize KB-agnostic approaches.

**Anno**: \`kb_id\` + \`kb_name\` fields enable linking to any KB (Wikidata, UMLS, custom).

### 5. Temporal Validity ✓
**Research**: Temporal Knowledge Graphs require validity intervals for facts.

**Anno**: \`valid_from\` / \`valid_until\` on Entity and Identity - supports time-varying facts.

---

## Gaps (Intentional Trade-offs)

### 1. EntityType Enum vs. Free-Form Labels
**Research**: GLiNER treats entity types as arbitrary strings embedded in same space as text. Ultra-Fine Entity Typing (Choi et al. 2018) uses free-form phrases.

**Anno**: Fixed enum with \`Custom { name, category }\` escape hatch.

**Why this is OK**: 
- Enum enables exhaustive matching and compile-time checks
- \`Custom\` variant provides escape hatch for domain-specific types
- TypeMapper normalizes heterogeneous labels to core types
- Production systems need determinism over flexibility

**Potential improvement**: Add optional \`type_embedding: Option<Vec<f32>>\` field for GLiNER-style semantic type matching.

### 2. No Hierarchical Type Ontology
**Research**: OntoType, TypeNet show benefits of hierarchical type systems (Person → Politician → Senator).

**Anno**: Flat type system (EntityCategory → EntityType, but not deeper).

**Why this is OK**:
- Research shows annotation quality degrades with deep hierarchies
- Flat systems (CoNLL's 4 types) still dominate production
- \`TypeMapper\` can map fine-grained → coarse at runtime

**Potential improvement**: Add optional \`type_hierarchy: Option<Vec<String>>\` for path-based types like ["Entity", "Person", "Politician"].

### 3. Relation Typing Less Rich Than Entity Typing
**Research**: Joint entity-relation extraction (TPLinker, GLiNER) treats relations as first-class.

**Anno**: \`Relation\` struct exists but simpler than \`Entity\`.

**Why this is OK**:
- Relations are currently secondary to NER focus
- \`Relation\` has \`relation_type: String\` which is flexible

---

## Research Gaps We Should NOT Chase

### 1. LLM-as-Extractor
**Research**: GPT-NER, PromptNER use LLMs for zero-shot NER.

**Anno decision**: Keep bidirectional encoder focus (GLiNER-style). LLMs are:
- Too slow for production (sequential token generation)
- Too expensive for bulk processing
- Not deterministic (temperature > 0)

### 2. Multi-Agent Systems for NER
**Research**: OEMA uses ontology-guided multi-agent collaboration.

**Anno decision**: Single-model simplicity. Multi-agent complexity not justified for NER.

---

## Recommended Improvements (Low-Effort, High-Value)

1. **Add type_embedding field** to Entity for GLiNER-style semantic typing:
   \`\`\`rust
   pub type_embedding: Option<Vec<f32>>,
   \`\`\`

2. **Add type_path for hierarchical types**:
   \`\`\`rust
   pub type_path: Option<Vec<String>>, // e.g., ["Entity", "Person", "Artist"]
   \`\`\`

3. **Expand Relation to match Entity richness**:
   - Add confidence, provenance, kb_id fields to Relation

4. **Add relation type taxonomy** in TypeMapper:
   - Currently handles entity types; could add relation types too

---

## Summary Table

| Feature | Research SOTA | Anno | Status |
|---------|--------------|------|--------|
| Cross-doc coref hierarchy | X-AMR 3-level | Signal/Track/Identity | ✓ Aligned |
| Multimodal locations | GMNER bbox | Location enum | ✓ Exceeds |
| Decomposed confidence | FET research | HierarchicalConfidence | ✓ Aligned |
| KB linking | FELA KB-agnostic | kb_id + kb_name | ✓ Aligned |
| Temporal validity | TKG research | valid_from/until | ✓ Aligned |
| Type embeddings | GLiNER bi-encoder | Missing | ○ Gap |
| Hierarchical types | OntoType | Missing | ○ Gap |
| Free-form types | Ultra-Fine ET | Custom variant | ◐ Partial |
| Relation richness | TPLinker | Simpler | ○ Gap |

---

*Generated from research review: 2024-12 papers on GLiNER, X-AMR, GMNER, OntoType, FELA*
