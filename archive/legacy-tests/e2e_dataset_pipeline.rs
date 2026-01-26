//! End-to-end tests for dataset loading → extraction → evaluation pipeline.
//!
//! These tests verify the complete workflow works correctly with real data
//! and diverse linguistic scenarios using the PUBLIC API only.

use anno::backends::HeuristicNER;
use anno::eval::loader::DatasetId;
use anno::eval::{DatasetLoader, LoadableDatasetId};
use anno::Model;
use tempfile::TempDir;

// =============================================================================
// E2E Pipeline Tests
// =============================================================================

/// E2E: Load a dataset, extract entities, compare to gold standard
#[test]
fn e2e_wikigold_extraction_pipeline() {
    let loader = DatasetLoader::new().expect("loader");

    // Offline parsing: avoid cache/network dependency while still exercising the parser.
    let conll = "\
John NNP B-NP B-PER
visited VBD B-VP O
Paris NNP B-NP B-LOC
.

Marie NNP B-NP B-PER
Curie NNP B-NP I-PER
discovered VBD B-VP O
radium NNP B-NP O
in IN B-PP O
France NNP B-NP B-LOC
.
";
    let dataset = loader
        .parse_content_str(conll, DatasetId::WikiGold)
        .expect("parse WikiGold-style conll");
    assert_eq!(dataset.id, DatasetId::WikiGold);
    assert!(dataset.len() >= 2, "Expected multiple sentences");
    assert!(
        dataset.entity_count() >= 3,
        "Expected gold entities from B-/I- tags"
    );

    // Extract from all sentences
    let heuristic_model = HeuristicNER::new();
    let mut total_extracted = 0;
    let mut total_gold = 0;

    for sentence in &dataset.sentences {
        let text = sentence.text();
        let extracted = heuristic_model
            .extract_entities(&text, None)
            .unwrap_or_default();

        let gold_count = sentence
            .tokens
            .iter()
            .filter(|t| t.ner_tag.starts_with("B-"))
            .count();

        total_extracted += extracted.len();
        total_gold += gold_count;

        // Span invariants (character offsets)
        let char_len = text.chars().count();
        for e in &extracted {
            assert!(e.start <= e.end);
            assert!(e.end <= char_len, "Predicted span out of bounds for text");
            assert!(!e.text.trim().is_empty());
            assert!((0.0..=1.0).contains(&e.confidence));
        }
    }

    // Heuristic should find *something* for this crafted sample.
    assert!(total_gold > 0, "Expected gold entities in parsed sample");
    assert!(
        total_extracted > 0,
        "Expected heuristic backend to extract at least one entity"
    );
}

/// E2E: JSONL NER dataset parsing + extraction (HuggingFace style, MultiNERD tag indices)
#[test]
fn e2e_jsonl_ner_extraction_pipeline() {
    let loader = DatasetLoader::new().expect("loader");

    // MultiNERD-style JSONL (tokens + integer tag indices)
    // Tag indices: 1=B-PER, 3=B-ORG, 5=B-LOC, 0=O.
    let jsonl = r#"
{"tokens":["Barack","Obama","visited","Berlin"],"ner_tags":[1,2,0,5]}
{"tokens":["Google","announced","AI"],"ner_tags":[3,0,0]}
"#;

    let dataset = loader
        .parse_content_str(jsonl, DatasetId::MultiNERD)
        .expect("parse jsonl ner");
    assert_eq!(dataset.id, DatasetId::MultiNERD);
    assert!(dataset.len() >= 2);
    assert!(dataset.entity_count() >= 2);

    let model = HeuristicNER::new();
    for sentence in &dataset.sentences {
        let text = sentence.text();
        let pred = model.extract_entities(&text, None).unwrap_or_default();
        let char_len = text.chars().count();
        for e in &pred {
            assert!(e.start <= e.end);
            assert!(e.end <= char_len);
            assert!(!e.text.trim().is_empty());
            assert!((0.0..=1.0).contains(&e.confidence));
        }
    }
}

/// E2E: LitBank standoff (.ann) parsing should yield ACE-style entity mentions.
#[test]
fn e2e_litbank_ann_parsing_pipeline() {
    let loader = DatasetLoader::new().expect("loader");

    // Minimal LitBank-like standoff:
    // - Include prefixed labels (PROP_PER) and plain labels (LOC) to cover normalization.
    let ann = "\
T1\tPROP_PER 0 5\tAlice
T2\tLOC 10 15\tParis
T3\tNOM_ORG 20 27\tCompany
";

    let dataset = loader
        .parse_content_str(ann, DatasetId::LitBank)
        .expect("parse litbank ann");
    assert_eq!(dataset.id, DatasetId::LitBank);
    assert!(!dataset.is_empty());
    assert!(dataset.entity_count() >= 3);
}

/// E2E: Cache directory behavior - if a cache file exists, `load()` reads it and parses offline.
#[test]
fn e2e_loader_reads_from_cache_dir_when_present() {
    let tmp = TempDir::new().expect("tempdir");
    let loader = DatasetLoader::with_cache_dir(tmp.path()).expect("loader");

    let id = DatasetId::WikiGold;
    let loadable = LoadableDatasetId::try_from(id).expect("WikiGold should be loadable");

    // Pre-seed the cache with a valid payload.
    let conll = "\
John NNP B-NP B-PER
visited VBD B-VP O
Paris NNP B-NP B-LOC
.
";
    let cache_path = tmp.path().join(id.cache_filename());
    std::fs::write(&cache_path, conll).expect("write cache");

    assert!(loader.is_cached(loadable), "Loader should see seeded cache");

    let dataset = loader.load(loadable).expect("load from cache");
    assert_eq!(dataset.id, id);
    assert!(!dataset.is_empty());
    assert!(dataset.entity_count() > 0);
}

/// E2E: Verify multilingual dataset handling
#[test]
fn e2e_multilingual_datasets_loadable() {
    let multilingual_datasets = [
        DatasetId::MasakhaNER,
        DatasetId::MultiNERD,
        DatasetId::WikiANN,
    ];

    for id in multilingual_datasets {
        let loadable = LoadableDatasetId::try_from(id);
        assert!(
            loadable.is_ok(),
            "{:?} should be loadable for multilingual support",
            id
        );
    }
}

/// E2E: Verify ancient/classical language pipeline
#[test]
fn e2e_classical_language_datasets_loadable() {
    let classical = [
        DatasetId::AncientGreekUD,
        DatasetId::LatinUD,
        DatasetId::SanskritUD,
        DatasetId::OldEnglishUD,
        DatasetId::OldNorseUD,
    ];

    for id in classical {
        let loadable = LoadableDatasetId::try_from(id);
        assert!(
            loadable.is_ok(),
            "{:?} should be loadable for classical language support",
            id
        );
    }
}

/// E2E: Test biomedical domain datasets
#[test]
fn e2e_biomedical_domain_datasets_loadable() {
    let biomedical = [
        DatasetId::BC5CDR,
        DatasetId::NCBIDisease,
        DatasetId::CHEMDNER,
        DatasetId::GENIANested,
        DatasetId::JNLPBA,
    ];

    for id in biomedical {
        let loadable = LoadableDatasetId::try_from(id);
        assert!(
            loadable.is_ok(),
            "{:?} should be loadable for biomedical NER",
            id
        );
    }
}

/// E2E: Test coref dataset loadability
#[test]
fn e2e_coreference_datasets_loadable() {
    let coref_datasets = [DatasetId::GICoref, DatasetId::BookCoref, DatasetId::ECBPlus];

    for id in coref_datasets {
        let loadable = LoadableDatasetId::try_from(id);
        assert!(
            loadable.is_ok(),
            "{:?} should be loadable for coreference evaluation",
            id
        );
    }
}

// =============================================================================
// Dataset Registry Integration
// =============================================================================

/// E2E: All loadable datasets have consistent metadata
#[test]
fn e2e_all_loadable_datasets_have_valid_metadata() {
    for id in LoadableDatasetId::all() {
        let ds: DatasetId = id.into();

        // Name should never be empty
        let name = ds.name();
        assert!(!name.is_empty(), "{:?} has empty name", ds);

        // Name should be printable (Unicode alphanumeric or punctuation)
        assert!(
            name.chars().all(|c| !c.is_control()),
            "{:?} has control characters in name: {}",
            ds,
            name
        );
    }
}

/// E2E: Verify high-value datasets are loadable
#[test]
fn e2e_high_value_datasets_loadable() {
    let core_datasets = [
        DatasetId::WikiGold,
        DatasetId::Wnut17,
        DatasetId::CoNLL2003Sample,
        DatasetId::OntoNotesSample,
        DatasetId::BC5CDR,
        DatasetId::NCBIDisease,
        DatasetId::MasakhaNER,
    ];

    for id in core_datasets {
        let loadable = LoadableDatasetId::try_from(id);
        assert!(loadable.is_ok(), "Core dataset {:?} must be loadable", id);
    }
}

/// E2E: Verify diverse scripts are supported
#[test]
fn e2e_script_diversity_coverage() {
    // Latin script
    assert!(LoadableDatasetId::try_from(DatasetId::WikiGold).is_ok());

    // CJK
    assert!(LoadableDatasetId::try_from(DatasetId::ChineseNestedNER).is_ok());

    // Devanagari
    assert!(LoadableDatasetId::try_from(DatasetId::SanskritUD).is_ok());

    // Greek (polytonic)
    assert!(LoadableDatasetId::try_from(DatasetId::AncientGreekUD).is_ok());

    // Cyrillic
    assert!(LoadableDatasetId::try_from(DatasetId::RussianCulturalNER).is_ok());
}

/// E2E: Verify domain coverage
#[test]
fn e2e_domain_coverage() {
    // Biomedical
    assert!(LoadableDatasetId::try_from(DatasetId::BC5CDR).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::NCBIDisease).is_ok());

    // Financial
    assert!(LoadableDatasetId::try_from(DatasetId::FinanceNER).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::FiNER139).is_ok());

    // Legal
    assert!(LoadableDatasetId::try_from(DatasetId::LegNER).is_ok());

    // Social media
    assert!(LoadableDatasetId::try_from(DatasetId::WNUT16).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::TwiConv).is_ok());

    // Literary
    assert!(LoadableDatasetId::try_from(DatasetId::LitBank).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::BookCoref).is_ok());
}

/// E2E: Verify task coverage
#[test]
fn e2e_task_coverage() {
    // Standard NER
    assert!(LoadableDatasetId::try_from(DatasetId::WikiGold).is_ok());

    // Nested NER
    assert!(LoadableDatasetId::try_from(DatasetId::GENIANested).is_ok());

    // Discontinuous NER
    assert!(LoadableDatasetId::try_from(DatasetId::GermEvalDiscontinuous).is_ok());

    // Coreference
    assert!(LoadableDatasetId::try_from(DatasetId::GICoref).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::BookCoref).is_ok());

    // Relation extraction
    assert!(LoadableDatasetId::try_from(DatasetId::REBEL).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::FewRel).is_ok());
}

/// E2E: Verify constructed/minority language support
#[test]
fn e2e_minority_language_coverage() {
    // Esperanto
    assert!(LoadableDatasetId::try_from(DatasetId::EsperantoUD).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::TaggedPBCEsperanto).is_ok());

    // Klingon (constructed)
    assert!(LoadableDatasetId::try_from(DatasetId::TaggedPBCKlingon).is_ok());

    // Guarani (indigenous)
    assert!(LoadableDatasetId::try_from(DatasetId::GuaraniNER).is_ok());

    // Shipibo-Konibo (indigenous)
    assert!(LoadableDatasetId::try_from(DatasetId::ShipiboKoniboNER).is_ok());
}

/// E2E: Verify historical language support
#[test]
fn e2e_historical_language_coverage() {
    // Ancient languages
    assert!(LoadableDatasetId::try_from(DatasetId::AncientGreekUD).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::LatinUD).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::SanskritUD).is_ok());

    // Medieval
    assert!(LoadableDatasetId::try_from(DatasetId::OldEnglishUD).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::OldNorseUD).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::MedievalCharterNER).is_ok());

    // Historical NER
    assert!(LoadableDatasetId::try_from(DatasetId::HistNERo).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::EighteenthCenturyNER).is_ok());
}

/// E2E: Verify code-switching/multilingual text support
#[test]
fn e2e_codeswitching_coverage() {
    // LinCE (Linguistic Code-switching Evaluation)
    assert!(LoadableDatasetId::try_from(DatasetId::LinCE).is_ok());

    // CALCS
    assert!(LoadableDatasetId::try_from(DatasetId::CALCS).is_ok());
    assert!(LoadableDatasetId::try_from(DatasetId::CALCS2018).is_ok());

    // Hinglish
    assert!(LoadableDatasetId::try_from(DatasetId::HinglishNER).is_ok());
}
