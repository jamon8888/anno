<div align="center">

# anno

Information extraction: NER and coref.

[![CI](https://github.com/arclabs561/anno/actions/workflows/ci.yml/badge.svg)](https://github.com/arclabs561/anno/actions)
[![Crates.io](https://img.shields.io/crates/v/anno.svg)](https://crates.io/crates/anno)
[![Docs](https://docs.rs/anno/badge.svg)](https://docs.rs/anno)

</div>

Library and CLI for named entity recognition and coref, with a built-in evaluation framework. Multiple backends: regex (~400ns), transformers (~50-150ms), zero-shot NER.

## Installation

```bash
cargo install anno
# or from source:
git clone https://github.com/arclabs561/anno
cd anno
cargo build --release -p anno --bin anno
```

## Usage

### Extract entities

```bash
$ anno extract "Marie Curie won the Nobel Prize in Paris"

PER:2 "Marie Curie" "Nobel Prize"
LOC:1 "Paris"
```

Verbose output levels:
- `-v`: Add confidence, negation, quantifiers, context snippets
- `-vv`: Add tracks (coreference chains), statistics
- `-vvv`: Add identities (KB links), full metadata, annotated text

JSON output:
```bash
$ anno extract --format json "Marie Curie won the Nobel Prize in Paris"
[{"text":"Marie Curie","type":"PER","start":0,"end":11,"confidence":0.75},...]
```

**Offsets**: `start`/`end` are character offsets (Unicode-safe). See [`docs/UNICODE_OFFSETS.md`](docs/UNICODE_OFFSETS.md).

### Coref

```bash
# Process directory of text files
$ anno crossdoc --directory ./docs --threshold 0.6 --format tree

# Or import pre-processed JSON files
$ anno extract --file doc1.txt --export doc1.json
$ anno extract --file doc2.txt --export doc2.json
$ anno crossdoc --import doc1.json --import doc2.json --threshold 0.6 --format tree
```

## Common Use Cases

### Ingest directory and coalesce → see entity clusters

```bash
$ anno crossdoc --directory ./docs --threshold 0.7 --format tree
```

Example output:
```
● Marie Curie (PER) [cross-doc]
  3 mentions • 3 docs • conf: 0.85
  Docs: doc1.txt, doc2.txt, doc3.txt
    • doc1.txt: "Marie Curie"
    • doc2.txt: "Marie Curie"
    • doc3.txt: "Marie Curie"

● Paris (LOC) [cross-doc]
  2 mentions • 2 docs • conf: 0.90
  Docs: doc1.txt, doc3.txt
    • doc1.txt: "Paris"
    • doc3.txt: "Paris"
```

### Ingest URL and extract entities

```bash
$ anno extract --url https://example.com/article -v
```

Example output (verbose mode):
```
PER:2
  "Marie Curie" (0.85)
    ...Marie Curie won the Nobel Prize...
  "Nobel Prize" (0.75)
    ...won the Nobel Prize in Paris...
LOC:1
  "Paris" (0.90)
    ...Nobel Prize in Paris. She was...
ORG:1
  "Acme Corp" (0.88)
    ...founded by Acme Corp in 2024...
DATE:1
  "2024-01-15" (0.95)
    ...Acme Corp in 2024-01-15...
```

### Ingest URL and debug (HTML visualization)

```bash
$ anno debug --url https://example.com/article --html --output debug.html
```

Opens interactive HTML with entity highlighting, coreference chains, and metadata.

### Ingest URL and see entities in terminal (with coreference)

```bash
$ anno debug --url https://example.com/article --coref
```

Shows entities with intra-document coreference resolution (pronouns linked to antecedents).

**Note**: `crossdoc` requires `eval-advanced` feature. Use `--model gliner` for better entity detection. Import pre-processed JSON files with `--import` for best results.

**More examples**: See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for advanced workflows.

## Library

```toml
[dependencies]
anno = "0.2"
```

## Structure

Workspace crates (top-level directories):

```
anno-core/      # Foundation: Entity, GroundedDocument
anno/           # NER backends, evaluation, CLI binary (src/bin/anno.rs)
anno-coalesce/  # Cross-document coreference (anno-coalesce)
anno-strata/    # Hierarchical clustering (anno-strata)
```

Each crate is independently usable. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for details.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for full architecture.

## Pipeline

Three-level hierarchy: **Signal → Track → Identity**.[^1]

1. **Extract** (Signal): Detect entities in text
2. **Track** (Level 2): Within-document coreference - cluster mentions in same document
3. **Coalesce** (Identity): Merge tracks across documents into canonical entities

[^1]: See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for architecture details.

### Extract

Named entity recognition: detect persons, organizations, locations, dates, etc.

- **Input**: raw text
- **Output**: entities with types, confidence (spans available in JSON/TSV formats)
- **CLI**: `anno extract --model gliner "text"`

```rust
use anno::{Model, RegexNER};

let ner = RegexNER::new();
let entities = ner.extract_entities("Contact alice@acme.com by Jan 15", None)?;
```

### Coalesce

Coref: merge mentions across documents into canonical entities.

- **Input**: entities from multiple documents
- **Output**: identity clusters linking mentions across documents
- **CLI**: `anno crossdoc --directory ./docs --model gliner --threshold 0.6 --format tree`
- **When to use**: Processing multiple documents, need to know "Barack Obama" in doc1 and doc2 refer to the same person
- **Note**: Requires `eval-advanced` feature. Use `--import` for pre-processed GroundedDocument JSON files.

```rust
use anno_coalesce::Resolver;

let resolver = Resolver::new();
let identities = resolver.resolve_inter_doc_coref(&mut corpus, Some(0.7), Some(true))?;
```

## Library Examples

```rust
use anno::{Model, RegexNER};

let ner = RegexNER::new();
let entities = ner.extract_entities("Contact alice@acme.com by Jan 15", None)?;
```

**Zero-shot NER** (custom entity types):
```rust
#[cfg(feature = "onnx")]
use anno::{ZeroShotNER, GLiNEROnnx};

#[cfg(feature = "onnx")]
let ner = GLiNEROnnx::new("onnx-community/gliner_small-v2.1")?;
#[cfg(feature = "onnx")]
let entities = ner.extract_with_types(
    "Patient presents with diabetes, prescribed metformin",
    &["disease", "medication"],
    0.5,
)?;
```

**See [`docs/SCOPE.md`](docs/SCOPE.md) for scope and maturity notes.**

## Backends

| Backend | Latency | Accuracy | Feature | Use Case |
|---------|---------|----------|---------|----------|
| `RegexNER` | ~400ns | ~95%¹ | always | Structured entities (dates, money, emails) |
| `HeuristicNER` | ~50μs | ~65%² | always | Person/Org/Location heuristics |
| `GLiNEROnnx` | ~100ms | ~92%³ | `onnx` | Zero-shot NER (custom types) |
| `BertNEROnnx` | ~50ms | ~86%⁴ | `onnx` | Fixed 4-type NER (PER/ORG/LOC/MISC) |

¹ Pattern accuracy on structured entities only. ² F1 on Person/Org/Location. ³ Zero-shot F1 varies by entity types. ⁴ F1 on CoNLL-2003.

**See [`docs/reference/TASK_DATASET_MAPPING.md`](docs/reference/TASK_DATASET_MAPPING.md) for complete backend list and task support.**

## Features

- `onnx`: BERT, GLiNER, GLiNER2 via ONNX Runtime (GLiNER2 is multi-task: entities + relations)
- `eval`: Evaluation framework, datasets, metrics
- `eval-advanced`: Coref, advanced evaluation

**See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for complete feature list.**

## Documentation

- **API docs**: https://docs.rs/anno
- **Architecture**: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- **Evaluation**: [docs/EVALUATION.md](docs/EVALUATION.md)

## License

MIT OR Apache-2.0
