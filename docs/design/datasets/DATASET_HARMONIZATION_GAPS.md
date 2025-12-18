# Dataset Harmonization Gaps

Analysis of inconsistencies between datasets.toml, Rust enum, and download systems.

## Statistics

| Source | Count |
|--------|-------|
| `datasets.toml` entries | 65 |
| Rust `DatasetId` enum variants | 166 |
| Download URLs in `loader.rs` | 117 |
| Extended download script datasets | 10 |
| HF download script datasets | 6 |

**Missing download URLs**: 49 datasets have no download mechanism.

## Three Separate Download Systems (NOT CENTRALIZED)

### 1. Rust `loader.rs::download_url()`
- 117 datasets with explicit URLs
- Uses HuggingFace datasets-server API for many
- Has catch-all `_ => ""` for unimplemented

### 2. `scripts/download_hf_datasets.py`
- Only 6 datasets: jnlpba, bc2gm_full, uner, biomner, craft, msner
- Some use proxies/alternatives

### 3. `scripts/download_extended_datasets.py`
- 10 datasets: esperanto_ud, toki_pona, klingon, fiction_ner_750m, charactercodex, lince, gluecos, masakhaner (4 langs)
- Handles zip extraction

## Naming Inconsistencies

| Rust Enum | TOML | Issue |
|-----------|------|-------|
| `BC5CDR` | `bc5cdr` | CamelCase vs snake_case |
| `AmericasNLI` | `americasnli` | Missing underscore |
| `FictionNER750M` | `fiction_ner_750m` | Different underscore positions |
| `CharacterCodex` | (missing) | Not in TOML at all |
| `AMIMeeting` | `ami_meeting` | Different casing |
| `AISHELLNER` | `aishell_ner` | Different formatting |

## Datasets Without Full Integration

### In TOML but Rust enum uses different name (40)
These need `FromStr` aliases or TOML name updates:
- `aishell_ner` → `AISHELLNER`
- `ami_meeting` → `AMIMeeting`
- `bc5cdr` → `BC5CDR`
- `bitimebert` → `BiTimeBERT`
- `conll2003` → `CoNLL2003Sample`
- `corefud` → `CorefUD`
- `fewnerd` → `FewNERD`
- `fiction_ner_750m` → `FictionNER750M`
- `genia_nested` → `GENIANested`
- `gluecos` → `GLUECoS`
- ... (30 more)

### In Rust enum but NOT in TOML (100+)
These datasets have code but no TOML metadata:
- ACE2005, AIDA, AIOner, AnatEM
- ARRAU3, ARRAU_TRAINS, ARRAU_PEAR, ARRAU_GENIA, ARRAU_RST
- ASN, AstroNER, Bashi
- BC2GM, BC2GMFull, BC4CHEMD
- BioMNER, BioRED, BroadTwitterCorpus
- CADEC, CeREC, ChemDataExtractor
- CLEFClinicalCoref, CoNLL2002
- ... (80+ more)

## Missing Functions Coverage

These functions use `_ => default` catch-all, meaning many datasets get generic/wrong values:

| Function | Catch-all Value | Issue |
|----------|-----------------|-------|
| `download_url()` | `""` | No download for 49 datasets |
| `name()` | `"Unknown Dataset"` | Bad display name |
| `description()` | `"Dataset not yet fully integrated"` | No documentation |
| `entity_types()` | `&["ENTITY"]` | Wrong/generic types |
| `expected_counts()` | `None` | No validation |
| `language()` | `"en"` | Wrong for multilingual |
| `domain()` | `"general"` | Wrong domain classification |

## Recommended Actions

### High Priority
1. **Centralize download logic**: Move all download URLs to `datasets.toml` and read at runtime
2. **Add all 166 datasets to TOML** with proper metadata
3. **Generate Rust enum from TOML** via build script (single source of truth)

### Medium Priority
4. **Normalize naming**: Pick snake_case everywhere and generate CamelCase enum
5. **Add validation test**: Ensure TOML ↔ Rust enum 1:1 correspondence
6. **Fill in entity_types**: Critical for correct evaluation

### Low Priority
7. **Add expected_counts**: For test validation
8. **Add temporal metadata**: For historical datasets
9. **Document domain/language**: For filtering

## Placeholder/Proxy Usage in loader.rs

Many datasets use proxy datasets instead of the real data:

| Dataset | Proxy Used | Reason |
|---------|------------|--------|
| LinCE | CrossRE | "code-mixed datasets are rare and may require licenses" |
| CALCS | CrossRE | "placeholder proxy until direct source available" |
| FinnER | WikiGold | "placeholder proxy until proper legal dataset URL available" |
| NLMChem | CADEC | "similar medical domain, discontinuous entities" |
| CRAFT | CADEC | "placeholder proxy" |
| several coref | GAP | "similar Wikipedia-based coreference" |

## Incomplete Designs

### 1. Task Evaluator (`task_evaluator.rs`)
- Uses "placeholder" variance for statistical confidence intervals
- Per-type entity metrics use aggregate F1 as placeholder
- Relation extraction uses placeholder relations for nearby entity pairs

### 2. Coref Resolver (`coref_resolver.rs`)  
- TODO: "Make discourse referents first-class citizens with proper linking"
- Uses temporary workaround storing antecedent info in normalized field

### 3. Dataset Loading
- 49 datasets have empty download URLs (no download mechanism)
- Many datasets fall through to generic "Unknown Dataset" name
- Entity types default to generic `["ENTITY"]` for unimplemented

## What Should Be Centralized

### Currently Scattered
1. **Download URLs** - in 3 places (loader.rs, 2 Python scripts)
2. **Dataset metadata** - in 2 places (loader.rs methods, datasets.toml)
3. **Entity type definitions** - hardcoded in loader.rs, should be in TOML
4. **Expected counts** - hardcoded in loader.rs, should be in TOML
5. **Domain classification** - hardcoded in loader.rs, should be in TOML

### Proposed Centralization

```
datasets.toml (SINGLE SOURCE OF TRUTH)
       │
       ├── build.rs generates → DatasetId enum
       │
       ├── runtime reads → download URLs, metadata
       │
       └── validation ensures → 1:1 correspondence
```

This eliminates:
- Duplicate definitions
- Naming mismatches
- Missing metadata
- Multiple download systems

## Action Items for Full Harmonization

### Phase 1: Audit (current state) - COMPLETED
- [x] Count discrepancies between TOML and Rust
- [x] Identify datasets with no download URLs
- [x] Document placeholder/proxy usage

### Phase 2: Consolidate - COMPLETED
- [x] Add all 166 enum variants to datasets.toml (now 198 with aliases)
- [x] Add download_url field to 146+ TOML entries
- [x] Add entity_types field to 67+ TOML entries
- [x] Normalize names to snake_case in TOML with alias support
- [x] Add `rust_variant` field linking TOML to enum

### Phase 3: FromStr Sync - COMPLETED
- [x] Update FromStr to handle all TOML snake_case names
- [x] Add comprehensive aliases for flexibility
- [x] Validation test confirms all TOML entries parse correctly

### Phase 4: Validate - COMPLETED
- [x] Test: toml_entries_are_valid_dataset_ids (all 198 entries parse)
- [x] Test: dataset_ids_have_consistent_metadata
- [x] Test: all_groups_have_members
- [x] Test: group_members_are_in_all

### Phase 5: Automation & Validation - COMPLETED
- [x] Create build.rs to validate TOML against Rust enum
- [x] Add expected_counts to 17 major datasets
- [x] Create URL verification script (scripts/verify_dataset_urls.py)
- [x] Fix/mark 31 broken URLs (11 fixed, 20 marked unavailable due to licensing)
- [x] Add entity_types to 102 additional datasets (170 total with types)

### Final Statistics
- **Total datasets**: 198 (164 primary + 34 aliases)
- **With entity_types**: 170 (86%)
- **With download URLs**: 97 working (50%)
- **With expected_counts**: 17 major benchmarks

### Remaining Work (Future)
- [ ] Add expected_counts to remaining datasets
- [ ] Obtain licensed datasets (LDC, OntoNotes, TACRED, etc.)
- [ ] Generate DatasetId enum from TOML (full code generation)
- [ ] Add CI job for URL verification

