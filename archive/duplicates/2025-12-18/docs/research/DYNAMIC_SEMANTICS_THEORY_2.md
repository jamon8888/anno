# Dynamic Semantics and Coreference Resolution: From Theory to Implementation

## Overview

This document traces the theoretical foundations of anno's coreference resolution
infrastructure from David Israel's 1994 critique of dynamic semantics through
Michael Strube's S-list model to modern neural approaches—and shows how anno
operationalizes these ideas.

## The Problem: Indeterminacy in Media Res

Israel (1994) "The Very Idea of Dynamic Semantics" identifies a fundamental issue:
the semantic import of a discourse marker cannot be determined from the linguistic
context encountered so far. Consider:

```
A man walked in. He sat down. He ordered coffee. ... He left.
```

At each pronoun "he", the antecedent seems clear—but what if sentence 47 introduces
"Another man walked in"? The *extent* of the first "man" as a discourse referent
is not statically determinable.

Israel draws an analogy to programming languages:

| Programming Concept | Discourse Analog | Anno Implementation |
|---------------------|------------------|---------------------|
| Variable scope | Syntactic scope of NP | `DiscourseScope` |
| Variable extent | How long referent is "live" | `EntityMemory` eviction |
| Garbage collection | Forgetting inactive referents | LRU/LFU policies |
| Assignment | Introducing new referent | `create_cluster()` |
| Dereference | Resolving anaphor | `find_best_match()` |

## Key Theoretical Models

### 1. Centering Theory (Grosz, Joshi, Weinstein 1995)

Centering posits that each utterance U_n has:

- **Cf(U_n)**: Forward-looking centers — entities mentioned, ranked by salience
- **Cb(U_n)**: Backward-looking center — the most salient entity from U_{n-1}
  that is also in U_n

Transition types based on Cb continuity:

| Transition | Cb(U_n) = Cb(U_{n-1})? | Cb(U_n) = highest Cf? |
|------------|------------------------|----------------------|
| CONTINUE | Yes | Yes |
| RETAIN | Yes | No |
| SHIFT | No | — |

**Preference**: CONTINUE > RETAIN > SHIFT

This predicts discourse coherence: texts that CONTINUE are easier to process.

### 2. S-List Model (Strube 1998)

Strube's "Never Look Back" simplifies centering to a single **salience list**:

> "I propose a model for determining the hearer's attentional state which depends
> solely on a list of salient discourse entities (S-list). The ordering among
> the elements of the S-list covers also the function of the backward-looking
> center in the centering model."

Key ranking criteria:
1. **Hearer-old before hearer-new**: Known entities rank higher
2. **Inter-sentential before intra-sentential**: Cross-sentence anaphora preferred
3. **Recency**: Recently mentioned entities rank higher
4. **Grammatical role**: Subjects > objects > obliques

This processes **incrementally, word by word**—no need to revise past decisions.

### 3. CT + Recency (Jiang et al. 2022)

Modern analysis shows:
- Vanilla centering theory provides little gain for neural coref
- **BUT**: CT augmented with recency bias captures more coreference signal
- Contextualized embeddings already encode much coherence information

**Key finding**: Recency is undermodeled in classical CT but crucial for coreference.

## Anno's Implementation

### The S-List as EntityMemory

```rust
/// Entity memory for incremental coreference.
pub struct EntityMemory {
    clusters: HashMap<u64, ClusterMetadata>,
    policy: MemoryPolicy,
    current_window: usize,
    l_cache: VecDeque<u64>,  // LRU order (recency)
    g_cache: Vec<u64>,       // LFU order (salience)
}
```

This IS Strube's S-list:
- `l_cache` maintains recency ordering
- `g_cache` maintains frequency/salience ordering
- `ClusterMetadata.last_accessed_window` tracks when entity was last mentioned
- `ClusterMetadata.access_count` tracks mention frequency

### Distance Decay

The mention-ranking module applies log-decay to distance:

```rust
score -= self.config.distance_weight * (distance as f64).ln().max(0.0);
```

This implements Shepard's Universal Law: probability of generalizing a response
decays exponentially with psychological distance.

### Centering Module (NEW)

Anno now includes explicit centering theory support:

```rust
use anno::discourse::centering::{
    CenteringState, CenteringTransition, ForwardCenter,
    track_centers, compute_transition
};
```

See `anno/src/discourse/centering.rs` for full implementation.

## The Epsilon-Term Connection

Israel discusses Hilbert's epsilon-terms as an alternative to standard quantifiers.
For each predicate A[x], the term ε_x(A[x]) denotes "some A" without specifying which.

For coreference, this suggests:

1. **Introduce** a discourse referent as an epsilon-term (uncertain reference)
2. **Refine** as more context arrives
3. **Resolve** when disambiguation is required or discourse ends

Anno implements this via `UncertainReference`:

```rust
pub struct UncertainReference {
    /// Candidate antecedents with probabilities
    pub candidates: Vec<(u64, f64)>,
    /// Whether resolution has been forced
    pub resolved: bool,
    /// The epsilon-term description
    pub description: String,
}
```

This allows maintaining uncertainty about referents until forced to commit.

## Information Status

Following Prince (1981) and Strube's hearer-old/hearer-new distinction:

```rust
pub enum InformationStatus {
    /// First mention, indefinite ("a man")
    New,
    /// First mention but inferrable ("the door" after "a room")
    Inferrable,
    /// Previously mentioned ("the man" after "a man")
    Evoked,
    /// Known from world knowledge ("the sun")
    Unused,
}
```

Information status affects salience ranking and antecedent selection.

## Practical Implications

### For Pronoun Resolution

1. **Pronouns prefer hearer-old antecedents** (high on S-list)
2. **Distance matters** — apply recency decay
3. **Gender/number agreement filters** but doesn't rank
4. **Grammatical role affects salience** — subjects outrank objects

### For Discourse Deixis

Abstract anaphora ("this", "that" referring to propositions) requires:

1. **Clause/sentence segmentation** — `DiscourseScope`
2. **Event extraction** — `EventMention`
3. **Shell noun classification** — constraints on antecedent type
4. **Bounded context windows** — n=2-3 clauses outperforms unbounded

### For Long Documents

At book scale (200k+ tokens):

1. **Incremental processing** is mandatory
2. **Memory bounds** via eviction policies (LRU, LFU, dual-cache)
3. **Cross-context merging** for entities that span windows

## References

```bibtex
@article{israel1994dynamic,
  title={The Very Idea of Dynamic Semantics},
  author={Israel, David},
  journal={Proceedings of the Ninth Amsterdam Colloquium},
  year={1994}
}

@inproceedings{strube1998never,
  title={Never Look Back: An Alternative to Centering},
  author={Strube, Michael},
  booktitle={COLING-ACL},
  year={1998}
}

@inproceedings{jiang2022centering,
  title={Investigating the Role of Centering Theory in Neural Coreference},
  author={Jiang, Yuchen Eleanor and Cotterell, Ryan and Sachan, Mrinmaya},
  booktitle={arXiv:2210.14678},
  year={2022}
}

@article{grosz1995centering,
  title={Centering: A Framework for Modeling Local Coherence},
  author={Grosz, Barbara J and Joshi, Aravind K and Weinstein, Scott},
  journal={Computational Linguistics},
  volume={21},
  number={2},
  year={1995}
}

@article{dalrymple1991ellipsis,
  title={Ellipsis and Higher-Order Unification},
  author={Dalrymple, Mary and Shieber, Stuart M and Pereira, Fernando CN},
  journal={Linguistics and Philosophy},
  volume={14},
  number={4},
  year={1991}
}

@inproceedings{guo2023dualcache,
  title={Dual Cache for Long Document Neural Coreference Resolution},
  author={Guo, Tianyu and others},
  booktitle={ACL},
  year={2023}
}
```

## See Also

- `anno/src/discourse/centering.rs` — Centering theory implementation
- `anno/src/discourse/types.rs` — Discourse referent types
- `anno/src/eval/incremental_coref.rs` — Incremental coreference
- `anno/src/backends/mention_ranking.rs` — Mention-ranking coreference
- `docs/research/ABSTRACT_ANAPHORA_RESEARCH.md` — Abstract anaphora background
