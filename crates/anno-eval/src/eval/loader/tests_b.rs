use super::*;

#[test]
fn test_african_dataset_from_str() {
    // Test string parsing for African datasets
    assert_eq!(
        "masakhaner".parse::<DatasetId>().unwrap(),
        DatasetId::MasakhaNER
    );
    assert_eq!(
        "masakhaner2".parse::<DatasetId>().unwrap(),
        DatasetId::MasakhaNER2
    );
    assert_eq!(
        "afrisenti".parse::<DatasetId>().unwrap(),
        DatasetId::AfriSenti
    );
    assert_eq!("afriqa".parse::<DatasetId>().unwrap(), DatasetId::AfriQA);
    assert_eq!(
        "masakhanews".parse::<DatasetId>().unwrap(),
        DatasetId::MasakhaNEWS
    );
    assert_eq!(
        "masakhapos".parse::<DatasetId>().unwrap(),
        DatasetId::MasakhaPOS
    );

    // Alternative spellings
    assert_eq!(
        "masakhane-ner".parse::<DatasetId>().unwrap(),
        DatasetId::MasakhaNER
    );
    assert_eq!(
        "afri-senti".parse::<DatasetId>().unwrap(),
        DatasetId::AfriSenti
    );
    assert_eq!(
        "masakhane-news".parse::<DatasetId>().unwrap(),
        DatasetId::MasakhaNEWS
    );
}

#[test]
fn test_afrisenti_parse_with_tonal_diacritics() {
    // Test Yoruba text with tonal diacritics (common in AfriSenti)
    let yoruba_tsv = "Ó dára púpọ̀!\tpositive\n\
                      Kò dára rárá\tnegative\n\
                      Ẹ ṣé, mo dupẹ́\tpositive";

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_afrisenti(yoruba_tsv, DatasetId::AfriSenti);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 3);

    // Verify the text is preserved correctly with diacritics
    assert!(dataset.sentences[0].tokens[0].text.contains("dára"));
    assert!(dataset.sentences[1].tokens[0].text.contains("rárá"));
    assert!(dataset.sentences[2].tokens[0].text.contains("dupẹ́"));
}

#[test]
fn test_masakhaner_parse_with_ethiopic_script() {
    // Test Amharic text in MasakhaNER CoNLL format (Ethiopic script)
    let amharic_conll = "ዶክተር B-PER\n\
                         አቢይ I-PER\n\
                         አህመድ I-PER\n\
                         ኢትዮጵያ B-LOC\n\
                         ውስጥ O\n\
                         ተወለዱ O\n";

    let result = parse::ner::parse_conll(amharic_conll, DatasetId::MasakhaNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);

    let tokens = &dataset.sentences[0].tokens;
    assert_eq!(tokens.len(), 6);

    // Verify Ethiopic script is preserved
    assert_eq!(tokens[0].text, "ዶክተር");
    assert_eq!(tokens[0].ner_tag, "B-PER");
    assert_eq!(tokens[3].text, "ኢትዮጵያ");
    assert_eq!(tokens[3].ner_tag, "B-LOC");
}

#[test]
fn test_conllu_parse_with_nguni_clicks() {
    // Test isiXhosa/isiZulu text with click consonants (MasakhaPOS)
    let xhosa_conllu = "# sent_id = xho_test_1\n\
                       # text = UMongameli uCyril Ramaphosa\n\
                       1\tUMongameli\tumongameli\tNOUN\tN\t_\t0\troot\t_\t_\n\
                       2\tuCyril\tuCyril\tPROPN\tNNP\t_\t1\tappos\t_\t_\n\
                       3\tRamaphosa\tRamaphosa\tPROPN\tNNP\t_\t2\tflat:name\t_\t_\n\
                       \n\
                       # sent_id = xho_test_2\n\
                       # text = Ndiqala ukuthetha isiXhosa\n\
                       1\tNdiqala\tqala\tVERB\tV\t_\t0\troot\t_\t_\n\
                       2\tukuthetha\tthetha\tVERB\tV\t_\t1\txcomp\t_\t_\n\
                       3\tisiXhosa\tisiXhosa\tNOUN\tN\t_\t2\tobj\t_\t_\n";

    let result = parse::ner::parse_conllu(xhosa_conllu, DatasetId::MasakhaPOS);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2);

    // Verify text with click-like consonants is preserved
    assert_eq!(dataset.sentences[0].tokens[0].text, "UMongameli");
    assert_eq!(dataset.sentences[1].tokens[2].text, "isiXhosa");
}

#[test]
fn test_masakhanews_parse_with_arabic_variants() {
    // MasakhaNEWS includes Algerian/Moroccan Arabic variants
    let news_tsv = "headline\tbody\tcategory\n\
                    الأخبار العاجلة\tتفاصيل الخبر...\tpolitics\n\
                    رياضة محلية\tمباراة اليوم...\tsports";

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_masakhanews(news_tsv, DatasetId::MasakhaNEWS);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    // Header skipped, 2 data rows
    assert_eq!(dataset.sentences.len(), 2);

    // Check Arabic text is preserved
    assert!(dataset.sentences[0].tokens[0].text.contains("الأخبار"));
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-politics");
    assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-sports");
}

#[test]
fn test_afriqa_multilingual_qa() {
    // AfriQA has questions in target language, context may be in English
    let qa_json = r#"[
        {
            "context": "Yorùbá is a tonal language spoken in Nigeria.",
            "question": "Kí ni Yorùbá?",
            "answers": {
                "text": ["tonal language"],
                "answer_start": [13]
            },
            "language": "yo"
        }
    ]"#;

    let loader = DatasetLoader::new().unwrap();
    let result = parse::classification::parse_afriqa(qa_json, DatasetId::AfriQA);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
}

// NOTE: We intentionally do not embed language-code→URL expansion helpers here.
// If we add them, they should live in the registry (metadata) or a dedicated dataset
// URL builder module, not in the loader tests.

// =========================================================================
// Comprehensive Parser Smoke Tests (one per DatasetParsePlan)
// =========================================================================

#[test]
fn test_parse_content_rejects_empty_for_all_loadable_datasets() {
    // Global invariant: no dataset should parse from empty content.
    for loadable in LoadableDatasetId::all() {
        let id: DatasetId = loadable.into();
        let err = parse::parse_content("   \n\t", id)
            .expect_err("empty content must error");
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("empty"),
            "Expected an 'empty' error message for {:?}, got: {}",
            id,
            msg
        );
    }
}

#[test]
fn test_parse_docred_smoke() {
    let sample = r#"{"doc_key":"d1","sentence":["John","met","Mary","in","Paris","."],"ner":[[0,0,"PER"],[2,2,"PER"],[4,4,"LOC"]],"relations":[]}"#;
    let loader = DatasetLoader::new().unwrap();
    let ds = parse::relation::parse_docred(sample, DatasetId::DocRED).unwrap();
    assert!(!ds.sentences.is_empty());
    assert!(ds.sentences[0]
        .tokens
        .iter()
        .any(|t| t.ner_tag.starts_with("B-")));
}

#[test]
fn test_parse_cadec_jsonl_smoke() {
    // Character offsets in `entities` are interpreted over reconstructed text from tokens.
    // "I took aspirin" → aspirin starts at char 7, ends at 14 (exclusive).
    let sample = r#"{"tokens":["I","took","aspirin"],"entities":[{"text":"aspirin","label":"DRUG","start":7,"end":14}]}"#;
    let ds = parse::ner::parse_cadec_jsonl(sample, DatasetId::CADEC).unwrap();
    assert!(!ds.sentences.is_empty());
    assert!(ds.sentences[0]
        .tokens
        .iter()
        .any(|t| t.ner_tag == "B-DRUG" || t.ner_tag == "I-DRUG"));
}

#[test]
fn test_parse_cadec_hf_api_smoke() {
    let sample = r#"{"rows":[{"row":{"text":"I took aspirin","ade":"aspirin"}}]}"#;
    let ds = parse::ner::parse_cadec_hf_api(sample, DatasetId::CADEC).unwrap();
    assert!(!ds.sentences.is_empty());
    assert!(ds.sentences[0]
        .tokens
        .iter()
        .any(|t| t.ner_tag.contains("adverse_drug_event")));
}

#[test]
fn test_parse_bc5cdr_smoke() {
    let sample = "Aspirin\tNN\tO\tB-CHEMICAL\nhelps\tVBZ\tO\tO\n\n";
    let ds = parse::ner::parse_bc5cdr(sample, DatasetId::BC5CDR).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-CHEMICAL");
}

#[test]
fn test_parse_ncbi_disease_smoke() {
    let sample = "Cancer\tNN\tO\tB-Disease\nprogresses\tVBZ\tO\tO\n\n";
    let ds = parse::ner::parse_ncbi_disease(sample, DatasetId::NCBIDisease).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
}

#[test]
fn test_parse_gap_smoke() {
    let sample =
        "ID\tText\tPronoun\tPronoun-offset\tA\tA-offset\tA-coref\tB\tB-offset\tB-coref\tURL\n\
g1\tJohn met Mary. He waved.\tHe\t14\tJohn\t0\tTRUE\tMary\t9\tFALSE\thttp://example\n";
    let ds = parse::coref::parse_gap(sample, DatasetId::GAP).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert!(!ds.sentences[0].tokens.is_empty());
}

#[test]
fn test_parse_preco_jsonl_smoke() {
    let sample = r#"{"sentences":[["John","went","home","."],["He","slept","."]]}"#;
    let ds = parse::coref::parse_preco_jsonl(sample, DatasetId::PreCo).unwrap();
    assert_eq!(ds.sentences.len(), 2);
    assert_eq!(ds.sentences[0].tokens[0].text, "John");
}

#[test]
fn test_parse_wikiann_json_array_smoke() {
    let sample =
        r#"[{"tokens":["John","went","to","Paris"],"ner_tags":["B-PER","O","O","B-LOC"]}]"#;
    let ds = parse::ner::parse_wikiann_json(sample, DatasetId::UNER).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-PER");
}

#[test]
fn test_parse_hf_api_response_smoke() {
    let sample = r#"{
  "features":[{"name":"tokens"},{"name":"ner_tags","type":{"feature":{"names":["O","B-PER","I-PER"]}}}],
  "rows":[{"row_idx":0,"row":{"tokens":["John"],"ner_tags":[1]}}]
}"#;
    let ds = parse::ner::parse_hf_api_response(sample, DatasetId::UniversalNER).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-PER");
}

#[test]
fn test_parse_hf_api_response_temporal_standoff_smoke() {
    let sample = r#"{
  "features":[{"name":"text"},{"name":"time_expressions"}],
  "rows":[{"row_idx":0,"row":{
"text":"A 10/30/89 .",
"time_expressions":[{"text":"10/30/89","start_char":2,"end_char":10,"tid":"t1","type":"DATE","value":"1989-10-30"}],
"event_expressions":[],
"signal_expressions":[]
  }}]
}"#;
    let ds = parse::ner::parse_hf_api_response(sample, DatasetId::TimexRecognitionSentenceOriginal).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert_eq!(ds.sentences[0].tokens.len(), 3);
    assert_eq!(ds.sentences[0].tokens[0].text, "A");
    assert_eq!(ds.sentences[0].tokens[0].ner_tag, "O");
    assert_eq!(ds.sentences[0].tokens[1].text, "10/30/89");
    assert_eq!(ds.sentences[0].tokens[1].ner_tag, "B-TIMEX");
    assert_eq!(ds.sentences[0].tokens[2].text, ".");
    assert_eq!(ds.sentences[0].tokens[2].ner_tag, "O");
}

#[test]
fn test_parse_hf_api_response_pairwise_discourse_smoke() {
    let sample = r#"{
  "features":[{"name":"unit1_txt"},{"name":"unit2_txt"},{"name":"label"}],
  "rows":[{"row_idx":0,"row":{
"unit1_txt":"Because it rained",
"unit2_txt":"the game was canceled",
"label":"Cause"
  }}]
}"#;
    let ds = parse::ner::parse_hf_api_response(sample, DatasetId::DisrptEngDepScidtbRels).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert_eq!(ds.sentences[0].tokens.len(), 1);
    assert_eq!(
        ds.sentences[0].tokens[0].text,
        "Because it rained [SEP] the game was canceled"
    );
    assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-Cause");
}

#[test]
fn test_parse_hf_api_response_disrpt_conllu_seg_smoke() {
    let sample = r#"{
  "features":[{"name":"form"},{"name":"misc"}],
  "rows":[{"row_idx":0,"row":{
"form":["We","propose","a","method","."],
"misc":["Seg=B-seg","Seg=O","Seg=O","Seg=B-seg","Seg=O"]
  }}]
}"#;
    let ds = parse::ner::parse_hf_api_response(sample, DatasetId::DisrptEngDepScidtbConlluSeg).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    let tags: Vec<&str> = ds.sentences[0]
        .tokens
        .iter()
        .map(|t| t.ner_tag.as_str())
        .collect();
    assert_eq!(tags, vec!["B-SEG", "I-SEG", "I-SEG", "B-SEG", "I-SEG"]);
}

#[test]
fn test_parse_agnews_smoke() {
    let sample = r#"{"text":"Stocks rally on earnings","label":2}"#;
    let loader = DatasetLoader::new().unwrap();
    let ds = parse::classification::parse_agnews(sample, DatasetId::AGNews).unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
}

#[test]
fn test_parse_dbpedia14_smoke() {
    let sample = r#"{"content":"The Beatles released Abbey Road","label":5}"#;
    let ds = parse::classification::parse_dbpedia14(sample, DatasetId::DBPedia14)
        .unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
}

#[test]
fn test_parse_yahoo_answers_smoke() {
    let sample = r#"{"question_title":"Why is the sky blue?","topic":1}"#;
    let ds = parse::classification::parse_yahoo_answers(sample, DatasetId::YahooAnswers)
        .unwrap();
    assert_eq!(ds.sentences.len(), 1);
    assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
}

// =========================================================================
// Dataset Registry Integration Tests
// =========================================================================

#[test]
fn test_sec_filings_has_raw_url() {
    // Verify SEC-filings dataset has a raw GitHub URL for direct download
    let url = DatasetId::SECFilingsNER.download_url();
    assert!(
        url.contains("raw.githubusercontent.com"),
        "SEC-filings should have raw GitHub URL, got: {}",
        url
    );
    assert!(
        url.ends_with(".txt"),
        "SEC-filings should point to a .txt file, got: {}",
        url
    );
}

#[test]
fn test_twiconv_has_format() {
    // Verify TwiConv dataset has format field
    let format = DatasetId::TwiConv.format();
    assert!(format.is_some(), "TwiConv should have format field");
    // TwiConv uses CoNLL format for coreference data
    assert_eq!(format.unwrap(), "CoNLL", "TwiConv should be CoNLL format");
}

#[test]
fn test_mudoco_has_format() {
    // Verify MuDoCo dataset has format field
    let format = DatasetId::MuDoCo.format();
    assert!(format.is_some(), "MuDoCo should have format field");
    assert_eq!(format.unwrap(), "JSON", "MuDoCo should be JSON format");
}

#[test]
fn test_all_public_ud_datasets_have_conllu_format() {
    // All Universal Dependencies datasets should have CoNLLU format
    let ud_datasets = vec![
        DatasetId::AncientGreekUD,
        DatasetId::LatinUD,
        DatasetId::SanskritUD,
        DatasetId::OldEnglishUD,
        DatasetId::UDEsperantoCairo,
    ];

    for ds in ud_datasets {
        let format = ds.format();
        assert!(format.is_some(), "{:?} should have format field", ds);
        assert_eq!(
            format.unwrap(),
            "CoNLLU",
            "{:?} should be CoNLLU format",
            ds
        );
    }
}

#[test]
fn test_datasets_with_public_urls_are_accessible() {
    // Verify that key public datasets have valid URLs (format check only)
    let test_cases = vec![
        (DatasetId::AncientGreekUD, "universaldependencies"),
        (DatasetId::LatinUD, "universaldependencies"),
        (DatasetId::SECFilingsNER, "entity-recognition-datasets"),
        (DatasetId::TwiConv, "twiconv"), // lowercase for case-insensitive match
    ];

    for (ds, expected_substring) in test_cases {
        let url = ds.download_url();
        assert!(!url.is_empty(), "{:?} should have a download URL", ds);
        assert!(
            url.to_lowercase()
                .contains(&expected_substring.to_lowercase()),
            "{:?} URL should contain '{}', got: {}",
            ds,
            expected_substring,
            url
        );
    }
}

#[test]
fn test_loadable_datasets_count_is_stable() {
    // Track the number of loadable datasets to detect regressions
    // This should only increase as we add more loaders
    let loadable = LoadableDatasetId::all();
    let count = loadable.len();

    // As of 2025-12-15, after recent fixes we have 295 loadable datasets
    assert!(
        count >= 295,
        "Expected at least 295 loadable datasets, got {}. \
         This may indicate a regression in the loading system.",
        count
    );
}

#[test]
fn test_conll_format_variants_all_detected() {
    // Ensure CoNLL format variants for pure NER datasets are properly detected
    // We exclude RE/coref datasets as they have special parsing needs
    for &ds in DatasetId::all() {
        let format = ds.format();
        if let Some(fmt) = format {
            let is_conll_variant =
                fmt == "CoNLL" || fmt == "CoNLLU" || fmt == "CoNLL-U" || fmt == "CoNLL03";

            // Only check pure NER datasets (no coref, no RE)
            let is_pure_ner = ds.supports_ner() && !ds.supports_coref() && !ds.supports_re();

            if is_conll_variant && is_pure_ner {
                // Should be loadable via hint system
                let hint = LoadableDatasetId::registry_hint_plan(ds);
                assert!(
                    hint.is_some() || LoadableDatasetId::is_loadable_dataset(ds),
                    "{:?} with format {} and pure NER task should be loadable",
                    ds,
                    fmt
                );
            }
        }
    }
}

#[test]
fn test_parse_csv_ner_smoke() {
    // Test CSV NER format (E-NER/EDGAR-NER style: Token,Tag)
    let sample = "\
-DOCSTART-,O
,O
Check,O
the,O
appropriate,O
box,O
,O
Nuveen,I-BUSINESS
New,I-BUSINESS
York,I-BUSINESS
Fund,I-BUSINESS
,O

The,O
SEC,I-GOVERNMENT
filed,O
charges,O
.

John,I-PERSON
Smith,I-PERSON
is,O
the,O
CEO,O
.
";
    let ds = parse::ner::parse_csv_ner(sample, DatasetId::ENer).unwrap();

    // Should have 3 sentences (separated by empty lines and -DOCSTART-)
    assert_eq!(
        ds.sentences.len(),
        3,
        "Expected 3 sentences, got {:?}",
        ds.sentences.len()
    );

    // First sentence should have BUSINESS entities
    let first_sentence = &ds.sentences[0];
    assert!(
        first_sentence
            .tokens
            .iter()
            .any(|t| t.ner_tag == "I-BUSINESS"),
        "First sentence should contain I-BUSINESS tags"
    );

    // Second sentence should have GOVERNMENT entity
    let second_sentence = &ds.sentences[1];
    assert!(
        second_sentence
            .tokens
            .iter()
            .any(|t| t.ner_tag == "I-GOVERNMENT"),
        "Second sentence should contain I-GOVERNMENT tag"
    );

    // Third sentence should have PERSON entities
    let third_sentence = &ds.sentences[2];
    assert!(
        third_sentence
            .tokens
            .iter()
            .any(|t| t.ner_tag == "I-PERSON"),
        "Third sentence should contain I-PERSON tags"
    );

    // Check specific token/tag pairs
    let nuveen_token = first_sentence.tokens.iter().find(|t| t.text == "Nuveen");
    assert!(nuveen_token.is_some(), "Should have Nuveen token");
    assert_eq!(nuveen_token.unwrap().ner_tag, "I-BUSINESS");

    let john_token = third_sentence.tokens.iter().find(|t| t.text == "John");
    assert!(john_token.is_some(), "Should have John token");
    assert_eq!(john_token.unwrap().ner_tag, "I-PERSON");
}

#[test]
fn test_csv_ner_format_is_detected() {
    // Ensure CSV format datasets with NER tasks are properly detected as loadable
    let ener_hint = LoadableDatasetId::registry_hint_plan(DatasetId::ENer);
    assert_eq!(
        ener_hint,
        Some(DatasetParsePlan::CsvNer),
        "ENer should use CsvNer parse plan"
    );

    // Verify ENer is loadable
    assert!(
        LoadableDatasetId::is_loadable_dataset(DatasetId::ENer),
        "ENer should be loadable"
    );
}

// =========================================================================
// Tests for newly added dataset loaders (2025-12)
// =========================================================================

#[test]
fn test_newly_added_conll_datasets_are_loadable() {
    let new_conll = [
        DatasetId::QxoRef,
        DatasetId::GICoref,
        DatasetId::WNUT16,
        DatasetId::NoiseBench,
        DatasetId::CrossWeigh,
        DatasetId::ZELDA,
        DatasetId::GENIANested,
    ];

    for id in new_conll {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable with Conll parse plan",
            id
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(id),
            Some(DatasetParsePlan::Conll),
            "{:?} should use Conll plan",
            id
        );
    }
}

#[test]
fn test_newly_added_jsonl_datasets_are_loadable() {
    let new_jsonl = [
        DatasetId::REBEL,
        DatasetId::BBQ,
        DatasetId::RealToxicityPrompts,
        DatasetId::BookCoref,
        DatasetId::BookCorefSplit,
        DatasetId::WIESP2022NER,
        DatasetId::FewRel,
        DatasetId::PIIMasking200k,
        DatasetId::B2NERD,
        DatasetId::OpenNER,
        DatasetId::FictionNER750M,
    ];

    for id in new_jsonl {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable with JsonlNer parse plan",
            id
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(id),
            Some(DatasetParsePlan::JsonlNer),
            "{:?} should use JsonlNer plan",
            id
        );
    }
}

#[test]
fn test_genia_nested_conll_parse() {
    // GENIA nested NER uses multi-layered BIO tags
    let nested_conll = "IL-2\tB-protein\n\
                        gene\tI-protein\n\
                        expression\tO\n\
                        \n\
                        T\tB-cell_type\n\
                        cells\tI-cell_type\n";

    let result = parse::ner::parse_conll(nested_conll, DatasetId::GENIANested);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2);
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-protein");
    assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-cell_type");
}

#[test]
fn test_gicoref_gender_inclusive_parse() {
    // GICoref uses neopronouns and singular they
    let gicoref_conll = "Alex\tB-PER\n\
                         uses\tO\n\
                         they\tB-PER\n\
                         pronouns\tO\n\
                         \n\
                         Jordan\tB-PER\n\
                         introduced\tO\n\
                         themself\tB-PER\n";

    let result = parse::ner::parse_conll(gicoref_conll, DatasetId::GICoref);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2);

    // Verify pronoun tokens are correctly tagged
    assert_eq!(dataset.sentences[0].tokens[2].text, "they");
    assert_eq!(dataset.sentences[0].tokens[2].ner_tag, "B-PER");
    assert_eq!(dataset.sentences[1].tokens[2].text, "themself");
    assert_eq!(dataset.sentences[1].tokens[2].ner_tag, "B-PER");
}

#[test]
fn test_fewrel_jsonl_parse() {
    // FewRel has relation extraction in JSONL format
    // The parser expects integer tags mapping to MultiNERD labels:
    // 0=O, 1=B-PER, 3=B-ORG
    let fewrel_sample =
        r#"{"tokens":["John","works","at","Google","."],"ner_tags":[1,0,0,3,0]}"#;

    let result = parse::ner::parse_jsonl_ner(fewrel_sample, DatasetId::FewRel);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens.len(), 5);
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-PER");
    assert_eq!(dataset.sentences[0].tokens[3].ner_tag, "B-ORG");
}

#[test]
fn test_b2nerd_business_entities_parse() {
    // B2NERD focuses on business/financial entity types
    // Using standard MultiNERD indices: 3=B-ORG (closest to COMPANY), 0=O
    let b2nerd_sample =
        r#"{"tokens":["Apple","Inc",".","reports","Q4","earnings"],"ner_tags":[3,4,4,0,0,0]}"#;

    let result = parse::ner::parse_jsonl_ner(b2nerd_sample, DatasetId::B2NERD);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-ORG");
    assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "I-ORG");
}

#[test]
fn test_ud_classical_languages_are_loadable() {
    // Universal Dependencies treebanks for classical/ancient languages
    let ud_datasets = [
        DatasetId::AncientGreekUD,
        DatasetId::LatinUD,
        DatasetId::SanskritUD,
        DatasetId::OldEnglishUD,
        DatasetId::OldNorseUD,
        DatasetId::UDEsperantoCairo,
    ];

    for id in ud_datasets {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable with Conllu parse plan",
            id
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(id),
            Some(DatasetParsePlan::Conllu),
            "{:?} should use Conllu plan",
            id
        );
    }
}

#[test]
fn test_hipe2022_tsv_is_loadable() {
    assert!(
        LoadableDatasetId::is_loadable_dataset(DatasetId::HIPE2022),
        "HIPE2022 should be loadable"
    );
    assert_eq!(
        LoadableDatasetId::parse_plan(DatasetId::HIPE2022),
        Some(DatasetParsePlan::TsvNer),
        "HIPE2022 should use TsvNer plan"
    );
}

#[test]
fn test_ener_csv_is_loadable() {
    assert!(
        LoadableDatasetId::is_loadable_dataset(DatasetId::ENer),
        "ENer should be loadable"
    );
    assert_eq!(
        LoadableDatasetId::parse_plan(DatasetId::ENer),
        Some(DatasetParsePlan::CsvNer),
        "ENer should use CsvNer plan"
    );
}

#[test]
fn test_loadable_count_increased() {
    // Regression test: ensure we have at least 295 loadable datasets
    // (Updated 2025-12 after adding CoNLL/JSONL/CoNLLU batches)
    let loadable_count = LoadableDatasetId::all().len();
    assert!(
        loadable_count >= 295,
        "Expected at least 295 loadable datasets, got {}",
        loadable_count
    );
}

// =========================================================================
// Domain-Specific Parser Tests
// =========================================================================

#[test]
fn test_biomedical_conll_with_chemical_entities() {
    // CHEMDNER-style biomedical NER with chemical entity types
    let chemdner_conll = "Aspirin\tB-CHEMICAL\n\
                          inhibits\tO\n\
                          COX-2\tB-GENE\n\
                          expression\tO\n\
                          \n\
                          Metformin\tB-CHEMICAL\n\
                          treats\tO\n\
                          diabetes\tB-DISEASE\n";

    let result = parse::ner::parse_conll(chemdner_conll, DatasetId::CHEMDNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2);
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-CHEMICAL");
    assert_eq!(dataset.sentences[0].tokens[2].ner_tag, "B-GENE");
    assert_eq!(dataset.sentences[1].tokens[2].ner_tag, "B-DISEASE");
}

#[test]
fn test_historical_ner_with_archaic_spelling() {
    // Historical NER should handle archaic spellings and diacritics
    let historical_conll = "Præsident\tB-PER\n\
                             Washington\tI-PER\n\
                             addresseth\tO\n\
                             ye\tO\n\
                             Congreſs\tB-ORG\n";

    let result = parse::ner::parse_conll(historical_conll, DatasetId::EighteenthCenturyNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    // Verify archaic characters are preserved
    assert!(dataset.sentences[0].tokens[0].text.contains('æ'));
    assert!(dataset.sentences[0].tokens[4].text.contains('ſ'));
}

#[test]
fn test_multilingual_code_switching_ner() {
    // LinCE/CALCS datasets have code-switched text (e.g., Spanish-English)
    let codeswitched_conll = "My\tO\n\
                               abuela\tB-PER\n\
                               lives\tO\n\
                               in\tO\n\
                               Ciudad\tB-LOC\n\
                               de\tI-LOC\n\
                               México\tI-LOC\n";

    let result = parse::ner::parse_conll(codeswitched_conll, DatasetId::LinCE);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens[1].text, "abuela");
    assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "B-PER");
    // Multi-word location
    assert_eq!(dataset.sentences[0].tokens[4].ner_tag, "B-LOC");
    assert_eq!(dataset.sentences[0].tokens[6].ner_tag, "I-LOC");
}

#[test]
fn test_indigenous_language_ner() {
    // Test parsing of indigenous language NER (Guarani/Shipibo-Konibo)
    let guarani_conll = "Paraguái\tB-LOC\n\
                          ha\tO\n\
                          yvypora\tO\n\
                          oiko\tO\n\
                          Asunción\tB-LOC\n\
                          pe\tO\n";

    let result = parse::ner::parse_conll(guarani_conll, DatasetId::GuaraniNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens[0].text, "Paraguái");
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-LOC");
}

#[test]
fn test_ancient_greek_conllu_with_polytonic() {
    // Ancient Greek with polytonic diacritics
    let greek_conllu = "# sent_id = grc_test_1\n\
                        # text = ἐπιστήμη καὶ δικαιοσύνη\n\
                        1\tἐπιστήμη\tἐπιστήμη\tNOUN\tN\tCase=Nom|Gender=Fem|Number=Sing\t0\troot\t_\tSpaceAfter=Yes\n\
                        2\tκαὶ\tκαί\tCCONJ\tC\t_\t3\tcc\t_\tSpaceAfter=Yes\n\
                        3\tδικαιοσύνη\tδικαιοσύνη\tNOUN\tN\tCase=Nom|Gender=Fem|Number=Sing\t1\tconj\t_\tSpaceAfter=No\n";

    let result = parse::ner::parse_conllu(greek_conllu, DatasetId::AncientGreekUD);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    // Verify polytonic Greek is preserved
    assert_eq!(dataset.sentences[0].tokens[0].text, "ἐπιστήμη");
    assert_eq!(dataset.sentences[0].tokens[2].text, "δικαιοσύνη");
}

#[test]
fn test_latin_conllu_with_macrons() {
    // Latin with optional macrons for vowel length
    let latin_conllu = "# sent_id = lat_test_1\n\
                        # text = Rōma āterna est\n\
                        1\tRōma\tRoma\tPROPN\tNNP\tCase=Nom|Gender=Fem|Number=Sing\t3\tnsubj\t_\tSpaceAfter=Yes\n\
                        2\tāterna\taeternus\tADJ\tA\tCase=Nom|Gender=Fem|Number=Sing\t1\tamod\t_\tSpaceAfter=Yes\n\
                        3\test\tsum\tAUX\tV\tMood=Ind|Number=Sing|Person=3|Tense=Pres|VerbForm=Fin|Voice=Act\t0\troot\t_\tSpaceAfter=No\n";

    let result = parse::ner::parse_conllu(latin_conllu, DatasetId::LatinUD);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    // Verify macrons are preserved
    assert!(dataset.sentences[0].tokens[0].text.contains('ō'));
    assert!(dataset.sentences[0].tokens[1].text.contains('ā'));
}

#[test]
fn test_sanskrit_conllu_with_devanagari() {
    // Sanskrit in Devanagari script
    let sanskrit_conllu = "# sent_id = sa_test_1\n\
                           # text = रामः सीतां पश्यति\n\
                           1\tरामः\tराम\tNOUN\tN\tCase=Nom|Gender=Masc|Number=Sing\t3\tnsubj\t_\tSpaceAfter=Yes\n\
                           2\tसीतां\tसीता\tPROPN\tNNP\tCase=Acc|Gender=Fem|Number=Sing\t3\tobj\t_\tSpaceAfter=Yes\n\
                           3\tपश्यति\tदृश्\tVERB\tV\tMood=Ind|Number=Sing|Person=3|Tense=Pres|VerbForm=Fin|Voice=Act\t0\troot\t_\tSpaceAfter=No\n";

    let result = parse::ner::parse_conllu(sanskrit_conllu, DatasetId::SanskritUD);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens.len(), 3);
    // Verify Devanagari is preserved
    assert_eq!(dataset.sentences[0].tokens[0].text, "रामः");
    assert_eq!(dataset.sentences[0].tokens[1].text, "सीतां");
}

#[test]
fn test_klingon_conllu_is_loadable() {
    // Klingon (tlh) is a constructed language in TaggedPBCKlingon
    assert!(
        LoadableDatasetId::is_loadable_dataset(DatasetId::TaggedPBCKlingon),
        "Klingon dataset should be loadable"
    );
    assert_eq!(
        LoadableDatasetId::parse_plan(DatasetId::TaggedPBCKlingon),
        Some(DatasetParsePlan::Conllu),
        "Klingon should use Conllu plan"
    );
}

#[test]
fn test_financial_ner_entities() {
    // FinanceNER with financial entity types
    let finance_conll = "Tesla\tB-COMPANY\n\
                         stock\tO\n\
                         rose\tO\n\
                         5%\tB-PERCENTAGE\n\
                         after\tO\n\
                         Q4\tB-PERIOD\n\
                         earnings\tO\n";

    let result = parse::ner::parse_conll(finance_conll, DatasetId::FinanceNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-COMPANY");
    assert_eq!(dataset.sentences[0].tokens[3].ner_tag, "B-PERCENTAGE");
}

#[test]
fn test_recipe_ner_food_entities() {
    // RecipeNER with culinary entity types
    let recipe_conll = "Add\tO\n\
                        2\tB-QUANTITY\n\
                        cups\tI-QUANTITY\n\
                        of\tO\n\
                        flour\tB-INGREDIENT\n\
                        and\tO\n\
                        1\tB-QUANTITY\n\
                        tsp\tI-QUANTITY\n\
                        salt\tB-INGREDIENT\n";

    let result = parse::ner::parse_conll(recipe_conll, DatasetId::RecipeNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens[4].ner_tag, "B-INGREDIENT");
    assert_eq!(dataset.sentences[0].tokens[8].ner_tag, "B-INGREDIENT");
}

#[test]
fn test_astronomy_ner_entities() {
    // AstroNER with astronomical entity types
    let astro_conll = "The\tO\n\
                       Andromeda\tB-GALAXY\n\
                       Galaxy\tI-GALAXY\n\
                       is\tO\n\
                       2.5\tB-DISTANCE\n\
                       million\tI-DISTANCE\n\
                       light-years\tI-DISTANCE\n\
                       away\tO\n";

    let result = parse::ner::parse_conll(astro_conll, DatasetId::AstroNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "B-GALAXY");
    assert_eq!(dataset.sentences[0].tokens[4].ner_tag, "B-DISTANCE");
}

#[test]
fn test_nested_ner_datasets_are_loadable() {
    // Nested NER datasets (entities within entities)
    let nested_datasets = [DatasetId::GENIANested, DatasetId::ChineseNestedNER];

    for id in nested_datasets {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable",
            id
        );
    }
}

#[test]
fn test_discontinuous_ner_datasets_are_loadable() {
    // Discontinuous NER datasets (non-contiguous entity spans)
    let discontinuous_datasets = [
        DatasetId::GermEvalDiscontinuous,
        DatasetId::PubMedDiscontinuous,
    ];

    for id in discontinuous_datasets {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable",
            id
        );
    }
}

#[test]
fn test_social_media_ner_datasets_are_loadable() {
    // Social media NER datasets (noisy text, hashtags, mentions)
    let social_datasets = [
        DatasetId::WNUT16,
        DatasetId::TwiConv,
        DatasetId::NERsocialFood,
    ];

    for id in social_datasets {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable",
            id
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(id),
            Some(DatasetParsePlan::Conll),
            "{:?} should use Conll plan",
            id
        );
    }
}

#[test]
fn test_literary_ner_datasets_are_loadable() {
    // Literary NER datasets (fiction, novels)
    let literary_datasets = [
        DatasetId::CharacterCodex,
        DatasetId::FictionNER750M,
        DatasetId::BookCoref,
    ];

    for id in literary_datasets {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable",
            id
        );
    }
}

#[test]
fn test_jsonl_ner_with_empty_tokens_handled() {
    // Edge case: JSONL with some empty tokens
    let jsonl_with_empty = r#"{"tokens":["Hello","","world"],"ner_tags":[0,0,0]}"#;

    let result = parse::ner::parse_jsonl_ner(jsonl_with_empty, DatasetId::MultiWOZNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    // Empty token should still be preserved in parsing
    assert_eq!(dataset.sentences[0].tokens.len(), 3);
}

#[test]
fn test_conll_with_long_entity_spans() {
    // Test parsing of very long entity spans (e.g., legal document titles)
    let long_span_conll = "The\tB-DOCUMENT\n\
                           United\tI-DOCUMENT\n\
                           States\tI-DOCUMENT\n\
                           Constitution\tI-DOCUMENT\n\
                           Article\tI-DOCUMENT\n\
                           I\tI-DOCUMENT\n\
                           Section\tI-DOCUMENT\n\
                           8\tI-DOCUMENT\n\
                           Clause\tI-DOCUMENT\n\
                           3\tI-DOCUMENT\n";

    let result = parse::ner::parse_conll(long_span_conll, DatasetId::LegNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens.len(), 10);
    // All tokens should be part of the same entity
    assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-DOCUMENT");
    assert_eq!(dataset.sentences[0].tokens[9].ner_tag, "I-DOCUMENT");
}

#[test]
fn test_all_added_conll_datasets_are_loadable() {
    // Comprehensive check for all newly added CoNLL datasets
    let added_conll = [
        DatasetId::HistNERo,
        DatasetId::DutchArchaeology,
        DatasetId::FINER,
        DatasetId::CALCS2018,
        DatasetId::MedievalCharterNER,
        DatasetId::RockNER,
        DatasetId::AIDACoNLL,
        DatasetId::NNE,
        DatasetId::IndicNER,
        DatasetId::NorNE,
        DatasetId::TASTEset,
        DatasetId::TechNER,
        DatasetId::FinTechPatent,
        DatasetId::WaterAgriNER,
        DatasetId::RussianCulturalNER,
        DatasetId::BASHI,
        DatasetId::ENER,
    ];

    for id in added_conll {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable",
            id
        );
    }
}

#[test]
fn test_all_added_jsonl_datasets_are_loadable() {
    // Comprehensive check for all newly added JSONL datasets
    let added_jsonl = [
        DatasetId::MultiWOZNER,
        DatasetId::HinglishNER,
        DatasetId::AgCNER,
        DatasetId::LongDocNER,
        DatasetId::MultiBioNERLong,
        DatasetId::ReasoningNER,
        DatasetId::BioNERLLaMA,
        DatasetId::LexGLUENER,
        DatasetId::FinBenNER,
        DatasetId::FiNER139,
        DatasetId::SciNER,
        DatasetId::AIONER,
        DatasetId::WIESPAstro,
        DatasetId::CEREC,
        DatasetId::DELICATE,
        DatasetId::CSN,
    ];

    for id in added_jsonl {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable",
            id
        );
    }
}

#[test]
fn test_all_added_ud_datasets_are_loadable() {
    // Comprehensive check for all newly added UD datasets
    let added_ud = [
        DatasetId::CopticScriptorium,
        DatasetId::TaggedPBCEsperanto,
        DatasetId::TaggedPBCKlingon,
        DatasetId::AkkadianUD,
        DatasetId::AncientHebrewUD,
        DatasetId::ClassicalChineseUD,
        DatasetId::CopticUD,
        DatasetId::GothicUD,
        DatasetId::HittiteUD,
        DatasetId::OldChurchSlavonicUD,
        DatasetId::LatinITTB,
        DatasetId::LatinPROIEL,
        DatasetId::EsperantoUD,
        DatasetId::NavajoMorph,
    ];

    for id in added_ud {
        assert!(
            LoadableDatasetId::is_loadable_dataset(id),
            "{:?} should be loadable",
            id
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(id),
            Some(DatasetParsePlan::Conllu),
            "{:?} should use Conllu plan",
            id
        );
    }
}

// =========================================================================
// Edge Case and Robustness Tests
// =========================================================================

#[test]
fn test_conll_handles_windows_line_endings() {
    // Test CRLF line endings (Windows format)
    let windows_conll = "John\tB-PER\r\nSmith\tI-PER\r\n\r\nLondon\tB-LOC\r\n";

    let result = parse::ner::parse_conll(windows_conll, DatasetId::WikiGold);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 2);
}

#[test]
fn test_conll_handles_extra_whitespace() {
    // Test lines with trailing/leading whitespace
    let whitespace_conll = "  John  \t  B-PER  \n  meets  \t  O  \n\n";

    let result = parse::ner::parse_conll(whitespace_conll, DatasetId::WikiGold);
    // Should handle gracefully (may skip malformed lines)
    assert!(result.is_ok());
}

#[test]
fn test_conllu_handles_multiword_tokens() {
    // CoNLL-U multi-word tokens (e.g., "don't" -> "do" + "n't")
    let mwt_conllu = "# sent_id = test\n\
                      # text = I can't go\n\
                      1\tI\tI\tPRON\tPRP\t_\t3\tnsubj\t_\tSpaceAfter=Yes\n\
                      2-3\tcan't\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=Yes\n\
                      2\tca\tcan\tAUX\tMD\t_\t3\taux\t_\t_\n\
                      3\tn't\tnot\tPART\tRB\t_\t0\troot\t_\t_\n\
                      4\tgo\tgo\tVERB\tVB\t_\t3\txcomp\t_\tSpaceAfter=No\n";

    let result = parse::ner::parse_conllu(mwt_conllu, DatasetId::LatinUD);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    // MWT range tokens (2-3) should be skipped, only atomic tokens kept
    assert!(dataset.sentences[0].tokens.len() >= 3);
}

#[test]
fn test_conllu_handles_empty_nodes() {
    // CoNLL-U empty nodes (e.g., 2.1 for elided elements)
    let empty_node_conllu = "# sent_id = test\n\
                             # text = I saw and heard\n\
                             1\tI\tI\tPRON\tPRP\t_\t2\tnsubj\t_\tSpaceAfter=Yes\n\
                             2\tsaw\tsee\tVERB\tVBD\t_\t0\troot\t_\tSpaceAfter=Yes\n\
                             2.1\tI\tI\tPRON\tPRP\t_\t4\tnsubj\t_\t_\n\
                             3\tand\tand\tCCONJ\tCC\t_\t4\tcc\t_\tSpaceAfter=Yes\n\
                             4\theard\thear\tVERB\tVBD\t_\t2\tconj\t_\tSpaceAfter=No\n";

    let result = parse::ner::parse_conllu(empty_node_conllu, DatasetId::LatinUD);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    // Empty nodes (2.1) should be skipped
    assert_eq!(dataset.sentences[0].tokens.len(), 4);
}

#[test]
fn test_conll_with_bio_tag_normalization() {
    // Some corpora use I- at start of entity (should be B-)
    let malformed_bio = "Paris\tI-LOC\n\
                         is\tO\n\
                         beautiful\tO\n";

    let result = parse::ner::parse_conll(malformed_bio, DatasetId::WikiGold);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    // Parser should normalize or preserve the tag
    let first_tag = &dataset.sentences[0].tokens[0].ner_tag;
    assert!(first_tag == "I-LOC" || first_tag == "B-LOC");
}

#[test]
fn test_conll_with_unicode_normalization() {
    // Test that precomposed (U+00E9) and decomposed (U+0065 U+0301) forms
    // both parse and produce equivalent tokens.
    let composed = "Caf\u{00e9}\tB-LOC\n";
    let decomposed = "Cafe\u{0301}\tB-LOC\n";
    assert_ne!(
        composed, decomposed,
        "test inputs must actually differ in bytes"
    );

    let d1 = parse::ner::parse_conll(composed, DatasetId::WikiGold).unwrap();
    let d2 = parse::ner::parse_conll(decomposed, DatasetId::WikiGold).unwrap();

    assert_eq!(d1.sentences.len(), d2.sentences.len());
    assert_eq!(d1.sentences[0].tokens.len(), d2.sentences[0].tokens.len());
    assert_eq!(
        d1.sentences[0].tokens[0].ner_tag,
        d2.sentences[0].tokens[0].ner_tag
    );
}

#[test]
fn test_cadec_hf_api_unicode_prefix_case_insensitive_span_search_is_safe() {
    // Regression: span finding must not rely on Unicode lowercasing index alignment.
    let loader = DatasetLoader::new().unwrap();
    // Include `features` and compact JSON so `is_hf_api_response()` recognizes it.
    let content = r#"{"features":[{"name":"text"},{"name":"ade"},{"name":"term_PT"}],"rows":[{"row":{"text":"Müller reported HEADACHE after taking aspirin.","ade":"headache","term_PT":"Headache"}}]}"#;

    let ds = loader
        .parse_content_str(content, DatasetId::CADEC)
        .expect("parse CADEC HF API");
    assert_eq!(ds.id, DatasetId::CADEC);
    assert!(!ds.sentences.is_empty());

    let sent = &ds.sentences[0];
    // We should tag the ADE as B-/I- adverse_drug_event in BIO space.
    assert!(
        sent.tokens
            .iter()
            .any(|t| t.ner_tag == "B-adverse_drug_event" || t.ner_tag == "I-adverse_drug_event"),
        "Expected ADE tags in tokens: {:?}",
        sent.tokens
    );
}

#[test]
fn test_jsonl_with_unicode_tokens() {
    // JSONL with various Unicode characters
    let unicode_jsonl = r#"{"tokens":["北京","🎉","Москва","القاهرة"],"ner_tags":[5,0,5,5]}"#;

    let result = parse::ner::parse_jsonl_ner(unicode_jsonl, DatasetId::MultiWOZNER);
    assert!(result.is_ok());

    let dataset = result.unwrap();
    assert_eq!(dataset.sentences.len(), 1);
    assert_eq!(dataset.sentences[0].tokens.len(), 4);
    assert_eq!(dataset.sentences[0].tokens[0].text, "北京");
    assert_eq!(dataset.sentences[0].tokens[1].text, "🎉");
    assert_eq!(dataset.sentences[0].tokens[2].text, "Москва");
}

#[test]
fn test_parse_plan_consistency_with_is_loadable() {
    // Invariant: parse_plan returns Some iff is_loadable_dataset returns true
    for &id in DatasetId::all() {
        let has_plan = LoadableDatasetId::parse_plan(id).is_some();
        let is_loadable = LoadableDatasetId::is_loadable_dataset(id);
        assert_eq!(
            has_plan, is_loadable,
            "Mismatch for {:?}: parse_plan={}, is_loadable={}",
            id, has_plan, is_loadable
        );
    }
}

#[test]
fn test_dataset_coverage_by_plan() {
    // Verify we have good coverage across different parse plans
    let mut conll_count = 0;
    let mut jsonl_count = 0;
    let mut conllu_count = 0;
    let mut _other_count = 0;

    for id in LoadableDatasetId::all() {
        let ds: DatasetId = id.into();
        match LoadableDatasetId::parse_plan(ds) {
            Some(DatasetParsePlan::Conll) => conll_count += 1,
            Some(DatasetParsePlan::JsonlNer) => jsonl_count += 1,
            Some(DatasetParsePlan::Conllu) => conllu_count += 1,
            Some(_) => _other_count += 1,
            None => {}
        }
    }

    // Should have substantial coverage for each format
    assert!(
        conll_count >= 50,
        "Expected at least 50 CoNLL datasets loadable, got {}",
        conll_count
    );
    assert!(
        jsonl_count >= 20,
        "Expected at least 20 JSONL datasets loadable, got {}",
        jsonl_count
    );
    assert!(
        conllu_count >= 10,
        "Expected at least 10 CoNLLU datasets loadable, got {}",
        conllu_count
    );
}

#[test]
fn test_loadable_datasets_have_valid_metadata() {
    // All loadable datasets should have basic metadata
    for id in LoadableDatasetId::all() {
        let ds: DatasetId = id.into();

        // Name should not be empty
        assert!(!ds.name().is_empty(), "{:?} has empty name", ds);

        // Description should exist for most
        // (not asserting as some may legitimately have no description)
    }
}
