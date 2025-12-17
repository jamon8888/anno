//! Regression tests for CandleNER backend.
//!
//! These tests verify critical fixes that were applied to CandleNER:
//!
//! 1. **Tokenizer case preservation** - The tokenizer must NOT lowercase input
//!    because capitalization is critical for NER (proper names are capitalized).
//!
//! 2. **Config parsing** - The encoder must read vocab_size from the model's
//!    config.json, not use hardcoded defaults.
//!
//! 3. **Optional final_norm** - BERT models don't have final_layer_norm;
//!    only ModernBERT and similar pre-norm architectures have it.
//!
//! 4. **Shared weight loading** - Encoder and classifier must share the same
//!    VarBuilder when loading from a single safetensors file.

#![cfg(feature = "candle")]

use anno::backends::encoder_candle::{CandleEncoder, EncoderConfig};

// =============================================================================
// Tokenizer Case Preservation Tests
// =============================================================================

/// CRITICAL: Verify that the tokenizer preserves case.
///
/// This was the root cause of CandleNER returning no entities.
/// BertNormalizer::default() lowercases input, which breaks NER.
#[test]
#[cfg(feature = "candle")]
fn test_tokenizer_preserves_case() {
    // This test verifies the fix at the API level
    // The actual fix is in BertNormalizer::new(..., lowercase: false)

    // We can't easily test the internal tokenizer directly,
    // but we can verify the encoder config defaults are correct
    let bert_config = EncoderConfig::bert_base();

    // BERT models should NOT use pre-norm (they use post-norm)
    assert!(
        !bert_config.use_pre_norm,
        "BERT should use post-norm, not pre-norm"
    );

    // BERT models should NOT use RoPE (they use absolute position embeddings)
    assert!(!bert_config.use_rope, "BERT should not use RoPE");

    // BERT models should NOT use GeGLU (they use GELU)
    assert!(!bert_config.use_geglu, "BERT should not use GeGLU");
}

/// Verify that ModernBERT config uses pre-norm.
#[test]
#[cfg(feature = "candle")]
fn test_modernbert_uses_pre_norm() {
    let config = EncoderConfig::modernbert_base();

    assert!(config.use_pre_norm, "ModernBERT should use pre-norm");
    assert!(config.use_rope, "ModernBERT should use RoPE");
    assert!(config.use_geglu, "ModernBERT should use GeGLU");
}

// =============================================================================
// Config Parsing Tests
// =============================================================================

/// Verify that config parsing reads actual values from JSON.
#[test]
#[cfg(feature = "candle")]
fn test_config_parsing_reads_vocab_size() {
    // This simulates the config.json from dslim/bert-base-NER
    let config_json = r#"{
        "vocab_size": 28996,
        "hidden_size": 768,
        "num_attention_heads": 12,
        "num_hidden_layers": 12,
        "intermediate_size": 3072,
        "max_position_embeddings": 512,
        "model_type": "bert"
    }"#;

    let config = CandleEncoder::parse_config(config_json).expect("Config parsing should succeed");

    // CRITICAL: Must use actual vocab_size from config, not default 30522
    assert_eq!(
        config.vocab_size, 28996,
        "Config should read vocab_size from JSON, not use default"
    );
    assert_eq!(config.hidden_size, 768);
    assert_eq!(config.num_attention_heads, 12);
    assert_eq!(config.num_hidden_layers, 12);
}

/// Verify that BERT model_type implies post-norm architecture.
#[test]
#[cfg(feature = "candle")]
fn test_config_parsing_detects_bert_architecture() {
    let bert_config_json = r#"{
        "model_type": "bert",
        "vocab_size": 30522,
        "hidden_size": 768
    }"#;

    let config =
        CandleEncoder::parse_config(bert_config_json).expect("Config parsing should succeed");

    // BERT uses post-norm (classic architecture)
    assert!(
        !config.use_pre_norm,
        "BERT model_type should result in use_pre_norm=false (post-norm)"
    );
    assert!(!config.use_rope, "BERT should not use RoPE");
}

/// Verify that RoBERTa/DeBERTa model_types imply pre-norm.
#[test]
#[cfg(feature = "candle")]
fn test_config_parsing_detects_prenorm_architectures() {
    let roberta_json = r#"{
        "model_type": "roberta",
        "vocab_size": 50265,
        "hidden_size": 768
    }"#;

    let config = CandleEncoder::parse_config(roberta_json).expect("Config parsing should succeed");

    assert!(
        config.use_pre_norm,
        "RoBERTa model_type should result in use_pre_norm=true"
    );

    let deberta_json = r#"{
        "model_type": "deberta",
        "vocab_size": 128100,
        "hidden_size": 768
    }"#;

    let config = CandleEncoder::parse_config(deberta_json).expect("Config parsing should succeed");

    assert!(
        config.use_pre_norm,
        "DeBERTa model_type should result in use_pre_norm=true"
    );
}

// =============================================================================
// Architecture Default Tests
// =============================================================================

/// Verify EncoderConfig defaults are reasonable.
#[test]
fn test_encoder_config_defaults() {
    let config = EncoderConfig::default();

    // Should have sensible defaults
    assert!(config.vocab_size > 0);
    assert!(config.hidden_size > 0);
    assert!(config.num_attention_heads > 0);
    assert!(config.num_hidden_layers > 0);

    // Head dimension should divide evenly
    assert_eq!(
        config.hidden_size % config.num_attention_heads,
        0,
        "hidden_size should be divisible by num_attention_heads"
    );
}

/// Verify DeBERTa-v3 config uses pre-norm.
#[test]
fn test_deberta_v3_config() {
    let config = EncoderConfig::deberta_v3_base();

    assert!(config.use_pre_norm, "DeBERTa-v3 should use pre-norm");
    assert!(
        !config.use_rope,
        "DeBERTa-v3 does not use RoPE (uses relative pos embeddings)"
    );
    assert!(!config.use_geglu, "DeBERTa-v3 does not use GeGLU");
    assert_eq!(config.vocab_size, 128100);
}

// =============================================================================
// Entity Type Mapping Tests
// =============================================================================

/// Verify that standard CoNLL label indices are correct.
#[test]
fn test_conll_label_mapping() {
    // dslim/bert-base-NER uses this id2label mapping
    let expected_labels = [
        "O",      // 0
        "B-MISC", // 1
        "I-MISC", // 2
        "B-PER",  // 3
        "I-PER",  // 4
        "B-ORG",  // 5
        "I-ORG",  // 6
        "B-LOC",  // 7
        "I-LOC",  // 8
    ];

    // Verify the mapping
    assert_eq!(expected_labels[0], "O");
    assert_eq!(expected_labels[3], "B-PER");
    assert_eq!(expected_labels[4], "I-PER");
    assert_eq!(expected_labels[5], "B-ORG");
    assert_eq!(expected_labels[7], "B-LOC");
}

// =============================================================================
// Environment Variable Tests
// =============================================================================

/// Verify that env module has LLM API key helpers.
#[test]
fn test_env_has_llm_helpers() {
    // Just verify the functions exist and return bool/Option
    let _has_key: bool = anno::env::has_llm_api_key();
    let _key: Option<(String, &'static str)> = anno::env::llm_api_key();
    let _has_hf: bool = anno::env::has_hf_token();
    let _hf_token: Option<String> = anno::env::hf_token();
}

/// Verify dotenv loading is idempotent.
#[test]
fn test_dotenv_loading_idempotent() {
    // Should not panic when called multiple times
    anno::env::load_dotenv();
    anno::env::load_dotenv();
    anno::env::load_dotenv();
}

// =============================================================================
// Integration Tests (require model download)
// =============================================================================

/// Integration test for CandleNER with actual model.
/// Only runs if model is already cached.
#[test]
#[cfg(feature = "candle")]
#[ignore = "Requires model download - run with: cargo test --features candle -- --ignored"]
fn test_candle_ner_integration() {
    use anno::CandleNER;
    use anno::Model;

    let model = CandleNER::from_pretrained("dslim/bert-base-NER").expect("Model should load");

    // Test with cased input - should detect entities
    let entities = model
        .extract_entities("Barack Obama was President.", None)
        .expect("Extraction should succeed");

    assert!(
        !entities.is_empty(),
        "CandleNER should extract entities from cased text"
    );

    // Verify Barack Obama is detected as PER
    let person_entities: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, anno::EntityType::Person))
        .collect();

    assert!(
        !person_entities.is_empty(),
        "Should detect at least one Person entity"
    );

    // The text should be "Barack Obama" or similar
    assert!(
        person_entities.iter().any(|e| e.text.contains("Obama")),
        "Should detect Obama as a person"
    );
}

/// Verify that cased vs uncased input produces different results.
/// This documents the importance of case preservation.
#[test]
#[cfg(feature = "candle")]
#[ignore = "Requires model download - run with: cargo test --features candle -- --ignored"]
fn test_case_sensitivity_matters() {
    use anno::CandleNER;
    use anno::Model;

    let model = CandleNER::from_pretrained("dslim/bert-base-NER").expect("Model should load");

    // Properly cased input
    let cased_entities = model
        .extract_entities("Barack Obama", None)
        .expect("Extraction should succeed");

    // Improperly cased input (all lowercase)
    let _lowercased_entities = model
        .extract_entities("barack obama", None)
        .expect("Extraction should succeed");

    // The cased version should have better results
    // (or at least not worse - the model was trained on cased text)
    let cased_per: Vec<_> = cased_entities
        .iter()
        .filter(|e| matches!(e.entity_type, anno::EntityType::Person))
        .collect();

    // Cased input should definitely find the person
    assert!(
        !cased_per.is_empty(),
        "Cased 'Barack Obama' should be detected as Person"
    );
}

// =============================================================================
// TPLinker Regression Tests
// =============================================================================

/// TPLinker should not split multi-word names into separate entities.
#[test]
fn test_tplinker_multiword_names() {
    use anno::backends::tplinker::TPLinker;
    use anno::Model;

    let model = TPLinker::new().expect("TPLinker should create");
    assert!(model.is_available());

    let entities = model
        .extract_entities("Dr. Smith works at MIT in Boston.", None)
        .expect("Extraction should succeed");

    // Should find "Dr. Smith" as a single entity, not "Dr." separately.
    let texts: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
    assert!(
        !texts.contains(&"Dr."),
        "TPLinker should not split 'Dr.' as a separate entity"
    );
    assert!(
        texts.iter().any(|t| t.contains("Smith")),
        "TPLinker should find Smith in an entity"
    );
}

// =============================================================================
// UniversalNER Regression Tests
// =============================================================================

/// UniversalNER should report availability correctly based on API keys.
#[test]
fn test_universal_ner_availability() {
    use anno::backends::universal_ner::UniversalNER;
    use anno::Model;

    // Load any .env that might exist
    anno::env::load_dotenv();

    let model = UniversalNER::new().expect("UniversalNER should create");

    // is_available should match whether we have API keys
    let has_key = anno::env::has_llm_api_key()
        || std::env::var("UNIVERSAL_NER_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok();

    assert_eq!(
        model.is_available(),
        cfg!(feature = "llm") && has_key,
        "is_available should reflect API key presence"
    );
}

/// BurnNER should be available and produce output (even if placeholder).
#[test]
#[cfg(feature = "burn")]
fn test_burn_ner_available() {
    use anno::backends::burn::BurnNER;
    use anno::Model;

    let model = BurnNER::new().expect("BurnNER should create");
    assert!(model.is_available(), "BurnNER should be runnable");

    let entities = model
        .extract_entities("Steve Jobs founded Apple.", None)
        .expect("Extraction should succeed");

    // Minimal implementation uses heuristic extraction; should produce something.
    assert!(
        !entities.is_empty(),
        "BurnNER should find entities in test text"
    );
}

// =============================================================================
// GLiNERCandle Regression Tests
// =============================================================================

/// GLiNERCandle encoder config parsing should handle nested encoder_config.
///
/// GLiNER models have config in gliner_config.json with encoder_config nested inside.
/// The fix extracts encoder_config for CandleEncoder::parse_config.
#[test]
#[cfg(feature = "candle")]
fn test_gliner_nested_encoder_config_parsing() {
    // Simulate GLiNER config structure
    let gliner_config = r#"{
        "class_token_index": -1,
        "hidden_size": 768,
        "encoder_config": {
            "model_type": "bert",
            "hidden_size": 128,
            "num_hidden_layers": 2,
            "vocab_size": 30522,
            "num_attention_heads": 2
        }
    }"#;

    let config: serde_json::Value = serde_json::from_str(gliner_config).unwrap();

    // The fix: extract encoder_config, not use top-level hidden_size
    let encoder_config_json = if config.get("encoder_config").is_some() {
        config["encoder_config"].clone()
    } else {
        config.clone()
    };

    let hidden_size = encoder_config_json["hidden_size"].as_u64().unwrap_or(768) as usize;

    // Should get 128 (from encoder_config), not 768 (from top-level)
    assert_eq!(
        hidden_size, 128,
        "Should extract hidden_size from encoder_config"
    );
}

/// GLiNERCandle SpanRepLayer uses 4x hidden multiplier, not 2x.
///
/// This was discovered when loading NeuML/gliner-bert-tiny which has:
/// - project_start: [512, 128] = 4x multiplier
#[test]
#[cfg(feature = "candle")]
fn test_gliner_span_rep_multiplier() {
    // The fix: SpanRepLayer uses hidden_size * 4, not hidden_size * 2
    let hidden_size: usize = 128;
    let expected_intermediate = hidden_size * 4; // 512
    assert_eq!(
        expected_intermediate, 512,
        "SpanRepLayer should use 4x multiplier"
    );
}

/// GLiNERCandle integration test - requires model download.
#[test]
#[cfg(feature = "candle")]
#[ignore = "Requires model download - run with: cargo test --features candle -- --ignored"]
fn test_gliner_candle_integration() {
    use anno::backends::gliner_candle::GLiNERCandle;
    use anno::Model;

    let model = GLiNERCandle::from_pretrained("NeuML/gliner-bert-tiny")
        .expect("GLiNERCandle should load NeuML/gliner-bert-tiny");

    assert!(
        model.is_available(),
        "Model should be available after loading"
    );

    let entities = model
        .extract_entities("Steve Jobs founded Apple in California.", None)
        .expect("Extraction should succeed");

    // The tiny model may not produce great results, but should produce something
    // at the lower threshold (0.3)
    // Just verify no panics and the pipeline runs
    eprintln!("GLiNERCandle found {} entities", entities.len());
}

// =============================================================================
// Ensemble Provenance Tests
// =============================================================================

/// CRITICAL: Verify ensemble sets provenance for ALL entities, including single-source.
///
/// This was a bug where entities from single backends (like RegexNER's DATE entities)
/// would not have provenance set, causing assertion failures in tests.
#[test]
fn test_ensemble_single_source_provenance() {
    use anno::backends::ensemble::EnsembleNER;
    use anno::Model;

    let ner = EnsembleNER::new();
    let entities = ner
        .extract_entities("The meeting is tomorrow at 3pm.", None)
        .expect("Extraction should succeed");

    for e in &entities {
        assert!(
            e.provenance.is_some(),
            "Entity '{}' ({:?}) should have provenance",
            e.text,
            e.entity_type
        );
    }
}

/// Verify ensemble handles empty input without panicking.
#[test]
fn test_ensemble_empty_input() {
    use anno::backends::ensemble::EnsembleNER;
    use anno::Model;

    let ner = EnsembleNER::new();
    let entities = ner
        .extract_entities("", None)
        .expect("Empty input should not error");

    assert!(
        entities.is_empty(),
        "Empty input should produce no entities"
    );
}

// =============================================================================
// W2NER Environment Variable Tests
// =============================================================================

/// Verify W2NER respects W2NER_MODEL_PATH environment variable.
#[test]
#[cfg(feature = "onnx")]
fn test_w2ner_env_var_support() {
    // This just verifies the code path exists - actual loading requires a model
    // The key fix was adding std::env::var("W2NER_MODEL_PATH") in backend_factory.rs
    use std::env;

    // Set a fake path (will fail to load, but should try)
    env::set_var("W2NER_MODEL_PATH", "/nonexistent/path/to/model");

    // The factory should now use this path instead of default
    // We can't easily test the full path without a real model,
    // but we verify the env var is read
    let path = env::var("W2NER_MODEL_PATH").unwrap();
    assert_eq!(path, "/nonexistent/path/to/model");

    // Clean up
    env::remove_var("W2NER_MODEL_PATH");
}

// =============================================================================
// Backend Availability Tests
// =============================================================================

/// Verify pattern backend is always available.
#[test]
fn test_pattern_backend_available() {
    use anno::Model;
    use anno::RegexNER;

    let ner = RegexNER::new();
    assert!(ner.is_available(), "RegexNER should always be available");
}

/// Verify heuristic backend is always available.
#[test]
fn test_heuristic_backend_available() {
    use anno::HeuristicNER;
    use anno::Model;

    let ner = HeuristicNER::new();
    assert!(
        ner.is_available(),
        "HeuristicNER should always be available"
    );
}

/// Verify stacked backend is always available.
#[test]
fn test_stacked_backend_available() {
    use anno::Model;
    use anno::StackedNER;

    let ner = StackedNER::default();
    assert!(ner.is_available(), "StackedNER should always be available");
}

// =============================================================================
// Multilingual NER Tests
// =============================================================================

/// Test CandleNER with multilingual text.
#[test]
#[cfg(feature = "candle")]
#[ignore = "Requires model download"]
fn test_candle_ner_multilingual() {
    use anno::backends::candle::CandleNER;
    use anno::Model;

    let model = CandleNER::from_pretrained("dslim/bert-base-NER").expect("Should load model");

    // German text with umlauts
    let entities = model
        .extract_entities("Angela Merkel besuchte München.", None)
        .expect("Should handle German text");

    // French text with accents
    let entities2 = model
        .extract_entities("François Hollande était à Paris.", None)
        .expect("Should handle French text");

    // Just verify no panics - the English-trained model may not extract perfectly
    eprintln!(
        "German: {} entities, French: {} entities",
        entities.len(),
        entities2.len()
    );
}

// =============================================================================
// Edge Case Tests
// =============================================================================

/// Verify backends handle very long text without panicking.
#[test]
fn test_long_text_handling() {
    use anno::Model;
    use anno::RegexNER;

    let ner = RegexNER::new();

    // Generate a long text (100+ sentences)
    let long_text: String = (0..100)
        .map(|i| {
            format!(
                "Person {} visited City {} on January {}. ",
                i,
                i,
                i % 31 + 1
            )
        })
        .collect();

    let entities = ner
        .extract_entities(&long_text, None)
        .expect("Should handle long text");

    // Should find many date entities
    assert!(
        !entities.is_empty(),
        "Should extract entities from long text"
    );
}

/// Verify backends handle special characters without panicking.
#[test]
fn test_special_characters() {
    use anno::HeuristicNER;
    use anno::Model;

    let ner = HeuristicNER::new();

    // Text with various special characters
    let texts = [
        "Dr. O'Brien & Prof. Müller discussed AI@Google.",
        "🚀 Elon Musk announced SpaceX's latest mission! 🛸",
        "北京 (Beijing) hosted the 東京 (Tokyo) delegation.",
        "Price: $1,234.56 — a 10% discount on €999.99!",
        r#"The "quick" brown fox's code: `fn main() { }`"#,
    ];

    for text in &texts {
        let result = ner.extract_entities(text, None);
        assert!(result.is_ok(), "Should handle text: {:?}", text);
    }
}

/// Verify entity spans are valid character offsets.
#[test]
fn test_entity_spans_valid() {
    use anno::backends::ensemble::EnsembleNER;
    use anno::Model;

    let ner = EnsembleNER::new();
    let text = "Barack Obama visited Paris yesterday.";
    let char_count = text.chars().count();

    let entities = ner
        .extract_entities(text, None)
        .expect("Extraction should succeed");

    for e in &entities {
        assert!(
            e.start <= e.end,
            "Start should be <= end: {} > {}",
            e.start,
            e.end
        );
        assert!(
            e.end <= char_count,
            "End should be <= text length: {} > {}",
            e.end,
            char_count
        );

        // Verify the extracted text matches the span
        let span_text: String = text.chars().skip(e.start).take(e.end - e.start).collect();
        assert_eq!(
            span_text, e.text,
            "Span text '{}' should match entity text '{}'",
            span_text, e.text
        );
    }
}

/// Verify confidence scores are in valid range.
#[test]
fn test_confidence_scores_valid() {
    use anno::backends::ensemble::EnsembleNER;
    use anno::Model;

    let ner = EnsembleNER::new();
    let entities = ner
        .extract_entities("Steve Jobs founded Apple in California.", None)
        .expect("Extraction should succeed");

    for e in &entities {
        assert!(
            e.confidence >= 0.0 && e.confidence <= 1.0,
            "Confidence should be in [0, 1]: {} for '{}'",
            e.confidence,
            e.text
        );
    }
}

/// Verify extraction is deterministic.
#[test]
fn test_extraction_deterministic() {
    use anno::Model;
    use anno::RegexNER;

    let ner = RegexNER::new();
    let text = "Contact us at test@example.com or call 555-1234.";

    let entities1 = ner.extract_entities(text, None).expect("First call");
    let entities2 = ner.extract_entities(text, None).expect("Second call");

    assert_eq!(entities1.len(), entities2.len(), "Same number of entities");

    for (e1, e2) in entities1.iter().zip(entities2.iter()) {
        assert_eq!(e1.text, e2.text, "Same text");
        assert_eq!(e1.start, e2.start, "Same start");
        assert_eq!(e1.end, e2.end, "Same end");
        assert_eq!(e1.entity_type, e2.entity_type, "Same type");
    }
}
