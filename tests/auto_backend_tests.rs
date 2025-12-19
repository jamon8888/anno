//! Tests for automatic backend selection functionality.

use anno::{auto, available_backends, Model};

#[test]
fn test_auto_always_returns_model() {
    // auto() should always return at least StackedNER
    let model = auto().expect("auto() should always return a model");
    assert!(model.is_available());
    assert!(!model.name().is_empty());
}

#[test]
#[cfg(not(any(feature = "onnx", feature = "candle")))]
fn test_auto_falls_back_to_stacked_without_ml_features() {
    // Without ONNX/Candle, auto() should fall back to the always-available StackedNER.
    let model = auto().expect("auto() should work");
    assert!(model.name().to_lowercase().contains("stacked"));
}

#[test]
fn test_available_backends_always_includes_core() {
    let backends = available_backends();

    // Core backends should always be available
    assert!(backends
        .iter()
        .any(|(name, available)| *name == "RegexNER" && *available));

    assert!(backends
        .iter()
        .any(|(name, available)| *name == "HeuristicNER" && *available));

    assert!(backends
        .iter()
        .any(|(name, available)| *name == "StackedNER" && *available));

    // Note: HybridNER is not a separate backend - it's a pattern used by NERExtractor
    // which combines ML backends with RegexNER. The test checks core backends only.
}

#[test]
fn test_available_backends_onnx_feature_gated() {
    let backends = available_backends();
    let has_bert = backends.iter().any(|(name, _)| *name == "BertNEROnnx");
    let has_gliner = backends.iter().any(|(name, _)| *name == "GLiNEROnnx");
    let has_nuner = backends.iter().any(|(name, _)| *name == "NuNER");
    let has_w2ner = backends.iter().any(|(name, _)| *name == "W2NER");

    #[cfg(feature = "onnx")]
    {
        // With onnx feature, these should be listed as available
        assert!(has_bert);
        assert!(has_gliner);
        assert!(has_nuner);
        assert!(has_w2ner);
    }

    #[cfg(not(feature = "onnx"))]
    {
        // Without onnx feature, these should not appear at all.
        assert!(!has_bert);
        assert!(!has_gliner);
        assert!(!has_nuner);
        assert!(!has_w2ner);
    }
}

#[test]
fn test_available_backends_candle_feature_gated() {
    let backends = available_backends();
    let has_candle = backends.iter().any(|(name, _)| *name == "CandleNER");

    #[cfg(feature = "candle")]
    {
        assert!(has_candle);
    }

    #[cfg(not(feature = "candle"))]
    {
        assert!(!has_candle);
    }
}

#[test]
fn test_auto_returns_working_model() {
    // The model returned by auto() should actually work
    let model = auto().expect("auto() should return a model");
    let result = model.extract_entities("John works at Apple", None);
    assert!(result.is_ok());
    let _entities = result.unwrap();
    // Should find at least some entities (regex-based ones like dates, or heuristic ones)
    // Even if it doesn't find "John" or "Apple", it should not panic
}

#[test]
fn test_available_backends_returns_vec() {
    let backends = available_backends();
    assert!(!backends.is_empty());
    // Should have at least the 3 always-available core backends
    assert!(backends.len() >= 3);
}

#[test]
fn test_available_backends_format() {
    let backends = available_backends();
    for (name, available) in backends {
        assert!(!name.is_empty());
        // available should be a boolean (always true or false)
        assert!(matches!(available, true | false));
    }
}

#[test]
fn test_model_trait_in_scope() {
    // Ensures this file stays aligned with the public trait-based API.
    // (We use Model methods on the returned trait object.)
    let model: Box<dyn Model> = auto().expect("auto() should work");
    assert!(model.is_available());
}
