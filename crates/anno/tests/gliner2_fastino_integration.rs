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
fn fastino_batch_per_sample_labels() {
    // Phase 1.5 M4.2: each text in the batch carries its own label set.
    // Text 0 only looks for orgs; text 1 only looks for people + places.
    use anno::backends::gliner2_fastino::BatchSchemaMode;

    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load gliner2-multi-v1");
    let texts: Vec<&str> = vec![
        "Acme Corp signed a deal.",
        "Marie Curie worked in France.",
    ];
    let labels_per_text: Vec<Vec<&str>> = vec![
        vec!["organization"],
        vec!["person", "location"],
    ];

    let results = model
        .batch_extract_with_schema_mode(
            &texts,
            BatchSchemaMode::PerSample(&labels_per_text),
            0.5,
        )
        .expect("batch extract");

    assert_eq!(results.len(), 2);
    // Text 0 should detect Acme as organization (or at least produce non-empty).
    assert!(
        results[0].iter().any(|e| matches!(
            e.entity_type,
            anno::EntityType::Organization
        )),
        "expected an Organization in results[0], got {:#?}", results[0],
    );
    // Text 1 should detect Marie as person OR France as location.
    assert!(
        results[1].iter().any(|e| matches!(
            e.entity_type,
            anno::EntityType::Person | anno::EntityType::Location
        )),
        "expected a Person or Location in results[1], got {:#?}", results[1],
    );
}

#[test]
#[ignore]
fn fastino_batch_per_sample_length_mismatch_errors() {
    // Defensive: PerSample with mismatched outer-slice length returns
    // a typed Backend error, not a panic.
    use anno::backends::gliner2_fastino::BatchSchemaMode;

    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load");
    let texts: Vec<&str> = vec!["one", "two"];
    let labels_per_text: Vec<Vec<&str>> = vec![vec!["organization"]]; // 1 entry, not 2.

    let err = model
        .batch_extract_with_schema_mode(
            &texts,
            BatchSchemaMode::PerSample(&labels_per_text),
            0.5,
        )
        .expect_err("expected length-mismatch error");
    assert!(
        err.to_string().contains("PerSample label count")
            && err.to_string().contains("!= texts count"),
        "got: {err}",
    );
}

#[test]
#[ignore]
fn fastino_batch_extract_streaming_fires_callbacks_in_order() {
    // Phase 1.5 M3.2: drives 5 short texts through batch_extract_streaming
    // with batch_size = 2, so we get chunks (0..2), (2..4), (4..5). Each
    // text's callback fires sequentially with the right index. Asserts:
    //   1. All five indices are seen, in order.
    //   2. Total entities >= number of texts (each one has at least one).
    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load gliner2-multi-v1");
    let texts: Vec<&str> = vec![
        "Acme Corp signed a deal in Paris.",
        "Globex acquired Hooli.",
        "Marie Curie worked in France.",
        "Apple is based in California.",
        "Tokyo is the capital of Japan.",
    ];

    let mut indices_seen: Vec<usize> = Vec::new();
    let mut total_entities = 0usize;

    model
        .batch_extract_streaming(
            &texts,
            &["organization", "location", "person"],
            0.5,
            2, // batch_size
            |idx, ents| {
                indices_seen.push(idx);
                total_entities += ents.len();
            },
        )
        .expect("batch_extract_streaming");

    assert_eq!(indices_seen, vec![0, 1, 2, 3, 4]);
    assert!(
        total_entities >= 5,
        "expected at least 5 total entities across 5 texts, got {total_entities}",
    );
}

#[test]
#[ignore]
fn fastino_batch_extract_streaming_rejects_zero_batch_size() {
    // Defensive: typed Backend error rather than silent loop / divide-by-zero.
    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load");
    let texts: &[&str] = &["irrelevant"];
    let result = model.batch_extract_streaming(
        texts,
        &["organization"],
        0.5,
        0,
        |_, _| panic!("callback should not fire when batch_size = 0"),
    );
    let err = result.expect_err("expected an error for batch_size = 0");
    assert!(
        err.to_string().contains("batch_size must be > 0"),
        "got: {err}",
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
