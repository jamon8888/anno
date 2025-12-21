# NER/Coref Research Synthesis: Gaps and Priorities for Anno

> Generated from comprehensive literature review. Maps research findings to Anno implementation priorities.

## Executive Summary

Anno has solid coverage of core NER and basic coreference, but significant gaps exist in:
1. **Cross-document event coreference** - The emerging standard
2. **Ancient/Historical languages** - Underexplored but with good resources
3. **Abstract anaphora & discourse** - Already started, needs completion
4. **Robustness evaluation** - Adversarial, OCR, temporal drift
5. **Joint NER+RE+Coref pipelines** - The future direction

---

## Priority 1: Cross-Context Coreference (Long-Doc + Cross-Doc Unified)

### Why It Matters
Standard entity coref links mentions within documents. **Cross-context** coreference (xCoRe formulation) unifies:
- **Long-document coref**: Document split into windows → merge clusters across windows
- **Cross-document coref**: Multiple documents → merge clusters across documents

The unification enables **joint training** on both settings, increasing data availability.

### Maverick Validation: TikTalkCoref (arXiv:2504.14321, Li et al. 2025)

**TikTalkCoref** is the first Chinese multimodal coreference dataset for social media dialogues (Douyin/TikTok comments). While the multimodal aspects are out of scope for anno, the textual coreference findings validate our Maverick implementation:

| Model | MUC F1 | B³ F1 | CEAFφ4 F1 | Avg F1 |
|-------|--------|-------|-----------|--------|
| **Maverick** | 51.9 | 68.1 | 76.3 | **65.5** |
| e2e-coref | 66.0 | 30.6 | 20.6 | 39.1 |

**Key findings:**
- Maverick significantly outperforms e2e-coref on Chinese social media dialogues (+26.4 Avg.F1)
- Singleton handling is critical: 44% pronouns, 34% proper names, 22% common nouns
- e2e-coref struggles with singletons (11.6 F1 vs Maverick's 51.3 F1)
- MUC favors e2e-coref (link-based), but B³/CEAF favor Maverick (entity-based)

**Implications for anno:**
- Validates `maverick_coref.rs` as SOTA choice for coreference
- Chinese social media domain requires explicit singleton handling
- Short dialogues have different mention type distributions than long documents

### xCoRe Architecture (EMNLP 2025, Martinelli et al.)

**Three-step pipeline**:
1. Within-context mention extraction (Maverick-style start-to-end)
2. Within-context mention clustering (LingMess multi-expert scorer)
3. **Cross-context cluster merging** (THE KEY INNOVATION)
   - Single-layer Transformer encodes cluster members → cluster embedding
   - Bilinear scorer: `p_merge(cluster_a, cluster_b) = sigmoid(W·concat(h_a, h_b))`
   - Single-pass merging (unlike hierarchical approaches)

**Results**: +6.7 F1 on ECB+ (40.3 vs 33.6), +7.2 on SciCo (30.5 vs 23.3) over predicted-mention baselines.

### Current State in Anno
- `ECBPlus` defined in registry
- `cdcr.rs` module exists with LSH blocking + Union-Find
- `incremental_coref.rs` has within-window coref (separate from cross-doc)
- `maverick_coref.rs` has the within-context architecture xCoRe builds on
- **Gap**: No learned cluster representations; no unified cross-context abstraction

See `systems/XCORE_INTEGRATION.md` for detailed integration design.

### Research-Backed Additions Needed

| Dataset | Description | URL Available | Priority |
|---------|-------------|---------------|----------|
| **ECB+META** | ECB+ with metaphoric paraphrases; ChatGPT-transformed sentences | Research access | HIGH |
| **GVC** | Gun Violence Corpus; tests domain transfer | Yes | HIGH |
| **FCC-T** | Football Coreference; requires temporal reasoning | Yes | MEDIUM |
| **MCECR** | Multilingual cross-document event coref | Research | MEDIUM |

### Implementation Notes
- Feature importance varies dramatically across corpora (ECB+ = lemma overlap; GVC/FCC = participant/time reasoning)
- Models trained on ECB+ don't generalize well - evaluate on all three

---

## Priority 2: Ancient & Historical Languages

### Why It Matters
Growing field with surprisingly good resources. Tests handling of:
- Morphological complexity (Latin, Greek, Sanskrit)
- Orthographic variation (no standardized spelling before 1700s)
- Script diversity (cuneiform, hieroglyphic)
- Fragmentary texts (lacunae filling)

### Current State in Anno
- `HIPE2022` defined (multilingual historical NER)
- No ancient language models or treebanks

### Research-Backed Additions Needed

| Resource | Languages | Type | Source |
|----------|-----------|------|--------|
| **ORACC** | Sumerian, Akkadian, Urartian | Cuneiform corpora | oracc.museum.upenn.edu |
| **LatinBERT** | Latin (Classical→Medieval) | Encoder | HuggingFace |
| **Ancient Greek UD** | Homeric→Byzantine | Treebank | universaldependencies.org |
| **Coptic Scriptorium** | Sahidic Coptic | Multi-layer | copticscriptorium.org |
| **LT4HALA Hebrew** | Biblical Hebrew | NER + Coref | LREC-COLING 2024 |

### Implementation Notes
- **Lacunae filling** - LatinBERT achieves 33% accuracy on textual emendations; could integrate as evaluation task
- **Pro-drop languages** - Greek, Latin, Hebrew are heavily pro-drop; coref must handle null subjects
- **Dialect variation** - "Ancient Greek" spans 1500 years and multiple dialects; model on Attic drops 20% on Byzantine

---

## Priority 3: Abstract Anaphora & Discourse

### Why It Matters
Standard coref links noun phrases. Abstract anaphora links pronouns ("this", "that") to *events*, *propositions*, or *situations* - fundamentally different task that current methods fail at.

### Theoretical Foundation: Higher-Order Unification

The Dalrymple, Shieber & Pereira (1991) paper provides a rigorous equational framework for anaphora resolution. The key insight: anaphora resolution = solving `P(s₁, ..., sₙ) = s` where `s` is the source interpretation and `P` is the property being predicated.

**Why this matters for implementation:**

1. **Strict vs sloppy readings emerge from equation solving**, not source ambiguity. This simplifies the resolution pipeline—we don't need multiple derivations of the source.

2. **Primary vs secondary occurrences** constrain abstraction. Primary occurrences (from parallel elements) *must* be abstracted; secondary (pronouns) *may* be. This maps directly to what we track in `DiscourseReferent`.

3. **Non-constituent abstractions are valid**. The recovered property need not correspond to any syntactic constituent—crucial for abstract anaphora where "this" refers to complex discourse segments.

4. **Cascaded resolution** works naturally: each equation is solved independently, and solutions can chain.

5. **Shell noun classes act as type constraints** on the unification variable—`ShellNounClass::Factual` constrains P to properties over facts/events.

6. **Quantifier scope interacts** with ellipsis resolution order—antecedent-contained ellipsis uses the "occurs check" to block problematic readings.

7. **Missing readings with multiple pronouns**: When two pronouns corefer with the same parallel element, abstracting deeply embedded but not shallower ones is blocked (3 readings, not 4).

8. **Obligatory sloppy readings** in control ("John tried to run, and Bill did too") and certain reflexives—controlled elements are primary occurrences.

See `systems/ABSTRACT_ANAPHORA_RESEARCH.md` for detailed treatment with examples.

### Current State in Anno
- `abstract_anaphora.rs` exists
- `shell_nouns.rs` exists  
- `discourse_deixis.rs` exists
- `bridging.rs` exists
- Good documentation in `systems/ABSTRACT_ANAPHORA_RESEARCH.md`
- `discourse/types.rs` now includes theoretical foundations
- **NEW**: Givenness Hierarchy cognitive status modeling (see below)

### Givenness Hierarchy Integration (NEW)

**Implemented in**: `anno/src/discourse/types.rs`

The Givenness Hierarchy (Gundel, Hedberg & Zacharski, 1993) is a foundational
psycholinguistic theory linking cognitive status to referring expression choice.
We've implemented the Cognitive Status Filter (CSF) from Pal et al. (2020, CogSci)
for tracking entity salience through discourse.

#### Theory

The GH proposes six hierarchically nested tiers of cognitive status:

```text
{in focus ⊆ activated ⊆ familiar ⊆ uniquely identifiable ⊆ referential ⊆ type identifiable}
```

| Status | Criteria | Referring Forms |
|--------|----------|-----------------|
| In Focus | Topic of last utterance | "it", "them" |
| Activated | Mentioned in last 2 clauses | "this", "that", "this N" |
| Familiar | Mentioned earlier in discourse | "that N" |
| Uniquely Identifiable | Identifiable from description | "the N" |

Key insight: The referring form a speaker uses *signals* their assumption about
the referent's cognitive status in the listener's mind.

#### Implementation Components

1. **`CognitiveStatus`** enum - Six-tier status hierarchy with form requirements
2. **`LinguisticStatus`** enum - Observation variable (NotMentioned/Mentioned/Topic)
3. **`ReferringForm`** enum - Classifier for referring expressions
4. **`TransitionMatrix`** - Learned 9×3 probability matrix for Bayesian updates
5. **`CognitiveStatusFilter`** - Per-entity Bayesian filter maintaining P(status|history)
6. **`CognitiveStatusEngine`** - Collection of CSFs with GetStatus algorithm

#### Usage for Coreference Resolution

The CSE enables GH-theoretic antecedent ranking in `DiscourseAwareResolver`:

```rust
// For "it", find InFocus candidates
let form = ReferringForm::classify("it");
let min_status = form.minimum_status(); // InFocus
let candidates = cse.entities_with_status_at_least(min_status);

// Candidates are sorted by P(InFocus) descending
```

#### Human Study Validation (Han et al. 2025)

GH-informed instruction sequencing produced (N=82):
- More natural instructions (BF₁₀ = 95.58, very strong evidence)
- More understandable instructions (BF₁₀ = 11.34, strong evidence)
- Lower cognitive workload for complex tasks

#### References

- Gundel, Hedberg & Zacharski (1993): "Cognitive Status and the Form of Referring
  Expressions in Discourse" - Language 69(2):274-307
- Pal et al. (2020): "Givenness Hierarchy Theoretic Cognitive Status Filtering" -
  CogSci 2020 - https://osf.io/qse7y/
- Williams & Scheutz (2018): "Reference in Robotics: A Givenness Hierarchy
  Theoretic Approach" - Oxford Handbook of Reference
- Han et al. (2025): "Givenness Hierarchy Theoretic Sequencing of Robot Task
  Instructions" - Frontiers in Robotics and AI

### Research-Backed Additions Needed

| Dataset | Focus | Size | Source |
|---------|-------|------|--------|
| **ARRAU 3.0** | Multi-genre with identity, bridging, discourse deixis, split antecedents | Full corpus | LDC + free subsets |
| **ISNotes** | Unrestricted bridging on OntoNotes | ~660 pairs | H-ITS |
| **BASHI** | Bridging subtypes (definite, indefinite, comparative) | ~400 pairs | ACL |
| **CSN/ASN** | Shell noun resolution | ~670 nouns | Toronto |
| **PDTB 3.0** | Discourse relations | 43 relations | Penn |
| **eRST** | Extended RST with graph structure | Full docs | Georgetown |

### Implementation Notes
- **dd-utt model** by Li & Ng (2022) is SOTA for discourse deixis - consider integration
- **Shell noun taxonomy** (Schmid 2000): factual, linguistic, mental, modal, eventive, circumstantial
- **LEA metric** should be used (not MUC/B³) for abstract anaphora - already implemented?

---

## Priority 4: Indigenous & Low-Resource Languages

### Why It Matters
- Tests generalization beyond well-resourced languages
- Unique linguistic features (polysynthetic morphology, pro-drop)
- Community-driven annotation priorities

### Current State in Anno
- `QxoRef` (Quechua coref) defined
- `AmericasNLI` defined
- `CherokeeNER` defined
- MasakhaNER entries in download script

### Research-Backed Additions Needed

| Dataset | Languages | Type | Notes |
|---------|-----------|------|-------|
| **Te Taumata** | Māori | Speech + NER | Community-in-the-loop; reduced WER 27%→10% |
| **AI4Bharat Naamapadam** | 11 Indic | NER | 5.7M sentences - largest Indic NER |
| **pywirrarika/naki catalog** | ~50 Indigenous American | Meta-resource | Comprehensive index |
| **ChoCo** | Choctaw | Multimodal | Audio, video, text |
| **Shipibo-Konibo** | Peruvian | UD + NER | Multiple NLP tools |

### Implementation Notes
- **Polysynthetic challenge**: Single word = entire sentence in Navajo/Inuktitut; breaks span-based NER
- **Pro-drop**: Quechua is heavily pro-drop; qxoRef had to handle null arguments
- **Orthographic variation**: Many languages lack standardized spelling

---

## Priority 5: Robustness & Adversarial Evaluation

### Why It Matters
Production NER faces noise not present in academic benchmarks:
- OCR errors from scanned documents
- Typographic attacks (Unicode homoglyphs)
- Temporal concept drift
- Emerging entities not in training

### Current State in Anno
- `adversarial_dataset()` in synthetic
- `robustness.rs` exists
- `drift.rs` exists

### Research-Backed Additions Needed

| Category | Evaluation Type | Implementation |
|----------|----------------|----------------|
| **Typographic** | Space injection, zero-width chars, homoglyphs | Synthetic generation |
| **OCR noise** | Character substitution, line break errors | Synthetic + historical |
| **Concept drift** | Same types, different time periods | BiTimeBERT-style evaluation |
| **Emerging entities** | Zero-shot novel entity detection | WNUT-17 style but temporal |
| **Entity injection** | "Ignore previous Corp." attacks | Security testing |

### Implementation Notes
- **WNUT16** has 89% unseen test entities vs CoNLL03's lower ratio - good for OOD testing
- **Dataset difficulty metrics** (2025 paper): Unseen Entity Ratio, Entity Ambiguity, Entity Density, Model Differentiation

---

## Priority 6: Arcane & Specialized Domains

### Why It Matters
Domain-specific NER often requires specialized entity types and has unique challenges.

### Current State in Anno
- Good coverage of biomedical
- Some legal/financial synthetic data

### Research-Backed Additions Needed

| Domain | Dataset | Key Challenge |
|--------|---------|---------------|
| **Astronomy** | Astro-NER, WIESP | Celestial objects, missions, instruments |
| **Archaeology** | Dutch Archaeology NER | 60k reports, artefact types, time periods |
| **Patents** | HUPD, TechNER | Technology extraction, FinTech taxonomy |
| **Food/Culinary** | NERsocial Food | Social media food mentions |
| **Fiction** | Fiction-NER 750M, FantasyCoref | Shape-shifting entities, magical aliases |

### Implementation Notes
- **Dutch Archaeology**: 658M words across 60k reports - massive specialized corpus
- **FantasyCoref**: Handles entity transformations (shape-shifting, possession, disguise) - unique challenge

---

## Priority 7: Bias & Fairness Evaluation

### Why It Matters
NER systems show systematic bias based on name origin and gender.

### Current State in Anno
- `GICoref` defined
- `WinoBias` defined
- `GAP` defined
- `bias_evaluation` category exists

### Research-Backed Additions Needed

| Dataset | Focus | Finding |
|---------|-------|---------|
| **WinoQueer** | Anti-LGBTQ+ bias | Community-in-the-loop design |
| **BBQ** | QA bias including sexual orientation | Hand-built ambiguous contexts |
| **GICoref** | Gender-inclusive coref | Neopronouns (ze/hir, xe/xem); singular they |
| **Name-based bias** | Ethnicity discrimination | Danish study: ethnicity bias > gender bias |

### Implementation Notes
- **Singular they**: Models trained on plural "they" struggle with singular antecedents
- **Neopronouns**: Near-zero recognition in most systems
- Dutch study: Adding 2% neutral pronoun examples to training closed performance gap

---

## Priority 8: Constructed & Hypothetical Languages

### Why It Matters
Tests model assumptions about language universals. Edge cases for tokenization and word order.

### Current State in Anno
- `download_extended_datasets.py` has Esperanto, Toki Pona, Klingon (synthetic)

### Research-Backed Additions Needed

| Language | Why Useful | Resource |
|----------|-----------|----------|
| **Esperanto UD** | ~2000 native speakers; tests "living" conlang handling | universaldependencies.org |
| **Toki Pona** | 120 words; tests compositional entity recognition | Fan corpora |
| **Klingon** | OVS order, highly regular; language ID surprisingly accurate | UDHR translation |
| **Tolkien Elvish** | Historical linguistics simulation | Fan corpora |

### Implementation Notes
- **Klingon effect**: 2025 study found LangID works surprisingly well on Klingon despite minimal training data
- **Toki Pona**: Forces compositional semantics - "jan pona" (person + good) = "friend"

---

## Priority 9: Genomic/Bioacoustic NLP (Exploratory)

### Why It Matters
DNA/RNA sequences and animal vocalizations are "languages" with sequential structure. Transfer learning from human NLP shows surprising success.

### Current State in Anno
Not implemented (probably out of scope)

### Research Highlights
- **DNABERT**: k-mer tokenization for DNA; masked language modeling
- **FinchGPT**: GPT on textualized birdsong; outperformed Markov models
- **NatureLM-audio**: Zero-shot species classification; human→animal transfer

### Implementation Notes
This is exploratory - probably not core Anno scope but interesting edge case.

---

## New Category Flags Needed

Based on research, add to `dataset_registry.rs`:

```rust
// New categories
is_event_coref      // Cross-document event coreference
is_ancient          // Ancient/classical languages
is_abstract_anaphora // Shell nouns, discourse deixis
is_low_resource     // Indigenous, endangered languages
is_constructed      // Conlangs (Esperanto, Klingon)
is_arcane_domain    // Astronomy, archaeology, etc.
is_adversarial      // Robustness evaluation
is_speech           // Audio/speech NER
```

---

## Dataset Groups for Evaluation

### Quick Evaluation (5 min)
```toml
quick_comprehensive = [
    "WikiGold",      # NER baseline
    "GAP",           # Coref
    "Wnut17",        # Social/emerging
    "BC5CDR",        # Biomedical
]
```

### Standard Evaluation (30 min)
```toml
standard = [
    "CoNLL2003Sample",
    "MultiNERD",
    "PreCo",
    "LitBank",
    "ECBPlus",
    "HIPE2022",
]
```

### Comprehensive Evaluation (2+ hours)
```toml
comprehensive = [
    # All standard +
    "ARRAU",        # Abstract anaphora
    "CorefUD",      # Multilingual coref
    "GVC",          # CDCR domain transfer
    "MasakhaNER",   # African languages
    "QxoRef",       # Indigenous coref
]
```

---

## References (Key Papers)

1. **CDCR Generalization**: Bugert et al. (2021) - Feature importance varies by corpus
2. **Ancient Language ML**: Survey in Computational Linguistics (2023)
3. **Abstract Anaphora SOTA**: Li & Ng (2022) dd-utt model
4. **Bridging**: Hou et al. (2018) ISNotes
5. **Dataset Difficulty**: Cambridge NLP (2025) statistical metrics
6. **Bias Evaluation**: Dutch gender-neutral pronoun study (2024)
7. **Ellipsis & Anaphora Theory**: Dalrymple, Shieber & Pereira (1991) - Higher-order unification for strict/sloppy resolution
8. **Shell Nouns**: Schmid (2000) - 670 English shell nouns, 6 semantic classes

---

---

## Priority 10: Generalized Entity Recognition (Cross-Substrate)

### The Abstraction

The core insight: NER/coreference/relation extraction are not NLP tasks—they're **cognitive primitives** for representing structured knowledge about any domain. Text is just one encoding.

The operations that define entity understanding are substrate-independent:

1. **Segmentation** — identify candidate "mention" regions in substrate
2. **Typing** — classify mentions into ontological categories
3. **Linking** — connect mentions to canonical entities (internal or external KB)
4. **Coreference** — group mentions referring to same underlying entity
5. **Relation extraction** — identify typed connections between entities
6. **Temporal tracking** — maintain entity identity through transformations

### Cross-Substrate Entity Analogs

| Substrate | Entity Analog | "Mention" | Identity Criterion |
|-----------|--------------|-----------|-------------------|
| **Text** | Named entity | Token span | String + context |
| **Video** | Object | Bounding box sequence | Visual continuity + appearance |
| **Audio** | Sound source | Spectrogram region | Acoustic fingerprint |
| **Sensor streams** | Anomaly/event pattern | Time window | Statistical signature |
| **EEG/neural** | Brain state / seizure | Spike train segment | Firing pattern coherence |
| **Molecular** | Functional group / motif | Atom subset | Bond graph substructure |
| **Music** | Motif / theme | Note sequence | Melodic-rhythmic identity |
| **Ecosystem** | Trophic guild / species | Population observation | Functional role in food web |
| **Swarm** | Emergent collective | Agent cluster | Behavioral coherence |

### Object Permanence as Temporal Coreference

In video understanding, **object permanence** is literally the coreference problem:

- An object disappears behind an occluder — does it still exist?
- A ball enters a cup, the cup moves — where is the ball?
- **Loci-Looped** maintains latent "what" and "where" encodings through occlusion via slot-based attention

This maps directly to text coreference: "The CEO left. **She** returned later." — we track an entity through a gap where it's not mentioned.

### Musical Entity Recognition

| Musical Concept | NLP Analog | Task |
|-----------------|------------|------|
| **Motif** | Named entity | Recognition of recurring melodic-rhythmic pattern |
| **Theme** | Entity cluster | Grouping of related motifs |
| **Leitmotif** | Entity linking | Mapping theme → character/concept |
| **Variation** | Coreference chain | Tracking transformed motif through piece |
| **Recapitulation** | Anaphora | Return to earlier material |

**Leitmotif** is entity linking: Wagner's "Siegfried motif" links to the character Siegfried across the Ring cycle.

### Molecular "Entity" Recognition

In chemistry, "entities" are:

- **Atoms** (nodes in molecular graph)
- **Bonds** (edges)
- **Functional groups** (subgraph patterns — analogous to multi-word entities)
- **Reactions** (graph transformations — analogous to events)

**MolGrapher** extracts molecular structure from images via keypoint detection (atom localization) + GNN classification (bond typing). This is **visual NER for chemistry**.

### Swarm/Collective Entity Identity

When does a swarm become an "entity"?

| Property | Implication for "Entity" |
|----------|-------------------------|
| **Emergent behavior** | Entity exists at collective level, not individual |
| **No central control** | No single "antecedent" to resolve |
| **Fluid membership** | Entity boundaries are fuzzy/temporal |
| **Behavioral coherence** | Identity defined by function, not structure |

This challenges NER assumptions: a "flock of birds" isn't reducible to individual birds for certain predicates ("the flock turned south").

### What This Means for Anno

The substrate determines:

- What counts as a "mention"
- What identity criteria apply
- What transformations preserve identity
- What relations are meaningful

**Implications for design**:

1. Core data structures (`Entity`, `Span`, `CorefChain`) should be generalizable
2. Evaluation metrics (MUC, B³, LEA) are substrate-independent
3. The `Model` trait could theoretically accept non-text inputs
4. Cross-modal entity alignment is a valid extension

**Near-term relevance**:

- **Multimodal NER** (text + image entity grounding)
- **Temporal entity tracking** (entity identity through state changes)
- **Symbolic grounding** (linking text mentions to KB entries with different modality representations)

### Critical Perspective

This abstraction has real limits:

1. **Not everything is NER** — Stretching too far risks semantic dilution
2. **The original task had specific affordances** — Discrete token spans, finite types, human-annotated gold standards
3. **Evaluation metrics don't transfer cleanly** — What's precision for musical motif detection?
4. **Domain-specific regularities matter** — NER works because natural language has capitalization, determiners, syntactic positions

The abstraction is *productive* but not *universal*. Use it as a lens for cross-domain insight, not a Procrustean bed.

---

## Priority 11: Niche Domain Datasets (Recently Added)

### Why It Matters

Any domain with expert practitioners and specialized vocabulary has latent annotation potential. The datasets exist implicitly in catalogs, reviews, specifications, and professional documentation — they just need extraction and formalization.

### Recently Integrated Datasets

| Dataset | Domain | Entities | Format | Notes |
|---------|--------|----------|--------|-------|
| **FINER** | Food & Ingredients | INGREDIENT, PRODUCT, QUANTITY, UNIT, STATE | CoNLL/IOB2 | 182k sentences from AllRecipes |
| **AnnoCTR** | Cybersecurity | 10 types + MITRE ATT&CK linking | JSONL/BIO | 400 threat reports, entity linking to KB |
| **Distant Listening Corpus** | Music Theory | KEY, CHORD, PHRASE, CADENCE | TSV | 1,283 scores, 240k labels, harmonic analysis |

### Additional Candidates (Not Yet Integrated)

| Domain | Dataset | Key Challenge |
|--------|---------|---------------|
| **Fashion** | DeepFashion, FashionBrain NER | 800k images, noisy product description labels |
| **Aviation** | aeroBERT-NER | SYSTEM, COMPONENT, REQUIREMENT from FARs |
| **Maritime** | SPSCD | 19k images, 27k ships, 12 vessel categories |
| **Architecture** | BTS Corpus | BIM entities from building specifications (French) |
| **Recipe** | RecipeDB | 10M ingredient phrases with Stanford NER |

### Implementation Notes

- **Cybersecurity** is particularly relevant: threat intelligence, MITRE ATT&CK mapping, security KB construction
- **Music** tests whether NLP techniques transfer to non-linguistic sequential structure
- **Food** has practical applications in recipe understanding, nutritional analysis, allergy detection

---

---

## Priority 12: Historical Document Entity Disambiguation

### Why It Matters

Historical documents present unique entity linking challenges:
- **Out-of-knowledgebase entities**: Many historical figures never made it to Wikipedia
- **OCR noise**: Scanned documents have character-level errors
- **Temporal language shift**: Language evolves over centuries
- **Confusable names**: Father-son pairs, common names across generations

### Research Background: Contrastive Entity Disambiguation (Arora et al. 2024)

**Paper**: "Contrastive Entity Coreference and Disambiguation for Historical Texts" (arXiv:2406.15576)

**Key Contributions:**
1. **WikiConfusables dataset**: 190M entity pairs from Wikipedia disambiguation pages
2. **Hard negative mining**: Family relationships from Wikidata, disambiguation page confusables
3. **Bi-encoder + HAC**: Contrastive training with 0.15 threshold for clustering
4. **Mean pooling for disambiguation**: Cluster prototype embeddings before KB lookup
5. **Threshold-based NIL detection**: No-match if similarity < threshold (tuned on validation)

**Performance:**
- 78.3% accuracy on historical newswire (vs 65% for next best)
- 98.2% on MSNBC (modern news)
- 90% on ACE2004

### Implementation Notes for Anno

| Component | Status | Location |
|-----------|--------|----------|
| **Bi-encoder architecture** | ✓ Exists | `linking/linker.rs` |
| **HAC clustering** | ✓ Exists | `anno-coalesce/hierarchical.rs` |
| **NIL detection** | ✓ Exists | `linking/nil.rs` |
| **Hard negative mining** | ○ Needed | See WikiConfusables pattern below |
| **Mean pooling prototypes** | ○ Needed | Add to `eval/cdcr.rs` |
| **FAISS integration** | ○ Future | See design notes |

### WikiConfusables Pattern

```rust
// Mining hard negatives from Wikipedia disambiguation pages
struct ConfusablesPair {
    context_a: String,      // "Kennedy met with advisors..."
    context_b: String,      // "Kennedy won the Senate race..."
    is_positive: bool,      // Same Kennedy or different?
    source: ConfusableSource,
}

enum ConfusableSource {
    DisambiguationPage,     // John Kennedy disambiguation page
    FamilyRelation,         // From Wikidata P22 (father), P25 (mother)
    InContextNegative,      // Other entities in same paragraph
}
```

### FAISS Integration Path

For billion-scale entity linking, FAISS provides:
- `IndexFlatIP` for exact inner product search
- `IndexIVFFlat` for approximate search with clustering
- GPU acceleration via `faiss-gpu`

**Recommended approach:**
1. Embed all KB entities once (offline)
2. Store in FAISS index
3. At inference: embed mention, query top-k from FAISS
4. Apply threshold for NIL detection

---

## Priority 13: Heritage Language Considerations for NER

### Why It Matters

Heritage speakers exhibit systematic linguistic patterns that affect NER:
- **Morphological erosion**: Simplified case marking, gender agreement
- **Code-switching**: Mixing heritage and dominant language
- **Name pattern variation**: Western vs. origin culture conventions
- **Word order changes**: Fixed SVO when case marking erodes

### Research Background: Heritage Languages (Montrul 2023)

**Paper**: "Heritage Languages: Language Acquired, Language Lost, Language Regained" (Annual Review of Linguistics)

**Key Findings for NLP:**
1. **Morphological simplification**: Heritage speakers of Russian, Korean, Turkish, Hindi show case marking erosion
2. **Word order rigidity**: Heritage speakers default to SVO even in free-word-order languages
3. **Age effects**: Earlier bilingualism → more simplification
4. **Code-switching is normal**: Not confused, just mixed input

### Implications for Anno Testing

Add heritage language patterns to test fixtures:

```rust
let heritage_test_cases = vec![
    // Code-switching (heritage Spanish/English)
    "María went to the store y compró leche.",
    
    // Morphological erosion (heritage Russian - case omission)
    "Иван видел Мария в магазин.",  // Should be "Марию" (accusative)
    
    // Honorific variation (heritage Korean)
    "Mr. Kim said 선생님 will arrive at 3pm.",
    
    // Name order variation (heritage Chinese)
    "Wang Xiaoming, also known as John Wang, presented...",
];
```

### Testing Recommendations

1. **Don't assume baseline grammar**: Heritage language text may have "errors" that are systematic
2. **Support mixed-script input**: "Dr. 田中 spoke at MIT" is valid heritage output
3. **Name pattern flexibility**: Same person may appear as "Zhang Wei" and "Wei Zhang"
4. **Morphological tolerance**: Case/gender marking may be inconsistent

---

## Priority 6: Arabic and Semitic Coreference (Zero Pronouns)

### Why It Matters
Arabic presents unique challenges for coreference that expose fundamental limitations in current architectures:
- **Pro-drop**: Anaphoric zero pronouns (AZPs) are frequent and have no surface realization
- **Rich morphology**: Verb conjugations encode person/gender/number; pronominal clitics attach to words
- **Flexible word order**: Both SVO and VSO are common; annotation conventions differ between datasets
- **Dialect variation**: MSA, Classical Arabic, and dialects (Egyptian, Gulf) require different handling

This is a test case for Anno's multilingual architecture: if we can handle Arabic zeros correctly, we can handle Japanese, Chinese, Spanish, and Korean.

### Current State in Anno
- `Language::Arabic` defined with RTL detection
- `OntoNotes50`, `OntoNotesCoref`, `OntoNotesArabicCoref`, `ACE2005Arabic`, `BOLTEgyptianCoref` in dataset registry
- `MentionType::Zero` variant for pro-drop representation (implemented 2025)
- `PhiFeatures` (person/number/gender) for morphological agreement
- `Mention::zero()` constructor for zero mentions with anchor position
- Coreference metrics (MUC, B³, CEAF, LEA) implemented - UA Scorer 2.0 integration pending for zero evaluation

### Research-Backed Findings (Aloraini et al. 2025)

**State of the Art on Arabic OntoNotes:**

| Model | MUC F1 | B³ F1 | CEAFφ4 F1 | Avg F1 |
|-------|--------|-------|-----------|--------|
| Bohnet et al. 2023 (seq2seq) | 70.9 | 66.6 | 68.4 | **68.7** |
| Lai & Ji 2023 (ensemble) | - | - | - | 66.7 |
| Aloraini et al. 2020 (AraBERT) | 66.8 | 61.3 | 63.5 | 63.9 |
| Rule-based baselines | <50 | <50 | <50 | <50 |

**Key insight**: Neural methods with AraBERT/mBERT dramatically outperform rule-based, but still lag English SOTA (~80+ F1).

### Zero Pronoun Statistics (OntoNotes Arabic)

| Split | Zero Pronouns |
|-------|---------------|
| Train | 3,495 |
| Dev | 474 |
| Test | 412 |

### Critical Gap: Comprehensive Coreference

**Problem**: Standard coreference resolves only *realized* mentions. Arabic requires **comprehensive coreference** that jointly resolves:
1. Overt noun phrases ("the president", "Barack Obama")
2. Overt pronouns (هو/huwa = "he", هي/hiya = "she")
3. Zero pronouns (dropped subjects marked as *pro* in OntoNotes)

**Best Results** (Aloraini et al. 2022):

| Model | MUC F1 | B³ F1 | CEAFφ4 F1 | Avg F1 |
|-------|--------|-------|-----------|--------|
| Joint Learning | 70.0 | 65.3 | 66.2 | **67.1** |
| Pipeline (LM repr.) | 66.5 | 61.2 | 62.7 | 63.5 |
| Chen et al. 2021 | 66.6 | 61.6 | 64.2 | 64.2 |

### Datasets Needed

| Dataset | Language | Size | Zero Pronouns | Status |
|---------|----------|------|---------------|--------|
| **OntoNotes 5.0 Arabic** | MSA | ~300k tokens | Yes | In registry |
| **ACE 2005 Arabic** | MSA | ~112k tokens | Partial | **Not in registry** |
| **BOLT Egyptian** | Egyptian Arabic | ~600k tokens | Yes | **Not in registry** |
| **QuarAna** | Classical (Quran) | ~128k tokens | Pronouns only | **Not in registry** |
| **A3C** | MSA | ~1.3M tokens | Pronouns only | **Not in registry** |

### Morphological Tokenization

**Critical finding**: Morpheme-based tokenization (ATB segmentation, Farasa, Madamira) significantly outperforms subword tokenization (BPE, WordPiece) for Arabic mention detection.

Example:
```
Input: وأبوه أخذه إليها
Subword: ["وأبوه", "أخذه", "إليها"]  # Misses clitics
Morpheme: ["و", "أب", "ه", "أخذ", "ه", "إلى", "ها"]  # Correct: "and", "father", "his", "took", "him", "to", "it"
```

The possessive clitic (ه = "his") and object clitic (ه = "him") must be identified as separate mentions.

### Universal Anaphora Scorer 2.0

The UA scorer (Yu et al. 2023) extends standard metrics to handle:
1. Zero pronouns in **non-gold** settings (predicted zeros, not just annotated)
2. Fuzzy matching for discontinuous spans
3. Split-antecedent anaphora

**Anno gap**: We reference UA datasets (CODI-CRAC) but don't implement UA scorer 2.0's zero handling.

### Implementation Notes

1. **Mention representation**: Add `ZeroMention` variant to handle pro-drop
   ```rust
   pub enum Mention {
       Overt { surface: String, span: (usize, usize) },
       Zero { position: usize, grammatical_role: Role },
   }
   ```

2. **ACE vs OntoNotes annotation difference**: ACE annotates verb conjugations as coreferent mentions; OntoNotes does not. Evaluation must account for this.

3. **Morphological preprocessing**: Consider Farasa or CAMeL Tools integration for Arabic tokenization.

4. **Dialect handling**: Egyptian Arabic (BOLT) requires different models than MSA—no cross-dialect transfer.

### References
- Aloraini et al. (2025): "A Survey of Coreference and Zeros Resolution for Arabic" - ACM TALLIP
- Yu et al. (2023): "Universal Anaphora Scorer 2.0" - IWCS
- Bohnet et al. (2023): "Coreference Resolution through seq2seq Transition-Based System" - TACL

---

## Priority 7: Joint Entity Analysis (Durrett & Klein 2014)

### Why It Matters
The Durrett & Klein (2014) joint model addresses a fundamental problem: NER, coreference, and entity linking have heavy interdependencies with **no obvious pipeline order**. Their structured CRF approach jointly models all three tasks, achieving state-of-the-art results on ACE 2005 and OntoNotes.

Key insight: Propagating information across coreference arcs informs both semantic typing and entity linking. Example: "Dell posted revenues... The company..." - the coreferent "the company" helps determine that "Dell" refers to the organization (not Michael Dell) and should link to the Dell Inc. Wikipedia article.

### Current State in Anno
- **`anno/src/joint/`**: Complete factor graph infrastructure implemented (36 tests passing)
  - `types.rs`: 
    - Core types: `VariableId`, `VariableType`, `JointVariable`, `Assignment`, `LinkValue`, `AntecedentValue`
    - Mention representation: `JointMention`, `MentionKind`
    - Score providers: `CorefScoreProvider`, `NerScoreProvider`, `LinkScoreProvider` traits with default implementations
    - Pruning: `CoarsePruner` for antecedent candidate reduction (k=5 threshold)
    - Configuration: `JointConfig`, `JointModelBuilder` pattern
    - Full `JointModel::analyze()` pipeline implemented
  - `factors.rs`:
    - Unary factors: `UnaryCorefFactor`, `UnaryNerFactor`, `UnaryLinkFactor` with precomputed scores
    - Cross-task factors: `LinkNerFactor`, `CorefNerFactor`, `CorefLinkFactor` with configurable weights
    - `WikipediaKnowledgeStore` for KB type lookups and entity relatedness
    - Comprehensive tests for factor interactions
  - `inference.rs`:
    - `BeliefPropagation` with parallel/sequential message schedules
    - `Message` type with log-space operations, damping, normalization
    - `Marginals` for MBR decoding with argmax and probability access
    - `InferenceConfig` for tuning (iterations, convergence threshold, damping)
    - Domain enumeration: `DomainValue`, `get_domain_values`, `apply_domain_value`
- **Existing components** ready for integration:
  - `MentionRankingCoref`: Provides antecedent ranking baseline
  - `EntityLinker`: Wikidata/Wikipedia linking with candidate generation
  - Multiple NER backends (`GLiNER`, `NuNER`, `CrfNER`)

### Implementation Notes
- **Pruning is critical**: Paper uses k=5 pruning threshold for coreference arcs (90% pruned, 98% gold retained)
- **Cross-task factors**:
  - Link+NER: Conjoin Wikipedia semantics (infoboxes, categories) with NER type
  - Coref+NER: Encourage consistent types across coreferent mentions
  - Coref+Link: Encourage related Wikipedia articles across coreference arcs
- **Loss functions**: Softmax-margin with task-specific weights (αc=1, αt=3, αe=0)
- **Efficiency**: ~2x slower than union of independent models

### Remaining Work
1. [x] Factor graph infrastructure with belief propagation inference
2. [x] Score provider traits for pluggable NER/coref/linking backends
3. [x] Coarse pruner for antecedent candidate reduction
4. [x] Full `JointModel::analyze()` pipeline with MBR decoding
5. [ ] Rich feature engineering for unary factors (Brown clusters, word shapes, gazetteers)
6. [ ] Wikipedia semantic extraction pipeline (infobox types, categories, copula)
7. [ ] Integration with existing `MentionRankingCoref` as score provider
8. [ ] Integration with `EntityLinker` for real link candidates
9. [ ] Learning objective implementation (AdaGrad + L1, softmax-margin loss)
10. [ ] Evaluation on ACE 2005 and OntoNotes with gold mentions
11. [ ] BIO-chain NER adaptation for divergent mention boundaries (OntoNotes setting)

### References
- Durrett & Klein (2014): "A Joint Model for Entity Analysis: Coreference, Typing, and Linking"
- ACE 2005 entity linking annotations: Bentivogli et al. (2010)
- Related: Singh et al. (2013) for joint coref+NER+RE

---

## Priority 8: Memory-Augmented Entity Tracking

### Why It Matters

Sequential text processing requires tracking entities across sentence boundaries. Standard transformers have a fixed context window; for long documents (10k+ tokens), memory mechanisms provide bounded-resource alternatives. Three paradigms have emerged:

1. **Incremental Entity Memory** (Referential Reader): Fixed-size memory cells updated via gating
2. **Graph-Based Refinement** (G2GT, GraphCoref): Iterative graph message passing
3. **Span-Attention with Memory** (SpanEIT): Cross-attention between entity/sentiment spans + GRU memory

### Research Papers Analyzed

#### The Referential Reader (Liu, Zettlemoyer, Eisenstein; ACL 2019)

**Core idea**: Fixed-length memory bank where each cell holds an entity representation. Reading the text triggers either:
- **Update**: GRU-like integration of new mention into existing cell (implies coreference)
- **Overwrite**: Replace cell with new entity (breaks coreference chain)

```
m_i(t) = u_i · GRU(x_t, m_i(t-1)) + (1-u_i) · o_i · x_t + (1-u_i) · (1-o_i) · m_i(t-1)
```

Where `u_i` is update gate, `o_i` is overwrite gate, `x_t` is current mention embedding.

**Insight**: Memory operations are differentiable gates—trained end-to-end.

**Implementation**: https://github.com/liufly/refreader (fairseq-based)

#### SpanEIT (Hossain et al.; arXiv:2509.11604, Oct 2025)

**Task**: Entity-level sentiment classification (related but distinct from coreference)

**Architecture components**:
1. BERT contextual embeddings
2. POS-based span extraction (entity spans + sentiment spans)
3. Combined syntactic + semantic graph: `G = (V, E_syn ∪ E_sem)`
4. GAT layers for graph message passing
5. Bidirectional multi-head attention: entity→sentiment and entity→sentence
6. Coreference-aware memory with GRU: `m_ci(t) = GRU(h^(L,t)_e, m_ci(t-1))`
7. Fusion + classification

**Auxiliary losses** (Equations 9-12):
- Span detection loss (BCE for sentiment span tokens)
- Pair relevance loss (entity-sentiment pair classification)
- Span relevance loss (overall span importance)

**Results**: 92.75% accuracy on FSAD, outperforming RoBERTa+BiLSTM+GAT baseline.

#### Graph Refinement for Coreference (G2GT; Miculicich & Henderson 2022)

Already documented in `graph_coref.rs`. Key insight: conditioning each iteration on the full predicted graph enables global transitivity.

### Current State in Anno

| Component | Anno Implementation | SpanEIT Equivalent | Gap |
|-----------|--------------------|--------------------|-----|
| Entity memory | `EntityMemory` with LRU/LFU/DualCache | GRU-based memory module | Heuristic vs learned |
| Graph reasoning | `GraphCoref` (iterative refinement) | GAT layers | Similar paradigm |
| Span extraction | NER backends (`Model` trait) | POS-based + heuristics | Anno uses neural, SpanEIT uses linguistic |
| Cross-mention attention | Not explicit | Bidirectional multi-head | **Gap**: Anno lacks explicit span-pair attention |
| Memory indexing | Cluster ID (uint) | Coreference cluster | Same concept |

**Anno's `EntityMemory` (incremental_coref.rs)**:

```rust
pub struct EntityMemory {
    clusters: HashMap<u64, ClusterMetadata>,
    next_cluster_id: u64,
    policy: MemoryPolicy,  // LRU, LFU, DualCache, Unbounded
    current_window: usize,
    l_cache: VecDeque<u64>,
    g_cache: Vec<u64>,
}
```

This is a **heuristic** implementation:
- String matching for cluster assignment
- Discrete add/evict operations
- No learned representations

**SpanEIT's Memory Module**:

```python
m_ci(t) = GRU(h^(L,t)_e, m_ci(t-1))  # Learned update
```

This is **neural**:
- GRU smooths entity representations across mentions
- Trained end-to-end
- Requires coreference supervision

### What's Worth Adopting

| Idea | Effort | Benefit | Verdict |
|------|--------|---------|---------|
| GRU-based memory for neural backends | HIGH (training infrastructure) | Moderate (cleaner representation) | LATER: Only if we build trainable coref |
| Syntactic + semantic graph fusion | MEDIUM | Moderate (co-occurrence signals) | CONSIDER: Could extend GraphCoref |
| Bidirectional span attention | MEDIUM | Moderate (explicit entity-context modeling) | CONSIDER: For discourse-aware resolution |
| Auxiliary span detection losses | HIGH (training) | Moderate (regularization) | LATER: Only for trained models |

### What's Out of Scope

**Sentiment classification**: Anno's scope is NER → Coreference → Relation Extraction → Event Extraction. Sentiment polarity classification is a different task family.

**The full SpanEIT architecture**: Designed for supervised sentiment classification, not unsupervised/heuristic coreference.

### Recommended Enhancements for Anno

1. **Co-occurrence graph edges in GraphCoref** (LOW effort):
   Add statistical co-occurrence edges alongside syntactic edges. In `graph_coref.rs`:
   
   ```rust
   // Current: syntactic edges only
   // Enhancement: add co-occurrence edges
   fn add_cooccurrence_edges(&mut self, window: usize) {
       for i in 0..self.num_mentions {
           for j in i+1..min(i+window, self.num_mentions) {
               if self.mentions_cooccur(i, j) {
                   self.add_edge(i, j, EdgeType::Cooccurrence);
               }
           }
       }
   }
   ```

2. **Span-pair attention for discourse-aware resolution** (MEDIUM effort):
   In `discourse/types.rs`, add explicit cross-attention between anaphor and candidate antecedent spans:
   
   ```rust
   pub fn compute_span_attention(
       anaphor: &DiscourseReferent,
       candidates: &[DiscourseReferent],
   ) -> Vec<f64> {
       // Bidirectional attention weights
   }
   ```

3. **Document the paradigm differences in EntityMemory** (LOW effort):
   Add a doc comment explaining the trade-offs:
   
   ```rust
   /// # Memory Paradigms in Entity Tracking
   ///
   /// | Approach | Memory | Update | Training |
   /// |----------|--------|--------|----------|
   /// | EntityMemory (Anno) | Discrete clusters | String match | None |
   /// | Referential Reader | Fixed cells | GRU gate | E2E |
   /// | SpanEIT | Per-cluster | GRU | Supervised |
   ```

### References

- Liu, Zettlemoyer & Eisenstein (2019): "The Referential Reader: A Recurrent Entity Network for Anaphora Resolution" - ACL 2019. https://arxiv.org/abs/1902.01541
- Hossain et al. (2025): "Dynamic Span Interaction and Graph-Aware Memory for Entity-Level Sentiment Classification" - arXiv:2509.11604
- Miculicich & Henderson (2022): "Graph Refinement for Coreference Resolution" - arXiv:2203.16574
- Martinelli, Barba & Navigli (2024): "Maverick: Efficient and Accurate Coreference Resolution Defying Recent Trends" - ACL 2024. https://arxiv.org/abs/2407.21489

---

## Priority 9: Efficient Coreference at Scale (Maverick Paradigm)

### Why It Matters

Recent SOTA coreference moved to 13B+ parameter generative models. Maverick (ACL 2024) challenges this, achieving 83.6 CoNLL-F1 with 500M parameters, 0.006x memory, and 170x faster inference.

### Architecture Overview

```text
1. Encode text with DeBERTa-v3
2. Mention Extraction (start + end boundary detection)
3. Multi-Expert Scorer (MES) for antecedent linking
   ├── Project start/end to K category-specific spaces
   ├── Bilinear scoring: s2s, e2e, s2e, e2s
   └── Categories: Proper, Nominal, Pronoun, ALL
4. Clustering via transitivity
```

**Key innovation**: Category-specific bilinear scorers (Proper/Nominal/Pronoun) improve precision without sacrificing recall.

### Current State in Anno

**Implemented**: `anno/src/eval/maverick_coref.rs` contains:
- `MaverickConfig` with all hyperparameters
- Multi-expert architecture documentation
- Category-specific mention handling

**Gap**: No actual model weights or inference. The Python `maverick-coref` package exists on PyPI for inference.

### Integration Options

1. **Python subprocess**: Call `maverick-coref` CLI from Rust
2. **ONNX export**: Convert PyTorch → ONNX, load in Rust
3. **Candle reimplementation**: Port DeBERTa + MES scorer to Candle

For production use, Option 2 (ONNX) provides the best balance of accuracy and deployment simplicity.

### References

- Martinelli, Barba & Navigli (2024): "Maverick: Efficient and Accurate Coreference Resolution Defying Recent Trends" - ACL 2024
- Code: https://github.com/SapienzaNLP/maverick-coref
- HuggingFace: https://huggingface.co/sapienzanlp/maverick-mes-ontonotes

---

## Next Steps

1. [x] Add high-priority datasets to `dataset_registry.rs` (FINER, AnnoCTR, Distant Listening)
2. [x] Add `cybersecurity` category flag
3. [x] Implement initial factor graph infrastructure for joint entity analysis (`anno/src/joint/`)
4. [ ] Add remaining category flags (event_coref, ancient, abstract_anaphora, etc.)
5. [ ] Update `download_extended_datasets.py` with URLs
6. [ ] Create evaluation harness for CDCR
7. [ ] Consider co-occurrence graph edges in GraphCoref (Priority 8)
8. [ ] Evaluate Maverick ONNX export for production coreference (Priority 9)
7. [ ] Complete abstract anaphora evaluation pipeline
8. [ ] Add robustness/adversarial generation
9. [ ] Consider multimodal entity alignment for future scope
10. [ ] Add WikiConfusables-style hard negative mining for entity disambiguation training
11. [ ] Add mean pooling for cluster prototypes before KB disambiguation
12. [ ] Add heritage language test examples to multilingual test fixtures
13. [ ] Document FAISS integration path for billion-scale entity linking
14. [ ] Complete joint model feature engineering and learning implementation

### Geometric Foundations (NEW - Dec 2025)

**Stub implementations completed** in `anno/src/geometric/`:

15. [x] **Sheaf Neural Networks**: Stub implementation with trait definitions
    - See `anno/src/geometric/sheaf.rs` for `SheafGraph`, `RestrictionMap`, `SheafCoref` trait
    - See [`GEOMETRIC_FOUNDATIONS.md`](../design/embeddings/GEOMETRIC_FOUNDATIONS.md) for theory
    - Apache 2.0 reference: twitter-research/neural-sheaf-diffusion
    - [ ] TODO: Port diffusion layer from PyTorch to Candle
16. [x] **Hyperbolic Embeddings**: Stub implementation with core operations
    - See `anno/src/geometric/hyperbolic.rs` for `HyperbolicEmbedding`, `PoincareDistance`
    - [ ] TODO: Implement Möbius addition, exponential/logarithmic maps
    - [ ] TODO: Add Riemannian optimizer for training
17. [x] **Persistent Homology**: Stub implementation with types
    - See `anno/src/geometric/tda.rs` for `PersistencePair`, `PersistenceDiagram`
    - [ ] TODO: Integrate with Python TDA libraries (giotto-tda, ripser)
    - [ ] TODO: Add coreference-specific analysis utilities
18. [ ] **eRST Integration**: Enhanced RST graph structure for discourse-aware coref
    - Graph-based (not tree-based) discourse relations
    - Enables non-projective cross-discourse links

**Unified traits** in `anno/src/geometric/mod.rs`:
- `GeometricMention`: Common interface for coreference scoring
- `GeometricSpace`: Common interface for embedding spaces
- `transitivity_violations()`: Detect transitivity inconsistencies
- `transitivity_consistency()`: Measure global consistency score

### xCoRe Integration (High Priority)

15. [ ] Add BookCoref and Animal Farm datasets to registry
16. [ ] Implement ClusterEncoder trait + Transformer-based encoder
17. [ ] Implement MergeScorer with bilinear classification
18. [ ] Unify `incremental_coref.rs` and `cdcr.rs` under CrossContextResolver trait
19. [ ] Add dynamic batching for cross-context training
20. [ ] Evaluate on ECB+, SciCo, LitBank, BookCoref
15. [x] **Arabic/Zero Pronoun Support** (partial):
    - [x] Add `MentionType::Zero` variant for pro-drop representation
    - [x] Add `PhiFeatures` (person/number/gender) for morphological agreement
    - [x] Add `Mention::zero()` constructor with anchor position and phi-features
    - [x] Add ACE Arabic, BOLT Egyptian, QuarAna, A3C to dataset registry
    - [ ] Implement UA scorer 2.0 zero pronoun alignment
    - [ ] Add Arabic morphological tokenization examples to test fixtures
    - [ ] Evaluate Farasa/CAMeL Tools integration for Arabic preprocessing

