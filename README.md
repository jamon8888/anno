<p align="center"><b>anno</b></p>

```mermaid
flowchart LR
    T["Marie Curie was born in Warsaw."] --> NER
    NER --> E1["[Marie Curie] PERSON"]
    NER --> E2["[Warsaw] LOCATION"]
```

Information extraction for Rust: named entities, coreference, discourse.

Dual-licensed under MIT or Apache-2.0.

```sh
# CLI
anno extract --text "Marie Curie was born in Warsaw."
```

```rust
// Library
use anno::{Model, StackedNER};

let model = StackedNER::default();
let entities = model.extract_entities("Marie Curie was born in Warsaw.", None)?;

for e in entities {
    println!("{} [{}]", e.text, e.entity_type.as_label());
}
```

## Crates

| Crate | Purpose |
|-------|---------|
| `anno` | Main library + CLI |
| `anno-core` | Core types (Entity, Span) |
| `anno-strata` | Graph algorithms (Leiden, Louvain) |

## Features

- Named entity recognition (NER)
- Coreference resolution
- Discourse analysis (shell nouns, anaphora)
- Multiple backends (ONNX, Candle, Burn)

## Documentation

- [`docs/QUICKSTART.md`](docs/QUICKSTART.md) — getting started
- [`docs/BACKENDS.md`](docs/BACKENDS.md) — ML backend configuration
- [`docs/EVALUATION.md`](docs/EVALUATION.md) — evaluation datasets
