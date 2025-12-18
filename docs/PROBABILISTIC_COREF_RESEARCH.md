# Probabilistic Coreference Resolution: Research Synthesis

**Status**: Research notes connecting foundational work to modern approaches.

---

## 1. Kehler (1997): The Foundation

### Key Features for Maximum Entropy Coreference

Kehler conditioned on binary features in three categories:

| Category | Features |
|----------|----------|
| **Lexical/String** | Exact match, head-noun match, partial overlap, shared modifiers |
| **Syntactic/Agreement** | Number agreement, gender agreement, person agreement, syntactic category (NP, subject vs. object) |
| **Positional/Distance** | Intervening mentions count, same sentence, adjacent sentences, most-recent compatible candidate |
| **Information-Structure** | Prominent position (subject), sentence-initial (topicality) |

These features are **coarse by modern standards** but Kehler showed they achieved reasonable
cross-entropy on the pairwise task. The insight: you don't need perfect pairwise predictions;
the Dempster-Shafer combination handles inconsistency.

### The Two-Stage Architecture

```
Stage 1: Pairwise MaxEnt Model
  Input:  Template pair (S, T) + context features
  Output: P(S ~ T | context)
  
Stage 2: Configuration Inference
  Input:  All pairwise P(i ~ j), incompatibility constraints
  Output: Distribution over partitions
  Method: Dempster's Rule or Merging Decision Model
```

**Critical insight**: Pairwise probabilities can be globally inconsistent. If P(A~D)=0.505
and P(C~D)=0.504 but A/C are incompatible, no valid configuration can satisfy both.
Dempster's Rule normalizes away conflict mass rather than averaging.

---

## 2. Modern Neural Approaches

### Lee et al. (2017): End-to-End Neural Coreference

**Not purely greedy.** The model:
1. Learns distributions over antecedents via marginal likelihood training
2. Uses aggressive pruning (unary mention scores) for computational efficiency
3. Factors scores into span embeddings + pairwise features

The pruning is **computational**, not conceptual—the underlying model maintains probabilistic
structure during training. At inference, it effectively takes argmax after pruning.

**Gap from Kehler**: Loses configuration-level uncertainty. No downstream system can ask
"how confident are you in this clustering?" Only gets the most-likely assignment.

### Clark & Manning (2016): Mention-Ranking with RL

Direct descendant of Kehler's "merging decision model":
- Models P(antecedent | mention) rather than P(config)
- Uses REINFORCE to optimize cluster-level metrics (not pairwise cross-entropy)
- Greedy decoding with learned scoring

### Meng & Rumshisky (2018): Triad Networks

Addresses transitivity that pairwise models miss:
- Input: Three mentions simultaneously
- Output: Affinity scores respecting mutual dependencies
- **Key**: If A~B and B~C, the model can enforce consistency with A~C

First neural system to model >2 mentions at the mention level. Achieved SOTA on CoNLL 2012
with gold mentions.

**Connection to Kehler**: Triads implicitly handle the transitivity constraints that caused
Kehler's pairwise inconsistencies. Rather than post-hoc Dempster combination, they build
consistency into the scoring function.

### Miculicich & Henderson (2022): Graph Refinement

**Key insight**: Model coreference as a graph where nodes are tokens and edges are
mention/coref links, then iteratively refine predictions using Graph-to-Graph Transformer.

**Architecture**:
```
Document D → Initial Graph G₀ (empty) → Predict G₁ (mentions only)
          → Refine G₂ (mentions + coref) → ... → Gₜ (converged)
```

**Graph encoding in attention**:
```
Attention(Q,K,V,Lk,Lv) = softmax(Q·(K+Lk)/√d)·(V+Lv)
where Lk = E(G^{t-1})·Wk, Lv = E(G^{t-1})·Wv
```

The previous iteration's graph is encoded via relation embeddings added to attention keys/values.

**Input vs Output Graph Asymmetry**:

| Graph Type | Mention Links | Coref Links |
|------------|---------------|-------------|
| **Input** (rich) | All tokens → head | All heads in cluster connected |
| **Output** (sparse) | Last → first token | ≥1 antecedent link |

This design provides rich context for encoding but sparse targets for prediction.

**Long Document Strategies**:
1. **Overlapping windows**: Split at K tokens, overlap K/2, join graphs
2. **Reduced document**: Two-stage—first model detects mentions, second operates on condensed input

**Results** (CoNLL 2012, SpanBERT-large):
- Baseline: 79.6 CoNLL F1
- G2GT reduced: **80.5 CoNLL F1** (+0.9)
- Optimal iterations: T=4 (more doesn't help)
- Complexity: O(N² × T) vs O(N⁴) for Lee et al.

**Connection to anno**: The "reduced document" strategy validates anno's Extract → Coalesce
two-stage pipeline. First detect mentions (NER), then resolve on the condensed entity set.
Iterative refinement pattern already exists in anno-strata (Leiden's local moving phase).

---

## 3. Bayesian Entity Resolution

### Steorts (2014+): Microclustering and Partition Priors

Traditional clustering assumes cluster size grows linearly with N. Entity resolution violates
this: one person appears in ~3 records regardless of whether the database has 1K or 1M entries.

**Microclustering property**: M_N / N → 0 in probability (largest cluster is sublinear in N).

**Partition Priors** (Ewens-Pitman class):
- Place prior on partition structure itself
- Allow posterior to learn entity count and sizes
- Handle uncertainty about how many true entities exist

**Kolchin Partition Models**:
- NBNB (Negative Binomial-Negative Binomial): Prior on both cluster count and sizes
- NBD (Negative Binomial-Dirichlet): Flexible for varying entity size distributions

**d-blink**: Distributed algorithm scaling to millions of records via:
- k-d tree blocking
- Partially-collapsed Gibbs sampling
- Preserved marginal posterior

---

## 4. Correlation Clustering Approximations

| Year | Method | Approximation | Notes |
|------|--------|---------------|-------|
| 2004 | Pivot (Bansal-Blum-Chawla) | 3 | Combinatorial, simple |
| 2015 | Standard LP rounding | 2.06 | CMSY STOC |
| 2022 | Improved LP rounding | 1.73 | Cohen-Addad et al., beats LP gap |
| 2024 | Cluster LP | **1.485** | Best known, factor-revealing SDP |

**Note**: The 2024 paper originally claimed 1.437 but retracted to 1.485 after proof gap.
Integrality gap of cluster LP is 4/3.

---

## 5. Evaluation Metrics Gap

### Kehler's Metric: Cross-Entropy

Kehler evaluated on `-log P(correct config)`, a **probabilistic** measure. Lower = better.
This measures **calibration**: does the model's confidence match reality?

### Modern Metrics: Clustering Structure

| Metric | Measures | Weaknesses |
|--------|----------|------------|
| MUC | Missing/spurious links | Ignores singletons, tolerates fragmentation |
| B³ | Mention-level overlap | Inflated by many singletons |
| CEAF | Entity alignment | Sensitive to alignment choice |
| LEA | Link-weighted by entity importance | Newer, less standardized |
| BLANC | Rand-index style | Explicit singleton handling |

**The Gap**: Modern metrics operate on **hard clusterings** after decoding. They don't see
model probabilities. Cross-entropy and calibration are **orthogonal**—you can have good
clustering F1 but poor calibration, or vice versa.

### Bridging the Gap

For applications needing confidence (data fusion, active learning):
1. Track cross-entropy/NLL alongside clustering metrics
2. Generate reliability diagrams / ECE for coreference decisions
3. Consider maintaining configuration distributions à la Kehler

---

## 6. Type-Theoretic Connections (Curry-Howard)

The Curry-Howard correspondence illuminates NER/coref architecture:

### Types as Propositions

| Concept | Type | Logical Reading |
|---------|------|-----------------|
| Entity | `Entity` | ∃(span, type). "entity exists here" |
| Extraction | `fn(&str) -> Result<Vec<Entity>>` | "given text, produce entity proofs" |
| Coreference | `fn(entities) -> Vec<Cluster>` | "given mentions, partition them" |
| Configuration | `CorefConfiguration` | Conjunction of cluster memberships |
| Distribution | `ConfigurationDistribution` | Weighted disjunction over configs |

### Evidence as Independent Proof Systems

Each `EvidenceSource` provides evidence from an independent domain:
- `StringSimilarity`: Syntactic proof (edit distance)
- `Embedding`: Semantic proof (vector space)
- `KnowledgeBase`: External authority proof

Mediation strategies are **proof combinators**:
- `Max`: Disjunction (any strong proof suffices)
- `Min`: Conjunction (all proofs must agree)
- `Product`: Independence assumption

### Implications for Design

1. **Make invalid states unrepresentable**: Encode invariants in types
2. **Functions are contracts**: Signatures promise what implementations deliver
3. **Composition preserves validity**: Combining valid entities yields valid results
4. **Testing verifies contracts**: Tests discharge proof obligations types can't

See `docs/TYPE_THEORY_AND_NER.md` for full treatment.

---

## 7. Research Directions for Anno

### Near-term
1. Implement cross-entropy evaluation for probabilistic backends (GLiNER, etc.)
2. Add calibration metrics (ECE, reliability curves) to eval framework
3. Expose configuration-level uncertainty in API for data fusion use cases
4. Add newtypes for `Probability`, `ValidSpan` to enforce invariants
5. **Graph-based coref with iterative refinement** (inspired by G2GT)

### Medium-term
1. Triad-style scoring for transitivity-aware clustering
2. Integration with Bayesian ER methods (partition priors) for entity linking
3. Active learning support using uncertainty from `ConfigurationDistribution`
4. Type-level encoding of partition validity
5. **Graph encoding in attention** for transformer backends (G2GT-style)

### Long-term
1. Full hypergraph evidence aggregation (see `research/HYPERGRAPH_EVIDENCE_DESIGN.md`)
2. Learned evidence combination (attention over sources) vs. fixed strategies
3. Cross-document configuration distributions with scalable inference
4. Explore dependent types for stronger compile-time guarantees
5. **Full G2GT training** with graph-conditioned transformers

---

## References

### Foundational

**Hirschman, Lynette; Robinson, Patricia; Burger, John; Vilain, Marc.** (1998).
"Automating Coreference: The Role of Annotated Training Data".
*AAAI 1998 Spring Symposium*. [arXiv:cmp-lg/9803001](https://arxiv.org/abs/cmp-lg/9803001)

> Key contribution: Analysis of interannotator agreement for MUC coreference.
> Found 84% of disagreements were systematic errors (missing pronouns, overlooked chains).
> Two-stage annotation (markable detection → linking) improved agreement from ~83% to ~91% F.
> Led to MUC-7's extensional/intensional distinction for grounding descriptions.

**Kehler, Andrew.** (1997). "Probabilistic Coreference in Information Extraction".
*Proceedings of the Second Conference on Empirical Methods in NLP (EMNLP-2)*.
Providence, RI. [arXiv:cmp-lg/9706012](https://arxiv.org/abs/cmp-lg/9706012)

> Key contribution: First to treat coreference as distribution over configurations.
> Introduced Dempster-Shafer evidence combination for resolving pairwise inconsistency.
> Compared evidential vs. merging decision approaches on FASTUS IE system.

**Dempster, Arthur P.** (1968). "A Generalization of Bayesian Inference".
*Journal of the Royal Statistical Society*, Series B, 30:205–247.
[JSTOR](https://www.jstor.org/stable/2984504)

> Foundation for belief functions. Shows how probability on underlying space
> induces upper/lower probabilities on observable space. The combination rule
> normalizes away conflicting mass rather than averaging.

### Neural Coreference

**Lee, Kenton; He, Luheng; Lewis, Mike; Zettlemoyer, Luke.** (2017).
"End-to-end Neural Coreference Resolution".
*EMNLP 2017*, Copenhagen. [ACL Anthology D17-1018](https://aclanthology.org/D17-1018/)

> First end-to-end model without syntactic parser or hand-engineered mention detector.
> Key innovation: consider all spans as potential mentions, learn antecedent distributions.
> Trained on marginal likelihood of gold antecedents. +1.5 F1 on OntoNotes.

**Lee, Kenton; He, Luheng; Zettlemoyer, Luke.** (2018).
"Higher-Order Coreference Resolution with Coarse-to-Fine Inference".
*NAACL 2018*, New Orleans. [ACL Anthology N18-2108](https://aclanthology.org/N18-2108/)

> Iteratively updates mention representations with weighted average of antecedent
> representations. Still makes local decisions but with refined representations.
> +3 F1 over 2017 model. Complexity remains O(N⁴).

**Meng, Yuanliang; Rumshisky, Anna.** (2018).
"Triad-based Neural Network for Coreference Resolution".
*COLING 2018*, Santa Fe. [ACL Anthology C18-1004](https://aclanthology.org/C18-1004/)

> First neural system to model >2 mentions simultaneously at mention level.
> Scores mention triples to capture transitivity constraints pairwise models miss.
> Can extend to polyads of higher order.

**Miculicich, Lesly; Henderson, James.** (2022).
"Graph Refinement for Coreference Resolution".
[arXiv:2203.16574](https://arxiv.org/abs/2203.16574)

> First Graph-to-Graph Transformer for coreference with iterative refinement.
> Models entire graph structure, conditions predictions on previous iterations.
> Reduced document strategy outperforms overlapping windows.
> +0.9 F1 over SpanBERT baseline with O(N² × T) complexity.

### Evaluation Metrics

**Moosavi, Nafise Sadat; Strube, Michael.** (2016).
"Which Coreference Evaluation Metric Do You Trust?"
*ACL 2016*, Berlin. [ACL Anthology P16-1060](https://aclanthology.org/P16-1060/)

> Introduces LEA (Link-based Entity-Aware) metric. Shows MUC/B³/CEAF have issues
> with singletons and partial matches. LEA weights entities by importance,
> measures internal link resolution proportionally.

### Bayesian Entity Resolution

**Steorts, Rebecca C.** (2014). "Entity Resolution with Empirically Motivated Priors".
*Bayesian Analysis*. [arXiv:1409.0643](https://arxiv.org/abs/1409.0643)

**Zanella, Giacomo et al.** (2016). "Flexible Models for Microclustering".
*NIPS 2016*. [arXiv:1610.09780](https://arxiv.org/abs/1610.09780)

> Introduces microclustering property: cluster sizes sublinear in N.
> Essential for entity resolution where entities appear few times regardless of corpus size.

### Correlation Clustering

**Bansal, Nikhil; Blum, Avrim; Chawla, Shuchi.** (2004).
"Correlation Clustering". *Machine Learning* 56(1-3):89-113.

> Original 3-approximation via random Pivot algorithm.

**Cohen-Addad et al.** (2023). "Understanding the Cluster LP for Correlation Clustering".
[arXiv:2404.17509](https://arxiv.org/abs/2404.17509)

> Current best 1.485-approximation via Cluster LP rounding.

### Type Theory

**Wadler, Philip.** (2015). "Propositions as Types".
*Communications of the ACM* 58(12):75-84.
[DOI:10.1145/2699407](https://doi.org/10.1145/2699407)

> Accessible introduction to Curry-Howard correspondence.
> Types = propositions, programs = proofs, evaluation = proof normalization.

