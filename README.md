# anno

Information extraction: named entities, coreference, discourse.

Dual-licensed under MIT or Apache-2.0.

```text
"Marie Curie was born in Warsaw."
    → [Marie Curie] PERSON
    → [Warsaw] LOCATION
```

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
