# Dataset Type System Design

## Current Problem

We have **dual sources of truth**:
1. `datasets.toml` - metadata (names, URLs, entity types)
2. `DatasetId` enum in Rust - type-safe identifiers

This causes:
- Sync issues (TOML and Rust can diverge)
- Runtime parsing errors possible
- No IDE autocomplete for dataset names
- Need for validation tests

## User Insight: Rust Should Be Primary

**The Rust type system should be the single source of truth.** TOML/JSON should be generated *from* Rust, not the other way around.

---

## Proposed Design: Derive-Based Metadata

### Core Enum with Metadata Attributes

```rust
use anno_derive::Dataset;

#[derive(Dataset, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatasetId {
    /// CoNLL 2003 NER benchmark (English)
    #[dataset(
        name = "CoNLL-2003",
        task = "ner",
        languages = ["en"],
        entity_types = ["PER", "LOC", "ORG", "MISC"],
        url = "https://huggingface.co/datasets/eriktks/conll2003",
        doc_count = 14041,
        domain = "newswire"
    )]
    CoNLL2003,
    
    /// OntoNotes 5.0 multilingual NER
    #[dataset(
        name = "OntoNotes 5.0",
        task = "ner",
        languages = ["en", "zh", "ar"],
        entity_types = ["PERSON", "GPE", "ORG", "DATE", "CARDINAL", ...],
        license = "LDC",
        doc_count = 18000
    )]
    OntoNotes50,
    
    /// WikiANN multilingual NER (282 languages)
    #[dataset(
        name = "WikiANN",
        task = "ner", 
        languages = ["multilingual"],
        entity_types = ["PER", "LOC", "ORG"],
        url = "https://huggingface.co/datasets/unimelb-nlp/wikiann"
    )]
    WikiANN,
    
    // ... 160+ more variants
}
```

### Generated Code (by derive macro)

The `#[derive(Dataset)]` macro generates:

```rust
impl DatasetId {
    /// Returns the human-readable name
    pub const fn name(&self) -> &'static str {
        match self {
            Self::CoNLL2003 => "CoNLL-2003",
            Self::OntoNotes50 => "OntoNotes 5.0",
            Self::WikiANN => "WikiANN",
            // ...
        }
    }
    
    /// Returns the primary task
    pub const fn task(&self) -> Task {
        match self {
            Self::CoNLL2003 | Self::OntoNotes50 | Self::WikiANN => Task::NER,
            // ...
        }
    }
    
    /// Returns entity types for NER datasets
    pub const fn entity_types(&self) -> &'static [&'static str] {
        match self {
            Self::CoNLL2003 => &["PER", "LOC", "ORG", "MISC"],
            Self::OntoNotes50 => &["PERSON", "GPE", "ORG", ...],
            // ...
        }
    }
    
    /// Returns download URL if available
    pub const fn download_url(&self) -> Option<&'static str> {
        match self {
            Self::CoNLL2003 => Some("https://huggingface.co/..."),
            Self::OntoNotes50 => None, // LDC license
            // ...
        }
    }
    
    /// All dataset IDs as a slice (for iteration)
    pub const fn all() -> &'static [DatasetId] {
        &[Self::CoNLL2003, Self::OntoNotes50, Self::WikiANN, ...]
    }
}
```

### FromStr with Aliases (also generated)

```rust
impl std::str::FromStr for DatasetId {
    type Err = ParseError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Generated from variant names + aliases
        match s.to_lowercase().as_str() {
            "conll2003" | "conll-2003" | "conll_2003" => Ok(Self::CoNLL2003),
            "ontonotes50" | "ontonotes" | "onto_notes" => Ok(Self::OntoNotes50),
            "wikiann" | "wiki_ann" | "wiki-ann" => Ok(Self::WikiANN),
            _ => Err(ParseError::UnknownDataset(s.to_string())),
        }
    }
}
```

---

## Benefits of Rust-Primary Design

| Aspect | TOML-Primary | Rust-Primary |
|--------|--------------|--------------|
| **Type Safety** | Runtime validation | Compile-time |
| **IDE Support** | None | Full autocomplete |
| **Refactoring** | Manual sync | Automatic |
| **Documentation** | Separate | Doc comments |
| **Testing** | Integration tests | Type system |
| **Serialization** | Parse at runtime | Derive Serialize |

---

## Migration Path

### Phase 1: Create Derive Macro
```rust
// anno-derive/src/lib.rs
#[proc_macro_derive(Dataset, attributes(dataset))]
pub fn derive_dataset(input: TokenStream) -> TokenStream {
    // Parse #[dataset(...)] attributes
    // Generate impl blocks
}
```

### Phase 2: Annotate Existing Enum
- Add `#[dataset(...)]` attributes to all 164 variants
- One-time migration from TOML metadata

### Phase 3: Generate TOML from Rust
```rust
// For documentation/export purposes only
impl DatasetId {
    pub fn to_toml(&self) -> String {
        // Generate TOML from compile-time metadata
    }
    
    pub fn export_all_toml() -> String {
        // Generate complete datasets.toml for reference
    }
}
```

### Phase 4: Remove datasets.toml as Source
- Keep it as generated artifact for human readability
- Rust enum becomes single source of truth

---

## Type-Safe Dataset Groups

```rust
#[derive(Dataset)]
pub enum DatasetId {
    // NER datasets
    #[dataset(group = "standard_ner")]
    CoNLL2003,
    #[dataset(group = "standard_ner")]
    OntoNotes50,
    
    // Biomedical NER
    #[dataset(group = "biomedical")]
    JNLPBA,
    #[dataset(group = "biomedical")]
    BC5CDR,
    
    // Code-switching
    #[dataset(group = "code_switching")]
    LinCE,
    #[dataset(group = "code_switching")]
    GLUECoS,
}

// Generated group accessors
impl DatasetId {
    pub const fn all_standard_ner() -> &'static [DatasetId] {
        &[Self::CoNLL2003, Self::OntoNotes50]
    }
    
    pub const fn all_biomedical() -> &'static [DatasetId] {
        &[Self::JNLPBA, Self::BC5CDR]
    }
    
    pub const fn all_code_switching() -> &'static [DatasetId] {
        &[Self::LinCE, Self::GLUECoS]
    }
}
```

---

## Expected Counts with Type Safety

```rust
#[derive(Dataset)]
pub enum DatasetId {
    #[dataset(
        expected_counts(train = 14041, dev = 3250, test = 3453)
    )]
    CoNLL2003,
}

// Generated
impl DatasetId {
    pub const fn expected_counts(&self) -> Option<ExpectedCounts> {
        match self {
            Self::CoNLL2003 => Some(ExpectedCounts {
                train: Some(14041),
                dev: Some(3250),
                test: Some(3453),
            }),
            _ => None,
        }
    }
}

pub struct ExpectedCounts {
    pub train: Option<usize>,
    pub dev: Option<usize>,
    pub test: Option<usize>,
}
```

---

## Implementation Priority

1. **High**: Create `anno-derive` crate with `#[derive(Dataset)]`
2. **High**: Migrate top 50 datasets with full metadata
3. **Medium**: Add remaining 114 datasets
4. **Medium**: Generate TOML/JSON exports
5. **Low**: Add expected_counts to all datasets

This design makes the **Rust type system the authority**, with TOML/JSON as optional exports for documentation and interoperability.

