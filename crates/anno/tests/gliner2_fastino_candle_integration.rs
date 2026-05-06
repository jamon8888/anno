//! Phase 4 integration tests against `fastino/gliner2-multi-v1` (the
//! PyTorch repo, not the SemplificaAI ONNX export).
//!
//! All tests are `#[ignore]`-gated because they require the model
//! cached locally (~280M base, plus tokenizer/config). Run with:
//!
//! ```bash
//! cargo test -p anno --features gliner2-fastino-candle \
//!     --test gliner2_fastino_candle_integration -- --ignored --nocapture
//! ```

#![cfg(feature = "gliner2-fastino-candle")]

use anno::backends::gliner2_fastino::GLiNER2Fastino;
use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
use anno::backends::inference::ZeroShotNER;

#[test]
#[ignore]
fn from_pretrained_smoke() {
    // Phase 4 M4 smoke test. Download fastino/gliner2-multi-v1 from HF
    // Hub, instantiate the engine, verify the encoder loaded successfully.
    // No inference — that's M5+M6's parity test.
    let model = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
        .expect("load fastino/gliner2-multi-v1");
    eprintln!("loaded: {model:?}");
    assert_eq!(model.active_adapter(), None, "no adapter active on fresh load");
}

#[test]
#[ignore]
fn parity_onnx_candle_extract_with_types() {
    let onnx = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
        .expect("load ONNX");
    let candle = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
        .expect("load Candle");

    let text = "Marie Curie won the Nobel Prize in Physics in 1903.";
    let types = ["person", "award", "year"];

    let onnx_result = ZeroShotNER::extract_with_types(&onnx, text, &types, 0.5)
        .expect("ONNX extract");
    let candle_result = ZeroShotNER::extract_with_types(&candle, text, &types, 0.5)
        .expect("Candle extract");

    eprintln!("onnx ({}): {:#?}", onnx_result.len(), onnx_result);
    eprintln!("candle ({}): {:#?}", candle_result.len(), candle_result);

    let mut onnx_sorted = onnx_result.clone();
    let mut candle_sorted = candle_result.clone();
    onnx_sorted.sort_by_key(|e| (e.start(), e.end(), e.text.clone()));
    candle_sorted.sort_by_key(|e| (e.start(), e.end(), e.text.clone()));

    assert_eq!(
        onnx_sorted.len(),
        candle_sorted.len(),
        "entity count mismatch: onnx={}, candle={}",
        onnx_sorted.len(),
        candle_sorted.len()
    );

    let mut max_diff: f64 = 0.0;
    for (o, c) in onnx_sorted.iter().zip(candle_sorted.iter()) {
        assert_eq!(o.start(), c.start(), "start mismatch: {o:?} vs {c:?}");
        assert_eq!(o.end(), c.end(), "end mismatch: {o:?} vs {c:?}");
        assert_eq!(o.text, c.text, "text mismatch: {o:?} vs {c:?}");
        let diff = (o.confidence.value() - c.confidence.value()).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    eprintln!("ONNX↔Candle max_abs_diff: {max_diff}");
    assert!(
        max_diff < 5e-3,
        "parity broken: max_abs_diff = {max_diff} > 5e-3"
    );
}
