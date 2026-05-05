//! Tier-2 integration tests for `gliner2_fastino`. `#[ignore]`-gated.
//!
//! **Status (2026-05-05): these tests will FAIL against the documented
//! `SemplificaAI/gliner2-multi-v1-onnx` pin and against
//! `fastino/gliner2-multi-v1` directly.** The fastino reference exports
//! are 5-graph pipelines (encoder + span_rep + scorer + count_pred +
//! classifier as separate ONNX files), but Phase 1 of the gliner2_fastino
//! backend implements a single-graph load path. See the ARCHITECTURAL
//! FINDING comment in `crates/anno/src/backends/gliner2_fastino/mod.rs`
//! for the full discovery and the Phase 3 scope that delivers the
//! multi-session IOBinding chain.
//!
//! Until Phase 3 lands (or someone produces a unified-graph fastino ONNX
//! via `scripts/gliner2_export_onnx.py`), these tests are placeholders
//! demonstrating the eventual API surface. Running them surfaces the
//! "multi-graph fastino export" error from `from_pretrained`/`from_local`.
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
    let model = GLiNER2Fastino::from_pretrained("fastino/gliner2-multi-v1")
        .expect("load fastino/gliner2-multi-v1");
    let ents = model
        .extract_with_types(FIXTURE, &["organization", "location"], 0.5)
        .expect("extract");

    assert!(
        ents.iter().any(|e| e.text == "Acme Corp"
            || e.text == "Acme"
            || e.text == "Acme Corp signed"),
        "expected at least 'Acme Corp' organization, got {ents:#?}"
    );
    assert!(
        ents.iter().any(|e| e.text == "Paris"),
        "expected 'Paris' location, got {ents:#?}"
    );
}

#[test]
#[ignore]
fn fastino_classify_smoke() {
    let model = GLiNER2Fastino::from_pretrained("fastino/gliner2-multi-v1")
        .expect("load");
    let scores = model
        .classify(
            "This product is wonderful, I love it.",
            &["positive", "negative"],
            0.0,
        )
        .expect("classify");
    assert_eq!(scores.len(), 2);
    // Phase 1 classify uses NER-head approximation (see classify rustdoc);
    // assert only that the call returns a stable shape, not specific values.
}

#[test]
#[ignore]
fn semplifica_external_pin_loads() {
    // Sanity check that the docs' fast path (SemplificaAI/gliner2-multi-v1-onnx)
    // still resolves. If this fails, the docs need updating — not the code.
    let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("SemplificaAI pin failed — check repo availability");
    let _ = model
        .extract_with_types(FIXTURE, &["organization"], 0.5)
        .expect("extract");
}
