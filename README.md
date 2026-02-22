# anno

Information extraction for unstructured text: named entity recognition (NER), within-document coreference resolution, and structured pattern extraction.

Dual-licensed under MIT or Apache-2.0. API docs: [docs.rs/anno](https://docs.rs/anno).

## Task definitions

**Named entity recognition.** Given input text, identify spans `(start, end, type, confidence)` where each span denotes a named entity [1, 2]. Entity types follow standard taxonomies (PER, ORG, LOC, MISC for CoNLL-style [2]) or caller-defined labels for zero-shot extraction. Offsets are **character offsets** (Unicode scalar values), not byte offsets; see [CONTRACT.md](docs/CONTRACT.md).

**Coreference resolution.** Identify mention spans and partition them into equivalence classes, where each class corresponds to a single real-world entity [3, 4]. "Sophie Wilson" and "She" in adjacent sentences form a cluster.

**Structured pattern extraction.** Dates, monetary amounts, email addresses, URLs, phone numbers via deterministic regex grammars.

## Backends

All backends produce the same output type: variable-length spans with character offsets.

| Backend | Architecture | Labels | Zero-shot | Weights | Reference |
|---|---|---|---|---|---|
| `stacked` (default) | Selector/fallback | Best available | No | HuggingFace (when ML enabled) | -- |
| `gliner` | Bi-encoder span classifier | Custom | Yes | [gliner_small-v2.1](https://huggingface.co/onnx-community/gliner_small-v2.1) | Zaratiana et al. [5] |
| `gliner2` | Multi-task span classifier | Custom | Yes | [gliner-multitask-large-v0.5](https://huggingface.co/onnx-community/gliner-multitask-large-v0.5) | [5] |
| `nuner` | Token classifier (BIO) | Custom | Yes | [NuNerZero_onnx](https://huggingface.co/deepanwa/NuNerZero_onnx) | Bogdanov et al. [6] |
| `w2ner` | Word-word relation grids | Trained (nested) | No | [w2ner-bert-base](https://huggingface.co/ljynlp/w2ner-bert-base) | Li et al. [7] |
| `bert-onnx` | Sequence labeling (BERT) | PER/ORG/LOC/MISC | No | [bert-base-NER-onnx](https://huggingface.co/protectai/bert-base-NER-onnx) | Devlin et al. [8] |
| `crf` | Conditional Random Field | Trained | No | Bundled (`bundled-crf-weights`) | Lafferty et al. [9] |
| `hmm` | Hidden Markov Model | Trained | No | Bundled (`bundled-hmm-params`) | [9] |
| `pattern` | Regex grammars | DATE/MONEY/EMAIL/URL/PHONE | N/A | None | -- |
| `heuristic` | Capitalization + context | PER/ORG/LOC | N/A | None | -- |
| `ensemble` | Weighted voting combiner | Mixed | Varies | Varies | -- |

ML backends are feature-gated (`onnx` or `candle`). Weights download from HuggingFace on first use. See [BACKENDS.md](docs/BACKENDS.md) for selection guidance and feature-flag details.

## Install

Not using crates.io as primary distribution; see [publish status](docs/PUBLISH_STATUS.md).

### Full CLI (`anno-cli`)

```sh
cargo install --path crates/anno-cli --bin anno --features "onnx eval"
```

Without cloning:

```sh
cargo install --git https://github.com/arclabs561/anno --package anno-cli --bin anno --features "onnx eval"
```

### Minimal CLI (package `anno`)

`anno extract` only:

```sh
cargo install --path . --bin anno
cargo install --path . --bin anno --features onnx  # with ML
```

### Offline

`ANNO_NO_DOWNLOADS=1` or `HF_HUB_OFFLINE=1` forces cached-only behavior.

## Examples

Full CLI (`anno-cli`) unless noted.

```sh
anno extract --text "Lynn Conway worked at IBM and Xerox PARC in California."
```

```text
PER:1 "Lynn Conway"
ORG:2 "IBM" "Xerox PARC"
LOC:1 "California"
```

JSON output (schema-stable; uses `pattern` for offline reproducibility):

```sh
anno extract --model pattern --format json --text "Contact jobs@acme.com by March 15 for the \$50K role."
```

```json
[
  {"text": "jobs@acme.com", "entity_type": "EMAIL", "start": 8, "end": 21, "confidence": 0.98},
  {"text": "March 15", "entity_type": "DATE", "start": 25, "end": 33, "confidence": 0.95},
  {"text": "$50K", "entity_type": "MONEY", "start": 42, "end": 46, "confidence": 0.95}
]
```

Zero-shot (custom entity types via GLiNER [5]):

```sh
anno extract --model gliner --extract-types "DRUG,SYMPTOM" \
  --text "Aspirin can treat headaches and reduce fever."
```

```text
drug:1 "Aspirin" symptom:2 "headaches" "fever"
```

Coreference:

```sh
anno debug --coref -t "Sophie Wilson designed the ARM processor. She revolutionized mobile computing."
```

```text
Coreference: "Sophie Wilson" → "She"
```

## Library

```toml
[dependencies]
anno = { git = "https://github.com/arclabs561/anno", rev = "<commit>" }
```

```rust
use anno::{Model, StackedNER};

let m = StackedNER::default();
let ents = m.extract_entities("Sophie Wilson designed the ARM processor.", None)?;
assert!(!ents.is_empty());
# Ok::<(), anno::Error>(())
```

## Architecture

| Crate | Purpose |
|---|---|
| `anno` (root) | Published facade; re-exports `anno-lib` |
| `anno-lib` | Backends, `Model` trait, extraction pipeline |
| `anno-core` | Stable data model (`Entity`, `Signal`, `Track`, `Identity`, `Corpus`) |
| `anno-eval` | Evaluation harnesses, dataset loaders, matrix sampling |
| `anno-cli` | Full CLI |
| `anno-metrics` | Shared evaluation primitives |
| `anno-lattix` | Adapters to `lattix` graph substrate |

Pipeline: Text -> Extract -> Coalesce -> structured output. See [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Evaluation

`anno-eval` provides dataset loading, backend-vs-dataset compatibility gating, and CoNLL-style [2] span-level evaluation (precision, recall, F1) with label mapping between backend and dataset taxonomies.

```sh
anno benchmark --model gliner --dataset conll2003
```

`anno sampler` (`--features eval`) exposes a [muxer](https://github.com/arclabs561/muxer)-backed randomized matrix sampler with two modes: **triage** (worst-first) and **measure** (ML-only stable measurement).

## Scope

Inference-time extraction only. Training is out of scope -- use upstream frameworks (Hugging Face Transformers, Flair [10], etc.) and export ONNX weights for consumption.

## Documentation

- [QUICKSTART](docs/QUICKSTART.md)
- [CONTRACT](docs/CONTRACT.md) -- offset semantics, scope, feature gating
- [BACKENDS](docs/BACKENDS.md) -- backend selection, architecture, feature flags
- [ARCHITECTURE](docs/ARCHITECTURE.md) -- crate layout, dependency flow
- [API docs](https://docs.rs/anno)
- [Changelog](CHANGELOG.md)

## References

1. R. Grishman and B. Sundheim. "Message Understanding Conference -- 6: A Brief History." *COLING*, 1996.
2. E. F. Tjong Kim Sang and F. De Meulder. "Introduction to the CoNLL-2003 Shared Task." *CoNLL*, 2003.
3. K. Lee, L. He, M. Lewis, and L. Zettlemoyer. "End-to-end Neural Coreference Resolution." *EMNLP*, 2017.
4. D. Jurafsky and J. H. Martin. *Speech and Language Processing*, Ch. 21, 3rd ed. draft, 2024.
5. U. Zaratiana, N. Tomeh, P. Holat, and T. Charnois. "GLiNER: Generalist Model for Named Entity Recognition using Bidirectional Transformer." *NAACL*, 2024.
6. D. Bogdanov, A. Mokhov, et al. "NuNER: Entity Recognition Encoder Pre-training via LLM-Annotated Data." arXiv:2402.15343, 2024.
7. J. Li, Y. Fei, et al. "Unified Named Entity Recognition as Word-Word Relation Classification." *AAAI*, 2022.
8. J. Devlin, M.-W. Chang, K. Lee, and K. Toutanova. "BERT: Pre-training of Deep Bidirectional Transformers." *NAACL*, 2019.
9. J. Lafferty, A. McCallum, and F. Pereira. "Conditional Random Fields." *ICML*, 2001.
10. A. Akbik, T. Bergmann, D. Blythe, K. Rasul, S. Schweter, and R. Vollgraf. "FLAIR: An Easy-to-Use Framework for State-of-the-Art NLP." *NAACL*, 2019.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
