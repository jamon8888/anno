//! Integration tests for `pii::scan_patterns_with_ner` with the real GLiNER2 ONNX model.
//!
//! These tests are `#[ignore]`d — they require the HF model cache to be warm (~400 MB).
//! Warm the cache first by running the pii_ner test from anno-rag, or any other test that
//! calls `Detector::new`. Then run:
//!
//! ```text
//! cargo test -p anno --features "pii-eu,gliner2-fastino" --test pii_eu_ner -- --ignored --nocapture
//! ```
#![cfg(all(feature = "pii-eu", feature = "gliner2-fastino"))]
#![allow(clippy::unwrap_used)]

use anno::backends::gliner2_fastino::GLiNER2Fastino;
use anno::pii;

fn load_model() -> GLiNER2Fastino {
    GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("warm the HF cache first: SemplificaAI/gliner2-multi-v1-onnx")
}

#[test]
#[ignore = "requires SemplificaAI/gliner2-multi-v1-onnx in HF cache; ~1 min"]
fn ner_detects_health_condition_in_clinical_sentence() {
    let model = load_model();
    let text = "The patient was diagnosed with type 2 diabetes last year.";
    let found = pii::scan_patterns_with_ner(text, &model, 0.4).expect("scan ok");
    assert!(
        found.iter().any(|e| e.pii_type == "SPECIAL_CATEGORY_HEALTH"),
        "expected SPECIAL_CATEGORY_HEALTH for diabetes in clinical context, got: {found:?}"
    );
}

#[test]
#[ignore = "requires SemplificaAI/gliner2-multi-v1-onnx in HF cache; ~1 min"]
fn ner_detects_criminal_record_mention() {
    let model = load_model();
    let text = "He was convicted of fraud in 2019.";
    let found = pii::scan_patterns_with_ner(text, &model, 0.4).expect("scan ok");
    assert!(
        found.iter().any(|e| e.pii_type == "SPECIAL_CATEGORY_CRIMINAL"),
        "expected SPECIAL_CATEGORY_CRIMINAL: {found:?}"
    );
}

#[test]
#[ignore = "requires SemplificaAI/gliner2-multi-v1-onnx in HF cache; ~1 min"]
fn ner_preserves_national_id_alongside_art9() {
    let model = load_model();
    let text = "PESEL: 80051501231. Patient diagnosed with type 2 diabetes.";
    let found = pii::scan_patterns_with_ner(text, &model, 0.4).expect("scan ok");
    assert!(
        found.iter().any(|e| e.pii_type == "NATIONAL_ID_PL"),
        "structured PESEL must survive NER path: {found:?}"
    );
    assert!(
        found.iter().any(|e| e.pii_type == "SPECIAL_CATEGORY_HEALTH"),
        "health condition must be found by NER: {found:?}"
    );
}

#[test]
#[ignore = "requires SemplificaAI/gliner2-multi-v1-onnx in HF cache; ~1 min; precision check"]
fn ner_reduces_false_positives_vs_keywords() {
    // "Catholic church was built in 1832" — historical fact, not a personal religious belief.
    // The keyword path fires on "Catholic"; NER at threshold 0.5 should not.
    let model = load_model();
    let text = "The Catholic church was built in 1832.";
    let found = pii::scan_patterns_with_ner(text, &model, 0.5).expect("scan ok");
    assert!(
        !found.iter().any(|e| e.pii_type == "SPECIAL_CATEGORY_RELIGION"),
        "NER should not fire on historical church reference (false positive): {found:?}"
    );
}
