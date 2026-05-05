//! Tier-2 integration tests for `gliner2_fastino`. `#[ignore]`-gated since
//! they download the SemplificaAI/gliner2-multi-v1-onnx model (~6 GB) on
//! first run and require a working multi-session pipeline (Phase 3).
//!
//! Run locally with:
//!
//!     cargo test -p anno --features gliner2-fastino \
//!         --test gliner2_fastino_integration -- --ignored

#![cfg(feature = "gliner2-fastino")]

use anno::backends::gliner2_fastino::GLiNER2Fastino;
use anno::backends::inference::ZeroShotNER;

const FIXTURE: &str = "Acme Corp signed a deal with Globex in Paris on January 5th.";

#[test]
#[ignore]
fn fastino_multi_v1_extracts_org_and_loc() {
    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load gliner2-multi-v1");
    let ents = model
        .extract_with_types(FIXTURE, &["organization", "location"], 0.5)
        .expect("extract");

    eprintln!("entities: {ents:#?}");

    // Loose assertions — the model's exact tokenization-driven output
    // varies, but Acme Corp + Paris are clearly correct labels.
    let acme = ents.iter().find(|e| e.text.contains("Acme"));
    let paris = ents.iter().find(|e| e.text == "Paris" || e.text.contains("Paris"));
    assert!(acme.is_some(), "expected an Acme org entity, got {ents:#?}");
    assert!(paris.is_some(), "expected a Paris entity, got {ents:#?}");
}

#[test]
#[ignore]
fn fastino_extract_with_label_descriptions() {
    // Phase 1.5 M1.3: verify the [DESCRIPTION]-emitting prompt path runs
    // end-to-end against the real model and returns expected entities.
    // The actual accuracy boost vs labels-only isn't measured here —
    // that's a benchmark concern. This just exercises the pipeline.
    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load gliner2-multi-v1");
    let labeled: Vec<(&str, &str)> = vec![
        ("organization", "a company, corporation, or institution"),
        ("location", "a geographic place, city, country, or region"),
    ];
    let ents = model
        .extract_with_label_descriptions(FIXTURE, &labeled, 0.5)
        .expect("extract_with_label_descriptions");

    eprintln!("entities (with descriptions): {ents:#?}");
    assert!(
        ents.iter().any(|e| e.text.contains("Acme")),
        "expected an Acme org entity, got {ents:#?}",
    );
    assert!(
        ents.iter().any(|e| e.text == "Paris" || e.text.contains("Paris")),
        "expected a Paris location entity, got {ents:#?}",
    );
}

#[test]
#[ignore]
fn fastino_classify_smoke() {
    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load");
    let scores = model
        .classify(
            "This product is wonderful, I love it.",
            &["positive", "negative", "neutral"],
            0.0,
        )
        .expect("classify");
    assert_eq!(scores.len(), 3);
    eprintln!("classify scores: {scores:?}");
    // Top-ranked should be 'positive' for this clearly-positive text.
    assert_eq!(scores[0].0, "positive", "expected 'positive' top-ranked, got {scores:?}");
}
