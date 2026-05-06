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

use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;

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
