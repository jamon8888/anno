# Backend Selection Guide

When to choose each NER backend based on your requirements.

## Quick Decision Matrix

| If you need... | Use | Feature | Status |
|----------------|-----|---------|--------|
| **Just works** (no deps) | `StackedNER` | none | Working |
| **Best accuracy** (multi-backend voting) | `EnsembleNER` | none (better with `onnx`) | Working |
| **Fastest** (patterns only) | `RegexNER` | none | Working |
| **Zero-shot custom types** | `GLiNEROnnx` | `onnx` | Working |
| **Pure Rust** (no C++) | `CandleNER` | `candle` | Working |
| **Pure Rust trainable** | `BurnNER` | `burn` | Planned (errors clearly; inference not wired yet) |
| **LLM-powered** | `UniversalNER` | `eval-advanced` + API key | Working |
| **Relations + NER** | `GLiNER2Onnx` | `onnx` | Working |
| **CoNLL-2003 types** | `BertNEROnnx` | `onnx` | Working |

## Backend Status Summary

> **Live Results:** See [`reports/RESULTS.md`](../reports/RESULTS.md) for current benchmark scores.
> 
> The metrics below are reference baselines from published papers. For actual performance
> on our test sets, run `just spot-summary` or check the live results file.

| Backend | CLI Flag | Status | Notes |
|---------|----------|--------|-------|
| Pattern | `pattern` | Working | Patterns only (emails, dates, etc.) |
| Heuristic | `heuristic` | Working | Capitalization-based |
| HMM | `hmm` | Working | Classical HMM |
| CRF | `crf` | Working | Default uses heuristic weights (non-empty baseline) |
| BiLSTM-CRF | `bi-lstm-crf` | Working | Heuristic emissions |
| BERT ONNX | `bert-onnx` | Working | Uses `dslim/bert-base-NER` |
| GLiNER ONNX | `gliner` | Working | Zero-shot NER |
| GLiNER2 | `gliner2` | Working | Relations + NER |
| NuNER | `nuner` | Working | Zero-shot NER |
| **CandleNER** | `candle-ner` | **Working** | Pure Rust, uses `dslim/bert-base-NER` |
| TPLinker | `tplinker` | Working | Placeholder uses HeuristicNER |
| **BurnNER** | `burn` | **Planned** | Burn scaffolding; returns a clear error (no silent fallback) |
| Stacked | `stacked` | Working | Multi-backend priority |
| Ensemble | `ensemble` | Working | Multi-backend voting |
| **UniversalNER** | `universal-ner` | **Working** | LLM-based (requires API key + eval-advanced) |
| **GLiNER Candle** | `gliner-candle` | **Experimental** | Experimental Candle port; word/token span alignment fixed recently. Prefer `gliner` (ONNX) for production until further validation. |
| W2NER | `w2ner` | Usable | Self-export via `scripts/export_w2ner_to_onnx.py` |
| DeBERTa-v3 | `deberta-v3` | Requires Export | Run `scripts/export_deberta_ner_to_onnx.py` |
| ALBERT | `albert` | Requires Export | Custom ONNX export needed |

## Performance Comparison (Empirical)

Based on comprehensive evaluation across WikiGold, CoNLL2003, Wnut17, MultiNERD, and FewNERD:

| Backend | Best F1 | Speed | Labels | Best For |
|---------|---------|-------|--------|----------|
| `bert_onnx` | **80%** | ~0.5s | Fixed (trained on CoNLL) | Highest accuracy on standard NER |
| `nuner` | **70%** | ~5s | 3 only (PER/ORG/LOC) | Good accuracy, but slow and inflexible |
| `gliner_onnx` | **53%** | ~1s | Any (zero-shot) | Arbitrary entity types |
| `gliner2` | **46%** | ~4.5s | Any (multi-task) | NER + relations + classification |
| `stacked` | **50%** | ~0.5s | PER/ORG/LOC + patterns | No ML dependencies |
| `heuristic` | **29%** | ~0.02s | PER/ORG/LOC | Fastest, no deps |
| `crf` | **5%** | ~0.02s | Trained types | Classical ML baseline |

**Key findings:**
- For **best accuracy**: Use `bert_onnx` (80% F1 on MultiNERD)
- For **zero-shot with custom types**: Use `gliner_onnx` over NuNER (faster, more flexible)
- For **no ML dependencies**: Use `stacked` (50% F1)
- For **biomedical NER**: Export `d4data/biomedical-ner-all` via `scripts/export_biomedical_ner_to_onnx.py`

### Biomedical NER

General-purpose models (GLiNER, NuNER, BERT-NER) don't detect biomedical entities.
For Chemical/Disease/Drug/Gene extraction, export a biomedical model:

```bash
uv run scripts/export_biomedical_ner_to_onnx.py --output ~/.cache/anno/models/biomedical-ner/
```

This exports `d4data/biomedical-ner-all` which detects:
- Medication/Drug, MedicalCondition, AnatomicalStructure
- BiologicalProcess, ClinicalAttribute, BodySubstance
- Gene, Disease, Chemical

> See [`reports/RESULTS.md`](../reports/RESULTS.md) for detailed per-dataset results.

## Backend Details

### No Feature Required (Zero Dependencies)

#### `StackedNER` (Default)
**Best for:** Most users who want "it just works"

```rust
let ner = StackedNER::default();
```

- Automatically uses best available backend (GLiNER → BERT → patterns)
- Falls back gracefully when ML features not compiled
- Combines pattern + heuristic results
- ~50μs without ML, ~100ms with GLiNER

#### `RegexNER`
**Best for:** Structured data extraction (dates, emails, money, URLs)

```rust
let ner = RegexNER::new();
```

- ~400ns per extraction
- High precision for pattern-based entities
- Zero false positives for its supported types
- **Types:** DATE, EMAIL, URL, PHONE, MONEY, PERCENT

#### `HeuristicNER`
**Best for:** News articles, formal text with proper capitalization

```rust
let ner = HeuristicNER::new();
```

- ~50μs per extraction
- Uses capitalization patterns + honorifics
- Works well on news/Wikipedia text
- Struggles with social media/informal text
- **Types:** PER, ORG, LOC

#### `CrfNER`
**Best for:** Historical baseline, interpretable features, classical ML comparison

```rust
let ner = CrfNER::new();
```

- Classical Conditional Random Field sequence labeling
- ~88% F1 on CoNLL-2003 (when trained)
- Interpretable hand-crafted features
- No GPU required, deterministic inference
- **Types:** PER, ORG, LOC, MISC

**Note:** Default instance uses heuristic weights (not trained). For best results,
train on labeled data or use for comparison with modern methods.

#### `BiLstmCrfNER`
**Best for:** Neural baseline from 2015-2018, pre-transformer comparison

```rust
let ner = anno::backends::BiLstmCrfNER::new();
```

- Bidirectional LSTM + CRF decoding layer
- ~91% F1 on CoNLL-2003 (with trained weights)
- Dominant neural NER architecture before BERT
- Viterbi decoding with BIO constraints
- **Types:** PER, ORG, LOC, MISC

**Note:** Default instance uses heuristic emissions. Load ONNX weights for neural inference.

#### `HmmNER`
**Best for:** Classical statistical NER, educational purposes

```rust
let ner = anno::backends::HmmNER::new();
```

- Hidden Markov Model with Viterbi decoding
- ~85% F1 on CoNLL-2003 (with training)
- Generative model (emission + transition probabilities)
- Very fast inference
- **Types:** PER, ORG, LOC, MISC

**Note:** Historical system from 1990s-2000s. Useful for understanding probabilistic sequence modeling.

#### `EnsembleNER`
**Best for:** Maximum accuracy via weighted voting across multiple backends

```rust
let ner = EnsembleNER::new();
```

- Runs ALL available backends opportunistically
- Uses weighted voting for conflict resolution
- Backend-specific and type-specific reliability weights
- Agreement bonus when multiple backends agree
- Provenance shows which backends contributed

**How it works:**

```text
Input text → [Pattern, Heuristic, GLiNER, ...] → Candidates → Weighted Voting → Output
```

**Backend weights (configurable):**

| Backend | Default Weight | Best For |
|---------|---------------|----------|
| `RegexNER` | 0.98 | DATE, MONEY, EMAIL, URL |
| `GLiNEROnnx` | 0.85 | PER, ORG, LOC |
| `HeuristicNER` | 0.60 | ORG (Inc/Corp patterns) |

**Agreement bonus:** When 2+ backends agree on entity and type, confidence is boosted by 0.10-0.15.

**CLI usage:**
```bash
anno extract --model ensemble "Tim Cook leads Apple Inc."
# Output shows source: ensemble(GLiNER-ONNX+heuristic)
```

### ONNX Feature (`--features onnx`)

Requires ONNX Runtime. Models download from HuggingFace automatically.

#### `GLiNEROnnx`
**Best for:** Zero-shot NER with any entity types you define

```rust
let ner = GLiNEROnnx::new("onnx-community/gliner_small-v2.1")?;
let entities = ner.extract_with_types(text, &["disease", "medication"], 0.5)?;
```

- ~100ms per extraction
- **Zero-shot:** Define types at runtime, no retraining
- Good generalization to new domains
- Threshold tunable (default 0.5)
- **Models:** `gliner_small-v2.1` (129MB), `gliner_medium-v2.1` (414MB)

#### `BertNEROnnx`
**Best for:** Standard NER on news-like text

```rust
let ner = BertNEROnnx::new("dslim/bert-base-NER")?;
```

- ~50ms per extraction
- High quality on news text
- CoNLL-2003 trained
- **Types:** PER, ORG, LOC, MISC

#### `NuNER`
**Best for:** Zero-shot NER, alternative to GLiNER

```rust
let ner = NuNER::from_pretrained("numind/NuNER_Zero")?;
```

- ~80ms per extraction
- Different architecture than GLiNER
- Sometimes better on biomedical text
- **Zero-shot:** Custom types supported
- **Note:** `NuNER::new()` is configuration-only; calling `extract_entities` on an unloaded model returns a clear error. Use `from_pretrained(...)` for inference.

#### `W2NER`
**Best for:** Nested and discontinuous entities

```rust
let ner = W2NER::from_pretrained("ljynlp/w2ner-bert-base")?;
```

- ~150ms per extraction
- Handles nested entities: "New York" inside "New York Times"
- Handles discontinuous: "breast and ovarian cancer"
- **Types:** PER, ORG, LOC, MISC (can be fine-tuned)
- **Note:** Model requires HuggingFace authentication. Set `HF_TOKEN` and request access at the model page.
- **Note:** `W2NER::new()` is configuration-only; calling `extract_entities` on an unloaded model returns a clear error. Use `from_pretrained(...)` for inference.

#### `GLiNER2Onnx`
**Best for:** NER + Relation Extraction together

```rust
let ner = GLiNER2Onnx::new("knowledgator/gliner-multitask-v1.0")?;
let (entities, relations) = ner.extract_entities_and_relations(text, &["PER", "ORG"], &["works_at"])?;
```

- ~120ms per extraction
- Multi-task: NER + Relations in one pass
- **Zero-shot:** Custom entity and relation types
- More accurate than separate pipelines

### Candle Feature (`--features candle`)

Pure Rust inference via HuggingFace Candle. No C++ dependencies.

#### `CandleNER`
**Best for:** Rust-only deployments, WebAssembly targets, pure Rust NER

```rust
let ner = CandleNER::from_pretrained("dslim/bert-base-NER")?;
```

- ~60ms per extraction (slightly slower than ONNX)
- No ONNX Runtime dependency
- Works on wasm32 targets
- **Types:** PER, ORG, LOC, MISC
- Supports models with `vocab.txt` (older BERT) or `tokenizer.json`
- **Default model:** `dslim/bert-base-NER` (cased, English)

#### `GLiNERCandle`
**Best for:** Zero-shot NER in pure Rust

```rust
let ner = GLiNERCandle::from_pretrained("vicgalle/gliner-small-pii")?;
```

- ~120ms per extraction
- Zero-shot capabilities
- Pure Rust, no C++
- Requires `.safetensors` weights (see conversion below)
- **Note:** Model must have `tokenizer.json` and `.safetensors`. Use `--model gliner` (ONNX) if unavailable.

### Burn Feature (`--features burn`)

Pure Rust deep learning via the Burn framework. Best for training or custom architectures.

#### `BurnNER`
**Best for:** Custom model training, alternative pure Rust backend

```rust
let ner = BurnNER::new()?;
```

- Placeholder implementation (full Burn inference in development)
- Burn provides training support (unlike Candle)
- Multiple backend options: ndarray, tch, wgpu
- Better suited for model development than production inference
- **Note:** Currently experimental. Use `--model candle-ner` or `--model bert-onnx` for production.

**When to use Burn vs Candle:**

| Scenario | Recommendation |
|----------|----------------|
| Production NER inference | CandleNER, GLiNERCandle |
| Training custom NER models | BurnNER (when implemented) |
| WebAssembly deployment | CandleNER (more mature) |
| Research/experimentation | Burn (more flexible) |

### LLM-based NER (Experimental)

Uses CodeNER-style prompting to leverage LLMs for zero-shot entity extraction.

#### `LlmNER`
**Best for:** Maximum flexibility with external LLMs (GPT-4, Claude, etc.)

```rust
let config = LlmConfig {
    model_name: "gpt-4".to_string(),
    api_endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
    api_key: Some(std::env::var("OPENAI_API_KEY").unwrap()),
    ..Default::default()
};
let ner = LlmNER::new(config, &[EntityType::Person, EntityType::Organization])?;
```

- Frames NER as a coding task using structured JSON output
- Supports chain-of-thought reasoning for better accuracy
- Requires external API access (not self-contained)
- ~1-5s per extraction (depends on LLM latency)
- **Types:** Any types you specify

**Note:** This backend is experimental and requires configuring an external LLM endpoint.
See `docs/notes/design/llm/LLM_CLI_INTEGRATION.md` for setup details.

## Feature Comparison

| Backend | Speed | Zero-Shot | Nested | Relations | Pure Rust | Training |
|---------|-------|-----------|--------|-----------|-----------|----------|
| `RegexNER` | ~400ns | no | no | no | yes | no |
| `HeuristicNER` | ~50μs | no | no | no | yes | no |
| `StackedNER` | ~50μs-100ms | yes* | no | no | no* | no |
| `BertNEROnnx` | ~50ms | no | no | no | no | no |
| `GLiNEROnnx` | ~100ms | yes | no | no | no | no |
| `NuNER` | ~80ms | yes | no | no | no | no |
| `W2NER` | ~150ms | no | yes | no | no | no |
| `GLiNER2Onnx` | ~120ms | yes | no | yes | no | no |
| `CandleNER` | ~60ms | no | no | no | yes | no |
| `GLiNERCandle` | ~120ms | yes | no | no | yes | no |
| `BurnNER` | N/A† | no | no | no | yes | yes |

*StackedNER is pure Rust without ML features; with `onnx` it uses GLiNER  
†BurnNER is scaffolding-only today (no inference); it returns a clear error. Use CandleNER/GLiNEROnnx for inference.

## Domain-Specific Recommendations

### Biomedical/Clinical
- **GLiNEROnnx** with types: `["disease", "drug", "gene", "symptom"]`
- Or **NuNER** which sometimes performs better on biomedical text
- Consider dedicated models: BC5CDR, NCBI-Disease

### Legal Documents
- **GLiNEROnnx** with types: `["party", "court", "statute", "date", "money"]`
- **RegexNER** for structured data (dates, monetary amounts)

### Social Media
- **GLiNEROnnx** (handles informal text better than BERT)
- Avoid **HeuristicNER** (poor on lowercase/informal text)

### News/Wikipedia
- **BertNEROnnx** (trained on news-like CoNLL-2003)
- **StackedNER** (default) works well

### Knowledge Graph Construction
- **GLiNER2Onnx** for entities + relations
- Or **GLiNEROnnx** followed by relation extraction

## Converting PyTorch to Safetensors

Candle backends require `.safetensors` format. Convert using:

```bash
# Uses PEP 723 inline dependencies - no venv needed
uv run scripts/convert_pytorch_to_safetensors.py \
    ~/.cache/huggingface/hub/models--gliner--gliner_small-v2.1/snapshots/*/pytorch_model.bin \
    model.safetensors
```

Or install globally:
```bash
pip install torch safetensors
python scripts/convert_pytorch_to_safetensors.py input.bin output.safetensors
```

## CLI Model Management

The `anno models` subcommand provides utilities for discovering and managing models:

```bash
# List all available backends and downloadable models
anno models list

# Show detailed information about a specific model
anno models info gliner

# Download a model from HuggingFace (when hf-hub feature enabled)
anno models download onnx-community/gliner_small-v2.1

# Get recommended models for a use case
anno models recommend speed      # Fast inference
anno models recommend accuracy   # Best quality
anno models recommend zero-shot  # Custom entity types
anno models recommend offline    # No network required

# Show cache location for a model
anno models path gliner_small-v2.1
```

## Benchmarking Your Setup

Run the criterion benchmarks:

```bash
# All backends
cargo bench --features "eval,onnx,candle" --bench ner

# Specific backends
cargo bench --features onnx --bench ner -- GLiNER
cargo bench --features candle --bench ner -- Candle
```

Or use the CLI benchmark command:

```bash
anno benchmark --tasks ner --backends pattern,heuristic,gliner --max-examples 100
```

## Known Issues

### GLiNER2Onnx
**Status:** Fixed in v0.2.x  
Previously had architecture mismatch due to hardcoded special token IDs. Now dynamically resolves
special tokens (`[P]`, `[E]`, `[C]`, `[L]`, `[SEP]`) from the tokenizer vocabulary.

### NuNER  
**Status:** Fixed in v0.2.x  
Previously had index out of bounds errors due to span tensor generation. Now uses the shared
`span_utils` module with proper bounds checking and overflow protection.

### W2NER
**Status:** Requires custom export OR HuggingFace authentication  
**Error:** `W2NER model 'ljynlp/w2ner-bert-base' not found or missing ONNX files`  
**Fix Options:** 

**Option 1: Export your own model (recommended)**
```bash
# Export a simplified W2NER model to ONNX
uv run scripts/export_w2ner_to_onnx.py --output /path/to/w2ner-model/model.onnx

# Use the exported model
W2NER_MODEL_PATH=/path/to/w2ner-model anno extract --model w2ner "Your text"
```

**Option 2: Use HuggingFace gated model**
1. Create `.env` file with `HF_TOKEN=hf_xxx` (automatically loaded)
2. Request access at https://huggingface.co/ljynlp/w2ner-bert-base
3. Wait for approval (may take days)

**Note:** The exported model produces reasonable entity boundaries but may need fine-tuning for best accuracy. Use `--model gliner2` for a ready-to-use alternative with nested entity support.

### GLiNERCandle
**Status:** Experimental  
**Notes:** This backend previously produced incorrect spans due to a **word/token index mismatch** (word-based spans applied to token-based embeddings). This has been addressed by aggregating token embeddings into **per-word embeddings** before span scoring.  
**Recommendation:** If you need production-grade zero-shot NER today, prefer `--model gliner` (ONNX) until `GLiNERCandle` has broader real-model validation and tuned defaults.

### CandleNER
**Status:** Working  
Pure Rust BERT-based NER using Candle. Works with `dslim/bert-base-NER` and similar cased models.
Uses `vocab.txt` tokenization with case preservation (critical for NER accuracy).

### UniversalNER
**Status:** Requires LLM integration + API key  
When unavailable, returns a clear runtime error (no silent empty fallback).
**Fix:** Set `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` in `.env` (and enable the appropriate features for your build).

### CrfNER
**Status:** Fixed in v0.2.x  
Previously returned byte offsets instead of character offsets for entities, causing issues with
non-ASCII text. Now uses `SpanConverter` for correct byte-to-char conversion.

### Working Backends (Tested December 2025)

#### Production Ready (No Feature Flags)
- `RegexNER` - Pattern matching (dates, emails, URLs, phones)
- `HeuristicNER` - Capitalization + context heuristics
- `StackedNER` - Pattern > Heuristic priority pipeline (default)
- `EnsembleNER` - Weighted voting across backends
- `CrfNER` - Classical CRF sequence labeling
- `BiLstmCrfNER` - BiLSTM + CRF (heuristic weights, CLI: `--model bilstm-crf`)
- `HmmNER` - Hidden Markov Model (CLI: `--model hmm`)
- `TPLinker` - Joint entity-relation extraction (CLI: `--model tplinker`)

#### Production Ready (`--features onnx`)
- `GLiNEROnnx` - Zero-shot span classification (recommended for custom types)
- `GLiNER2Onnx` - Multi-task extraction
- `NuNER` - Token classification zero-shot
- `BertNEROnnx` - BERT-based CoNLL-2003 style NER

#### Requires Setup / Limited
- `W2NER` - Requires HF authentication + model access (clear error message)
- `GLiNERCandle` - Experimental Candle port; prefer `--model gliner` (ONNX) for production until further validation
- `UniversalNER` - Requires LLM API key (set `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` in `.env`)

### Note on Burn Framework
Burn is a **deep learning framework** (like PyTorch), not a model itself.
Models like BiLSTM-CRF could use Burn for tensor operations.
For now, `BurnNER` is scaffolding and returns a clear error when used. For neural inference, use:
- `CandleNER` with `--features candle`
- `GLiNEROnnx` with `--features onnx`

## Coreference Resolution Backends

### `E2ECoref`
**Best for:** Modern span-based neural coreference resolution

```rust
let coref = anno::backends::E2ECoref::new();
let clusters = coref.resolve("John saw Mary. He waved to her.")?;
```

- End-to-end neural coref (Lee et al. 2017, 2018)
- Span enumeration + mention scoring + antecedent ranking
- ~80% F1 on CoNLL-2012 (with trained weights)
- Handles pronouns, proper nouns, and nominal mentions
- **Note:** Default uses heuristic scoring. Load ONNX for neural inference.

### `MentionRankingCoref`
**Best for:** Simpler coreference with external mention detection

```rust
let coref = anno::backends::MentionRankingCoref::new();
let clusters = coref.resolve("John entered. He smiled.")?;
```

- Feature-based antecedent ranking
- Uses string match, gender/number agreement, distance
- Faster than E2E-coref (~O(n²) vs O(n⁴))
- Can integrate with NER for mention detection

### `T5Coref` (requires `onnx` feature)
**Status:** Scaffolding / experimental (no seq2seq decoding yet)

```rust
#[cfg(feature = "onnx")]
let coref = anno::backends::coref_t5::T5Coref::from_pretrained("your-org/your-t5-coref-onnx")?
    .with_heuristic_fallback();
```

- Seq2Seq formulation of coreference
- Generates marked-up text with cluster IDs
- Works with T5/Flan-T5 models
- **Note:** Real T5 seq2seq decoding is not implemented yet. By default, `resolve*` returns a clear error.
  You can opt into a simple heuristic fallback via `.with_heuristic_fallback()`.

## CI Benchmark Results

Benchmark results are uploaded as artifacts on every CI run:

1. Go to [Actions](https://github.com/arclabs561/anno/actions)
2. Select a workflow run
3. Download `eval-sanity-report` or `eval-full-report` artifact

Full evaluations run on `eval-*` branches or manual workflow trigger.

## Accuracy vs Speed Trade-offs

```
Accuracy ↑
    │
    │    ★ GLiNER2 (RE)
    │    ★ GLiNEROnnx      ★ NuNER
    │    
    │              ★ BertNEROnnx
    │    
    │                        ★ W2NER (nested)
    │
    │
    │                                    ★ HeuristicNER
    │
    │                                              ★ RegexNER (patterns only)
    │
    └────────────────────────────────────────────────────────→ Speed
            100ms            50ms             50μs         400ns
```

## See Also

- [Architecture](ARCHITECTURE.md) - System design
- [Evaluation](EVALUATION.md) - Metrics and benchmarks
- [Task Matrix](notes/reference/TASKS_MODELS_DATASETS_EVALS_TESTS_MATRIX.md) - Full coverage

