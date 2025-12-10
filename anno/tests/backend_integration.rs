//! Integration tests for backend implementations via BackendFactory.
//!
//! Tests backend creation, availability, and basic extraction functionality.

#![cfg(feature = "eval-advanced")]

use anno::eval::backend_factory::BackendFactory;
use anno::Model;

/// Test that all builtin backends can be created
#[test]
fn test_builtin_backends_create() {
    let builtins = ["pattern", "heuristic", "crf", "stacked", "ensemble"];

    for name in builtins {
        let result = BackendFactory::create(name);
        assert!(
            result.is_ok(),
            "Failed to create builtin backend '{}': {:?}",
            name,
            result.err()
        );

        let model = result.unwrap();
        assert!(
            model.is_available(),
            "Backend '{}' should be available",
            name
        );
    }
}

/// Test that builtin backends can extract entities
#[test]
fn test_builtin_backends_extract() {
    let text = "Apple Inc. was founded by Steve Jobs in Cupertino, California.";
    let builtins = ["pattern", "heuristic"];

    for name in builtins {
        let model = BackendFactory::create(name).expect(&format!("Create {}", name));
        let result = model.extract_entities(text, None);

        assert!(
            result.is_ok(),
            "Backend '{}' failed to extract: {:?}",
            name,
            result.err()
        );

        // Entities may or may not be found depending on model
        // Just ensure no panic/error
    }
}

/// Test that unknown backend returns error
#[test]
fn test_unknown_backend_error() {
    let result = BackendFactory::create("nonexistent_backend_xyz");
    assert!(result.is_err(), "Unknown backend should return error");

    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => panic!("Expected error for unknown backend"),
    };
    assert!(
        err_msg.contains("Unknown backend") || err_msg.contains("nonexistent"),
        "Error should mention unknown backend: {}",
        err_msg
    );
}

/// Test available_backends returns non-empty list
#[test]
fn test_available_backends_list() {
    let backends = BackendFactory::available_backends();

    // Should have at least the builtin backends
    assert!(backends.len() >= 5, "Should have at least 5 backends");

    // Check builtins are present
    assert!(backends.contains(&"pattern"));
    assert!(backends.contains(&"heuristic"));
    assert!(backends.contains(&"crf"));
}

/// Test that CRF backend produces consistent results
#[test]
fn test_crf_deterministic() {
    let text = "John Smith works at Google in New York.";

    let model = BackendFactory::create("crf").expect("Create CRF");

    let result1 = model
        .extract_entities(text, None)
        .expect("First extraction");
    let result2 = model
        .extract_entities(text, None)
        .expect("Second extraction");

    // Same input should produce same output
    assert_eq!(result1.len(), result2.len(), "CRF should be deterministic");

    for (e1, e2) in result1.iter().zip(result2.iter()) {
        assert_eq!(e1.text, e2.text);
        assert_eq!(e1.start, e2.start);
        assert_eq!(e1.end, e2.end);
    }
}

/// Test pattern backend with custom patterns
#[test]
fn test_pattern_backend_custom() {
    use anno::RegexNER;

    let model = RegexNER::new();
    let text = "Contact us at support@example.com or 555-1234.";

    let entities = model.extract_entities(text, None).expect("Extract");

    // Should find at least the email pattern
    let has_email = entities.iter().any(|e| e.text.contains("@"));
    assert!(has_email, "Should find email pattern");
}

/// Test stacked backend combines results
#[test]
fn test_stacked_backend() {
    let model = BackendFactory::create("stacked").expect("Create stacked");
    let text = "Microsoft Corporation is headquartered in Redmond.";

    let result = model.extract_entities(text, None);
    assert!(result.is_ok(), "Stacked backend should not error");

    // Stacked uses multiple layers - just ensure it runs
}

/// Test ensemble backend
#[test]
fn test_ensemble_backend() {
    let model = BackendFactory::create("ensemble").expect("Create ensemble");
    let text = "Amazon Web Services launched in 2006.";

    let result = model.extract_entities(text, None);
    assert!(result.is_ok(), "Ensemble backend should not error");
}

/// Test HMM backend (if available)
#[test]
fn test_hmm_backend() {
    let result = BackendFactory::create("hmm");
    if let Ok(model) = result {
        let text = "Tesla Inc. CEO Elon Musk announced new products.";
        let _ = model.extract_entities(text, None);
        // Just ensure no panic
    }
}

/// Test BiLSTM-CRF backend (falls back to heuristic)
#[test]
fn test_bilstm_crf_backend() {
    let result = BackendFactory::create("bilstm_crf");
    if let Ok(model) = result {
        let text = "The United Nations is an international organization.";
        let _ = model.extract_entities(text, None);
        // Just ensure no panic - may use heuristic fallback
    }
}

#[cfg(feature = "onnx")]
mod onnx_tests {
    use super::*;

    #[test]
    fn test_onnx_backends_listed() {
        let backends = BackendFactory::available_backends();

        // ONNX backends should be in the list
        assert!(
            backends.contains(&"bert_onnx"),
            "bert_onnx should be available"
        );
        assert!(
            backends.contains(&"gliner_onnx"),
            "gliner_onnx should be available"
        );
        assert!(backends.contains(&"nuner"), "nuner should be available");
    }

    #[test]
    fn test_bert_onnx_create() {
        // Try to create - may fail if model not downloaded
        let result = BackendFactory::create("bert_onnx");

        // Either succeeds or fails with model download error
        if let Err(e) = &result {
            let msg = e.to_string();
            assert!(
                msg.contains("model") || msg.contains("download") || msg.contains("not found"),
                "Error should be about model availability: {}",
                msg
            );
        }
    }
}

#[cfg(feature = "candle")]
mod candle_tests {
    use super::*;

    #[test]
    fn test_candle_backends_listed() {
        let backends = BackendFactory::available_backends();

        assert!(
            backends.contains(&"candle_ner"),
            "candle_ner should be available"
        );
    }
}
