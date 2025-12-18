# Interoperability Guide

This document describes how to integrate `anno` into external pipelines and systems.

## Overview

`anno` provides several integration patterns:

1. **Rust API** - Direct library usage
2. **CLI** - Command-line interface for shell pipelines
3. **JSON Schema** - Type definitions for language interop
4. **Middleware** - Composable pipeline hooks
5. **Streaming** - Iterator-based extraction for large documents

## Rust API Integration

### Basic Usage

```rust
use anno::{Entity, EntityType, Model, StackedNER};

// Create a backend
let backend = StackedNER::default();

// Extract entities
let entities = backend.extract_entities("Apple Inc. is based in Cupertino, CA.", None)?;

for entity in entities {
    println!("{}: {} ({:.2})", entity.entity_type, entity.text, entity.confidence);
}
```

### Custom Backend Selection

```rust
use anno::{HeuristicNER, RegexNER, CrfNER, Model};

// Pattern-based (structured data only)
let pattern = RegexNER::new();
let dates = pattern.extract_entities("Meeting on Jan 15, 2024", None)?;

// ML-based (named entities)  
let crf = CrfNER::new();
let named = crf.extract_entities("John Smith works at Google", None)?;
```

### Feature-Gated Backends

```rust
#[cfg(feature = "onnx")]
{
    use anno::GLiNEROnnx;
    
    // Zero-shot NER with custom types
    let gliner = GLiNEROnnx::from_pretrained("onnx-community/gliner_small-v2.1")?;
    let entities = gliner.extract_entities(
        "The new iPhone 15 Pro costs $999",
        &["product", "price", "brand"]
    )?;
}
```

## CLI Integration

### Shell Pipeline

```bash
# Basic extraction
echo "Apple is headquartered in Cupertino" | anno extract --format json

# With specific backend
anno extract --model gliner "John works at Microsoft in Seattle"

# Batch processing
cat documents.txt | anno extract --format jsonl > entities.jsonl

# Filter by type
anno extract --types person,organization "Marie Curie founded the Radium Institute"
```

### Output Formats

```bash
# JSON (structured)
anno extract --format json "text..."

# JSONL (one entity per line)
anno extract --format jsonl "text..."

# CSV (tabular)
anno extract --format csv "text..."

# CoNLL (annotation format)
anno extract --format conll "text..."
```

### Integration with jq

```bash
# Extract person names only
anno extract --format json "Marie Curie won Nobel prizes" | \
    jq '.entities[] | select(.entity_type == "Person") | .text'

# Count entities by type
anno extract --format json "..." | jq '.entities | group_by(.entity_type) | map({type: .[0].entity_type, count: length})'
```

## JSON Schema Integration

Enable the `schema` feature for JSON Schema generation:

```toml
[dependencies]
anno-core = { version = "0.2", features = ["schema"] }
```

### Generate Schemas

```rust
use anno_core::schema;

// Generate individual schemas
let entity_schema = schema::generate_entity_schema();
let doc_schema = schema::generate_grounded_document_schema();

// Write to files
std::fs::write("entity.json", serde_json::to_string_pretty(&entity_schema)?)?;
```

### TypeScript Generation

```bash
# Using json-schema-to-typescript
npx json-schema-to-typescript entity.json > entity.d.ts
```

Generated TypeScript:

```typescript
export interface Entity {
    text: string;
    entity_type: EntityType;
    start: number;
    end: number;
    confidence: number;
    normalized?: string;
    kb_id?: string;
    canonical_id?: number;
}

export type EntityType = 
    | "Person" 
    | "Organization" 
    | "Location" 
    | { Custom: { name: string; category: string } }
    | { Other: string };
```

### Python Generation

```bash
# Using datamodel-code-generator
pip install datamodel-code-generator
datamodel-codegen --input entity.json --output entity.py
```

## Middleware Pattern

Create custom processing pipelines:

```rust
use anno::backends::middleware::{
    Pipeline, Middleware, MiddlewareContext,
    NormalizeWhitespace, FilterByConfidence, RemoveOverlaps
};
use anno::{Entity, StackedNER, Result};

// Built-in middleware
let pipeline = Pipeline::new(Box::new(StackedNER::default()))
    .with(NormalizeWhitespace)           // Clean input
    .with(FilterByConfidence(0.5))       // Remove low-confidence
    .with(RemoveOverlaps);               // Deduplicate

let entities = pipeline.extract("Hello   World")?;
```

### Custom Middleware

```rust
use anno::backends::middleware::{Middleware, MiddlewareContext};
use anno::{Entity, Result};

/// Add knowledge base links
struct EnrichWithWikidata {
    cache: HashMap<String, String>,
}

impl Middleware for EnrichWithWikidata {
    fn post_process(&self, ctx: &mut MiddlewareContext, mut entities: Vec<Entity>) -> Result<Vec<Entity>> {
        for entity in &mut entities {
            if let Some(qid) = self.cache.get(&entity.text.to_lowercase()) {
                entity.kb_id = Some(qid.clone());
            }
        }
        Ok(entities)
    }

    fn name(&self) -> &'static str { "enrich_wikidata" }
}

// Use in pipeline
let pipeline = Pipeline::new(Box::new(backend))
    .with(EnrichWithWikidata { cache: wikidata_cache });
```

### Preprocessing Middleware

```rust
struct LowercaseInput;

impl Middleware for LowercaseInput {
    fn pre_process<'a>(&self, ctx: &mut MiddlewareContext, text: &'a str) -> Result<Cow<'a, str>> {
        Ok(Cow::Owned(text.to_lowercase()))
    }
    
    fn name(&self) -> &'static str { "lowercase" }
}
```

## Streaming API

For large documents or real-time streams:

```rust
use anno::backends::streaming::{StreamingExtractor, ChunkConfig};
use anno::StackedNER;

let config = ChunkConfig {
    chunk_size: 1000,      // Characters per chunk
    overlap: 50,           // Overlap to avoid boundary splits
    sentence_aware: true,  // Respect sentence boundaries
};

let backend = StackedNER::default();
let extractor = StreamingExtractor::new(&backend, config);

// Process large text in chunks
for result in extractor.extract_iter(&large_text) {
    let entities = result?;
    process_batch(entities);
}
```

### Async Streaming

```rust
use futures::StreamExt;

let stream = extractor.extract_stream(&large_text);

stream
    .for_each(|batch| async {
        if let Ok(entities) = batch {
            send_to_downstream(entities).await;
        }
    })
    .await;
```

## Language Detection Integration

```rust
use anno::lang::{detect_language, detect_languages, detect_language_segments, Language};

// Single language
let lang = detect_language("Bonjour, comment allez-vous?");
assert_eq!(lang, Language::French);

// Multi-language with confidence
let langs = detect_languages("Hello! Bonjour!");
for (lang, confidence) in langs {
    println!("{}: {:.2}%", lang, confidence * 100.0);
}

// Per-segment detection (code-switching)
let segments = detect_language_segments("Hello world. Bonjour monde. Hola mundo.");
for segment in segments {
    println!("[{}:{}) {}: {}", segment.start, segment.end, segment.language, segment.text);
}
```

## Model Registry

Query available models:

```rust
use anno::backends::catalog::{ModelInfo, BackendInfo, MODEL_REGISTRY};

// Find models for a backend
let gliner_models = ModelInfo::for_backend("gliner_onnx");
for model in gliner_models {
    println!("{}: {} ({})", model.id, model.size_human(), model.license);
}

// Get recommended model
if let Some(model) = ModelInfo::recommended_for("nuner") {
    println!("Download: {}", model.hub_url);
}
```

### Download Models

```bash
# CLI
anno models download onnx-community/gliner_small-v2.1

# List available
anno models list

# Get info
anno models info gliner_small-v2.1
```

## Training Custom Models

### CRF

```bash
# Train on CoNLL-2003
uv run scripts/train_crf_weights.py

# Output: crf_weights.json, crf_model.crfsuite
```

### HMM

```bash
# Train on CoNLL-2003
uv run scripts/train_hmm_weights.py

# Output: hmm_weights.json
```

### BiLSTM-CRF

```bash
# Train on CoNLL-2003 (requires GPU)
uv run scripts/train_bilstm_crf.py

# Output: bilstm_crf_emissions.onnx, bilstm_crf_vocab.json, bilstm_crf_weights.json
```

## Error Handling

```rust
use anno::{Error, Result};

fn process_document(text: &str) -> Result<Vec<Entity>> {
    let backend = StackedNER::default();
    
    match backend.extract_entities(text, None) {
        Ok(entities) => Ok(entities),
        Err(Error::ModelNotFound(name)) => {
            eprintln!("Model {} not found, using fallback", name);
            HeuristicNER::new().extract_entities(text, None)
        }
        Err(e) => Err(e),
    }
}
```

## Performance Tips

1. **Reuse backends** - Don't recreate models for each document
2. **Use streaming** - For documents > 10KB
3. **Feature selection** - Only enable needed features
4. **Batch processing** - Process multiple documents together

```rust
// Good: Reuse backend
let backend = GLiNEROnnx::from_pretrained("...")?;
for doc in documents {
    let entities = backend.extract_entities(&doc, None)?;
}

// Bad: Recreate each time
for doc in documents {
    let backend = GLiNEROnnx::from_pretrained("...")?; // Slow!
    let entities = backend.extract_entities(&doc, None)?;
}
```

## Integration Examples

### FastAPI (Python)

```python
import subprocess
import json

def extract_entities(text: str) -> list[dict]:
    result = subprocess.run(
        ["anno", "extract", "--format", "json", text],
        capture_output=True,
        text=True
    )
    return json.loads(result.stdout)["entities"]
```

### Node.js

```javascript
const { execSync } = require('child_process');

function extractEntities(text) {
    const result = execSync(`anno extract --format json "${text}"`);
    return JSON.parse(result.toString()).entities;
}
```

### Rust WebAssembly (Future)

```rust
// Planned: wasm feature
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn extract_entities_wasm(text: &str) -> JsValue {
    let backend = HeuristicNER::new();
    let entities = backend.extract_entities(text, None).unwrap_or_default();
    serde_wasm_bindgen::to_value(&entities).unwrap()
}
```

## See Also

- [BACKENDS.md](BACKENDS.md) - Backend comparison
- [UNICODE_OFFSETS.md](UNICODE_OFFSETS.md) - Character offset handling
- [research/HISTORICAL_SYSTEMS.md](../research/HISTORICAL_SYSTEMS.md) - Classical NER systems

