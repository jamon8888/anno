//! Regression tests for backend error handling contracts.
//!
//! These tests focus on a specific invariant: if a backend can't initialize,
//! it must return a clear error (no silent fallback to a different model).

use anno::eval::backend_factory::BackendFactory;

/// UniversalNER should be explicit about missing API keys (no silent fallback).
#[test]
fn test_universal_ner_requires_api_key_or_errors_explicitly() {
    let model = BackendFactory::create("universal_ner")
        .expect("BackendFactory should be able to construct UniversalNER wrapper");
    // This test is about behavior when *unavailable*. If the environment (or a local .env)
    // provides keys, UniversalNER will be available and we can't assert the missing-key path.
    if model.is_available() {
        return;
    }

    let err = model
        .extract_entities("Marie Curie visited Paris.", None)
        .expect_err("UniversalNER should error (not return empty) when unavailable");
    let msg = format!("{err}").to_lowercase();

    assert!(
        msg.contains("api key")
            || msg.contains("openai_api_key")
            || msg.contains("anthropic_api_key"),
        "Expected explicit API-key guidance, got: {msg}"
    );
}

/// W2NER should not silently fallback if the model path is invalid.
#[test]
#[cfg(feature = "onnx")]
fn test_w2ner_invalid_local_model_path_errors_no_fallback() {
    use tempfile::tempdir;

    // Use an existing directory so W2NER goes down the "local path" branch
    // (avoids network or HF downloads).
    let dir = tempdir().expect("tempdir");
    std::env::set_var("W2NER_MODEL_PATH", dir.path());

    let err = match BackendFactory::create("w2ner") {
        Ok(_) => panic!("W2NER should error when required local files are missing"),
        Err(e) => e,
    };
    let msg = format!("{err}");

    assert!(
        msg.contains("W2NER") || msg.contains("w2ner"),
        "Expected W2NER mention in error, got: {msg}"
    );
    assert!(
        msg.contains("Failed to load model") || msg.contains("model.onnx"),
        "Expected missing model file guidance, got: {msg}"
    );

    std::env::remove_var("W2NER_MODEL_PATH");
}
