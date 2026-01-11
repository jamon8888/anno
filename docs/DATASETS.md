# Datasets

Comprehensive reference for all synthetic and real datasets available in `anno`.

## Quick Reference

| Category | Count | Use Case |
|----------|-------|----------|
| Synthetic (built-in) | 28+ | Unit testing, pattern validation, fast iteration |
| Real (registry) | 451 | Benchmarking, model comparison, production eval |
| Loadable (have parser) | 228 | Actually downloadable and parseable |
| With working URLs | 251 | Currently accessible for download |

## Important: Benchmark Quality Warning

Research (2023-2024) has revealed significant annotation errors in standard NER benchmarks:

| Dataset | Error Rate | Source |
|---------|-----------|--------|
| CoNLL-03 | **7.0%** of labels incorrect | CleanCoNLL (EMNLP 2023) |
| OntoNotes 5.0 | **~8%** of entities | Bernier-Colborne (2024) |
| WikiNER | **>10%** (semi-supervised) | WikiNER-fr-gold (2024) |

**Consequence**: On original CoNLL-03, **47% of "errors" scored by F1 were actually correct predictions** penalized by annotation mistakes. After correction, SOTA F1 jumped from 94% to 97.1%.

**Recommendation**: 
- Use **synthetic datasets** for development (verified annotations, no noise)
- Use **cleaned benchmarks** (CleanCoNLL) for true error analysis
- Use **original benchmarks** only for paper comparisons

See [`docs/EVALUATION.md`](EVALUATION.md) for citations and evaluation notes.

---

## Synthetic Datasets

Built-in datasets for testing. No network required.

### Core Domains

| Dataset | Function | Entity Types | Examples | Difficulty |
|---------|----------|--------------|----------|------------|
| News | `news_dataset()` | PER, ORG, LOC, DATE | ~15 | Easy-Hard |
| Biomedical | `biomedical_dataset()` | PER, ORG, LOC, Custom(GENE/DISEASE) | ~12 | Medium-Hard |
| Financial | `financial_dataset()` | PER, ORG, LOC, MONEY, PERCENT | ~10 | Medium |
| Legal | `legal_dataset()` | PER, ORG, LOC, DATE | ~8 | Medium-Hard |
| Scientific | `scientific_dataset()` | PER, ORG, LOC | ~8 | Medium |
| Entertainment | `entertainment_dataset()` | PER, ORG, LOC, DATE | ~8 | Easy-Medium |
| Social Media | `social_media_dataset()` | PER, ORG, LOC, URL, EMAIL | ~10 | Medium-Hard |

### Industry-Specific

| Dataset | Function | Focus | Entity Types | Examples |
|---------|----------|-------|--------------|----------|
| Technology | `technology_dataset()` | AI/tech companies | PER, ORG, LOC, MONEY, QUANTITY | 6 |
| Healthcare | `healthcare_dataset()` | Medical/clinical | PER, ORG, LOC, DATE, QUANTITY | 5 |
| Manufacturing | `manufacturing_dataset()` | Semiconductor/industrial | PER, ORG, LOC, MONEY, DATE | 5 |
| Automotive | `automotive_dataset()` | EV/vehicles | PER, ORG, LOC, MONEY, PERCENT, QUANTITY | 5 |
| Energy | `energy_dataset()` | Energy/climate | ORG, LOC, MONEY, PERCENT, DATE | 4 |
| Aerospace | `aerospace_dataset()` | Defense/space | PER, ORG, LOC, MONEY, DATE, QUANTITY | 4 |

### Specialized

| Dataset | Function | Focus | Examples |
|---------|----------|-------|----------|
| Sports | `sports_dataset()` | Athletes, teams, venues | ~8 |
| Politics | `politics_dataset()` | Politicians, parties | ~6 |
| E-commerce | `ecommerce_dataset()` | Products, prices | ~5 |
| Travel | `travel_dataset()` | Airlines, airports | ~5 |
| Weather | `weather_dataset()` | Forecasts, locations | ~4 |
| Academic | `academic_dataset()` | Universities, researchers | ~5 |
| Food | `food_dataset()` | Restaurants, cuisine | ~5 |
| Real Estate | `real_estate_dataset()` | Properties, prices | ~5 |
| Cybersecurity | `cybersecurity_dataset()` | CVEs, vendors | ~5 |

### Multilingual & Diversity

| Dataset | Function | Languages/Focus | Examples |
|---------|----------|-----------------|----------|
| Multilingual | `multilingual_dataset()` | DE, FR, ES, JP, CN, AR | ~8 |
| Globally Diverse | `globally_diverse_dataset()` | African, Asian, LatAm names | ~7 |

### Utility/Testing

| Dataset | Function | Purpose | Examples |
|---------|----------|---------|----------|
| Adversarial | `adversarial_dataset()` | Edge cases, ambiguity | ~20 |
| Structured | `structured_dataset()` | Tables, lists | ~10 |
| Conversational | `conversational_dataset()` | Dialog, chat | ~8 |
| Historical | `historical_dataset()` | Archaic text | ~6 |
| Hard Domain | `hard_domain_examples()` | Challenging cross-domain | ~5 |

---

## Real Datasets

The registry contains **451 datasets** covering NER, coreference, relation extraction, and more.

**Status**: 228 datasets (51%) have loader implementations and can be downloaded/parsed.

### Dataset Statistics

| Metric | Count | Notes |
|--------|-------|-------|
| Total in registry | 451 | All datasets with metadata |
| Loadable (have parser) | 228 | Can be downloaded and parsed |
| With working URLs | 251 | Currently accessible |
| With broken URLs | 118 | Need URL updates |
| Paper-only URLs | 34 | DOI/arXiv links, not direct downloads |
| No URL | 47 | May require licenses or contact authors |
| HuggingFace datasets | 32 | Available via HF API |
| S3 cached | 165 | Fast offline access |

### Key Datasets by Task

#### NER Datasets (Sample - 300+ total)

| Dataset | ID | Source | Size | Entity Types | Format | Status |
|---------|------|--------|------|--------------|--------|--------|
| WikiGold | `WikiGold` | Wikipedia | ~2k sent | PER, ORG, LOC, MISC | CoNLL | ✅ Loadable |
| WNUT-17 | `Wnut17` | Twitter | ~5k tweets | PER, ORG, LOC, etc. | CoNLL | ✅ Loadable |
| MIT Movie | `MitMovie` | Movie queries | ~10k | Actor, Director, etc. | BIO | ✅ Loadable |
| MIT Restaurant | `MitRestaurant` | Restaurant queries | ~10k | Cuisine, Location, etc. | BIO | ✅ Loadable |
| CoNLL-2003 | `CoNLL2003Sample` | News | ~1k sent | PER, ORG, LOC, MISC | CoNLL | ✅ Loadable |
| OntoNotes | `OntoNotesSample` | Mixed | ~1k sent | 18 types | CoNLL | ✅ Loadable |
| MultiNERD | `MultiNERD` | Wikipedia | ~50k | 15 types | JSONL | ✅ Loadable |
| BC5CDR | `BC5CDR` | PubMed | ~1.5k docs | Chemical, Disease | XML | ✅ Loadable |
| NCBI Disease | `NCBIDisease` | PubMed | ~800 docs | Disease | TXT | ✅ Loadable |
| FewNERD | `FewNERD` | Wikipedia | ~5k | 66 fine-grained | TXT | ✅ Loadable |
| CrossNER | `CrossNER` | Multi-domain | ~5k | Domain-specific | TXT | ✅ Loadable |
| UniversalNER | `UniversalNERBench` | Mixed | ~10k | Universal schema | JSON | ✅ Loadable |

**See full catalog**: Run `cargo run --example eval_basic --features eval` or check `generated/datasets_generated.json`

#### Multilingual NER (49+ datasets)

| Dataset | ID | Languages | Entity Types | Status |
|---------|------|-----------|--------------|--------|
| WikiANN | `WikiANN` | 282 languages | PER, LOC, ORG | ✅ Loadable |
| MultiCoNER | `MultiCoNER` | 12 languages | 6 coarse + 33 fine | ✅ Loadable |
| MultiCoNER v2 | `MultiCoNERv2` | 12 languages | 36 types | ✅ Loadable |
| MasakhaNER | `MasakhaNER` | 10 African languages | PER, ORG, LOC, DATE | ✅ Loadable |

#### Coreference (50+ datasets)

| Dataset | ID | Size | Focus | Status |
|---------|------|------|-------|--------|
| GAP | `GAP` | 4.5k | Gender-balanced pronouns | ✅ Loadable |
| PreCo | `PreCo` | 12k docs | Reading comprehension | ✅ Loadable |
| LitBank | `LitBank` | 100 docs | Literary text | ✅ Loadable |

#### Relation Extraction (30+ datasets)

| Dataset | ID | Focus | Relations | Status |
|---------|------|-------|-----------|--------|
| DocRED | `DocRED` | Document-level | 96 types | ✅ Loadable |
| ReTACRED | `ReTACRED` | Sentence-level | 40 types | ✅ Loadable |

### Discovering Datasets

```rust
use anno::eval::loader::DatasetId;

// Get all NER datasets
let ner_datasets: Vec<DatasetId> = DatasetId::all()
    .iter()
    .filter(|id| id.is_ner())
    .copied()
    .collect();

// Get all biomedical datasets
let bio_datasets: Vec<DatasetId> = DatasetId::all()
    .iter()
    .filter(|id| id.is_biomedical())
    .copied()
    .collect();

// Get all multilingual datasets
let multilingual: Vec<DatasetId> = DatasetId::all()
    .iter()
    .filter(|id| id.is_multilingual())
    .copied()
    .collect();
```

### Dataset Categories

The registry supports 23+ categories for filtering:
- **Domain**: `biomedical`, `legal`, `scientific`, `social_media`, `literary`, `news`, `dialogue`, `gaming`, `arcane_domain`
- **Language**: `multilingual`, `low_resource`, `code_switching`, `indigenous`, `historical`
- **Annotation**: `nested`, `discontinuous`, `long_document`
- **Evaluation**: `adversarial`, `bias_evaluation`, `few_shot`

---

## Usage

### Synthetic Datasets

```rust
use anno::eval::synthetic::{
    all_datasets,           // All ~200 examples
    technology_dataset,     // Single domain
    Domain, Difficulty,
};

// All examples
let all = all_datasets();

// Filter by domain
let tech = technology_dataset();
```

### Real Datasets

```rust
use anno::eval::loader::{DatasetLoader, DatasetId};

let loader = DatasetLoader::new();

// Load (downloads if needed)
let wikigold = loader.load_or_download(DatasetId::WikiGold)?;

// Check what's cached
for (id, is_cached) in loader.status() {
    println!("{:?}: {}", id, if is_cached { "✓" } else { "✗" });
}
```

### Backend Evaluation

```rust
use anno::eval::backend_eval::{BackendEvaluator, EvalConfig};

let evaluator = BackendEvaluator::new();
let report = evaluator.run_comprehensive();

println!("{}", report.to_markdown());
```

### NER Metrics

#### Modern Evaluation (2024+)

Modern NER evaluation goes beyond simple F1:

```rust
use anno::eval::dataset_quality::{DatasetQualityAnalyzer, QualityReport};

// Dataset quality metrics (research-backed)
let analyzer = DatasetQualityAnalyzer::default();
let report = analyzer.analyze(&train_data, &test_data);

println!("Unseen entity ratio: {:.1}%", report.difficulty.unseen_entity_ratio * 100.0);
println!("Entity ambiguity: {:.1}%", report.difficulty.entity_ambiguity * 100.0);
println!("Entity-null rate: {:.1}%", report.validity.entity_null_rate * 100.0);
```

Key modern metrics:
- **Unseen Entity Ratio**: % of test entities not in training (generalization test)
- **Entity Ambiguity**: Same surface form with different labels
- **Entity-Null Rate**: Token density of entities
- **Cross-corpus evaluation**: Train on X, test on Y

#### Legacy Standards (MUC/SemEval-2013)

For backwards compatibility with published benchmarks:

```rust
use anno::eval::ner_metrics::{evaluate_ner, EvalSpan};

let results = evaluate_ner(&gold, &predicted);
println!("{}", results.to_markdown());
```

Four schemas (useful for comparing with older papers):
- **Strict**: Exact boundary AND exact type
- **Exact**: Exact boundary only  
- **Partial**: Partial overlap (0.5 credit)
- **Type**: Overlap + type match

Note: These 2013 standards don't capture semantic similarity, LLM failure modes,
or cross-corpus generalization. Use `dataset_quality` for modern evaluation.

---

## Dataset Statistics

Run `cargo run --example eval_basic --features eval` to see live statistics:

```
┌─────────────────┬───────────┬────────┬────────┐
│ Backend         │ Precision │ Recall │ F1     │
├─────────────────┼───────────┼────────┼────────┤
│ Pattern         │    88.2%  │  12.8% │  22.4% │
│ Statistical     │    63.4%  │  22.2% │  32.9% │
│ Stacked         │    70.7%  │  35.0% │  46.9% │
└─────────────────┴───────────┴────────┴────────┘
```

---

## Adding Custom Datasets

```rust
use anno::eval::dataset::{AnnotatedExample, Domain, Difficulty};
use anno::eval::datasets::GoldEntity;
use anno::EntityType;

let custom = vec![
    AnnotatedExample {
        text: "Custom Corp hired Jane Doe on March 1.".into(),
        entities: vec![
            GoldEntity::new("Custom Corp", EntityType::Organization, 0),
            GoldEntity::new("Jane Doe", EntityType::Person, 19),
            GoldEntity::new("March 1", EntityType::Date, 31),
        ],
        domain: Domain::News,
        difficulty: Difficulty::Easy,
    },
];
```

