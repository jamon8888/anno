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
//!
//! The encoder smoke test (M3.2) is moved into the engine integration
//! test landing in M4 — at this layer we can't call the private
//! `anno::backends::hf_loader::hf_api`.

#![cfg(feature = "gliner2-fastino-candle")]

// M4 will re-add a `gliner2_fastino_candle_smoke` test once the
// `GLiNER2FastinoCandle::from_pretrained` constructor exists.
