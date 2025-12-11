//! # anno-models
//!
//! Runtime-agnostic ML model backends for Named Entity Recognition.
//!
//! See [README](https://github.com/arclabs561/anno/tree/main/anno-models) for status.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        Model Layer                          │
//! │  GLiNER<R>  │  NuNER<R>  │  BiLSTM-CRF<R>  │  CRF<R>  │ ... │
//! └─────────────────────────────────────────────────────────────┘
//!                               │
//!                    parameterized by Runtime
//!                               │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                       Runtime Layer                         │
//! │     OnnxRuntime     │    CandleRuntime    │   BurnRuntime   │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Current State
//!
//! - **Traits**: `Model`, `Runtime` - defined and stable
//! - **Configs**: `GLiNERConfig`, `NuNERConfig` - match anno patterns  
//! - **Runtimes**: Placeholder implementations (real backends in anno)
//!
//! New backends should implement these traits. Existing backends in
//! `anno/src/backends/` will be migrated incrementally.

#![warn(missing_docs)]

pub mod config;
pub mod model;
pub mod runtime;

#[cfg(feature = "onnx")]
pub mod onnx;

#[cfg(feature = "candle")]
pub mod candle;

#[cfg(feature = "burn")]
pub mod burn;

// Re-exports
pub use config::{GLiNERConfig, NuNERConfig};
pub use model::{Model, ModelConfig, ModelError, ModelInfo};
pub use runtime::{Device, Runtime, RuntimeError, Tensor};

#[cfg(feature = "onnx")]
pub use onnx::OnnxRuntime;

#[cfg(feature = "candle")]
pub use candle::CandleRuntime;

#[cfg(feature = "burn")]
pub use burn::BurnRuntime;
