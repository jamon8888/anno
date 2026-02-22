# anno

Information extraction for unstructured text: named entity recognition (NER), within-document coreference resolution, and structured pattern extraction. Written in Rust.

Dual-licensed under MIT or Apache-2.0.

API docs: [docs.rs/anno](https://docs.rs/anno) (may lag behind `main`; see [publish status](docs/PUBLISH_STATUS.md)).

## Task definitions

**Named entity recognition.** Given an input string `s`, identify spans `(start, end, type, confidence)` where each span denotes a named entity [1, 2]. Entity types follow standard taxonomies (PER, ORG, LOC, MISC for CoNLL-style [2]) or caller-defined labels for zero-shot extraction. Offsets are **character offsets** (Unicode scalar values), not byte offsets; see [interface contract](docs/CONTRACT.md).

**Coreference resolution.** Given a document, identify mention spans and partition them into equivalence classes (clusters), where each cluster corresponds to a single real-world entity [3, 4]. For example, "Sophie Wilson" and "She" in adjacent sentences form a cluster.

**Structured pattern extraction.** Identify and normalize domain-specific patterns (dates, monetary amounts, email addresses, URLs, phone numbers) via deterministic regex grammars.

## Backends

`anno` provides multiple NER backends spanning three architecture families. All backends produce the same output type: variable-length spans with character offsets.

| Backend | Architecture | Labels | Zero-shot | Weights | Reference |
|---|---|---|---|---|---|
| `stacked` (default) | Selector/fallback | Best available | No | HuggingFace (when ML enabled) | -- |
| `gliner` | Bi-encoder span classifier | Custom | Yes | [onnx-community/gliner_small-v2.1](https://huggingface.co/onnx-community/gliner_small-v2.1) | Zaratiana et al. [5] |
| `gliner2` | Multi-task span classifier | Custom | Yes | [onnx-community/gliner-multitask-large-v0.5](https://huggingface.co/onnx-community/gliner-multitask-large-v0.5) | [5] |
| `nuner` | Token classifier (BIO) | Custom | Yes | [deepanwa/NuNerZero_onnx](https://huggingface.co/deepanwa/NuNerZero_onnx) | Bogdanov et al. [6] |
| `w2ner` | Word-word relation grids | Trained (nested) | No | [ljynlp/w2ner-bert-base](https://huggingface.co/ljynlp/w2ner-bert-base) | Li et al. [7] |
| `bert-onnx` | Sequence labeling (BERT) | PER/ORG/LOC/MISC | No | [protectai/bert-base-NER-onnx](https://huggingface.co/protectai/bert-base-NER-onnx) | Devlin et al. [8] |
| `crf` | Conditional Random Field | Trained | No | Bundled (`bundled-crf-weights`) | Lafferty et al. [9] |
| `hmm` | Hidden Markov Model | Trained | No | Bundled (`bundled-hmm-params`) | [9] |
| `pattern` | Regex grammars | DATE/MONEY/EMAIL/URL/PHONE | N/A | None | -- |
| `heuristic` | Capitalization + context | PER/ORG/LOC | N/A | None | -- |
| `ensemble` | Weighted voting combiner | Mixed | Varies | Varies | -- |

Notes:

- ML backends are feature-gated behind `onnx` or `candle`. The published `anno` crate keeps defaults minimal; enable `onnx` explicitly for ML backends.
- ML model weights download from HuggingFace on first use (see "Offline / downloads" below).
- For backend selection guidance, architecture details, and feature-flag requirements, see [BACKENDS.md](docs/BACKENDS.md).
- Evaluation tooling (dataset/backend compatibility gates, label-shift accounting for true zero-shot evaluation) lives in `anno-eval`, not in the runtime `Model` trait surface.

## Install

This repo is **not** using crates.io as its primary distribution path; see [publish status](docs/PUBLISH_STATUS.md).

### Full CLI (`crates/anno-cli`)

The recommended binary (commands: `extract`, `debug`, `benchmark`, `models`, etc.):

```sh
cargo install --path crates/anno-cli --bin anno --features "onnx eval"
```

Or without cloning:

```sh
cargo install --git https://github.com/arclabs561/anno --package anno-cli --bin anno --features "onnx eval"
```

### Minimal facade CLI (package `anno`)

Supports `anno extract` only:

```sh
cargo install --path . --bin anno
```

With ONNX-backed ML selection for `stacked`:

```sh
cargo install --path . --bin anno --features onnx
```

## Offline / downloads

- The facade CLI does not enable ML backends by default.
- When ML backends are enabled (`--features onnx` / `candle`), weights download on first use.
- Force cached-only / offline behavior:
  - `ANNO_NO_DOWNLOADS=1` (preferred), or
  - `HF_HUB_OFFLINE=1`

## Examples

Most examples below assume the **full CLI** (`anno-cli`). The minimal facade CLI supports only `anno extract`.

### Named entities

```sh
anno extract --text "Lynn Conway worked at IBM and Xerox PARC in California."
```

```text
PER:1 "Lynn Conway"
ORG:2 "IBM" "Xerox PARC"
LOC:1 "California"
```

### Machine-readable output (JSON)

Schema-stable output (field values vary by backend). Uses `pattern` for offline reproducibility; all backends produce the same JSON shape:

```sh
anno extract --model pattern --format json --text "Contact jobs@acme.com by March 15 for the \$50K role."
```

```json
[
  {
    "text": "jobs@acme.com",
    "entity_type": "EMAIL",
    "start": 8,
    "end": 21,
    "confidence": 0.98
  },
  {
    "text": "March 15",
    "entity_type": "DATE",
    "start": 25,
    "end": 33,
    "confidence": 0.95
  },
  {
    "text": "$50K",
    "entity_type": "MONEY",
    "start": 42,
    "end": 46,
    "confidence": 0.95
  }
]
```

### Structured pattern extraction

```sh
anno extract --model pattern --text "Contact jobs@acme.com by March 15 for the \$50K role."
```

```text
EMAIL:1 "jobs@acme.com" DATE:1 "March 15" MONEY:1 "$50K"
```

### Zero-shot extraction (full CLI)

Define custom entity types at inference time via GLiNER [5]:

```sh
anno extract --model gliner --extract-types "DRUG,SYMPTOM" \
  --text "Aspirin can treat headaches and reduce fever."
```

```text
drug:1 "Aspirin" symptom:2 "headaches" "fever"
```

### Coreference resolution (full CLI)

```sh
anno debug --coref -t "Sophie Wilson designed the ARM processor. She revolutionized mobile computing."
```

```text
Coreference: "Sophie Wilson" → "She"
```

## Library (Rust)

```toml
[dependencies]
# Git (recommended while crates.io publishing is paused):
anno = { git = "https://github.com/arclabs561/anno", rev = "<commit>" }

# crates.io (may lag behind main):
# anno = "0.3"
```

```rust
use anno::{Model, StackedNER};

let m = StackedNER::default();
let ents = m.extract_entities("Sophie Wilson designed the ARM processor.", None)?;
assert!(!ents.is_empty());
# Ok::<(), anno::Error>(())
```

More examples: [QUICKSTART](docs/QUICKSTART.md).

## Architecture

`anno` is a Cargo workspace with six crates:

| Crate | Purpose |
|---|---|
| `anno` (root facade) | Published crate; re-exports `anno-lib` |
| `anno-lib` | Core library: backends, `Model` trait, extraction pipeline |
| `anno-core` | Stable data model (`Entity`, `Signal`, `Track`, `Identity`, `Corpus`) |
| `anno-eval` | Evaluation harnesses, dataset loaders, muxer-backed matrix sampling |
| `anno-cli` | Full CLI binary |
| `anno-metrics` | Shared evaluation/analysis primitives |
| `anno-lattix` | Adapters between `anno-core` and `lattix` graph substrate |

Dependency flow: `anno-cli` -> `anno-eval` -> `anno` -> `anno-core`; `anno-metrics` -> `anno-core`.

Pipeline: Text -> Extract (NER backends) -> Coalesce (merge overlapping spans) -> structured output.

For the full architecture and design rules, see [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Evaluation

`anno-eval` provides dataset loading, backend-vs-dataset compatibility gating, and CoNLL-style [2] span-level evaluation (precision, recall, F1). The evaluation harness accounts for label mapping between backend-specific and dataset-specific entity taxonomies.

Run benchmarks via the full CLI:

```sh
anno benchmark --model gliner --dataset conll2003
```

The `anno sampler` command (alias `anno muxer`, requires `--features eval`) exposes the muxer-backed [10] randomized matrix sampler used in CI:

- **triage**: regression-hunting defaults (worst-first routing)
- **measure**: stable measurement defaults (ML-only)

Use `anno help sampler` for details.

## Positioning

`anno` is an inference-time extraction library, not a training framework. Compared to spaCy [11] and Flair [12], which provide full training pipelines, `anno` focuses on multi-backend NER with zero-shot capability, character-offset contracts, and Rust-native inference. Compared to `rust-bert` [13], which wraps Hugging Face Transformers, `anno` adds backend orchestration (selector/fallback, ensemble, stacked), coreference resolution, and structured pattern extraction.

Training is explicitly out of scope. For model training, use upstream frameworks (Hugging Face Transformers, Flair, etc.) and export ONNX weights for consumption by `anno`.

## Documentation

- [QUICKSTART](docs/QUICKSTART.md) -- 5-minute CLI + library usage
- [CONTRACT](docs/CONTRACT.md) -- interface contract (offset semantics, scope, feature gating)
- [BACKENDS](docs/BACKENDS.md) -- backend selection, architecture details, feature flags
- [ARCHITECTURE](docs/ARCHITECTURE.md) -- crate layout, dependency flow, design rules
- [API docs (docs.rs)](https://docs.rs/anno)
- [Changelog](CHANGELOG.md)

## References

1. R. Grishman and B. Sundheim. "Message Understanding Conference -- 6: A Brief History." *COLING*, 1996. (Established NER as a formal task.)
2. E. F. Tjong Kim Sang and F. De Meulder. "Introduction to the CoNLL-2003 Shared Task: Language-Independent Named Entity Recognition." *CoNLL*, 2003. (Standard NER benchmark; PER/ORG/LOC/MISC taxonomy.)
3. K. Lee, L. He, M. Lewis, and L. Zettlemoyer. "End-to-end Neural Coreference Resolution." *EMNLP*, 2017. (End-to-end neural coreference baseline.)
4. D. Jurafsky and J. H. Martin. *Speech and Language Processing*, Ch. 21 (Coreference Resolution), 3rd ed. draft, 2024. (MUC, B-cubed, CEAF, LEA metrics.)
5. U. Zaratiana, N. Tomeh, P. Holat, and T. Charnois. "GLiNER: Generalist Model for Named Entity Recognition using Bidirectional Transformer." *NAACL*, 2024. (Zero-shot span classification.)
6. D. Bogdanov, A. Mokhov, et al. "NuNER: Entity Recognition Encoder Pre-training via LLM-Annotated Data." 2024. arXiv:2402.15343. (Token-level zero-shot NER.)
7. J. Li, Y. Fei, et al. "Unified Named Entity Recognition as Word-Word Relation Classification." *AAAI*, 2022. (W2NER; supports nested and discontinuous entities.)
8. J. Devlin, M.-W. Chang, K. Lee, and K. Toutanova. "BERT: Pre-training of Deep Bidirectional Transformers for Language Understanding." *NAACL*, 2019. (Foundation model for `bert-onnx` backend.)
9. J. Lafferty, A. McCallum, and F. Pereira. "Conditional Random Fields: Probabilistic Models for Segmenting and Labeling Sequence Data." *ICML*, 2001. (CRF for sequence labeling; also see Rabiner 1989 for HMM.)
10. Arc Labs. "muxer: Deterministic multi-objective routing for piecewise-stationary bandits." [github.com/arclabs561/muxer](https://github.com/arclabs561/muxer), 2025. (Backend selection routing in `anno-eval`.)
11. M. Honnibal, I. Montani, S. Van Landeghem, and A. Boyd. "spaCy: Industrial-strength Natural Language Processing in Python." 2020. (Industrial NLP pipeline.)
12. A. Akbik, T. Bergmann, D. Blythe, K. Rasul, S. Schweter, and R. Vollgraf. "FLAIR: An Easy-to-Use Framework for State-of-the-Art NLP." *NAACL (Demonstrations)*, 2019. (NLP framework with contextual string embeddings.)
13. G. Becquin. "rust-bert." [github.com/guillaume-be/rust-bert](https://github.com/guillaume-be/rust-bert), 2020. (Rust port of Hugging Face Transformers.)

## Citation

```bibtex
@software{anno,
  author  = {Arc Labs},
  title   = {anno: Information extraction for unstructured text},
  url     = {https://github.com/arclabs561/anno},
  version = {0.3.0},
  year    = {2025}
}
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
