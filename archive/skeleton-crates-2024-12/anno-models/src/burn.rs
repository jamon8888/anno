//! Burn Runtime backend.
//!
//! This module provides the Burn implementation of the `Runtime` trait.
//! Burn supports WebGPU and is designed for both training and inference.

use crate::runtime::{Device, ModelHandle, Runtime, RuntimeError, Tensor};
use std::path::Path;

/// A tensor backed by Burn.
#[derive(Debug, Clone)]
pub struct BurnTensor {
    /// Shape of the tensor.
    shape: Vec<usize>,
    /// Data (f32 for now).
    data: Vec<f32>,
}

impl Tensor for BurnTensor {
    type Scalar = f32;

    fn shape(&self) -> &[usize] {
        &self.shape
    }

    fn to_vec(&self) -> Vec<Self::Scalar> {
        self.data.clone()
    }
}

/// Burn Runtime backend.
#[derive(Debug, Clone)]
pub struct BurnRuntime {
    device: Device,
}

impl BurnRuntime {
    /// Create a new Burn runtime on CPU.
    pub fn new() -> Result<Self, RuntimeError> {
        Ok(Self {
            device: Device::Cpu,
        })
    }

    /// Create a new Burn runtime on WebGPU.
    pub fn webgpu() -> Result<Self, RuntimeError> {
        Ok(Self {
            device: Device::WebGpu,
        })
    }

    /// Create a new Burn runtime on the specified device.
    pub fn with_device(device: Device) -> Result<Self, RuntimeError> {
        Ok(Self { device })
    }
}

impl Default for BurnRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create Burn runtime")
    }
}

impl Runtime for BurnRuntime {
    type Tensor = BurnTensor;
    type Error = RuntimeError;

    fn name(&self) -> &'static str {
        "Burn"
    }

    fn device(&self) -> Device {
        self.device
    }

    fn is_available() -> bool {
        cfg!(feature = "burn")
    }

    fn tensor_from_slice<T: Clone>(
        &self,
        _data: &[T],
        shape: &[usize],
    ) -> Result<Self::Tensor, Self::Error> {
        Ok(BurnTensor {
            shape: shape.to_vec(),
            data: vec![0.0; shape.iter().product()],
        })
    }

    fn zeros(&self, shape: &[usize]) -> Result<Self::Tensor, Self::Error> {
        Ok(BurnTensor {
            shape: shape.to_vec(),
            data: vec![0.0; shape.iter().product()],
        })
    }

    fn ones(&self, shape: &[usize]) -> Result<Self::Tensor, Self::Error> {
        Ok(BurnTensor {
            shape: shape.to_vec(),
            data: vec![1.0; shape.iter().product()],
        })
    }

    fn matmul(&self, a: &Self::Tensor, b: &Self::Tensor) -> Result<Self::Tensor, Self::Error> {
        if a.shape.len() != 2 || b.shape.len() != 2 {
            return Err(RuntimeError::Unsupported("Only 2D matmul supported".into()));
        }
        if a.shape[1] != b.shape[0] {
            return Err(RuntimeError::ShapeMismatch {
                expected: vec![a.shape[0], a.shape[1], b.shape[0], b.shape[1]],
                actual: vec![],
            });
        }
        Ok(BurnTensor {
            shape: vec![a.shape[0], b.shape[1]],
            data: vec![0.0; a.shape[0] * b.shape[1]],
        })
    }

    fn softmax(&self, x: &Self::Tensor, _dim: i64) -> Result<Self::Tensor, Self::Error> {
        Ok(x.clone())
    }

    fn sigmoid(&self, x: &Self::Tensor) -> Result<Self::Tensor, Self::Error> {
        let data: Vec<f32> = x.data.iter().map(|&v| 1.0 / (1.0 + (-v).exp())).collect();
        Ok(BurnTensor {
            shape: x.shape.clone(),
            data,
        })
    }

    fn load_model(&self, path: &Path) -> Result<ModelHandle, Self::Error> {
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
        Err(RuntimeError::Unsupported(
            "Full Burn inference not yet implemented in anno-models".into(),
        ))
    }
}
