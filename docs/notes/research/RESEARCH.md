# Research Contributions and Implementations

This document clarifies what this codebase contains versus what implements existing research.

## Overview

This library primarily **implements** existing research papers and methods. The codebase includes:

1. **Architectural design** - Unified abstractions for NER, coreference, and evaluation
2. **Integration** - Combining multiple research findings into a cohesive evaluation framework
3. **Rust implementation** - Production-ready implementations of research methods

## Architectural Design

### 1. Grounded Entity Representation (`src/grounded.rs`)

The **Signal → Track → Identity** hierarchy provides a unified abstraction for entity representation across modalities (text and vision).

- **Signal**: Detection-level entity mentions (where + what)
- **Track**: Document-level coreference chains (same entity within document)
- **Identity**: Knowledge base-level resolution (canonical entity across documents)

This architecture separates concerns that are often conflated in NER systems, enabling:
- Clear separation between detection, clustering, and linking
- Unified treatment of text and visual signals
- Efficient streaming/incremental coreference

**Note**: The isomorphism between vision detection and NER is inspired by DETR and similar work.

### 1.5. Box Embeddings for Coreference (`src/backends/box_embeddings.rs`)

**Box embeddings** implement geometric representations (axis-aligned hyperrectangles) that encode logical invariants for coreference resolution. This work is related to the **matryoshka-box** research project (not yet published).

**Key innovation**: Box embeddings combine:
- **Geometric structure**: Hyperrectangles with min/max bounds
- **Logical invariants**: Transitivity, syntactic constraints (Principle B/C), temporal evolution
- **Uncertainty modeling**: Box volume = confidence
- **Noise robustness**: Gumbel-soft boundaries

**Research basis**: Implements ideas from BERE (2022), BoxTE (2022), and UKGE (2021), adapted for coreference resolution in the context of matryoshka-box research.

**Status**: Implementation complete, training system functional. Part of ongoing research collaboration.

### 2. Unified Evaluation Framework

The evaluation system integrates multiple recent research findings into a single framework:

- **Chain-length stratification** (arXiv:2401.00238) - Performs stratified analysis by chain length
- **Familiarity computation** (arXiv:2412.10121) - Quantifies label overlap to detect inflated zero-shot claims
- **Temporal stratification** - Analyzes performance across time periods
- **Confidence intervals** - Proper statistical reporting with sample variance

This integrates existing methods into a unified system.

### 3. Trait Architecture

The sealed trait pattern and unified `Model` interface enable:
- Swappable backends without code changes
- Consistent evaluation across different model types
- Type-safe abstractions for zero-shot, relation extraction, etc.

This is architectural design work, not algorithmic research.

## Implementations of Existing Research

### Backend Implementations

| Backend | Paper | What We Implement |
|---------|-------|-------------------|
| **GLiNER** | [arXiv:2311.08526](https://arxiv.org/abs/2311.08526) | Bi-encoder span-label matching for zero-shot NER |
| **NuNER** | NuMind research | Token-based zero-shot NER with BIO tagging |
| **W2NER** | [arXiv:2112.10070](https://arxiv.org/abs/2112.10070) | Word-word relation grid for nested/discontinuous entities |
| **TPLinker** | Original TPLinker paper | Handshaking matrix for relation extraction |
| **UniversalNER** | [arXiv:2308.03279](https://arxiv.org/abs/2308.03279) | Placeholder only - not fully implemented |
| **ModernBERT** | [arXiv:2412.13663](https://arxiv.org/abs/2412.13663) | Encoder implementation |
| **DeBERTaV3** | Original DeBERTa papers | Encoder implementation |

**Credit**: These are implementations of existing research. We do not claim algorithmic contributions.

### Coreference Metrics

All coreference metrics are standard implementations:

- **MUC** (Vilain et al., 1995)
- **B³** (Bagga & Baldwin, 1998)
- **CEAF-e/m** (Luo, 2005)
- **LEA** (Moosavi & Strube, 2016) - [arXiv:1605.06301](https://arxiv.org/abs/1605.06301)
- **BLANC** (Recasens & Hovy, 2011)
- **CoNLL F1** - Average of MUC, B³, CEAF

**Credit**: Standard implementations, no algorithmic changes.

### Evaluation Methods

| Method | Paper | Status |
|--------|-------|--------|
| Chain-length stratification | [arXiv:2401.00238](https://arxiv.org/abs/2401.00238) | Implemented |
| Familiarity computation | [arXiv:2412.10121](https://arxiv.org/abs/2412.10121) | Implemented (string-based, embedding placeholder) |
| Temporal stratification | TempEL, various temporal drift papers | Implemented |
| Robustness testing | RUIE-Bench, adversarial NER papers | Implemented |

**Credit**: These implement research findings. We integrate them but do not propose new methods.

## 2025-12: Paper → Implementation Mapping (CorefInst / CORE-KG / CoRE)

This section records what we actually implemented in `anno` after reading:
- CorefInst: `https://arxiv.org/abs/2509.17505`
- CORE-KG: `https://arxiv.org/abs/2506.21607`
- Inside CORE-KG (ablation): `https://arxiv.org/abs/2510.26512`
- CoRE (overthinking / early stopping): `https://arxiv.org/abs/2507.06087`

### CorefInst (arXiv:2509.17505) — multilingual coref + zero anaphora + long-doc stitching

- **What we implemented**
  - **CorefUD loader (local-only)**: CoNLL-U parsing that extracts coreference chains from `MISC` `Entity=` brackets and supports **empty nodes** (IDs like `8.1`) as `MentionType::Zero`. See `anno/src/eval/coref_loader.rs` (`parse_corefud_conllu`).
  - **Gold-mention coref evaluation mode**: `TaskEvalConfig.coref_use_gold_mentions` so coref evaluation on CorefUD can measure *clustering* rather than being blocked by NER mention coverage. See `anno/src/eval/task_evaluator.rs`.
  - **Zero-anaphor metric**: `ZeroAnaphorEvaluation` (“anaphor-decomposable” style) computed from `MentionType::Zero`. See `anno/src/eval/coref_metrics.rs`.
  - **Window fragmentation diagnostics**: `WindowFragmentationStats` to quantify boundary-induced cluster splitting for long-doc/windowed systems (proxy for local→global stitching quality). See `anno/src/eval/coref_metrics.rs` + surfaced in `anno/src/eval/task_evaluator.rs`.

- **What we did NOT implement (and why)**
  - CorefInst’s **instruction-tuned decoder-only LLM prompting** and **controlled inference** (ID-only placeholder decoding): Anno’s coref backends are not LLM-generative and we do not currently expose an LLM inference API that returns structured IDs from constrained generation.
  - CorefInst’s exact **overlapping frame stitching algorithm**: we added stitching *diagnostics* (fragmentation), but not a full LLM window-stitcher implementation.

- **Low-risk experiments now enabled**
  - Run `graph_coref` / mention-ranking coref in **gold-mention mode** across CorefUD languages and compare **CoNLL F1 vs `zero_f1` vs `window_fragmentation_rate`**.
  - Add a simple “proper-noun propagation” stitcher (CorefInst-inspired) and measure whether fragmentation drops without harming CoNLL F1.

### CORE-KG (arXiv:2506.21607) — KG construction; node duplication + legal noise metrics

- **What we implemented**
  - **CORE-KG-inspired diagnostics** for extracted entities:
    - `pred_duplication_rate`: conservative normalized duplicate detection (Unicode-aware).
    - `pred_noise_rate`: heuristic “garbage span” detection (numeric-only, punctuation-only, etc.).
    - Implemented in `anno/src/eval/metrics.rs` and surfaced from NER evaluation in `anno/src/eval/task_evaluator.rs`.

- **What we did NOT implement (and why)**
  - KG construction / GraphRAG pipeline, type-wise prompt chaining, legal-domain schemas: out of scope for Anno’s current focus (text NER/coref/RE evaluation) and would require an LLM orchestration layer + KG storage/query interfaces.

- **Low-risk experiments now enabled**
  - Track duplication/noise rates as guardrails when tuning NER label packs or post-processing filters (especially on noisy domains like OCR/social/legal).
  - Measure whether coref clustering reduces duplication on downstream “entity lists” (even without building a full KG).

### Inside CORE-KG (arXiv:2510.26512) — ablation study (coref vs structured prompts)

- **What we implemented**
  - We did not port their ablation pipeline; instead we implemented the *portable pieces* their ablation highlights:
    - duplication/noise diagnostics (above) to let us measure “CORE-KG-like” failure modes in Anno’s eval outputs.

- **What we did NOT implement (and why)**
  - Their LLM-prompt ablation variants rely on the same KG/prompting stack that Anno does not embed.

- **Low-risk experiments now enabled**
  - Add an evaluation report section that correlates **duplication/noise** with standard NER metrics across datasets (helps catch “looks good on F1 but outputs junk” regressions).

### CoRE (arXiv:2507.06087) — early stopping for redundant reasoning (hidden-state geometry)

- **What we implemented**
  - **Early-stop analogue** for a genuinely iterative inference loop we *do* have: `GraphCoref`.
    - Added `GraphCorefConfig.early_stop: Option<GraphCorefEarlyStopConfig>` and stop conditions based on observable state:
      - cycle detection via graph fingerprint repetition
      - stagnation via unchanged edge count for N iterations
    - Implemented in `anno/src/backends/graph_coref.rs`.

- **What we did NOT implement (and why)**
  - Hidden-state trajectory analysis (CoRE’s magnitude/angle signals) requires access to internal LLM activations; Anno’s coref backends do not expose those.

- **Low-risk experiments now enabled**
  - Benchmark `graph_coref` runtime vs quality with/without early-stop on large mention sets; check if speedups come “for free” on long-doc settings.
  - Use `GraphCorefStats.edge_history` to detect oscillatory parameter regimes during tuning.

### Dataset Loaders

All datasets are downloaded and parsed from existing sources:

- CoNLL-2003, WikiGold, WNUT-17, MultiNERD, GAP, LitBank, etc.
- No datasets are created by this project
- Proper attribution given in dataset documentation

## Research-Aware Design

The codebase is designed with awareness of recent research findings:

- **Evaluation pitfalls** - Addresses common mistakes identified in research
- **Stratified analysis** - Breaks down metrics by entity type, chain length, temporal strata
- **Statistical rigor** - Proper confidence intervals, sample variance
- **Label shift** - Detects when "zero-shot" evaluations have high label overlap

This is **integration work**, not novel research.

## Placeholders / Not Yet Implemented

Some features are documented but not fully implemented:

- **UniversalNER** - Placeholder only, LLM integration pending
- **Embedding-based familiarity** - Currently uses string similarity fallback
- **ReasoningNER** - Trait documented, not implemented
- **CMAS (multi-agent)** - Documented, not implemented

## Attribution

We strive to:

1. **Cite papers** in code comments and documentation
2. **Link to arXiv** when available
3. **Credit original authors** in relevant modules
4. **Be clear** about what's implementation vs. architectural design

If you find missing attributions, please open an issue.

## Summary

This codebase consists of:
- **Architectural design**: Unified abstractions and trait system
- **Implementation**: Backend implementations, metrics, dataset loaders (majority of codebase)
- **Integration**: Research-aware evaluation framework combining multiple findings

The main value is in the **unified architecture** and **production-ready Rust implementations** of research methods.

