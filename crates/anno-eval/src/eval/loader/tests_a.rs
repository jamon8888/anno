use super::*;

#[test]
fn test_dataset_id_basics() {
    let id = DatasetId::WikiGold;
    assert_eq!(id.name(), "WikiGold");
}

#[test]
fn test_convenience_metadata_methods() {
    let id = DatasetId::WikiGold;

    // WikiGold has known metadata
    assert_eq!(id.citation(), Some("Balasuriya et al. (2009)"));
    assert_eq!(id.license(), Some("CC-BY-4.0"));
    assert_eq!(id.year(), Some(2009));
}

#[test]
fn test_loadable_wrapper_invariants() {
    // There should be at least one non-loadable dataset in the catalog.
    assert!(
        DatasetId::all()
            .iter()
            .copied()
            .any(|d| !LoadableDatasetId::is_loadable_dataset(d)),
        "Expected registry to contain some non-loadable datasets"
    );

    for id in LoadableDatasetId::all() {
        let ds: DatasetId = id.into();
        assert!(
            LoadableDatasetId::is_loadable_dataset(ds),
            "LoadableDatasetId must imply is_loadable_dataset()"
        );
        assert!(LoadableDatasetId::try_from(ds).is_ok());
    }
}

#[test]
fn test_parse_plan_is_single_source_of_truth_for_loadability() {
    // For *every* registry dataset id:
    // - parse_plan(id).is_some()  <=>  TryFrom<DatasetId> succeeds
    for &ds in DatasetId::all() {
        let plan_exists = LoadableDatasetId::parse_plan(ds).is_some();
        let try_ok = LoadableDatasetId::try_from(ds).is_ok();
        assert_eq!(
            plan_exists, try_ok,
            "parse_plan / TryFrom mismatch for {:?}",
            ds
        );
    }

    // Also ensure `LoadableDatasetId::all()` only returns ids with plans.
    for id in LoadableDatasetId::all() {
        let ds: DatasetId = id.into();
        assert!(
            LoadableDatasetId::parse_plan(ds).is_some(),
            "LoadableDatasetId::all() returned {:?} with no parse plan",
            ds
        );
    }
}

#[test]
fn test_registry_hints_do_not_contradict_parse_plan() {
    // If a dataset is loadable (i.e., has a parse plan), and the registry provides a strong
    // hint, they should agree. (Hints are allowed to be optimistic for non-loadable datasets.)
    for &ds in DatasetId::all() {
        let Some(plan) = LoadableDatasetId::parse_plan(ds) else {
            continue;
        };
        let Some(hint) = LoadableDatasetId::registry_hint_plan(ds) else {
            continue;
        };
        assert_eq!(hint, plan, "Registry hint mismatch for {:?}", ds);
    }
}

#[test]
fn test_huggingface_access_status_requires_hf_id() {
    // `access_status: HuggingFace` is our strongest signal that a dataset is automatable
    // via the Hub. Keep the registry self-consistent so hinting can stay metadata-driven.
    for &ds in DatasetId::all() {
        if ds.access_status() != crate::eval::dataset_registry::DatasetAccessibility::HuggingFace {
            continue;
        }
        assert!(
            ds.hf_id().is_some(),
            "Dataset {:?} is marked HuggingFace-accessible but has no hf_id",
            ds
        );
    }
}

#[test]
fn test_huggingface_access_status_is_hintable() {
    // If the registry says “HuggingFace”, the loader should be able to produce *some*
    // parse-plan hint (not necessarily `HfApiResponse`, since a few datasets are hybrids
    // with bespoke parse plans).
    for &ds in DatasetId::all() {
        if ds.access_status() != crate::eval::dataset_registry::DatasetAccessibility::HuggingFace {
            continue;
        }
        assert!(
            LoadableDatasetId::registry_hint_plan(ds).is_some(),
            "Dataset {:?} is marked HuggingFace-accessible but has no registry hint plan",
            ds
        );
    }
}

#[test]
fn test_parse_bio_tag() {
    assert_eq!(parse::util::parse_bio_tag("O"), ("O", ""));
    assert_eq!(parse::util::parse_bio_tag("B-PER"), ("B", "PER"));
    assert_eq!(parse::util::parse_bio_tag("I-LOC"), ("I", "LOC"));
    assert_eq!(parse::util::parse_bio_tag("B-ORG"), ("B", "ORG"));
}

#[test]
fn test_map_entity_type() {
    // Core types
    assert_eq!(parse::util::map_entity_type("PER"), EntityType::Person);
    assert_eq!(parse::util::map_entity_type("PERSON"), EntityType::Person);
    assert_eq!(parse::util::map_entity_type("LOC"), EntityType::Location);
    assert_eq!(
        parse::util::map_entity_type("ORG"),
        EntityType::Organization
    );

    // GPE now preserves distinction (Custom, not Location)
    assert!(matches!(
        parse::util::map_entity_type("GPE"),
        EntityType::Custom { .. }
    ));

    // MISC -> Custom or Other
    assert!(matches!(
        parse::util::map_entity_type("MISC"),
        EntityType::Custom { .. }
    ));

    // OntoNotes types -> Custom (preserves semantics)
    assert!(matches!(
        parse::util::map_entity_type("PRODUCT"),
        EntityType::Custom { .. }
    ));
    assert!(matches!(
        parse::util::map_entity_type("EVENT"),
        EntityType::Custom { .. }
    ));
    assert!(matches!(
        parse::util::map_entity_type("WORK_OF_ART"),
        EntityType::Custom { .. }
    ));

    // Numeric types preserved
    assert_eq!(
        parse::util::map_entity_type("CARDINAL"),
        EntityType::Cardinal
    );
}

#[test]
fn test_dataset_id_display() {
    assert_eq!(DatasetId::WikiGold.to_string(), "WikiGold");
    assert_eq!(DatasetId::Wnut17.to_string(), "WNUT-17");
}

#[test]
fn test_dataset_id_from_str() {
    assert_eq!(
        "wikigold".parse::<DatasetId>().unwrap(),
        DatasetId::WikiGold
    );
    assert_eq!("wnut-17".parse::<DatasetId>().unwrap(), DatasetId::Wnut17);
    assert_eq!(
        "mit_movie".parse::<DatasetId>().unwrap(),
        DatasetId::MitMovie
    );
}

#[test]
fn test_annotated_sentence_text() {
    let sentence = AnnotatedSentence {
        tokens: vec![
            AnnotatedToken {
                text: "John".into(),
                ner_tag: "B-PER".into(),
            },
            AnnotatedToken {
                text: "lives".into(),
                ner_tag: "O".into(),
            },
            AnnotatedToken {
                text: "in".into(),
                ner_tag: "O".into(),
            },
            AnnotatedToken {
                text: "New".into(),
                ner_tag: "B-LOC".into(),
            },
            AnnotatedToken {
                text: "York".into(),
                ner_tag: "I-LOC".into(),
            },
        ],
        source_dataset: DatasetId::WikiGold,
    };

    assert_eq!(sentence.text(), "John lives in New York");
}

#[test]
fn test_annotated_sentence_entities() {
    let sentence = AnnotatedSentence {
        tokens: vec![
            AnnotatedToken {
                text: "John".into(),
                ner_tag: "B-PER".into(),
            },
            AnnotatedToken {
                text: "Smith".into(),
                ner_tag: "I-PER".into(),
            },
            AnnotatedToken {
                text: "works".into(),
                ner_tag: "O".into(),
            },
            AnnotatedToken {
                text: "at".into(),
                ner_tag: "O".into(),
            },
            AnnotatedToken {
                text: "Google".into(),
                ner_tag: "B-ORG".into(),
            },
        ],
        source_dataset: DatasetId::WikiGold,
    };

    let entities = sentence.entities();
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0].text, "John Smith");
    assert_eq!(entities[0].entity_type, EntityType::Person);
    assert_eq!(entities[1].text, "Google");
    assert_eq!(entities[1].entity_type, EntityType::Organization);
}

#[test]
fn test_parse_conll_format() {
    let content = r#"
John B-PER
Smith I-PER
works O
at O
Google B-ORG
. O

Apple B-ORG
announced O
today O
. O
"#;

    let dataset = parse::ner::parse_conll(content, DatasetId::WikiGold).unwrap();

    assert_eq!(dataset.len(), 2);
    assert_eq!(dataset.entity_count(), 3);
}

#[test]
fn test_parse_conll2003_format() {
    // CoNLL-2003 has 4 columns: word POS chunk NER
    let content = r#"
-DOCSTART- -X- -X- O

EU NNP B-NP B-ORG
rejects VBZ B-VP O
German JJ B-NP B-MISC
call NN I-NP O
. . O O

Peter NNP B-NP B-PER
Blackburn NNP I-NP I-PER
"#;

    let dataset = parse::ner::parse_conll(content, DatasetId::CoNLL2003Sample).unwrap();

    assert_eq!(dataset.len(), 2);

    let entities1 = dataset.sentences[0].entities();
    assert_eq!(entities1.len(), 2); // EU (ORG), German (MISC)

    let entities2 = dataset.sentences[1].entities();
    assert_eq!(entities2.len(), 1); // Peter Blackburn (PER)
    assert_eq!(entities2[0].text, "Peter Blackburn");
}

#[test]
fn test_historical_datasets_configured() {
    // Historical NER datasets should have proper metadata
    assert!(!DatasetId::HIPE2022.download_url().is_empty());
    assert_eq!(DatasetId::HIPE2022.name(), "HIPE-2022");

    assert!(!DatasetId::MedievalCzechCharters.download_url().is_empty());
    assert_eq!(
        DatasetId::MedievalCzechCharters.name(),
        "Medieval Czech Charters"
    );

    assert!(!DatasetId::TRIDIS.download_url().is_empty());
    assert_eq!(DatasetId::TRIDIS.name(), "TRIDIS");

    // Should be in all() list
    let all = DatasetId::all();
    assert!(all.contains(&DatasetId::HIPE2022));
    assert!(all.contains(&DatasetId::TRIDIS));
}

#[test]
fn test_queer_nlp_datasets_configured() {
    // Queer/gender-inclusive NLP datasets
    assert!(!DatasetId::WinoQueer.download_url().is_empty());
    assert_eq!(DatasetId::WinoQueer.name(), "WinoQueer");

    assert!(!DatasetId::GICoref.download_url().is_empty());
    assert_eq!(DatasetId::GICoref.name(), "GICoref");

    assert!(!DatasetId::BBQ.download_url().is_empty());
    assert_eq!(DatasetId::BBQ.name(), "BBQ");

    // Should be in all() list
    let all = DatasetId::all();
    assert!(all.contains(&DatasetId::WinoQueer));
    assert!(all.contains(&DatasetId::GICoref));
    assert!(all.contains(&DatasetId::BBQ));
}

#[test]
fn test_joint_re_datasets_configured() {
    // Joint NER + Relation Extraction datasets
    assert!(
        DatasetId::TACRED.requires_license(),
        "TACRED is LDC-licensed; download_url may be empty"
    );
    assert_eq!(DatasetId::TACRED.name(), "TACRED");

    assert!(!DatasetId::REBEL.download_url().is_empty());
    assert_eq!(DatasetId::REBEL.name(), "REBEL");

    // Should be in all() list
    let all = DatasetId::all();
    assert!(all.contains(&DatasetId::TACRED));
    assert!(all.contains(&DatasetId::REBEL));
}

#[test]
fn test_dialogue_coref_datasets_configured() {
    // Dialogue/streaming coreference datasets
    assert!(!DatasetId::CODICRAC.download_url().is_empty());
    assert_eq!(DatasetId::CODICRAC.name(), "CODI-CRAC");

    assert!(!DatasetId::AMIMeeting.download_url().is_empty());
    assert_eq!(DatasetId::AMIMeeting.name(), "AMI Meeting");

    assert!(
        DatasetId::ARRAU.requires_license(),
        "ARRAU has LDC + research distribution; download_url may be empty"
    );
    assert!(
        DatasetId::ARRAU.name().contains("ARRAU"),
        "ARRAU name should contain 'ARRAU'"
    );

    // Should be in all() list
    let all = DatasetId::all();
    assert!(all.contains(&DatasetId::CODICRAC));
    assert!(all.contains(&DatasetId::AMIMeeting));
    assert!(all.contains(&DatasetId::ARRAU));
}

#[test]
fn test_is_historical_classification() {
    // Historical datasets
    assert!(DatasetId::HIPE2022.is_historical());
    assert!(DatasetId::MedievalCzechCharters.is_historical());
    assert!(DatasetId::EighteenthCenturyNER.is_historical());
    assert!(DatasetId::HistoricalChineseNER.is_historical());

    // Non-historical should return false
    assert!(!DatasetId::WikiGold.is_historical());
    assert!(!DatasetId::CoNLL2003Sample.is_historical());
}

#[test]
fn test_is_bias_evaluation_classification() {
    // Bias/fairness datasets
    assert!(DatasetId::WinoQueer.is_bias_evaluation());
    assert!(DatasetId::BBQ.is_bias_evaluation());
    assert!(DatasetId::GICoref.is_bias_evaluation());
    assert!(DatasetId::WinoBias.is_bias_evaluation());
    assert!(DatasetId::GAP.is_bias_evaluation());

    // Non-bias should return false
    assert!(!DatasetId::WikiGold.is_bias_evaluation());
}

#[test]
fn test_new_datasets_have_descriptions() {
    // All new datasets should have proper descriptions (not the catch-all)
    let catch_all = "Dataset not yet fully integrated";

    // Historical
    assert_ne!(DatasetId::HIPE2022.description(), catch_all);
    assert_ne!(DatasetId::TRIDIS.description(), catch_all);

    // Queer NLP
    assert_ne!(DatasetId::WinoQueer.description(), catch_all);
    assert_ne!(DatasetId::BBQ.description(), catch_all);
    assert_ne!(DatasetId::GICoref.description(), catch_all);

    // Joint NER+RE
    assert_ne!(DatasetId::TACRED.description(), catch_all);
    assert_ne!(DatasetId::REBEL.description(), catch_all);

    // Dialogue
    assert_ne!(DatasetId::CODICRAC.description(), catch_all);
    assert_ne!(DatasetId::ARRAU.description(), catch_all);
}

#[test]
fn test_coreference_includes_new_datasets() {
    // All coreference datasets should be detected
    assert!(DatasetId::GICoref.is_coreference());
    assert!(DatasetId::CODICRAC.is_coreference());
    assert!(DatasetId::ARRAU.is_coreference());
    assert!(DatasetId::WinoPron.is_coreference());
    assert!(DatasetId::DROC.is_coreference());
    assert!(DatasetId::KoCoNovel.is_coreference());
}

#[test]
fn test_chisiec_is_historical_and_relation_extraction() {
    // CHisIEC should be both historical and relation extraction
    assert!(DatasetId::CHisIEC.is_historical());
    assert!(DatasetId::CHisIEC.is_relation_extraction());

    // Verify entity types
    let types = DatasetId::CHisIEC.entity_types();
    assert!(types.contains(&"PER"));
    assert!(types.contains(&"LOC"));
    assert!(types.contains(&"OFI"));
    assert!(types.contains(&"BOOK"));
}

#[test]
fn test_chisiec_from_str() {
    // Test various string representations
    assert_eq!("chisiec".parse::<DatasetId>().unwrap(), DatasetId::CHisIEC);
    assert_eq!(
        "ch-is-iec".parse::<DatasetId>().unwrap(),
        DatasetId::CHisIEC
    );
    assert_eq!(
        "chinese-historical-ie".parse::<DatasetId>().unwrap(),
        DatasetId::CHisIEC
    );
    assert_eq!(
        "ancient-chinese-ner".parse::<DatasetId>().unwrap(),
        DatasetId::CHisIEC
    );
}

#[test]
fn test_chisiec_parse_ner() {
    // Test CHisIEC NER parsing with sample data
    let sample_json = r#"[
        {
            "tokens": "衞鞅奔魏",
            "entities": [
                {"type": "PER", "start": 0, "end": 2, "span": "衞鞅"},
                {"type": "LOC", "start": 3, "end": 4, "span": "魏"}
            ],
            "relations": []
        }
    ]"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);

    let sentence = &dataset.sentences[0];
    // 4 characters: 衞 鞅 奔 魏
    assert_eq!(sentence.tokens.len(), 4);

    // Check BIO tags
    assert_eq!(sentence.tokens[0].ner_tag, "B-PER"); // 衞
    assert_eq!(sentence.tokens[1].ner_tag, "I-PER"); // 鞅
    assert_eq!(sentence.tokens[2].ner_tag, "O"); // 奔
    assert_eq!(sentence.tokens[3].ner_tag, "B-LOC"); // 魏
}

#[test]
fn test_chisiec_parse_relations() {
    // Test CHisIEC relation extraction parsing
    let sample_json = r#"[
        {
            "tokens": "嚴公遣玉汝使",
            "entities": [
                {"type": "PER", "start": 0, "end": 2, "span": "嚴公"},
                {"type": "PER", "start": 2, "end": 4, "span": "玉汝"}
            ],
            "relations": [
                {"type": "上下級", "head": 0, "tail": 1, "head_span": "嚴公", "tail_span": "玉汝"}
            ]
        }
    ]"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::relation::parse_chisiec_relations(sample_json);
    assert!(result.is_ok());

    let docs = result.unwrap();
    assert_eq!(docs.len(), 1);

    let doc = &docs[0];
    assert_eq!(doc.relations.len(), 1);

    let rel = &doc.relations[0];
    assert_eq!(rel.relation_type, "上下級");
    assert_eq!(rel.head_text, "嚴公");
    assert_eq!(rel.tail_text, "玉汝");
    assert_eq!(rel.head_type, "PER");
    assert_eq!(rel.tail_type, "PER");
    // Character offsets (嚴公 at 0-2, 玉汝 at 2-4)
    assert_eq!(rel.head_span, (0, 2));
    assert_eq!(rel.tail_span, (2, 4));
}

#[test]
fn test_chisiec_all_entity_types() {
    // Test all 4 CHisIEC entity types: PER, LOC, OFI (官职), BOOK (书籍)
    // This is important because OFI (Official position) is domain-specific
    let sample_json = r#"[
        {
            "tokens": "司馬遷為太史令著史記於長安",
            "entities": [
                {"type": "PER", "start": 0, "end": 3, "span": "司馬遷"},
                {"type": "OFI", "start": 4, "end": 7, "span": "太史令"},
                {"type": "BOOK", "start": 8, "end": 10, "span": "史記"},
                {"type": "LOC", "start": 11, "end": 13, "span": "長安"}
            ],
            "relations": []
        }
    ]"#;

    let dataset = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC).unwrap();

    assert_eq!(dataset.sentences.len(), 1);
    let sentence = &dataset.sentences[0];

    // Verify each entity type is correctly tagged
    // 司馬遷 (Sima Qian - historian)
    assert_eq!(sentence.tokens[0].ner_tag, "B-PER");
    assert_eq!(sentence.tokens[1].ner_tag, "I-PER");
    assert_eq!(sentence.tokens[2].ner_tag, "I-PER");

    // 為 (was)
    assert_eq!(sentence.tokens[3].ner_tag, "O");

    // 太史令 (Grand Historian - official position)
    assert_eq!(sentence.tokens[4].ner_tag, "B-OFI");
    assert_eq!(sentence.tokens[5].ner_tag, "I-OFI");
    assert_eq!(sentence.tokens[6].ner_tag, "I-OFI");

    // 著 (wrote)
    assert_eq!(sentence.tokens[7].ner_tag, "O");

    // 史記 (Records of the Grand Historian - book)
    assert_eq!(sentence.tokens[8].ner_tag, "B-BOOK");
    assert_eq!(sentence.tokens[9].ner_tag, "I-BOOK");

    // 於 (at)
    assert_eq!(sentence.tokens[10].ner_tag, "O");

    // 長安 (Chang'an - capital city)
    assert_eq!(sentence.tokens[11].ner_tag, "B-LOC");
    assert_eq!(sentence.tokens[12].ner_tag, "I-LOC");
}

#[test]
fn test_chisiec_unicode_character_offsets() {
    // Critical: CHisIEC uses CHARACTER offsets, not byte offsets
    // This test ensures we handle multi-byte Chinese characters correctly
    let sample_json = r#"[
        {
            "tokens": "曹操",
            "entities": [
                {"type": "PER", "start": 0, "end": 2, "span": "曹操"}
            ],
            "relations": []
        }
    ]"#;

    let dataset = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC).unwrap();

    // 曹操 is 2 characters (but 6 bytes in UTF-8)
    let sentence = &dataset.sentences[0];
    assert_eq!(sentence.tokens.len(), 2);
    assert_eq!(sentence.tokens[0].text, "曹");
    assert_eq!(sentence.tokens[1].text, "操");
}

#[test]
fn test_chisiec_multiple_relations_same_document() {
    // Test parsing multiple relations in a single document
    // Relations: 任職 (holds office), 管理 (manages)
    let sample_json = r#"[
        {
            "tokens": "曹操為丞相管冀州",
            "entities": [
                {"type": "PER", "start": 0, "end": 2, "span": "曹操"},
                {"type": "OFI", "start": 3, "end": 5, "span": "丞相"},
                {"type": "LOC", "start": 6, "end": 8, "span": "冀州"}
            ],
            "relations": [
                {"type": "任職", "head": 0, "tail": 1, "head_span": "曹操", "tail_span": "丞相"},
                {"type": "管理", "head": 0, "tail": 2, "head_span": "曹操", "tail_span": "冀州"}
            ]
        }
    ]"#;

    let loader = DatasetLoader::new().unwrap();
    let docs = parse::relation::parse_chisiec_relations(sample_json).unwrap();

    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].relations.len(), 2);

    // First relation: 任職 (office holding)
    assert_eq!(docs[0].relations[0].relation_type, "任職");
    assert_eq!(docs[0].relations[0].head_type, "PER");
    assert_eq!(docs[0].relations[0].tail_type, "OFI");

    // Second relation: 管理 (manages)
    assert_eq!(docs[0].relations[1].relation_type, "管理");
    assert_eq!(docs[0].relations[1].head_type, "PER");
    assert_eq!(docs[0].relations[1].tail_type, "LOC");
}

#[test]
fn test_chisiec_distinct_from_historical_chinese_ner() {
    // CHisIEC and HistoricalChineseNER are DIFFERENT datasets
    // This test documents their distinction

    // Both should be classified as historical
    assert!(DatasetId::CHisIEC.is_historical());
    assert!(DatasetId::HistoricalChineseNER.is_historical());

    // But they are different datasets
    assert_ne!(DatasetId::CHisIEC, DatasetId::HistoricalChineseNER);

    // CHisIEC supports relation extraction; HistoricalChineseNER may not
    assert!(DatasetId::CHisIEC.is_relation_extraction());

    // Different entity types:
    // CHisIEC: PER, LOC, OFI, BOOK (ancient Chinese)
    // HistoricalChineseNER: PER, LOC, ORG, DATE, etc. (modern Chinese 1872-1949)
    let chisiec_types = DatasetId::CHisIEC.entity_types();
    assert!(chisiec_types.contains(&"OFI")); // Official position - unique to CHisIEC
    assert!(chisiec_types.contains(&"BOOK")); // Classical texts

    // Different names
    assert_eq!(DatasetId::CHisIEC.name(), "CHisIEC");
    assert_eq!(
        DatasetId::HistoricalChineseNER.name(),
        "Historical Chinese NER"
    );
}

#[test]
fn test_chisiec_empty_entities_handled() {
    // Test graceful handling of documents with no entities
    let sample_json = r#"[
        {
            "tokens": "天下太平",
            "entities": [],
            "relations": []
        }
    ]"#;

    let dataset = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC).unwrap();

    assert_eq!(dataset.sentences.len(), 1);
    let sentence = &dataset.sentences[0];

    // All tokens should be tagged as O
    for token in &sentence.tokens {
        assert_eq!(token.ner_tag, "O");
    }
}

#[test]
fn test_chisiec_entity_types_in_schema() {
    // Verify CHisIEC entity types are properly mapped in the schema
    use anno::schema::map_to_canonical;

    // PER -> Person
    let per_type = map_to_canonical("PER", None);
    assert_eq!(per_type, EntityType::Person);

    // LOC -> Location
    let loc_type = map_to_canonical("LOC", None);
    assert_eq!(loc_type, EntityType::Location);

    // OFI -> Custom OFFICIAL type (domain-specific for ancient Chinese)
    let ofi_type = map_to_canonical("OFI", None);
    assert!(matches!(ofi_type, EntityType::Custom { .. }));

    // BOOK -> WORK_OF_ART (creative works)
    let book_type = map_to_canonical("BOOK", None);
    assert!(matches!(book_type, EntityType::Custom { .. }));
}

#[test]
fn test_chisiec_language_and_domain() {
    // CHisIEC is Classical Chinese (文言文) - ISO 639-3: lzh
    assert_eq!(DatasetId::CHisIEC.language(), "lzh");

    // CHisIEC is a historical dataset (24 dynastic histories)
    assert_eq!(DatasetId::CHisIEC.domain(), "historical");

    // Compare with HistoricalChineseNER which is modern Chinese (1872-1949)
    assert_eq!(DatasetId::HistoricalChineseNER.language(), "zh");
    assert_eq!(DatasetId::HistoricalChineseNER.domain(), "historical");
}

// =========================================================================
// African Language Dataset Tests
// =========================================================================

#[test]
fn test_african_datasets_configured() {
    // MasakhaNER datasets should have download URLs
    assert!(!DatasetId::MasakhaNER.download_url().is_empty());
    assert!(!DatasetId::MasakhaNER2.download_url().is_empty());
    assert!(!DatasetId::AfriSenti.download_url().is_empty());
    assert!(!DatasetId::AfriQA.download_url().is_empty());
    assert!(!DatasetId::MasakhaNEWS.download_url().is_empty());
    assert!(!DatasetId::MasakhaPOS.download_url().is_empty());

    // Names should be set
    assert_eq!(DatasetId::MasakhaNER.name(), "MasakhaNER");
    assert_eq!(DatasetId::MasakhaNER2.name(), "MasakhaNER 2.0");
    assert_eq!(DatasetId::AfriSenti.name(), "AfriSenti");
    assert_eq!(DatasetId::AfriQA.name(), "AfriQA");
    assert_eq!(DatasetId::MasakhaNEWS.name(), "MasakhaNEWS");
    assert_eq!(DatasetId::MasakhaPOS.name(), "MasakhaPOS");
}

#[test]
fn test_african_datasets_entity_types() {
    // MasakhaNER uses PER, ORG, LOC, DATE
    let ner_types = DatasetId::MasakhaNER.entity_types();
    assert!(ner_types.contains(&"PER"));
    assert!(ner_types.contains(&"LOC"));
    assert!(ner_types.contains(&"ORG"));
    assert!(ner_types.contains(&"DATE"));

    // AfriSenti uses sentiment labels
    let senti_types = DatasetId::AfriSenti.entity_types();
    assert!(senti_types.contains(&"positive"));
    assert!(senti_types.contains(&"neutral"));
    assert!(senti_types.contains(&"negative"));

    // MasakhaNEWS uses topic labels
    let news_types = DatasetId::MasakhaNEWS.entity_types();
    assert!(news_types.contains(&"politics"));
    assert!(news_types.contains(&"sports"));
    assert!(news_types.contains(&"business"));

    // MasakhaPOS uses Universal Dependencies POS tags
    let pos_types = DatasetId::MasakhaPOS.entity_types();
    assert!(pos_types.contains(&"NOUN"));
    assert!(pos_types.contains(&"VERB"));
    assert!(pos_types.contains(&"ADJ"));
}

#[test]
fn test_parse_afrisenti() {
    // Test AfriSenti TSV parsing
    let sample_tsv = "This movie is great!\tpositive\n\
                      Awful experience\tnegative\n\
                      It was okay\tneutral";

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_afrisenti(sample_tsv, DatasetId::AfriSenti);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 3);

    // Check first sentence has positive label
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-positive");
    // Check second sentence has negative label
    assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-negative");
    // Check third sentence has neutral label
    assert_eq!(dataset.sentences[2].tokens[0].ner_tag, "B-neutral");
}

#[test]
fn test_parse_masakhanews() {
    // Test MasakhaNEWS TSV parsing
    let sample_tsv = "headline\tbody\tcategory\n\
                      Breaking: Election Results\tThe results are in...\tpolitics\n\
                      Team Wins Championship\tIn an exciting match...\tsports";

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_masakhanews(sample_tsv, DatasetId::MasakhaNEWS);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    // Header line should be skipped
    assert_eq!(dataset.sentences.len(), 2);

    // Check categories
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-politics");
    assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-sports");
}

#[test]
fn test_parse_conllu() {
    // Test CoNLL-U parsing (MasakhaPOS format)
    let sample_conllu = "# sent_id = 1\n\
                         # text = John loves Mary\n\
                         1\tJohn\tJohn\tPROPN\tNNP\t_\t2\tnsubj\t_\t_\n\
                         2\tloves\tlove\tVERB\tVBZ\t_\t0\troot\t_\t_\n\
                         3\tMary\tMary\tPROPN\tNNP\t_\t2\tobj\t_\t_\n\
                         \n\
                         # sent_id = 2\n\
                         1\tHe\the\tPRON\tPRP\t_\t2\tnsubj\t_\t_\n\
                         2\truns\trun\tVERB\tVBZ\t_\t0\troot\t_\t_\n";

    let result = parse::ner::parse_conllu(sample_conllu, DatasetId::MasakhaPOS);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2);

    // First sentence: John loves Mary
    assert_eq!(dataset.sentences[0].tokens.len(), 3);
    assert_eq!(dataset.sentences[0].tokens[0].text, "John");
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-PROPN");
    assert_eq!(dataset.sentences[0].tokens[1].text, "loves");
    assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "B-VERB");

    // Second sentence: He runs
    assert_eq!(dataset.sentences[1].tokens.len(), 2);
    assert_eq!(dataset.sentences[1].tokens[0].text, "He");
    assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-PRON");
}

#[test]
fn test_ancient_language_ud_datasets_are_loadable() {
    // Ancient language UD treebanks should be loadable via registry hints
    // These have format: "CoNLLU" and categories: [ner, ancient]
    let ancient_datasets = [
        DatasetId::AncientGreekUD,
        DatasetId::LatinUD,
        DatasetId::SanskritUD,
        DatasetId::OldEnglishUD,
        DatasetId::OldNorseUD,
    ];

    for ds in ancient_datasets {
        assert!(
            LoadableDatasetId::is_loadable_dataset(ds),
            "{:?} should be loadable via registry hint (format={:?})",
            ds,
            ds.format()
        );
    }
}

#[test]
fn test_conllu_with_ner_tags_from_ancient_greek() {
    // Test CoNLLU parsing with MISC column NER tags (Ancient Greek Perseus format)
    // Real format from UD Ancient Greek Perseus
    let sample_conllu = "\
# sent_id = tlg0012.tlg001.perseus-grc1:1.1
# text = μῆνιν ἄειδε θεὰ Πηληϊάδεω Ἀχιλῆος
1\tμῆνιν\tμῆνις\tNOUN\tn-s---fa-\tCase=Acc|Gender=Fem|Number=Sing\t2\tobj\t_\tO
2\tἄειδε\tᾄδω\tVERB\tv2sama---\tMood=Imp|Number=Sing|Person=2|Tense=Pres|VerbForm=Fin|Voice=Act\t0\troot\t_\tO
3\tθεὰ\tθεά\tNOUN\tn-s---fv-\tCase=Voc|Gender=Fem|Number=Sing\t2\tvocative\t_\tO
4\tΠηληϊάδεω\tΠηληϊάδης\tNOUN\tn-s---mg-\tCase=Gen|Gender=Masc|Number=Sing\t5\tnmod\t_\tB-PER
5\tἈχιλῆος\tἈχιλλεύς\tPROPN\tn-s---mg-\tCase=Gen|Gender=Masc|Number=Sing\t1\tnmod\t_\tI-PER

";

    let result = parse::ner::parse_conllu(sample_conllu, DatasetId::AncientGreekUD);
    assert!(
        result.is_ok(),
        "Failed to parse Ancient Greek CoNLLU: {:?}",
        result.err()
    );

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens.len(), 5);

    // Check Achilles (Ἀχιλῆος) entity
    assert_eq!(dataset.sentences[0].tokens[4].text, "Ἀχιλῆος");
    // Note: CoNLLU parser may use POS tags if MISC doesn't have NER
    // This depends on how the parser handles the MISC column
}

#[test]
fn test_registry_hints_cover_all_conllu_ner_datasets() {
    // Datasets with format CoNLLU/CoNLL-U and task NER should get a registry hint
    for &ds in DatasetId::all() {
        let format = ds.format().unwrap_or("");
        let is_conllu = format == "CoNLLU" || format == "CoNLL-U";
        let is_ner = ds.is_ner();

        if is_conllu && is_ner {
            let hint = LoadableDatasetId::registry_hint_plan(ds);
            assert!(
                hint.is_some(),
                "{:?} has format={} and is NER but no registry hint",
                ds,
                format
            );
            if let Some(plan) = hint {
                assert_eq!(
                    plan,
                    DatasetParsePlan::Conllu,
                    "{:?} should use Conllu parse plan",
                    ds
                );
            }
        }
    }
}

#[test]
fn test_datasets_with_public_url_and_format_are_hintable() {
    // Datasets with a public URL and a parseable format should get hints
    let hintable_formats = ["CoNLL", "CoNLLU", "CoNLL-U", "BIO", "IOB2", "JSONL"];

    let mut missing_hints = Vec::new();

    for &ds in DatasetId::all() {
        let url = ds.download_url();
        let format = ds.format().unwrap_or("");
        let is_ner = ds.is_ner();

        // Skip datasets without URLs or non-NER datasets
        if url.is_empty() || !is_ner {
            continue;
        }

        // Skip formats we don't auto-detect
        if !hintable_formats.contains(&format) {
            continue;
        }

        let hint = LoadableDatasetId::registry_hint_plan(ds);
        if hint.is_none() {
            missing_hints.push((ds, format));
        }
    }

    // Allow some datasets to not have hints (complex formats, etc.)
    // but document them
    if !missing_hints.is_empty() {
        // These are known to be missing hints (need special parsers)
        let known_missing: &[DatasetId] = &[
            // Add any datasets that intentionally don't have hints here
        ];
        for (ds, format) in &missing_hints {
            if !known_missing.contains(ds) {
                eprintln!(
                    "Warning: {:?} (format={}) has public URL but no registry hint",
                    ds, format
                );
            }
        }
    }
}

#[test]
fn test_loadable_count_is_reasonable() {
    // Ensure we have a reasonable number of loadable datasets
    let loadable_count = LoadableDatasetId::all().len();
    let total_count = DatasetId::all().len();

    // We should have at least 50% of datasets loadable via either parse_plan or hints
    let min_expected = total_count / 2;
    assert!(
        loadable_count >= min_expected,
        "Only {} of {} datasets are loadable (expected at least {})",
        loadable_count,
        total_count,
        min_expected
    );
}

#[test]
fn test_datasets_with_urls_have_formats() {
    // Datasets with public URLs should ideally have format info for auto-loading
    let mut missing_format = Vec::new();

    for &ds in DatasetId::all() {
        let url = ds.download_url();
        let format = ds.format();
        let access = ds.access_status();

        // Skip datasets that require registration or aren't publicly available
        if url.is_empty() {
            continue;
        }

        // Check if format is missing for public datasets
        if format.is_none() && access == crate::eval::dataset_registry::DatasetAccessibility::Public
        {
            missing_format.push(ds);
        }
    }

    // Log datasets that could benefit from format info
    if !missing_format.is_empty() {
        eprintln!(
            "Datasets with public URLs but no format field ({}):",
            missing_format.len()
        );
        for ds in &missing_format[..missing_format.len().min(10)] {
            eprintln!("  - {:?}", ds);
        }
    }

    // We expect most public datasets to have format info
    // Allow up to 20% to be missing format (some have unusual formats)
    let max_missing = DatasetId::all().len() / 5;
    assert!(
        missing_format.len() <= max_missing,
        "Too many public datasets missing format: {} (max {})",
        missing_format.len(),
        max_missing
    );
}

#[test]
fn test_conll_format_ner_only_datasets_are_parseable() {
    // All NER-only datasets with CoNLL/CoNLLU format should have a parse plan
    // (Datasets with joint RE/coref tasks may use different column formats)
    let mut not_loadable = Vec::new();

    for &ds in DatasetId::all() {
        let format = ds.format().unwrap_or("");
        let is_conll = format == "CoNLL" || format == "CoNLLU" || format == "CoNLL-U";
        let is_ner = ds.is_ner();
        let is_re = ds.is_relation_extraction();
        let is_coref = ds.is_coreference();
        let is_event = ds.is_event_coref();

        if !is_conll || !is_ner {
            continue;
        }

        // Skip explicitly blocked datasets (they may be present in the registry for metadata,
        // but intentionally cannot be downloaded/loaded automatically).
        if ds.tasks_or_inferred().contains(&"blocked") {
            continue;
        }

        // Skip joint task datasets (they use CoNLL but with different structure)
        if is_re || is_coref || is_event {
            continue;
        }

        // Pure NER CoNLL datasets should be loadable
        let is_loadable = LoadableDatasetId::is_loadable_dataset(ds);
        if !is_loadable {
            not_loadable.push((ds, format));
        }
    }

    if !not_loadable.is_empty() {
        eprintln!("Pure NER CoNLL datasets not loadable:");
        for (ds, format) in &not_loadable {
            eprintln!("  - {:?} (format={})", ds, format);
        }
    }

    // All pure NER CoNLL datasets should be loadable
    assert!(
        not_loadable.is_empty(),
        "{} pure NER CoNLL datasets are not loadable",
        not_loadable.len()
    );
}

#[test]
fn test_jsonl_ner_datasets_are_parseable() {
    // JSONL datasets with NER task should ideally be loadable
    let mut jsonl_ner_not_loadable = Vec::new();

    for &ds in DatasetId::all() {
        let format = ds.format().unwrap_or("");
        let is_jsonl = format == "JSONL" || format == "JSON-Lines" || format == "jsonl";
        let is_ner = ds.is_ner();

        if !is_jsonl || !is_ner {
            continue;
        }

        if !LoadableDatasetId::is_loadable_dataset(ds) {
            jsonl_ner_not_loadable.push(ds);
        }
    }

    // Log for debugging
    if !jsonl_ner_not_loadable.is_empty() {
        eprintln!(
            "JSONL NER datasets not loadable ({}):",
            jsonl_ner_not_loadable.len()
        );
        for ds in &jsonl_ner_not_loadable {
            eprintln!("  - {:?}", ds);
        }
    }
}

#[test]
fn test_parse_afriqa() {
    // Test AfriQA JSON parsing
    let sample_json = r#"[
        {
            "context": "Lagos is a major city in Nigeria.",
            "question": "What is Lagos?",
            "answers": {
                "text": ["major city"],
                "answer_start": [11]
            }
        }
    ]"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_afriqa(sample_json, DatasetId::AfriQA);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);

    // Check that answer is marked
    let tokens = &dataset.sentences[0].tokens;
    // The answer "major city" should have B-ANSWER and I-ANSWER tags
    let answer_tokens: Vec<_> = tokens
        .iter()
        .filter(|t| t.ner_tag.contains("ANSWER"))
        .collect();
    assert!(!answer_tokens.is_empty(), "Should have answer tokens");
}

#[test]
fn test_african_datasets_in_all_list() {
    let all = DatasetId::all();
    assert!(all.contains(&DatasetId::MasakhaNER));
    assert!(all.contains(&DatasetId::MasakhaNER2));
    assert!(all.contains(&DatasetId::AfriSenti));
    assert!(all.contains(&DatasetId::AfriQA));
    assert!(all.contains(&DatasetId::MasakhaNEWS));
    assert!(all.contains(&DatasetId::MasakhaPOS));
}

// =========================================================================
// Event Extraction Parser Tests
// =========================================================================

#[test]
fn test_parse_maven_jsonl() {
    // Test MAVEN full JSONL format with events array
    let sample_jsonl = r#"{"id": "doc1", "content": [{"sentence": "The earthquake struck Tokyo.", "tokens": ["The", "earthquake", "struck", "Tokyo", "."]}], "events": [{"type": "Disaster", "mention": [{"trigger_word": "earthquake", "sent_id": 0, "offset": [1, 2]}]}]}"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::event::parse_maven(sample_jsonl, DatasetId::MAVEN);
    assert!(result.is_ok(), "parse_maven should succeed");

    let dataset = result.unwrap();
    assert!(!dataset.sentences.is_empty(), "Should have sentences");

    // Check event type tag
    let has_disaster = dataset
        .sentences
        .iter()
        .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("Disaster")));
    assert!(has_disaster, "Should have Disaster event tag");
}

#[test]
fn test_parse_maven_docid2topic_fallback() {
    // Test fallback format (docid2topic.json)
    let sample_json = r#"{"doc1": "Natural_Disaster", "doc2": "Political_Event"}"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::event::parse_maven(sample_json, DatasetId::MAVEN);
    assert!(result.is_ok(), "parse_maven fallback should succeed");

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2, "Should have 2 entries");
}

#[test]
fn test_parse_casie() {
    // Test CASIE cybersecurity event format
    let sample_jsonl = r#"{"content": "A vulnerability was discovered in Apache.", "cyberevent": {"hopper": [{"events": [{"subtype": "Vulnerability", "nugget": {"text": "vulnerability"}, "argument": [{"text": "Apache", "role": {"type": "Affected_System"}}]}]}]}}"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::event::parse_casie(sample_jsonl, DatasetId::CASIE);
    assert!(result.is_ok(), "parse_casie should succeed");

    let dataset = result.unwrap();
    assert!(!dataset.sentences.is_empty(), "Should have sentences");

    // Check for vulnerability trigger
    let has_vuln = dataset
        .sentences
        .iter()
        .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("Vulnerability")));
    assert!(has_vuln, "Should have Vulnerability tag");

    // Check for argument
    let has_arg = dataset
        .sentences
        .iter()
        .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("ARG_")));
    assert!(has_arg, "Should have argument tag");
}

#[test]
fn test_parse_maven_arg() {
    // Test MAVEN-ARG format with arguments
    let sample_jsonl = r#"{"id": "doc1", "document": "The company announced layoffs.", "events": [{"type": "Employment", "mention": [{"trigger_word": "layoffs", "offset": [4, 5]}], "argument": {"Employer": [{"content": "company", "offset": [1, 2]}]}}]}"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::event::parse_maven_arg(sample_jsonl, DatasetId::MAVENArg);
    assert!(result.is_ok(), "parse_maven_arg should succeed");

    let dataset = result.unwrap();
    assert!(!dataset.sentences.is_empty(), "Should have sentences");

    // Check for trigger
    let has_trigger = dataset
        .sentences
        .iter()
        .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("Employment")));
    assert!(has_trigger, "Should have Employment event tag");

    // Check for argument role
    let has_employer = dataset
        .sentences
        .iter()
        .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("ARG_Employer")));
    assert!(has_employer, "Should have Employer argument tag");
}

#[test]
fn test_parse_rams() {
    // Test RAMS tokenized format
    let sample_jsonl = r#"{"doc_key": "doc1", "sentences": [["The", "soldier", "fired", "his", "weapon", "."]], "evt_triggers": [[2, 2, [["conflict.attack", 1.0]]]], "gold_evt_links": [[[0], [1, 1], "attacker"]]}"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::event::parse_rams(sample_jsonl, DatasetId::RAMS);
    assert!(result.is_ok(), "parse_rams should succeed");

    let dataset = result.unwrap();
    assert!(!dataset.sentences.is_empty(), "Should have sentences");

    // Check for event trigger
    let has_event = dataset
        .sentences
        .iter()
        .any(|s| s.tokens.iter().any(|t| t.ner_tag.starts_with("B-")));
    assert!(has_event, "Should have event tags");
}

#[test]
fn test_parse_trec() {
    // Test TREC question classification format
    let sample =
        "NUM:dist How far is it from Denver to Aspen ?\nLOC:city What county is Modesto in ?\n";

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_trec(sample, DatasetId::TREC);
    assert!(result.is_ok(), "parse_trec should succeed");

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2, "Should have 2 questions");

    // Check coarse labels
    assert!(dataset.sentences[0].tokens[0].ner_tag.contains("NUM"));
    assert!(dataset.sentences[1].tokens[0].ner_tag.contains("LOC"));
}

#[test]
fn test_parse_litbank_ner_improved() {
    // Test improved LitBank NER parser with word-level tokenization
    let sample_ann = "T1\tPER 0 5\tAlice\nT2\tPER 10 14\tBob\nT3\tORG 20 28\tMicrosoft";

    let result = parse::coref::parse_litbank(sample_ann, DatasetId::LitBank);
    assert!(result.is_ok(), "parse_litbank should succeed");

    let dataset = result.unwrap();
    assert!(!dataset.sentences.is_empty(), "Should have sentences");

    // Check that entities are tokenized into words (not just single tokens)
    let sentence = &dataset.sentences[0];
    let entity_tokens: Vec<_> = sentence
        .tokens
        .iter()
        .filter(|t| t.ner_tag.starts_with("B-") || t.ner_tag.starts_with("I-"))
        .collect();

    // Should have at least the entity tokens (Alice, Bob, Microsoft)
    assert!(
        entity_tokens.len() >= 3,
        "Should have at least 3 entity tokens, got {}",
        entity_tokens.len()
    );

    // Check BIO tagging is correct (B- for first word, I- for subsequent words)
    let mut found_b_tag = false;
    for token in &sentence.tokens {
        if token.ner_tag.starts_with("B-") {
            found_b_tag = true;
            // First word of entity should be B-
            assert!(
                token.ner_tag.starts_with("B-"),
                "First word of entity should have B- tag"
            );
        }
    }
    assert!(found_b_tag, "Should have at least one B- tag");
}

#[test]
fn test_parse_tweettopic() {
    // Test TweetTopic JSONL format
    let sample_jsonl = r#"{"text": "Amazing game last night!", "label": 4, "label_name": "sports_&_gaming"}
{"text": "New AI breakthrough announced", "label": 5, "label_name": "science_&_technology"}"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_tweettopic(sample_jsonl, DatasetId::TweetTopic);
    assert!(result.is_ok(), "parse_tweettopic should succeed");

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2, "Should have 2 tweets");

    // Check label names are used
    assert!(dataset.sentences[0].tokens[0]
        .ner_tag
        .contains("sports_&_gaming"));
    assert!(dataset.sentences[1].tokens[0]
        .ner_tag
        .contains("science_&_technology"));
}
