//! ONNX Runtime backend.
//!
//! This module provides the ONNX Runtime implementation of the `Runtime` trait.
//! ONNX Runtime is recommended for production deployments due to its maturity
//! and broad hardware support.

use crate::runtime::{Device, ModelHandle, Runtime, RuntimeError, Tensor};
use std::path::Path;

/// A tensor backed by ONNX Runtime.
#[derive(Debug, Clone)]
pub struct OnnxTensor {
    /// Shape of the tensor.
    shape: Vec<usize>,
    /// Data (f32 for now).
    data: Vec<f32>,
}

impl Tensor for OnnxTensor {
    type Scalar = f32;

    fn shape(&self) -> &[usize] {
        &self.shape
    }

    fn to_vec(&self) -> Vec<Self::Scalar> {
        self.data.clone()
    }
}

/// ONNX Runtime backend.
#[derive(Debug, Clone)]
pub struct OnnxRuntime {
    device: Device,
}

impl OnnxRuntime {
    /// Create a new ONNX runtime on CPU.
    pub fn new() -> Result<Self, RuntimeError> {
        Ok(Self {
            device: Device::Cpu,
        })
    }

    /// Create a new ONNX runtime on the specified device.
    pub fn with_device(device: Device) -> Result<Self, RuntimeError> {
        // ONNX Runtime supports CUDA but not Metal directly
        match device {
            Device::Metal => {
                return Err(RuntimeError::DeviceUnavailable(
                    "ONNX Runtime does not support Metal. Use CoreML or Candle.".into(),
                ));
            }
            Device::WebGpu => {
                return Err(RuntimeError::DeviceUnavailable(
                    "ONNX Runtime does not support WebGPU. Use Burn.".into(),
                ));
            }
            _ => {}
        }
        Ok(Self { device })
    }
}

impl Default for OnnxRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create ONNX runtime")
    }
}

impl Runtime for OnnxRuntime {
    type Tensor = OnnxTensor;
    type Error = RuntimeError;

    fn name(&self) -> &'static str {
        "ONNX Runtime"
    }

    fn device(&self) -> Device {
        self.device
    }

    fn is_available() -> bool {
        // Check if ort is usable
        cfg!(feature = "onnx")
    }

    fn tensor_from_slice<T: Clone>(
        &self,
        _data: &[T],
        shape: &[usize],
    ) -> Result<Self::Tensor, Self::Error> {
        // Placeholder - real implementation would use ort
        Ok(OnnxTensor {
            shape: shape.to_vec(),
            data: vec![0.0; shape.iter().product()],
        })
    }

    fn zeros(&self, shape: &[usize]) -> Result<Self::Tensor, Self::Error> {
        Ok(OnnxTensor {
            shape: shape.to_vec(),
            data: vec![0.0; shape.iter().product()],
        })
    }

    fn ones(&self, shape: &[usize]) -> Result<Self::Tensor, Self::Error> {
        Ok(OnnxTensor {
            shape: shape.to_vec(),
            data: vec![1.0; shape.iter().product()],
        })
    }

    fn matmul(&self, a: &Self::Tensor, b: &Self::Tensor) -> Result<Self::Tensor, Self::Error> {
        // Placeholder - real implementation would use ort
        if a.shape.len() != 2 || b.shape.len() != 2 {
            return Err(RuntimeError::Unsupported("Only 2D matmul supported".into()));
        }
        if a.shape[1] != b.shape[0] {
            return Err(RuntimeError::ShapeMismatch {
                expected: vec![a.shape[0], a.shape[1], b.shape[0], b.shape[1]],
                actual: vec![],
            });
        }
        Ok(OnnxTensor {
            shape: vec![a.shape[0], b.shape[1]],
            data: vec![0.0; a.shape[0] * b.shape[1]],
        })
    }

    fn softmax(&self, x: &Self::Tensor, _dim: i64) -> Result<Self::Tensor, Self::Error> {
        // Placeholder
        Ok(x.clone())
    }

    fn sigmoid(&self, x: &Self::Tensor) -> Result<Self::Tensor, Self::Error> {
        let data: Vec<f32> = x.data.iter().map(|&v| 1.0 / (1.0 + (-v).exp())).collect();
        Ok(OnnxTensor {
            shape: x.shape.clone(),
            data,
        })
    }

    fn load_model(&self, path: &Path) -> Result<ModelHandle, Self::Error> {
        // Placeholder - real implementation would use ort::Session
        Ok(ModelHandle {
            id: 0,
            path: path.to_string_lossy().into_owned(),
            input_names: vec!["input_ids".into(), "attention_mask".into()],
            output_names: vec!["logits".into()],
        })
    }

    fn run_model(
        &self,
        _handle: &ModelHandle,
        _inputs: &[(&str, Self::Tensor)],
    ) -> Result<Vec<Self::Tensor>, Self::Error> {
        // Placeholder - real implementation would run ort::Session
        Err(RuntimeError::Unsupported(
            "Full ONNX inference not yet implemented in anno-models".into(),
        ))
    }
}
