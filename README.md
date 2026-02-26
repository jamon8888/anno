# anno

[![crates.io](https://img.shields.io/crates/v/anno-lib.svg)](https://crates.io/crates/anno-lib)
[![Documentation](https://docs.rs/anno/badge.svg)](https://docs.rs/anno)
[![CI](https://github.com/arclabs561/anno/actions/workflows/ci.yml/badge.svg)](https://github.com/arclabs561/anno/actions/workflows/ci.yml)

Information extraction for unstructured text: named entity recognition (NER), within-document coreference resolution, and structured pattern extraction.

Dual-licensed under MIT or Apache-2.0.

## Tasks

**Named entity recognition.** Given input text, identify spans `(start, end, type, confidence)` where each span denotes a named entity [1, 2]. Entity types follow standard taxonomies (PER, ORG, LOC, MISC for CoNLL-style [2]) or caller-defined labels for zero-shot extraction. Offsets are **character offsets** (Unicode scalar values), not byte offsets; see [CONTRACT.md](docs/CONTRACT.md).

**Coreference resolution.** Identify mention spans and group them into clusters, where each cluster tracks a consistent discourse referent within the document [3, 4]. Referents may be concrete or abstract, singular or plural, real-world or fictional — the constraint is within-document consistency, not ontological uniqueness. Coreferring mentions span proper names, definite descriptions, and pronouns: "Sophie Wilson", "the designer", and "She" form one cluster.

**Relation extraction.** Extract typed `(head, relation, tail)` triples from text. Available on `RelationCapable` backends; others fall back to co-occurrence edges for graph export.

**Structured pattern extraction.** Dates, monetary amounts, email addresses, URLs, phone numbers via deterministic regex grammars.

## Backends

All backends produce the same output type: variable-length spans with character offsets.

| Backend | Architecture | Labels | Zero-shot | Relations | Weights | Reference |
|---|---|---|---|---|---|---|
| `stacked` (default) | Selector/fallback | Best available | — | — | HuggingFace (when ML enabled) | -- |
| `gliner` | Bi-encoder span classifier | Custom | Yes | — | [gliner_small-v2.1](https://huggingface.co/onnx-community/gliner_small-v2.1) | Zaratiana et al. [5] |
| `gliner2` | Multi-task span classifier | Custom | Yes | Heuristic | [gliner-multitask-large-v0.5](https://huggingface.co/onnx-community/gliner-multitask-large-v0.5) | [11] |
| `nuner` | Token classifier (BIO) | Custom | Yes | — | [NuNerZero_onnx](https://huggingface.co/deepanwa/NuNerZero_onnx) | Bogdanov et al. [6] |
| `w2ner` | Word-word relation grids | Trained (nested) | No | — | [w2ner-bert-base](https://huggingface.co/ljynlp/w2ner-bert-base) | Li et al. [7] |
| `bert-onnx` | Sequence labeling (BERT) | PER/ORG/LOC/MISC | No | — | [bert-base-NER-onnx](https://huggingface.co/protectai/bert-base-NER-onnx) | Devlin et al. [8] |
| `tplinker` | Joint entity-relation (heuristic) | Custom | — | Heuristic | None | [10] |
| `crf` | Conditional Random Field | Trained | No | — | Bundled (`bundled-crf-weights`) | Lafferty et al. [9] |
| `hmm` | Hidden Markov Model | Trained | No | — | Bundled (`bundled-hmm-params`) | Rabiner [12] |
| `pattern` | Regex grammars | DATE/MONEY/EMAIL/URL/PHONE | N/A | — | None | -- |
| `heuristic` | Capitalization + context | PER/ORG/LOC | N/A | — | None | -- |
| `ensemble` | Weighted voting combiner | Mixed | Varies | — | Varies | -- |

ML backends are feature-gated (`onnx` or `candle`). Weights download from HuggingFace on first use. All backends expose `model.capabilities()` for runtime discovery. See [BACKENDS.md](docs/BACKENDS.md) for selection guidance and feature-flag details.

## Install

```sh
cargo install --git https://github.com/arclabs561/anno --package anno-cli --bin anno --features "onnx eval"
```

From a local clone:

```sh
cargo install --path crates/anno-cli --bin anno --features "onnx eval"
```

`ANNO_NO_DOWNLOADS=1` or `HF_HUB_OFFLINE=1` forces cached-only behavior.

## Examples

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

Zero-shot custom entity types (via GLiNER [5]):

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

## Downstream

Filter and pipe JSON output:

```sh
# All person entities, text only
anno extract --format json --file article.txt \
  | jq '[.[] | select(.entity_type == "PER") | .text]'

# Unique organizations, sorted
anno extract --format json --file article.txt \
  | jq '[.[] | select(.entity_type == "ORG") | .text] | unique | sort[]'
```

Batch a directory (parallel, cached):

```sh
anno batch --dir docs/ --parallel 4 --cache --output results/

# Stream stdin JSONL: {"id":"…","text":"…"} per line
cat corpus.jsonl | anno batch --stdin --parallel 4 --cache --output results/
```

### Knowledge Graph (RDF / Oxigraph)

`anno export` emits N-Triples or JSON-LD. `--base-uri` sets the IRI namespace — use a URI you own or one that aligns to a known linked-data vocabulary:

```sh
# Own namespace (recommended for private corpora)
anno export --input docs/ --output /tmp/kg/ --format ntriples \
  --base-uri https://myproject.example.com/kg/

# Align to DBpedia (widely used LOD namespace)
anno export --input docs/ --output /tmp/kg/ --format ntriples \
  --base-uri https://dbpedia.org/resource/
```

Default (`urn:anno:`) produces stable URNs suitable for local use without a registered domain.

Load into [Oxigraph](https://github.com/oxigraph/oxigraph) (pure-Rust RDF store, SPARQL 1.1):

```sh
cat /tmp/kg/*.nt | oxigraph load --location /tmp/anno-store --format ntriples

oxigraph query --location /tmp/anno-store \
  --query 'SELECT ?label WHERE {
    ?e a <https://myproject.example.com/kg/vocab#PERType> ;
       <http://www.w3.org/2000/01/rdf-schema#label> ?label .
  }'
```

**Semantic relation triples** — `RelationCapable` backends (`tplinker`, `gliner2`) produce typed `(head, relation, tail)` triples instead of co-occurrence edges. Use `--format graph-ntriples` (`--features graph`) for routing through the internal graph substrate:

```sh
anno export --input docs/ --output /tmp/kg/ \
  --format graph-ntriples --model gliner2 \
  --base-uri https://myproject.example.com/kg/
# → <entity/person/0_Lynn_Conway_0> <rel/works_for> <entity/org/1_IBM_13> .
```

### Property Graph CSV

Export node and edge tables as CSV for import into any property-graph database:

```sh
# Semantic edges (gliner2 or tplinker)
anno export --input docs/ --output /tmp/kg/ --format kuzu --model gliner2

# Co-occurrence edges (any other backend)
anno export --input docs/ --output /tmp/kg/ --format kuzu
```

Each file produces `{stem}-nodes.csv` + `{stem}-edges.csv` with columns:
- **Nodes**: `id`, `entity_type`, `text`, `start`, `end`, `source`
- **Edges**: `from`, `to`, `rel_type`, `confidence`

## Library

```toml
[dependencies]
anno-lib = "0.3"
```

The crate is published as `anno-lib` on crates.io; the Rust import name is `anno`.

### Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `onnx` | Yes | ONNX Runtime backends (GLiNER, NuNER, BERT, W2NER) via `ort` |
| `candle` | No | Pure-Rust Candle backends (no C++ runtime needed) |
| `eval` | No | Evaluation harnesses, dataset loaders, and matrix sampling |
| `graph` | No | Knowledge-graph export adapters (N-Triples, JSON-LD, CSV) |
| `schema` | No | JSON Schema generation for output types via `schemars` |

The `onnx` feature (default) pulls in `ort` (ONNX Runtime bindings), which requires a C++ runtime. For minimal builds, use `default-features = false`.

Basic extraction:

```rust
use anno::{Model, StackedNER};

let m = StackedNER::default();
let ents = m.extract_entities("Sophie Wilson designed the ARM processor.", None)?;
assert!(!ents.is_empty());
# Ok::<(), anno::Error>(())
```

Zero-shot custom types via `DynamicLabels` (GLiNER, GLiNER2, NuNER):

```rust
use anno::{DynamicLabels, GLiNEROnnx};

let m = GLiNEROnnx::new("onnx-community/gliner_small-v2.1")?;
let ents = m.extract_with_labels(
    "Aspirin treats headaches.",
    &["drug", "symptom"],
    None,
)?;
# Ok::<(), anno::Error>(())
```

Runtime capability discovery — all backends now report accurately:

```rust
use anno::Model;

fn print_caps(m: &dyn Model) {
    let c = m.capabilities();
    println!(
        "batch={} streaming={} gpu={} relations={} zero-shot={}",
        c.batch_capable, c.streaming_capable, c.gpu_capable,
        c.relation_capable, c.dynamic_labels,
    );
}
```

## Architecture

| Crate | Purpose |
|---|---|
| `anno` (root) | Published facade; re-exports `anno-lib` |
| `anno-lib` | Backends, `Model` / `RelationCapable` traits, extraction pipeline |
| `anno-core` | Stable data model (`Entity`, `Relation`, `Signal`, `Track`, `Identity`, `Corpus`) |
| `anno-graph` | Graph/KG export adapters: converts extraction output to `lattix::KnowledgeGraph`; owns triple construction and `--format graph-ntriples` |
| `anno-eval` | Evaluation harnesses, dataset loaders, matrix sampling (uses `muxer`) |
| `anno-cli` | Full CLI; graph feature routes through `anno-graph` |
| `anno-metrics` | Shared evaluation primitives (CoNLL F1, encoders) |

Pipeline: Text → Extract → Coalesce → structured output. See [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Evaluation

`anno-eval` provides dataset loading, backend-vs-dataset compatibility gating, and CoNLL-style [2] span-level evaluation (precision, recall, F1) with label mapping between backend and dataset taxonomies.

```sh
anno benchmark --model gliner --dataset conll2003
```

`anno sampler` (`--features eval`) provides a randomized matrix sampler with two modes: **triage** (worst-first) and **measure** (ML-only stable measurement).

### Sampler examples

```sh
# Deterministic offline decision-loop smoke example (also run in CI).
cargo run -p anno-eval --example muxer_decision_loop --features eval

# Run the CI sampler harness locally (uses ~/.anno_cache for history/cache).
just ci-matrix-local 42 ner
```

## Scope

Inference-time extraction only. Training is out of scope — use upstream frameworks (Hugging Face Transformers, Flair, etc.) and export ONNX weights for consumption.

## Documentation

- [QUICKSTART](docs/QUICKSTART.md)
- [CONTRACT](docs/CONTRACT.md) — offset semantics, scope, feature gating
- [BACKENDS](docs/BACKENDS.md) — backend selection, architecture, feature flags
- [ARCHITECTURE](docs/ARCHITECTURE.md) — crate layout, dependency flow
- [REFERENCES](docs/REFERENCES.md) — full bibliography (NER, coref, relation extraction, software)
- [API docs](https://docs.rs/anno)
- [Changelog](CHANGELOG.md)

## References

Key citations; see [docs/REFERENCES.md](docs/REFERENCES.md) for the full list with links.

1. Grishman & Sundheim, *COLING* 1996 (MUC-6 NER). [[PDF]](https://aclanthology.org/C96-1079.pdf)
2. Tjong Kim Sang & De Meulder, *CoNLL* 2003 (CoNLL benchmark). [[PDF]](https://aclanthology.org/W03-0419.pdf)
3. Lee et al., *EMNLP* 2017 (end-to-end coreference). [[arXiv]](https://arxiv.org/abs/1707.07045)
4. Jurafsky & Martin, *SLP3* 2024 (coreference fundamentals). [[Online]](https://web.stanford.edu/~jurafsky/slp3/)
5. Zaratiana et al., *NAACL* 2024 (GLiNER). [[arXiv]](https://arxiv.org/abs/2311.08526)
6. Bogdanov et al., 2024 (NuNER). [[arXiv]](https://arxiv.org/abs/2402.15343)
7. Li et al., *AAAI* 2022 (W2NER). [[arXiv]](https://arxiv.org/abs/2112.10070)
8. Devlin et al., *NAACL* 2019 (BERT). [[arXiv]](https://arxiv.org/abs/1810.04805)
9. Lafferty et al., *ICML* 2001 (CRF).
10. Wang et al., *COLING* 2020 (TPLinker). [[arXiv]](https://arxiv.org/abs/2010.13415)
11. Zaratiana et al., 2025 (GLiNER2). [[arXiv]](https://arxiv.org/abs/2507.18546)
12. Rabiner, *Proceedings of the IEEE* 1989 (HMM tutorial). [[PDF]](https://www.cs.ubc.ca/~murphyk/Bayes/rabiner.pdf)

Citeable via [CITATION.cff](CITATION.cff).

## License

Dual-licensed under MIT or Apache-2.0.
