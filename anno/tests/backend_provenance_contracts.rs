//! Regression tests for provenance attribution on wrapper/placeholder backends.
//!
//! These backends are runnable today, but may delegate to heuristic logic internally.
//! The contract: outputs must make that explicit via provenance.

use anno::Model;

#[test]
fn tplinker_sets_provenance_source() {
    let model = anno::backends::tplinker::TPLinker::new().expect("TPLinker");
    let entities = model
        .extract_entities("Dr. John Smith works at Google.", None)
        .expect("extract");

    for e in &entities {
        let prov = e.provenance.as_ref().expect("provenance");
        assert_eq!(prov.source.as_ref(), "tplinker");
        assert_eq!(prov.method, anno::ExtractionMethod::Heuristic);
        assert_eq!(prov.model_version.as_deref(), Some("heuristic"));
    }
}

#[test]
#[cfg(feature = "burn")]
fn burn_ner_errors_explicitly_instead_of_silent_fallback() {
    let model = anno::backends::burn::BurnNER::new().expect("BurnNER");
    let err = model
        .extract_entities("Dr. John Smith works at Google.", None)
        .expect_err("BurnNER should not silently fall back");
    assert!(
        matches!(err, anno::Error::ModelInit(_)),
        "Expected ModelInit error, got: {:?}",
        err
    );

    let model = anno::backends::burn::BurnNER::from_pretrained("dslim/bert-base-NER")
        .expect("BurnNER::from_pretrained");
    let err = model
        .extract_entities("Marie Curie won the Nobel Prize in Paris.", None)
        .expect_err("BurnNER is scaffolding-only today");
    assert!(
        matches!(err, anno::Error::FeatureNotAvailable(_)),
        "Expected FeatureNotAvailable error, got: {:?}",
        err
    );
}
