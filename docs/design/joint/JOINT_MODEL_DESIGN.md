# Joint Entity Analysis: NER + Coreference + Linking

Based on Durrett & Klein (2014): "A Joint Model for Entity Analysis: Coreference, Typing, and Linking"

## Paper Summary

The Durrett & Klein paper models three tasks jointly as a structured CRF:

1. **Coreference** (a): Antecedent variables `a_i ∈ {1,...,i-1,NEW}` for each mention
2. **NER/Typing** (t): Semantic type variables `t_i` for each mention  
3. **Entity Linking** (e): Wikipedia article variables `e_i` for each mention
4. **Latent Query** (q): Latent query variables for entity linking candidate generation

```
P(a,t,e|x; θ) ∝ exp(θᵀf(a,t,e,q,x))
```

### Key Insight: Cross-Task Interactions

The power comes from **binary and ternary factors** coupling predictions across tasks:

| Factor Type | What It Captures |
|-------------|------------------|
| Link+NER | Wikipedia semantics (infobox, categories) ↔ NER type |
| Coref+NER | Consistent types across coreference chains |
| Coref+Link | Related entities link to related Wikipedia articles |

Example (Figure 1): "Dell posted revenues... The company..."
- "The company" is clearly coreferent with "Dell"
- Coreference tells us "Dell" = ORGANIZATION (not person Michael Dell)
- Therefore link to `en.wikipedia.org/wiki/Dell` not `Michael_Dell`

### Inference

- Belief propagation (3-5 iterations)
- Pruning with coarse model (90% arcs pruned, 98% gold recall)
- MBR decoding on marginals

### Results

| Dataset | Joint vs Independent |
|---------|---------------------|
| ACE 2005 Coref | +1.23 F1 |
| ACE 2005 NER | +2.90 accuracy |
| ACE 2005 Linking | +2.07 accuracy |
| OntoNotes Coref | 61.71 F1 (SOTA at time) |
| OntoNotes NER | 84.04 F1 (SOTA at time) |

## Anno's Current Architecture

### What We Have

```
anno/
├── NER (Model trait)
│   ├── GLiNER, NuNER (zero-shot)
│   ├── CRF, HMM (classical)
│   └── StackedNER (composite)
├── Coreference (CoreferenceResolver trait)
│   ├── SimpleCorefResolver (rule-based)
│   ├── MentionRankingCoref
│   ├── DiscourseAwareResolver
│   └── BoxCorefResolver
├── Entity Linking (linking/)
│   ├── WikidataLinker
│   ├── CandidateGenerator
│   └── NilDetector
└── Pipeline: Extract → Coalesce → Stratify
```

### Gap Analysis (Updated)

| D&K Component | Anno Status | Notes |
|---------------|-------------|-------|
| Coreference (antecedent model) | ✅ `MentionRankingCoref` | Full features in `score_pair()` |
| NER (semantic typing) | ✅ `Model` trait | GLiNER, NuNER, CRF, etc. |
| Entity Linking | ✅ `WikidataLinker` | Search, type mapping |
| **Joint factors** | ✅ **Implemented** | All unary + cross-task factors |
| Belief propagation | ✅ **Implemented** | Sum-product with damping |
| Coarse pruning | ✅ **Implemented** | `CoarsePruner` in types.rs |
| Wikipedia semantics | ✅ **Implemented** | `SemanticsProvider` trait |
| Learning | ✅ **Implemented** | Softmax-margin + AdaGrad |
| Latent query variables | ❌ Not implemented | q_i for candidate generation |

## Integration Plan

### Phase 1: Factor Graph Infrastructure

```rust
/// Factor in a structured CRF
pub trait Factor: Send + Sync {
    /// Variables this factor touches
    fn scope(&self) -> &[VariableId];
    
    /// Log potential (unnormalized log probability)
    fn log_potential(&self, assignment: &Assignment) -> f64;
    
    /// Feature vector for learning
    fn features(&self, assignment: &Assignment) -> Vec<Feature>;
}

/// Variable types in the joint model
pub enum JointVariable {
    /// Antecedent for mention i: a_i ∈ {1,...,i-1,NEW}
    Antecedent { mention_idx: usize },
    /// Semantic type for mention i: t_i ∈ EntityTypes
    SemanticType { mention_idx: usize },
    /// Entity link for mention i: e_i ∈ WikipediaTitles
    EntityLink { mention_idx: usize },
    /// Latent query for mention i (marginalized)
    Query { mention_idx: usize },
}
```

### Phase 2: Cross-Task Factor Implementations

**Link+NER Factor** (§3.2.1):
```rust
pub struct LinkNerFactor {
    mention_idx: usize,
}

impl Factor for LinkNerFactor {
    fn log_potential(&self, a: &Assignment) -> f64 {
        let link = a.get_link(self.mention_idx);
        let ner_type = a.get_type(self.mention_idx);
        
        // Features from Wikipedia article semantics
        let mut score = 0.0;
        
        if let Some(wiki_info) = self.get_wiki_info(link) {
            // Infobox type ↔ NER type
            if wiki_info.infobox_type.compatible_with(ner_type) {
                score += self.weights.infobox_match;
            }
            
            // Categories ↔ NER type
            for cat in &wiki_info.categories {
                if cat.suggests_type(ner_type) {
                    score += self.weights.category_match;
                }
            }
            
            // First sentence copula
            if wiki_info.copula_suggests_type(ner_type) {
                score += self.weights.copula_match;
            }
        }
        
        score
    }
}
```

**Coref+NER Factor** (§3.2.2):
```rust
pub struct CorefNerFactor {
    mention_i: usize,
    mention_j: usize,
}

impl Factor for CorefNerFactor {
    fn log_potential(&self, a: &Assignment) -> f64 {
        // Only fires if i→j is a coreference link
        if a.get_antecedent(self.mention_i) != Some(self.mention_j) {
            return 0.0;
        }
        
        let type_i = a.get_type(self.mention_i);
        let type_j = a.get_type(self.mention_j);
        
        let mut score = 0.0;
        
        // Type pair feature
        score += self.weights.type_pair(type_i, type_j);
        
        // Monolexical features (type + head word)
        let head_i = self.get_head(self.mention_i);
        let head_j = self.get_head(self.mention_j);
        score += self.weights.type_head(type_i, head_j);
        score += self.weights.type_head(type_j, head_i);
        
        score
    }
}
```

**Coref+Link Factor** (§3.2.3):
```rust
pub struct CorefLinkFactor {
    mention_i: usize,
    mention_j: usize,
}

impl Factor for CorefLinkFactor {
    fn log_potential(&self, a: &Assignment) -> f64 {
        if a.get_antecedent(self.mention_i) != Some(self.mention_j) {
            return 0.0;
        }
        
        let link_i = a.get_link(self.mention_i);
        let link_j = a.get_link(self.mention_j);
        
        let mut score = 0.0;
        
        // Same title
        if link_i == link_j {
            score += self.weights.same_title;
        }
        
        // Share outlinks
        if self.share_outlinks(link_i, link_j) {
            score += self.weights.shared_outlinks;
        }
        
        // Link to each other
        if self.links_to(link_i, link_j) || self.links_to(link_j, link_i) {
            score += self.weights.mutual_link;
        }
        
        score
    }
}
```

### Phase 3: Inference

```rust
/// Belief propagation for joint inference
pub struct BeliefPropagation {
    factors: Vec<Box<dyn Factor>>,
    variables: Vec<JointVariable>,
    messages: MessageStore,
    max_iterations: usize,
}

impl BeliefPropagation {
    pub fn run(&mut self) -> Marginals {
        for _ in 0..self.max_iterations {
            // Update variable-to-factor messages
            for var in &self.variables {
                self.update_var_to_factor_messages(var);
            }
            
            // Update factor-to-variable messages
            for factor in &self.factors {
                self.update_factor_to_var_messages(factor);
            }
            
            // Check convergence
            if self.converged() {
                break;
            }
        }
        
        self.compute_marginals()
    }
}
```

### Phase 4: Pruning with Coarse Pass

Per §5, prune coreference arcs using a fast baseline model:

```rust
/// Prune antecedent candidates using coarse model
pub fn prune_antecedents(
    mentions: &[Mention],
    coarse_model: &MentionRankingCoref,
    threshold: f64,  // Paper uses k=5 in log space
) -> Vec<Vec<usize>> {
    let mut candidates = Vec::with_capacity(mentions.len());
    
    for i in 0..mentions.len() {
        let marginals = coarse_model.antecedent_marginals(mentions, i);
        let best_score = marginals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        
        let pruned: Vec<usize> = marginals.iter()
            .enumerate()
            .filter(|(_, &score)| score >= best_score - threshold)
            .map(|(j, _)| j)
            .collect();
        
        candidates.push(pruned);
    }
    
    candidates
}
```

## What This Enables

### 1. Better NER via Coreference Context

Before:
```
"Dell posted revenues... The company..."
NER: Dell = PERSON (common first name pattern)
```

After (joint):
```
Coref: "The company" → Dell
Joint: "company" in antecedent text → Dell = ORGANIZATION
NER: Dell = ORGANIZATION ✓
```

### 2. Better Linking via Type Constraints

Before:
```
Mention: "Dell"
Candidates: [Dell (company), Michael Dell (person), Dell Inc., ...]
Linking: Ambiguous
```

After (joint):
```
NER: Dell = ORGANIZATION
Linking: Filter to org candidates → Dell (company) ✓
```

### 3. Better Coreference via Entity Knowledge

Before:
```
"Obama met Putin. He said..." 
Coref: "He" → ? (could be either)
```

After (joint):
```
Link(Obama) = Q76, Link(Putin) = Q7747
Wiki: Both are PERSON, heads of state
Coref: Use entity knowledge to inform pronoun resolution
```

## Implementation Status

| Step | Component | Status |
|------|-----------|--------|
| 1 | Factor trait and message passing | ✅ `joint/factors.rs`, `joint/inference.rs` |
| 2 | Unary factors | ✅ `UnaryCorefFactor`, `UnaryNerFactor`, `UnaryLinkFactor` |
| 3 | Link+NER factor | ✅ `LinkNerFactor` with `SemanticsProvider` |
| 4 | Coref+NER factor | ✅ `CorefNerFactor` with monolexical features |
| 5 | Coref+Link factor | ✅ `CorefLinkFactor` with `WikipediaKnowledgeStore` |
| 6 | Coarse pruning | ✅ `CoarsePruner` with threshold k=5 |
| 7 | BP marginalization | ✅ Sum-product algorithm in `BeliefPropagation` |
| 8 | Learning | ✅ Softmax-margin + AdaGrad in `learning.rs` |
| 9 | Latent queries | ❌ q_i variables for candidate generation |

### What's Working Now

The inference pipeline is complete:
1. Input: entities from NER
2. Build variables (with pruned antecedents)
3. Build factors (unary + cross-task)
4. Run belief propagation
5. Decode via MBR
6. Output: typed entities, coreference chains, links

### What's Remaining

1. ~~**Learning infrastructure**~~ - ✅ Implemented in `learning.rs`
2. **Latent query variables** - For end-to-end linking candidate generation
3. **Real dataset training** - ACE 2005 / OntoNotes integration
4. **Integration tests** - Verify joint > independent on benchmarks
5. **Performance optimization** - Currently O(d^k) for k-ary factors

## Connection to Existing Anno Code

| Anno Module | Role in Joint Model |
|-------------|---------------------|
| `MentionRankingCoref` | Coarse model for pruning; unary coref factor features |
| `EntityLinker` | Unary link factors; candidate generation |
| `Model` (GLiNER etc.) | Unary NER factors |
| `WikidataLinker` | Source of Wikipedia semantics for Link+NER |
| `BoxCorefResolver` | Alternative geometric inference (future) |

## Learning Infrastructure (§4 of Paper)

### Objective Function: Softmax-Margin

The paper uses **softmax-margin** (Gimpel & Smith 2010) to incorporate task-specific loss:

```
L(θ) = Σ_d log Σ_{a*∈A(C*)} p'(a*,t*,e*|x; θ)

where:
  p'(a,t,e|x; θ) ∝ p(a,t,e|x) exp[α_c·ℓ_c(a,C*) + α_t·ℓ_t(t,t*) + α_e·ℓ_e(e,e*)]
```

This shapes the distribution to penalize outputs that are bad according to loss functions.

### Loss Functions

| Task | Loss Function | Weights |
|------|--------------|---------|
| **Coreference** | α_FA=0.1 (false anaphor), α_FN=3 (false new), α_WL=1 (wrong link) | α_c=1 |
| **NER** | Hamming distance | α_t=3 |
| **Linking** | Hamming distance | α_e=0 (not sensitive) |

### Optimization: AdaGrad with L1

```
θ_{t+1} = θ_t - η/√(Σ g²) · (∇L + λ·sign(θ))

where:
  η = learning rate
  g = gradient history
  λ = L1 regularization strength (paper uses 0.001)
```

### Implementation Requirements

To add learning to Anno's joint model:

1. **Feature Extraction**: Factor must provide `features(assignment) -> Vec<Feature>`
   ```rust
   pub trait Factor {
       fn log_potential(&self, assignment: &Assignment) -> f64;
       fn features(&self, assignment: &Assignment) -> Vec<Feature>;  // NEW
   }
   ```

2. **Gradient Computation**: Compute expected features under model vs. data
   ```rust
   // Expected features under model distribution (from BP marginals)
   let expected = sum_v P(v|x) * features(v)
   
   // Features under gold assignment
   let gold = features(a*, t*, e*)
   
   // Gradient = expected - gold
   let gradient = expected - gold
   ```

3. **Loss Augmentation**: Modify inference to include loss
   ```rust
   impl BeliefPropagation {
       pub fn run_with_loss(&mut self, gold: &Assignment, loss_config: &LossConfig) -> Marginals {
           // During inference, add loss-augmented potentials
           // This is equivalent to redefining ψ'(v) = ψ(v) · exp(α·ℓ(v, v*))
       }
   }
   ```

4. **Weight Tying**: Link weights to features via feature templates
   ```rust
   pub struct FeatureTemplate {
       name: String,
       extractor: Box<dyn Fn(&Assignment) -> Vec<(String, f64)>>,
   }
   
   // Weight lookup: θ[feature_name] -> weight
   pub struct WeightStore {
       weights: HashMap<String, f64>,
   }
   ```

### Training Data Requirements

| Dataset | What's Annotated | Use For |
|---------|-----------------|---------|
| **ACE 2005** | Mentions, types, coref chains, entity links | Full joint training |
| **OntoNotes** | Mentions, types, coref chains (no links) | NER + Coref (latent links) |
| **TAC-KBP** | Entity links | Link-only evaluation |

### Training Pipeline

```
1. Load annotated documents
2. Extract mentions (from gold or predicted)
3. For each document:
   a. Build factor graph
   b. Run BP to get marginals
   c. Compute expected features
   d. Compute gold features
   e. Compute gradient = expected - gold
   f. Update weights via AdaGrad
4. Evaluate on dev set
5. Repeat until convergence
```

### Current Status (Updated)

| Component | Status | Notes |
|-----------|--------|-------|
| Factor log_potential | ✅ Implemented | All factors have potentials |
| Factor features() | ✅ Implemented | `features()` method in `learning.rs` |
| BP inference | ✅ Implemented | Sum-product with damping |
| Loss augmentation | ✅ Implemented | Softmax-margin in `Trainer` |
| AdaGrad optimizer | ✅ Implemented | Per-parameter adaptive LR |
| Feature templates | ✅ Implemented | `FeatureExtractor` trait |
| Training loop | ✅ Implemented | Full pipeline in `learning.rs` |
| Gradient verification | ✅ Verified | Numerical gradient check passes |

### Training Verification

Gradient checking confirms analytical gradients match numerical (finite differences):

```
Test case: type_match
Analytical gradients:
  type_match: 1.000000
Numerical gradients:
  type_match: 1.000000
Gradient check: PASS (rel_diff: 0.000000)
```

Training on synthetic data shows loss decreasing from 0.68 → 0.46, with learned weights:
- `string_match > 0` (exact string matches are coreferent)
- `distance_decay < 0` (penalizes distance between mentions)

### Next Steps

With learning infrastructure complete, remaining work:
1. **Feature enrichment** - More granular cross-task features
2. **Real dataset training** - ACE 2005, OntoNotes integration
3. **Hyperparameter tuning** - Loss weights (α_c, α_t, α_e)
4. **Evaluation harness** - Compare joint vs independent

The system is now ready for:
- Training from annotated corpora
- Zero-shot transfer from component models
- Demonstrating joint inference benefit

## Research Extensions

The D&K architecture naturally extends to:

1. **Relation extraction** - Add relation variables r_ij between entity pairs
2. **Event extraction** - Add event trigger variables, argument linking
3. **Cross-document** - Identity variables across document boundaries

These align with anno's roadmap (SCOPE.md v0.3+).

## References

- Durrett & Klein (2014): "A Joint Model for Entity Analysis: Coreference, Typing, and Linking" TACL
- Durrett & Klein (2013): "Easy Victories and Uphill Battles in Coreference Resolution" EMNLP
- Durrett et al. (2013): "Decentralized Entity-Level Modeling for Coreference Resolution" ACL
- Singh et al. (2013): "Joint Inference of Entities, Relations, and Coreference" AKBC Workshop

