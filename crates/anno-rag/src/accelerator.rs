//! Runtime accelerator selection and diagnostics.

use crate::error::{Error, Result};
use candle_core::Device;
use serde::{Deserialize, Serialize};
use std::env::VarError;
use std::fmt;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant};

const CUDA_RUNTIME_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Runtime accelerator preference requested by config or environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceleratorPreference {
    /// Prefer the best available compiled accelerator, falling back to CPU.
    Auto,
    /// Force CPU execution.
    Cpu,
    /// Force Apple Metal execution where compiled and available.
    Metal,
    /// Force NVIDIA CUDA execution where compiled and available.
    Cuda,
}

impl AcceleratorPreference {
    /// Parse an environment/config value, accepting known values case-insensitively.
    #[must_use]
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "cpu" => Some(Self::Cpu),
            "metal" => Some(Self::Metal),
            "cuda" => Some(Self::Cuda),
            _ => None,
        }
    }

    /// Read `ANNO_ACCELERATOR`, falling back to `default` when it is not present.
    ///
    /// # Errors
    /// Returns [`Error::Config`] when the variable is not valid Unicode or contains
    /// a value other than `auto`, `cpu`, `metal`, or `cuda`.
    pub fn from_env_or(default: Self) -> Result<Self> {
        match std::env::var("ANNO_ACCELERATOR") {
            Ok(value) => Self::from_env_value(&value).ok_or_else(|| {
                Error::Config(format!(
                    "invalid ANNO_ACCELERATOR value '{value}'; expected auto, cpu, metal, or cuda"
                ))
            }),
            Err(VarError::NotPresent) => Ok(default),
            Err(err) => Err(Error::Config(format!("read ANNO_ACCELERATOR: {err}"))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Metal => "metal",
            Self::Cuda => "cuda",
        }
    }
}

impl Default for AcceleratorPreference {
    fn default() -> Self {
        Self::Auto
    }
}

impl FromStr for AcceleratorPreference {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::from_env_value(value).ok_or_else(|| {
            Error::Config(format!(
                "invalid accelerator preference '{value}'; expected auto, cpu, metal, or cuda"
            ))
        })
    }
}

impl fmt::Display for AcceleratorPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Accelerator selected for the current process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectedAccelerator {
    /// CPU execution.
    Cpu,
    /// Apple Metal execution.
    Metal,
    /// NVIDIA CUDA execution.
    Cuda,
}

impl SelectedAccelerator {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Metal => "metal",
            Self::Cuda => "cuda",
        }
    }
}

impl fmt::Display for SelectedAccelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Build-time accelerator support compiled into this binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CompiledAccelerators {
    /// Whether Apple Metal support was compiled in.
    pub metal: bool,
    /// Whether NVIDIA CUDA support was compiled in.
    pub cuda: bool,
}

/// Accelerator resolution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceleratorDecision {
    /// Preference requested after config and environment override handling.
    pub requested: AcceleratorPreference,
    /// Accelerator selected for runtime use.
    pub selected: SelectedAccelerator,
    /// Build-time accelerator support.
    pub compiled: CompiledAccelerators,
    /// Human-readable reason when resolution fell back to CPU.
    pub fallback_reason: Option<String>,
}

/// Serializable accelerator diagnostics for status and troubleshooting surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceleratorDiagnostics {
    /// Cargo target triple when available.
    pub target: &'static str,
    /// Preference requested after config and environment override handling.
    pub requested: AcceleratorPreference,
    /// Build-time accelerator support.
    pub compiled: CompiledAccelerators,
    /// Accelerator selected for runtime use.
    pub selected: SelectedAccelerator,
    /// Candle embedder device label.
    pub embedder_device: &'static str,
    /// Detector provider label.
    pub detector_provider: &'static str,
    /// Human-readable reason when resolution fell back to CPU.
    pub fallback_reason: Option<String>,
}

/// Return the accelerators compiled into this binary.
#[must_use]
pub fn compiled_accelerators() -> CompiledAccelerators {
    CompiledAccelerators {
        metal: cfg!(feature = "gpu-metal"),
        cuda: cfg!(feature = "gpu-cuda"),
    }
}

/// Resolve the requested accelerator to a concrete runtime selection.
///
/// # Errors
/// Returns [`Error::Config`] when an explicit accelerator is requested but was
/// not compiled in or is not available on the host.
pub fn resolve(preference: AcceleratorPreference) -> Result<AcceleratorDecision> {
    let compiled = compiled_accelerators();

    match preference {
        AcceleratorPreference::Cpu => Ok(AcceleratorDecision {
            requested: preference,
            selected: SelectedAccelerator::Cpu,
            compiled,
            fallback_reason: None,
        }),
        AcceleratorPreference::Metal => {
            if !compiled.metal {
                return Err(Error::Config(
                    "accelerator 'metal' requested but this binary was not built with gpu-metal"
                        .to_string(),
                ));
            }

            candle_metal_device()?;
            Ok(AcceleratorDecision {
                requested: preference,
                selected: SelectedAccelerator::Metal,
                compiled,
                fallback_reason: None,
            })
        }
        AcceleratorPreference::Cuda => {
            if !compiled.cuda {
                return Err(Error::Config(
                    "accelerator 'cuda' requested but this binary was not built with gpu-cuda"
                        .to_string(),
                ));
            }

            if !cuda_runtime_available() {
                return Err(Error::Config(
                    "accelerator 'cuda' requested but nvidia-smi -L did not succeed".to_string(),
                ));
            }

            Ok(AcceleratorDecision {
                requested: preference,
                selected: SelectedAccelerator::Cuda,
                compiled,
                fallback_reason: None,
            })
        }
        AcceleratorPreference::Auto => resolve_auto(compiled),
    }
}

/// Return the Candle device for the selected accelerator.
///
/// # Errors
/// Returns [`Error::Config`] if Metal was selected but the Metal device can no
/// longer be opened.
pub fn candle_device(decision: &AcceleratorDecision) -> Result<Device> {
    match decision.selected {
        SelectedAccelerator::Cpu => Ok(Device::Cpu),
        SelectedAccelerator::Metal => candle_metal_device(),
        SelectedAccelerator::Cuda => {
            // Windows CUDA V1 accelerates the ONNX detector only. Keep the Candle
            // embedder on CPU until Candle CUDA is validated.
            Ok(Device::Cpu)
        }
    }
}

/// Return a stable device label for diagnostics.
#[must_use]
pub fn device_label(device: &Device) -> &'static str {
    match device {
        Device::Cpu => "cpu",
        Device::Metal(_) => "metal",
        Device::Cuda(_) => "cuda",
    }
}

/// Build accelerator diagnostics using `ANNO_ACCELERATOR` over `default`.
///
/// # Errors
/// Returns [`Error::Config`] when the environment value is invalid or an
/// explicit accelerator cannot be selected.
pub fn diagnostics(default: AcceleratorPreference) -> Result<AcceleratorDiagnostics> {
    let requested = AcceleratorPreference::from_env_or(default)?;
    let decision = resolve(requested)?;
    let embedder_device = candle_device(&decision)?;

    Ok(AcceleratorDiagnostics {
        target: option_env!("TARGET").unwrap_or("unknown"),
        requested,
        compiled: decision.compiled,
        selected: decision.selected,
        embedder_device: device_label(&embedder_device),
        detector_provider: detector_provider_label(&decision),
        fallback_reason: decision.fallback_reason,
    })
}

/// Return the detector provider label for diagnostics.
#[must_use]
pub fn detector_provider_label(decision: &AcceleratorDecision) -> &'static str {
    match decision.selected {
        SelectedAccelerator::Cpu => "onnx-cpu",
        SelectedAccelerator::Metal => "candle-metal",
        SelectedAccelerator::Cuda => "onnx-cuda",
    }
}

fn resolve_auto(compiled: CompiledAccelerators) -> Result<AcceleratorDecision> {
    let mut unavailable = Vec::new();

    if compiled.metal {
        match candle_metal_device() {
            Ok(_) => {
                return Ok(AcceleratorDecision {
                    requested: AcceleratorPreference::Auto,
                    selected: SelectedAccelerator::Metal,
                    compiled,
                    fallback_reason: None,
                });
            }
            Err(err) => unavailable.push(format!("metal unavailable ({err})")),
        }
    } else {
        unavailable.push("gpu-metal not compiled".to_string());
    }

    if compiled.cuda {
        if cuda_runtime_available() {
            return Ok(AcceleratorDecision {
                requested: AcceleratorPreference::Auto,
                selected: SelectedAccelerator::Cuda,
                compiled,
                fallback_reason: None,
            });
        }
        unavailable.push("cuda unavailable (nvidia-smi -L did not succeed)".to_string());
    } else {
        unavailable.push("gpu-cuda not compiled".to_string());
    }

    Ok(AcceleratorDecision {
        requested: AcceleratorPreference::Auto,
        selected: SelectedAccelerator::Cpu,
        compiled,
        fallback_reason: Some(format!("auto selected cpu: {}", unavailable.join("; "))),
    })
}

#[cfg(all(target_os = "macos", feature = "gpu-metal"))]
fn candle_metal_device() -> Result<Device> {
    Device::new_metal(0).map_err(|err| Error::Config(format!("Metal device unavailable: {err}")))
}

#[cfg(not(all(target_os = "macos", feature = "gpu-metal")))]
fn candle_metal_device() -> Result<Device> {
    Err(Error::Config(
        "gpu-metal Metal runtime is only available on macOS builds".to_string(),
    ))
}

fn cuda_runtime_available() -> bool {
    let mut child = match Command::new("nvidia-smi")
        .arg("-L")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    let deadline = Instant::now() + CUDA_RUNTIME_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_preferences() {
        assert_eq!(
            AcceleratorPreference::from_env_value("auto"),
            Some(AcceleratorPreference::Auto)
        );
        assert_eq!(
            AcceleratorPreference::from_env_value(" CPU "),
            Some(AcceleratorPreference::Cpu)
        );
        assert_eq!(
            AcceleratorPreference::from_env_value("MeTaL"),
            Some(AcceleratorPreference::Metal)
        );
        assert_eq!(
            "cuda".parse::<AcceleratorPreference>().expect("parse cuda"),
            AcceleratorPreference::Cuda
        );
        assert_eq!(AcceleratorPreference::Metal.to_string(), "metal");
    }

    #[test]
    fn rejects_invalid_preference() {
        assert_eq!(AcceleratorPreference::from_env_value("gpu"), None);
        let err = "gpu"
            .parse::<AcceleratorPreference>()
            .expect_err("invalid preference must fail");

        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn cpu_resolves_without_gpu_features() {
        let decision = resolve(AcceleratorPreference::Cpu).expect("cpu resolves");

        assert_eq!(decision.requested, AcceleratorPreference::Cpu);
        assert_eq!(decision.selected, SelectedAccelerator::Cpu);
        assert_eq!(decision.compiled, compiled_accelerators());
        assert_eq!(decision.fallback_reason, None);
        let device = candle_device(&decision).expect("cpu device");
        assert_eq!(device_label(&device), "cpu");
    }

    #[test]
    fn explicit_cuda_errors_when_not_compiled() {
        if cfg!(feature = "gpu-cuda") {
            return;
        }

        let err = resolve(AcceleratorPreference::Cuda).expect_err("cuda should fail");
        let message = err.to_string();

        assert!(matches!(err, Error::Config(_)));
        assert!(message.contains("gpu-cuda"));
    }
}
