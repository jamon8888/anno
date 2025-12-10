//! Runtime abstraction for ML inference.
//!
//! The `Runtime` trait provides a unified interface for executing ML models
//! across different backends (ONNX, Candle, Burn).

use std::fmt::Debug;
use std::path::Path;

/// Error type for runtime operations.
#[derive(Debug, Clone)]
pub enum RuntimeError {
    /// Model loading failed.
    LoadError(String),
    /// Inference failed.
    InferenceError(String),
    /// Device not available.
    DeviceUnavailable(String),
    /// Shape mismatch.
    ShapeMismatch {
        /// Expected shape.
        expected: Vec<usize>,
        /// Actual shape.
        actual: Vec<usize>,
    },
    /// Unsupported operation.
    Unsupported(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadError(msg) => write!(f, "Load error: {}", msg),
            Self::InferenceError(msg) => write!(f, "Inference error: {}", msg),
            Self::DeviceUnavailable(msg) => write!(f, "Device unavailable: {}", msg),
            Self::ShapeMismatch { expected, actual } => {
                write!(
                    f,
                    "Shape mismatch: expected {:?}, got {:?}",
                    expected, actual
                )
            }
            Self::Unsupported(msg) => write!(f, "Unsupported: {}", msg),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Compute device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Device {
    /// CPU execution.
    Cpu,
    /// CUDA GPU with device index.
    Cuda(usize),
    /// Metal GPU (Apple Silicon).
    Metal,
    /// WebGPU.
    WebGpu,
}

impl Default for Device {
    fn default() -> Self {
        Self::Cpu
    }
}

impl Device {
    /// Check if this is a GPU device.
    pub fn is_gpu(&self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

/// Abstract tensor type.
///
/// This is a placeholder for runtime-specific tensor types.
/// Each runtime implements this differently.
pub trait Tensor: Clone + Debug + Send + Sync {
    /// Scalar type.
    type Scalar;

    /// Get the shape of the tensor.
    fn shape(&self) -> &[usize];

    /// Get the number of dimensions.
    fn ndim(&self) -> usize {
        self.shape().len()
    }

    /// Get the total number of elements.
    fn numel(&self) -> usize {
        self.shape().iter().product()
    }

    /// Convert to a flat Vec.
    fn to_vec(&self) -> Vec<Self::Scalar>;
}

/// ML runtime for executing models.
///
/// This trait abstracts the differences between ONNX Runtime, Candle, and Burn,
/// allowing models to be written once and run on any backend.
pub trait Runtime: Clone + Debug + Send + Sync {
    /// Tensor type for this runtime.
    type Tensor: Tensor;

    /// Error type for this runtime.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Get the name of this runtime.
    fn name(&self) -> &'static str;

    /// Get the device this runtime is using.
    fn device(&self) -> Device;

    /// Check if this runtime is available (dependencies installed, device accessible).
    fn is_available() -> bool;

    /// Create a tensor from a slice.
    fn tensor_from_slice<T: Clone>(
        &self,
        data: &[T],
        shape: &[usize],
    ) -> Result<Self::Tensor, Self::Error>;

    /// Create a zeros tensor.
    fn zeros(&self, shape: &[usize]) -> Result<Self::Tensor, Self::Error>;

    /// Create a ones tensor.
    fn ones(&self, shape: &[usize]) -> Result<Self::Tensor, Self::Error>;

    /// Matrix multiplication.
    fn matmul(&self, a: &Self::Tensor, b: &Self::Tensor) -> Result<Self::Tensor, Self::Error>;

    /// Softmax along a dimension.
    fn softmax(&self, x: &Self::Tensor, dim: i64) -> Result<Self::Tensor, Self::Error>;

    /// Sigmoid activation.
    fn sigmoid(&self, x: &Self::Tensor) -> Result<Self::Tensor, Self::Error>;

    /// Load a model from a path.
    ///
    /// Returns an opaque handle that can be used with `run_model`.
    fn load_model(&self, path: &Path) -> Result<ModelHandle, Self::Error>;

    /// Run a loaded model.
    fn run_model(
        &self,
        handle: &ModelHandle,
        inputs: &[(&str, Self::Tensor)],
    ) -> Result<Vec<Self::Tensor>, Self::Error>;
}

/// Opaque handle to a loaded model.
///
/// This is runtime-specific and should not be inspected directly.
#[derive(Debug, Clone)]
pub struct ModelHandle {
    /// Runtime-specific identifier.
    pub id: usize,
    /// Path the model was loaded from.
    pub path: String,
    /// Input names.
    pub input_names: Vec<String>,
    /// Output names.
    pub output_names: Vec<String>,
}

/// Extension trait for runtimes that support HuggingFace Hub downloads.
pub trait HubDownload: Runtime {
    /// Download a model from HuggingFace Hub.
    fn download_from_hub(
        &self,
        repo_id: &str,
        filename: &str,
    ) -> Result<std::path::PathBuf, Self::Error>;
}

/// Extension trait for runtimes that support GPU operations.
pub trait GpuRuntime: Runtime {
    /// Move a tensor to GPU.
    fn to_gpu(&self, tensor: &Self::Tensor) -> Result<Self::Tensor, Self::Error>;

    /// Move a tensor to CPU.
    fn to_cpu(&self, tensor: &Self::Tensor) -> Result<Self::Tensor, Self::Error>;

    /// Synchronize GPU operations.
    fn synchronize(&self) -> Result<(), Self::Error>;
}

/// Extension trait for runtimes that support training (autograd).
pub trait TrainableRuntime: Runtime {
    /// Enable gradient tracking for a tensor.
    fn requires_grad(&self, tensor: &Self::Tensor) -> Result<Self::Tensor, Self::Error>;

    /// Compute gradients via backpropagation.
    fn backward(&self, loss: &Self::Tensor) -> Result<(), Self::Error>;

    /// Zero all gradients.
    fn zero_grad(&self) -> Result<(), Self::Error>;
}
