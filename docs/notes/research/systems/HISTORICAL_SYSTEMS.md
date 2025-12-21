# Historical NER/Coref Systems

This document catalogs classical and pre-transformer NER and coreference resolution systems, documenting what's implemented in `anno` and what remains.

> **Primary Reference:** Munnangi (2020). "A Brief History of Named Entity Recognition."
> [arXiv:2411.05057](https://arxiv.org/abs/2411.05057)
>
> This document follows the structure and citations from Munnangi's survey,
> mapping each historical method to Anno's implementation.

## A Brief History of Named Entity Recognition

Named Entity Recognition first appeared at **MUC-6 (1996)** (Grishman & Sundheim), defining the task of identifying names of people, organizations, and locations in text. The task has evolved significantly over three decades.

Per Munnangi (2020), NER is part of a larger pipeline for knowledge extraction:

1. **Named Entity Recognition (NER)** - Identifying entity mentions
2. **Named Entity Disambiguation (NED)** - Resolving ambiguity
3. **Named Entity Linking (NEL)** - Connecting to knowledge bases

Anno implements all three stages through its `Signal → Track → Identity` hierarchy.

### Timeline of Key Developments

| Year | Milestone | Paper/Event | Impact |
|------|-----------|-------------|--------|
| 1987 | MUC-1 | Sundheim & Chinchor | First IE conference; no formal NER task |
| 1996 | **MUC-6** | Grishman & Sundheim | Formal NER definition (PER, ORG, LOC) |
| 1997 | HMM NER | Bikel et al. "Nymble" | First statistical NER system |
| 1999 | MUC-7 | MUC | Expanded entity types |
| 2000 | MEMM | McCallum et al. | Addressed HMM's independence assumptions |
| **2001** | **CRF** | Lafferty et al. | Solved label bias; dominated 2001-2015 |
| 2002 | CoNLL-2002 | Tjong Kim Sang | First multilingual NER shared task |
| 2003 | CoNLL-2003 | Tjong Kim Sang & De Meulder | De-facto benchmark (PER/LOC/ORG/MISC) |
| 2003 | Stanford NER | Finkel et al. | CRF-based; still widely used |
| 2011 | NLP from Scratch | Collobert et al. | CNN for NER; first neural approach |
| 2015 | BiLSTM-CRF | Huang et al. | Neural + CRF; SOTA until transformers |
| 2018 | ELMo | Peters et al. | Contextualized embeddings |
| 2018 | BERT | Devlin et al. | Transformers dominate NER |
| 2023+ | GLiNER, NuNER | Various | Zero-shot NER emerges |

### Methodological Evolution

```text
Rule-Based (1987-2000)      Statistical (1997-2015)      Neural (2011-present)
─────────────────────       ───────────────────────      ─────────────────────
Lexicons                    HMM (1997)                   CNN (2011)
Hand-crafted patterns       MEMM (2000)                  BiLSTM (2015)
Domain-specific rules       CRF (2001) ←──────────────── BiLSTM-CRF (2015)
High precision,             SVM (2002)                   Transformers (2018+)
low recall                  Feature engineering          Zero-shot (2023+)
```

### Understanding the Statistical Era (1997-2015)

The transition from rule-based to statistical methods was driven by three key innovations:

**1. Hidden Markov Models (HMM) - 1997**
- Generative model: P(words, tags) = P(tags) × P(words|tags)
- Bikel et al.'s "Nymble" achieved ~85% F1 on MUC
- Limitation: Cannot use arbitrary features (only word identity)
- Limitation: Independence assumption is violated

**2. Maximum Entropy Markov Models (MEMM) - 2000**
- Discriminative: Models P(tag|word, context) directly
- Can use arbitrary features (capitalization, prefixes, etc.)
- Limitation: "Label bias problem" - transitions depend only on state, not observation
- McCallum et al. 2000

**3. Conditional Random Fields (CRF) - 2001**
- Solves label bias by modeling entire sequence jointly
- P(y|x) = (1/Z) exp(∑ features × weights)
- Dominated NER from 2001-2015 (~88-91% F1)
- Lafferty et al. 2001; McCallum & Li 2003 (NER application)

```text
HMM:  y₁ → y₂ → y₃        ← States generate observations independently
      ↓    ↓    ↓
      x₁   x₂   x₃

MEMM: x₁ → x₂ → x₃        ← Observations inform transitions
      ↓    ↓    ↓           (but label bias: low-entropy states dominate)
      y₁ → y₂ → y₃

CRF:  x₁   x₂   x₃        ← Global normalization over entire sequence
      ↕    ↕    ↕           (no label bias; can use arbitrary features)
      y₁ ─ y₂ ─ y₃
```

**Why CRF Beat MEMM**: MEMM suffers from "label bias" - states with few successors effectively ignore observations. CRF's global normalization prevents this.

### Key Evaluation Metrics (Historical Context)

The MUC conferences established relaxed-match evaluation:
- **MUC-6 (1996)**: Correct type if overlap exists with ground truth
- **ACE (2004)**: Added subtypes, partial match scoring
- **CoNLL (2003)**: Exact match only - became the standard

CoNLL-2003's strict evaluation is still used today, though research has shown 7-10% of its labels are incorrect (CleanCoNLL, EMNLP 2023).

## NER Systems

### Implemented ✅

| System | Era | Module | CLI | Notes |
|--------|-----|--------|-----|-------|
| **CRF** | 2001-2015 | `CrfNER` | `--model crf` | Classical sequence labeling |
| **BiLSTM-CRF** | 2015-2018 | `BiLstmCrfNER` | `--model bilstm-crf` | Neural + CRF (heuristic fallback) |
| **HMM** | 1990s-2000s | `HmmNER` | `--model hmm` | Statistical Viterbi decoding |
| **Heuristic** | N/A | `HeuristicNER` | `--model heuristic` | Capitalization + context |
| **Pattern/Regex** | N/A | `RegexNER` | `--model pattern` | Dates, emails, URLs |
| **TPLinker** | 2020 | `TPLinker` | `--model tplinker` | Joint entity-relation |
| **UniversalNER** | 2023+ | `UniversalNER` | `--model universal-ner` | LLM-based zero-shot |
| **GLiNER** | 2023+ | `GLiNEROnnx` | `--model gliner` | Zero-shot spans (ONNX) |
| **NuNER** | 2024 | `NuNER` | `--model nuner` | Token classification (ONNX) |
| **W2NER** | 2022 | `W2NER` | `--model w2ner` | Nested entities (ONNX) |
| **BERT** | 2018+ | `BertNEROnnx` | (via ONNX) | Token classification |

### Not Yet Implemented ❌

#### Flair Embeddings (2018)
**Era:** Bridge between static word vectors and transformers  
**Architecture:** Character-level BiLSTM language model → contextual string embeddings  
**Performance:** ~93% F1 on CoNLL-2003 (without transformers!)  
**Key paper:** Akbik et al. 2018: "Contextual String Embeddings for Sequence Labeling"

**Why valuable:**
- Shows character-level context matters
- Often combined with word embeddings (GloVe + Flair)
- Fast inference compared to transformers

#### spaCy CNN (pre-v3)
**Era:** 2016-2020  
**Architecture:** CNN + transition-based parsing  
**Approach:** Bloom embeddings → CNN encoder → transition system  

**Why valuable:**
- Extremely fast inference
- Good accuracy/speed tradeoff
- Inspired modern efficient NER

## Coreference Resolution Systems

### Implemented ✅

| System | Era | Module | Notes |
|--------|-----|--------|-------|
| **T5 Seq2Seq Coref** | 2021+ | `T5Coref` | Text-to-text coreference |
| **E2E-Coref** | 2017-present | `E2ECoref` | Span-based neural coref (heuristic fallback) |
| **Mention-Ranking** | 2016-present | `MentionRankingCoref` | Feature-based antecedent ranking |
| **Box Embeddings** | 2020+ | `box_embeddings` | Geometric entity representation |
| **Rule-based clustering** | N/A | (in T5Coref) | Simple heuristic fallback |

### Not Yet Implemented ❌

#### Stanford CoreNLP Coref (Deterministic/Statistical)
**Era:** 2010-2016  
**Architecture:** Pipeline: mention detection → pairwise scoring → clustering  
**Approach:** Hand-crafted features (string match, syntax, semantic class)

**Why valuable:**
- Interpretable feature-based approach
- Good for analysis/debugging
- Works without neural networks

#### NeuralCoref (HuggingFace/spaCy)
**Era:** 2017-2019  
**Architecture:** Mention-ranking on top of spaCy pipeline  
**Performance:** Fast, reasonable accuracy

**How it works:**
1. Use spaCy parser/NER for mention detection
2. Neural ranker scores mention-antecedent pairs
3. Link to highest-scoring antecedent

**Why valuable:**
- Simpler than E2E-coref
- Faster inference
- Good integration point with existing NLP pipelines

#### Word-level Coref (Dobrovolskii 2021)
**Era:** 2021+  
**Architecture:** Token-level linking with RoBERTa  
**Approach:** Predict links at word level instead of span level  
**Complexity:** O(n²) instead of O(n⁴) for span-based

**Why valuable:**
- Much faster than span-based
- Competitive accuracy
- Scales to long documents

## Implementation Priorities

Based on research value and implementation complexity:

### Completed ✅
1. ~~**BiLSTM-CRF**~~ - Historical neural baseline, educational value (`BiLstmCrfNER`)
2. ~~**E2E-Coref**~~ - Modern coref standard, reuses span infrastructure (`E2ECoref`)
3. ~~**HMM**~~ - Classical statistical NER, educational value (`HmmNER`)
4. ~~**Mention-Ranking Coref**~~ - Simpler alternative to E2E (`MentionRankingCoref`)

### Remaining - Medium Priority
1. **Flair Embeddings** - Character-level context, pre-transformer SOTA
2. **Word-level Coref** - Efficient modern coref (O(n²) vs O(n⁴))

### Remaining - Lower Priority
3. **spaCy CNN** - Fast inference, transition-based
4. **Stanford pipeline coref** - Interpretable features

## Future Directions (from Survey)

Munnangi (2020) Section 7 identifies several future research directions. Here's Anno's coverage:

| Survey Direction | Anno Support | Module/Feature |
|------------------|--------------|----------------|
| **Active Learning** | ✅ Implemented | `anno::eval::active_learning` |
| **Semi-supervised** | Partial | Few-shot evaluation support |
| **Unsupervised NER** | Limited | Not primary focus |
| **Domain Adaptation** | ✅ Via zero-shot | GLiNER, NuNER with custom types |
| **Zero-shot Learning** | ✅ Extensive | GLiNER, NuNER, UniversalNER |
| **Few-shot Learning** | ✅ Implemented | `anno::eval::few_shot` |
| **Cross-domain Transfer** | ✅ Via zero-shot | Type-agnostic backends |

### Active Learning in Anno

Per Munnangi (2020): "Active learning offers a solution for [the annotation bottleneck], 
where it efficiently selects the set needed to label."

Anno implements four sampling strategies:

```rust
use anno::eval::active_learning::{ActiveLearner, SamplingStrategy};

let learner = ActiveLearner::new(SamplingStrategy::Uncertainty);
// Or: QueryByCommittee, Diversity, Hybrid
```

See `tests/active_learning_integration.rs` for examples.

### Zero-shot NER

The survey notes that neural methods "need a lot of manually annotated data." 
Anno's zero-shot backends (GLiNER, NuNER) address this:

```rust
let ner = GLiNEROnnx::new("onnx-community/gliner_small-v2.1")?;
let entities = ner.extract_with_types(text, &["disease", "medication"], 0.5)?;
```

## References

### Survey Papers
- [Munnangi 2020](https://arxiv.org/abs/2411.05057) - "A Brief History of Named Entity Recognition" (arXiv:2411.05057)
- [Li et al. 2018](https://arxiv.org/abs/1812.09449) - "A Survey on Deep Learning for Named Entity Recognition"
- [Nadeau & Sekine 2007](https://doi.org/10.1075/li.30.1.03nad) - Earlier NER survey

### Historical NER Papers (per survey citations)
- [Grishman & Sundheim 1996](https://aclanthology.org/C96-1079/) - MUC-6, formal NER definition
- [Bikel et al. 1997](https://aclanthology.org/A97-1029/) - "Nymble: HMM NER" (~85% F1)
- [McCallum et al. 2000](https://dl.acm.org/doi/10.5555/645529.658277) - Maximum Entropy Markov Models
- [Lafferty et al. 2001](https://dl.acm.org/doi/10.5555/645530.655813) - CRF introduction (solved label bias)
- [Tjong Kim Sang & De Meulder 2003](https://aclanthology.org/W03-0419/) - CoNLL-2003 shared task

### Neural Era Papers
- [Collobert et al. 2011](https://arxiv.org/abs/1103.0398) - "NLP from Scratch" (CNN for NER)
- [Huang et al. 2015](https://arxiv.org/abs/1508.01991) - BiLSTM-CRF
- [Lample et al. 2016](https://aclanthology.org/N16-1030/) - Neural Architectures for NER
- [Ma & Hovy 2016](https://aclanthology.org/P16-1101/) - BiLSTM-CNN-CRF
- [Peters et al. 2018](https://aclanthology.org/N18-1202/) - ELMo
- [Akbik et al. 2018](https://aclanthology.org/C18-1139/) - Flair embeddings
- [Devlin et al. 2019](https://aclanthology.org/N19-1423/) - BERT

### Coreference Papers
- [Lee et al. 2017](https://aclanthology.org/D17-1018/) - E2E neural coref
- [Lee et al. 2018](https://aclanthology.org/N18-2108/) - Higher-order coref
- [Joshi et al. 2020](https://aclanthology.org/2020.tacl-1.5/) - SpanBERT for coref
- [Dobrovolskii 2021](https://aclanthology.org/2021.emnlp-main.605/) - Word-level coref

### Datasets (per survey Section 2)
- MUC-6: Wall Street Journal (historical, limited availability)
- [CoNLL-2003](https://aclanthology.org/W03-0419/): Reuters news, de-facto benchmark
- OntoNotes: Multi-genre, 18 entity types
- [GENIA](https://pubmed.ncbi.nlm.nih.gov/12855455/): Biomedical (genes, proteins)
- [NCBI Disease](https://pubmed.ncbi.nlm.nih.gov/24393765/): PubMed abstracts

## Anno Implementation Mapping

| Survey Section | Anno Module | Test Coverage |
|----------------|-------------|---------------|
| §4.1 Rule-based | `RegexNER`, `HeuristicNER` | `property_backends.rs` |
| §4.2 HMM/CRF | `HmmNER`, `CrfNER` | `historical_methods_comparison.rs` |
| §5 BiLSTM-CRF | `BiLstmCrfNER` | `historical_methods_comparison.rs` |
| §5 BERT | `BertNEROnnx` | `backend_integration.rs` |
| §6 Active Learning | `active_learning` module | `active_learning_integration.rs` |
| §3 Metrics | `eval::ner_metrics`, `eval::modes` | `eval_integration.rs` |

## Contributing

To add a new historical system:

1. Create backend in `anno/src/backends/`
2. Implement `Model` trait
3. Add tests with historical benchmark data
4. Update this document
5. Add to `docs/BACKENDS.md` comparison table

When implementing a paper, add the citation to the module documentation:

```rust
//! # References
//!
//! - Author et al. (Year): "Paper Title" (Conference/Journal)
//! - [arXiv:XXXX.XXXXX](https://arxiv.org/abs/XXXX.XXXXX)
```

