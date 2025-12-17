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
        assert_eq!(prov.model_version.as_deref(), Some("placeholder"));
    }
}

#[test]
#[cfg(feature = "burn")]
fn burn_ner_sets_provenance_source() {
    let model = anno::backends::burn::BurnNER::new().expect("BurnNER");
    let entities = model
        .extract_entities("Dr. John Smith works at Google.", None)
        .expect("extract");

    for e in &entities {
        let prov = e.provenance.as_ref().expect("provenance");
        assert_eq!(prov.source.as_ref(), "burn_ner");
        assert_eq!(prov.method, anno::ExtractionMethod::Heuristic);
        assert_eq!(prov.model_version.as_deref(), Some("placeholder"));
    }
}
