# Apple and Windows GPU Builds Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit Apple Silicon Metal and Windows NVIDIA CUDA sidecar builds for `anno-rag` while preserving the current CPU default release path.

**Architecture:** Add a small runtime accelerator layer in `anno-rag`, propagate opt-in Cargo features through `anno-rag`, `anno-rag-mcp`, and `anno-rag-bin`, then wire device/provider selection into the lazy embedder and detector initialization paths. Package GPU artifacts in a separate manual release workflow so CPU cargo-dist/installers remain stable.

**Tech Stack:** Rust workspace, Candle `Device`, ONNX Runtime via `ort`, clap CLI, serde diagnostics, GitHub Actions, existing `scripts/release/*` packaging style.

---

## Source Spec

Design doc: `docs/superpowers/specs/2026-06-01-anno-apple-windows-gpu-builds-design.md`

Important constraints:

- CPU behavior stays default and unchanged.
- `ANNO_ACCELERATOR=auto|cpu|metal|cuda` is the runtime interface.
- Explicit `metal` / `cuda` requests fail loudly when unavailable or not compiled in.
- Only `auto` may fall back to CPU.
- Windows V1 is NVIDIA CUDA, not DirectML.
- Apple V1 is Apple Silicon Metal.
- GPU release jobs stay manual/tag-only because GitHub Actions billing was blocked on 2026-06-01 and GPU minutes are expensive.

## File Structure

Create:

- `crates/anno-rag/src/accelerator.rs` - shared accelerator preference parsing, compile/runtime capability detection, diagnostics structs, Candle device selection helpers.
- `.github/workflows/release-accelerated.yml` - manual/tag-only sidecar release workflow.
- `scripts/release/package-accelerated-windows.ps1` - Windows CUDA archive packaging with `-Flavor cuda`.
- `scripts/release/package-accelerated-unix.sh` - macOS Metal archive packaging with flavor suffix.
- `scripts/release/checksums-accelerated.sh` - checksum file for accelerated archives.
- `docs/release/accelerated-gpu-builds.md` - user-facing install and diagnostics docs.

Modify:

- `crates/anno-rag/Cargo.toml` - add `gpu-metal` and `gpu-cuda` pass-through features.
- `crates/anno-rag-mcp/Cargo.toml` - pass GPU features to `anno-rag`.
- `crates/anno-rag-bin/Cargo.toml` - pass GPU features to `anno-rag` and `anno-rag-mcp`.
- `crates/anno-rag/src/lib.rs` - export the new `accelerator` module.
- `crates/anno-rag/src/config.rs` - add serialized/defaulted accelerator preference to `AnnoRagConfig`.
- `crates/anno-rag/src/embed.rs` - choose Candle device via accelerator decision and expose device label.
- `crates/anno-rag/src/detect.rs` - build GLiNER2 ONNX with CUDA config and add Metal Candle detector backend where compiled.
- `crates/anno-rag/src/pipeline.rs` - pass config into `Detector::new`.
- `crates/anno-rag/src/download_models.rs` - download the Candle GLiNER2 model for Metal offline use.
- `crates/anno-rag-bin/src/main.rs` - add `diagnose-gpu` command before keyring/pipeline initialization.
- `crates/anno/src/backends/gliner2_fastino_candle/mod.rs` - expose public device-aware constructors.
- `README.md` - link the accelerated build docs.

## Task 0: Safety Baseline and Impact Analysis

**Files:**
- Read-only: all files above.

- [ ] **Step 1: Confirm isolated branch state**

Run:

```powershell
git status --short --branch
```

Expected:

```text
## codex/anno-apple-windows-gpu-builds...origin/main [ahead 2]
```

The exact ahead count can be higher if previous plan/spec commits were already made. There must be no unstaged code changes.

- [ ] **Step 2: Build or refresh the GitNexus index for this worktree**

Run:

```powershell
npx gitnexus analyze
npx gitnexus status
```

Expected:

```text
Repository indexed
```

The exact symbol and relationship counts can differ after analysis.

- [ ] **Step 3: Run required upstream impact checks before editing symbols**

Run:

```powershell
npx gitnexus impact Embedder::load --direction upstream --include-tests
npx gitnexus impact Detector::new --direction upstream --include-tests
npx gitnexus impact Pipeline::new --direction upstream --include-tests
npx gitnexus impact GLiNER2FastinoCandle::from_local --direction upstream --include-tests
npx gitnexus impact GLiNER2Fastino::from_local_with_config --direction upstream --include-tests
```

Expected:

```text
Impact analysis completed
```

Record the direct callers and risk level in the session notes. If any result is HIGH or CRITICAL, stop and report the blast radius before editing.

## Task 1: Cargo Feature Pass-Throughs

**Files:**
- Modify: `crates/anno-rag/Cargo.toml`
- Modify: `crates/anno-rag-mcp/Cargo.toml`
- Modify: `crates/anno-rag-bin/Cargo.toml`

- [ ] **Step 1: Verify the feature names fail before implementation**

Run:

```powershell
cargo metadata --no-deps --format-version 1 -p anno-rag-bin --features gpu-cuda
```

Expected: FAIL with a message containing:

```text
does not have the feature `gpu-cuda`
```

- [ ] **Step 2: Add `anno-rag` features**

Edit `crates/anno-rag/Cargo.toml` in `[features]`:

```toml
# Apple Silicon Metal acceleration for Candle-backed embedder/detector paths.
gpu-metal = ["anno/gliner2-fastino-candle-metal", "anno/metal"]
# Windows/Linux NVIDIA CUDA acceleration for ONNX-backed detector paths.
gpu-cuda = ["anno/gliner2-fastino-cuda"]
```

- [ ] **Step 3: Add `anno-rag-mcp` pass-through features**

Edit `crates/anno-rag-mcp/Cargo.toml` in `[features]`:

```toml
gpu-metal = ["anno-rag/gpu-metal"]
gpu-cuda = ["anno-rag/gpu-cuda"]
```

- [ ] **Step 4: Add `anno-rag-bin` pass-through features**

Edit `crates/anno-rag-bin/Cargo.toml` in `[features]`:

```toml
gpu-metal = ["anno-rag/gpu-metal", "anno-rag-mcp/gpu-metal"]
gpu-cuda = ["anno-rag/gpu-cuda", "anno-rag-mcp/gpu-cuda"]
```

- [ ] **Step 5: Verify feature metadata resolves**

Run:

```powershell
cargo metadata --no-deps --format-version 1 -p anno-rag-bin --features gpu-cuda
cargo metadata --no-deps --format-version 1 -p anno-rag-bin --features gpu-metal
```

Expected: both commands exit 0.

- [ ] **Step 6: Commit feature plumbing**

Run:

```powershell
git add crates/anno-rag/Cargo.toml crates/anno-rag-mcp/Cargo.toml crates/anno-rag-bin/Cargo.toml
git commit -m "feat: add anno-rag GPU feature pass-throughs"
```

## Task 2: Accelerator Preference and Diagnostics Core

**Files:**
- Create: `crates/anno-rag/src/accelerator.rs`
- Modify: `crates/anno-rag/src/lib.rs`
- Modify: `crates/anno-rag/src/config.rs`

- [ ] **Step 1: Add the module export first and confirm it fails**

Edit `crates/anno-rag/src/lib.rs`:

```rust
pub mod accelerator;
pub mod bench_cli;
```

Run:

```powershell
cargo test -p anno-rag accelerator --no-default-features
```

Expected: FAIL with a message containing:

```text
file not found for module `accelerator`
```

- [ ] **Step 2: Create the accelerator module**

Create `crates/anno-rag/src/accelerator.rs`:

```rust
//! Runtime accelerator selection for anno-rag.

use crate::error::{Error, Result};
use candle_core::Device;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// User-facing accelerator preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceleratorPreference {
    /// Try the compiled platform accelerator first, then fall back to CPU.
    Auto,
    /// Force CPU.
    Cpu,
    /// Require Apple Metal.
    Metal,
    /// Require NVIDIA CUDA.
    Cuda,
}

impl Default for AcceleratorPreference {
    fn default() -> Self {
        Self::Auto
    }
}

impl AcceleratorPreference {
    /// Parse `ANNO_ACCELERATOR` values.
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "cpu" => Some(Self::Cpu),
            "metal" => Some(Self::Metal),
            "cuda" => Some(Self::Cuda),
            _ => None,
        }
    }

    /// Read `ANNO_ACCELERATOR`, falling back to the supplied config value.
    pub fn from_env_or(default: Self) -> Result<Self> {
        match std::env::var("ANNO_ACCELERATOR") {
            Ok(value) => Self::from_env_value(&value).ok_or_else(|| {
                Error::Config(format!(
                    "ANNO_ACCELERATOR must be one of auto, cpu, metal, cuda; got {value:?}"
                ))
            }),
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(e) => Err(Error::Config(format!("ANNO_ACCELERATOR: {e}"))),
        }
    }
}

impl FromStr for AcceleratorPreference {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::from_env_value(value).ok_or_else(|| {
            Error::Config(format!(
                "accelerator must be one of auto, cpu, metal, cuda; got {value:?}"
            ))
        })
    }
}

impl fmt::Display for AcceleratorPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Metal => "metal",
            Self::Cuda => "cuda",
        };
        f.write_str(value)
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
    /// NVIDIA CUDA detector execution. The embedder remains CPU in V1.
    Cuda,
}

impl fmt::Display for SelectedAccelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Cpu => "cpu",
            Self::Metal => "metal",
            Self::Cuda => "cuda",
        };
        f.write_str(value)
    }
}

/// Compile-time accelerator feature state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CompiledAccelerators {
    /// Whether the `gpu-metal` feature is compiled in.
    pub metal: bool,
    /// Whether the `gpu-cuda` feature is compiled in.
    pub cuda: bool,
}

/// Resolved accelerator selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceleratorDecision {
    /// User/config request after env override.
    pub requested: AcceleratorPreference,
    /// Selected execution path.
    pub selected: SelectedAccelerator,
    /// Compile-time features.
    pub compiled: CompiledAccelerators,
    /// Fallback reason when `auto` selected CPU.
    pub fallback_reason: Option<String>,
}

/// JSON payload for `anno-rag diagnose-gpu`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceleratorDiagnostics {
    /// Current target triple.
    pub target: &'static str,
    /// User/config request after env override.
    pub requested: AcceleratorPreference,
    /// Compile-time features.
    pub compiled: CompiledAccelerators,
    /// Selected accelerator.
    pub selected: SelectedAccelerator,
    /// Selected Candle device label for the embedder.
    pub embedder_device: String,
    /// Expected detector provider/backend label.
    pub detector_provider: String,
    /// Fallback reason when present.
    pub fallback_reason: Option<String>,
}

/// Return compile-time feature state.
pub fn compiled_accelerators() -> CompiledAccelerators {
    CompiledAccelerators {
        metal: cfg!(feature = "gpu-metal"),
        cuda: cfg!(feature = "gpu-cuda"),
    }
}

/// Resolve accelerator preference without loading models.
pub fn resolve(preference: AcceleratorPreference) -> Result<AcceleratorDecision> {
    let compiled = compiled_accelerators();
    match preference {
        AcceleratorPreference::Cpu => Ok(decision(
            preference,
            SelectedAccelerator::Cpu,
            compiled,
            None,
        )),
        AcceleratorPreference::Metal => {
            if !compiled.metal {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=metal requires a binary built with feature gpu-metal".into(),
                ));
            }
            if candle_metal_device().is_err() {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=metal requested, but Candle Metal device 0 is unavailable"
                        .into(),
                ));
            }
            Ok(decision(
                preference,
                SelectedAccelerator::Metal,
                compiled,
                None,
            ))
        }
        AcceleratorPreference::Cuda => {
            if !compiled.cuda {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=cuda requires a binary built with feature gpu-cuda".into(),
                ));
            }
            if !cuda_runtime_available() {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=cuda requested, but nvidia-smi did not report a CUDA GPU"
                        .into(),
                ));
            }
            Ok(decision(
                preference,
                SelectedAccelerator::Cuda,
                compiled,
                None,
            ))
        }
        AcceleratorPreference::Auto => resolve_auto(compiled),
    }
}

fn resolve_auto(compiled: CompiledAccelerators) -> Result<AcceleratorDecision> {
    if compiled.metal && candle_metal_device().is_ok() {
        return Ok(decision(
            AcceleratorPreference::Auto,
            SelectedAccelerator::Metal,
            compiled,
            None,
        ));
    }
    if compiled.cuda && cuda_runtime_available() {
        return Ok(decision(
            AcceleratorPreference::Auto,
            SelectedAccelerator::Cuda,
            compiled,
            None,
        ));
    }
    Ok(decision(
        AcceleratorPreference::Auto,
        SelectedAccelerator::Cpu,
        compiled,
        Some("no compiled accelerator device was available; using CPU".into()),
    ))
}

fn decision(
    requested: AcceleratorPreference,
    selected: SelectedAccelerator,
    compiled: CompiledAccelerators,
    fallback_reason: Option<String>,
) -> AcceleratorDecision {
    AcceleratorDecision {
        requested,
        selected,
        compiled,
        fallback_reason,
    }
}

/// Convert a decision into a Candle device for embedder use.
pub fn candle_device(decision: &AcceleratorDecision) -> Result<Device> {
    match decision.selected {
        SelectedAccelerator::Cpu => Ok(Device::Cpu),
        SelectedAccelerator::Metal => candle_metal_device(),
        // Windows CUDA V1 accelerates the ONNX detector. Keep the Candle
        // embedder on CPU until Candle CUDA is validated on Windows.
        SelectedAccelerator::Cuda => Ok(Device::Cpu),
    }
}

/// Human-readable Candle device label.
pub fn device_label(device: &Device) -> &'static str {
    match device {
        Device::Cpu => "cpu",
        #[cfg(feature = "gpu-metal")]
        Device::Metal(_) => "metal",
        #[cfg(feature = "gpu-cuda")]
        Device::Cuda(_) => "cuda",
        #[allow(unreachable_patterns)]
        _ => "accelerator",
    }
}

/// Build diagnostics without loading model weights.
pub fn diagnostics(default: AcceleratorPreference) -> Result<AcceleratorDiagnostics> {
    let requested = AcceleratorPreference::from_env_or(default)?;
    let decision = resolve(requested)?;
    let device = candle_device(&decision)?;
    Ok(AcceleratorDiagnostics {
        target: option_env!("TARGET").unwrap_or("unknown"),
        requested: decision.requested,
        compiled: decision.compiled,
        selected: decision.selected,
        embedder_device: device_label(&device).to_string(),
        detector_provider: detector_provider_label(&decision).to_string(),
        fallback_reason: decision.fallback_reason,
    })
}

/// Expected detector provider/backend label from the selection.
pub fn detector_provider_label(decision: &AcceleratorDecision) -> &'static str {
    match decision.selected {
        SelectedAccelerator::Cpu => "onnx-cpu",
        SelectedAccelerator::Cuda => "onnx-cuda",
        SelectedAccelerator::Metal => "candle-metal",
    }
}

fn candle_metal_device() -> Result<Device> {
    #[cfg(all(target_os = "macos", feature = "gpu-metal"))]
    {
        return Device::new_metal(0)
            .map_err(|e| Error::Config(format!("Candle Metal device 0: {e}")));
    }
    #[allow(unreachable_code)]
    Err(Error::Config(
        "Metal is only available on macOS binaries built with feature gpu-metal".into(),
    ))
}

fn cuda_runtime_available() -> bool {
    std::process::Command::new("nvidia-smi")
        .arg("-L")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
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
            AcceleratorPreference::from_env_value("CPU"),
            Some(AcceleratorPreference::Cpu)
        );
        assert_eq!(
            AcceleratorPreference::from_env_value("metal"),
            Some(AcceleratorPreference::Metal)
        );
        assert_eq!(
            AcceleratorPreference::from_env_value("cuda"),
            Some(AcceleratorPreference::Cuda)
        );
    }

    #[test]
    fn rejects_invalid_preference() {
        assert!(AcceleratorPreference::from_env_value("directml").is_none());
    }

    #[test]
    fn cpu_resolves_without_gpu_features() {
        let decision = resolve(AcceleratorPreference::Cpu).expect("cpu resolves");
        assert_eq!(decision.selected, SelectedAccelerator::Cpu);
        assert!(decision.fallback_reason.is_none());
    }

    #[test]
    fn explicit_cuda_errors_when_not_compiled() {
        if !cfg!(feature = "gpu-cuda") {
            let err = resolve(AcceleratorPreference::Cuda).expect_err("cuda unavailable");
            assert!(err.to_string().contains("gpu-cuda"));
        }
    }
}
```

- [ ] **Step 3: Add accelerator to config**

Edit `crates/anno-rag/src/config.rs`:

```rust
use crate::accelerator::AcceleratorPreference;
```

Add a field to `AnnoRagConfig` near `embedder_dtype`:

```rust
    /// Runtime accelerator preference. Defaults to `auto`; `ANNO_ACCELERATOR`
    /// overrides this at process start.
    #[serde(default)]
    pub accelerator: AcceleratorPreference,
```

Add it to `Default`:

```rust
            accelerator: AcceleratorPreference::Auto,
```

Add a config regression test:

```rust
    #[test]
    fn old_config_defaults_accelerator_to_auto() {
        let v01_json = r#"{
            "data_dir": ".anno-rag",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256
        }"#;
        let c: AnnoRagConfig = serde_json::from_str(v01_json).expect("old config parses");
        assert_eq!(c.accelerator, AcceleratorPreference::Auto);
    }
```

- [ ] **Step 4: Run focused tests**

Run:

```powershell
cargo test -p anno-rag accelerator --no-default-features
cargo test -p anno-rag old_config_defaults_accelerator_to_auto --no-default-features
```

Expected: PASS.

- [ ] **Step 5: Commit accelerator core**

Run:

```powershell
git add crates/anno-rag/src/accelerator.rs crates/anno-rag/src/lib.rs crates/anno-rag/src/config.rs
git commit -m "feat: add anno-rag accelerator selection core"
```

## Task 3: `anno-rag diagnose-gpu`

**Files:**
- Modify: `crates/anno-rag-bin/src/main.rs`

- [ ] **Step 1: Add failing CLI parse tests**

Add imports near the top:

```rust
use clap::{Parser, Subcommand};
```

Append tests in `crates/anno-rag-bin/src/main.rs`:

```rust
#[cfg(test)]
mod gpu_cli_tests {
    use super::*;

    #[test]
    fn parses_diagnose_gpu_command() {
        let cli = Cli::try_parse_from(["anno-rag", "diagnose-gpu"]).expect("parse");
        assert!(matches!(cli.cmd, Cmd::DiagnoseGpu));
    }
}
```

Run:

```powershell
cargo test -p anno-rag-bin parses_diagnose_gpu_command --no-default-features
```

Expected: FAIL with:

```text
no variant or associated item named `DiagnoseGpu`
```

- [ ] **Step 2: Add the command variant**

Edit `enum Cmd`:

```rust
    /// Print GPU/accelerator diagnostics without loading model weights.
    DiagnoseGpu,
```

- [ ] **Step 3: Short-circuit before keyring and pipeline initialization**

Add this branch after config env setup and before `Mcp`:

```rust
    if let Cmd::DiagnoseGpu = &cli.cmd {
        let diagnostics = anno_rag::accelerator::diagnostics(cfg.accelerator)?;
        println!("{}", serde_json::to_string_pretty(&diagnostics)?);
        return Ok(());
    }
```

Add the unreachable match arm:

```rust
        Cmd::DiagnoseGpu => unreachable!("handled above before Pipeline::new"),
```

- [ ] **Step 4: Run CLI tests and a binary smoke**

Run:

```powershell
cargo test -p anno-rag-bin parses_diagnose_gpu_command --no-default-features
cargo run -p anno-rag-bin --no-default-features -- diagnose-gpu
```

Expected JSON contains:

```json
"requested": "auto"
```

and:

```json
"selected": "cpu"
```

on a non-GPU default build.

- [ ] **Step 5: Verify explicit unavailable CUDA fails**

Run:

```powershell
$env:ANNO_ACCELERATOR = "cuda"
cargo run -p anno-rag-bin --no-default-features -- diagnose-gpu
Remove-Item Env:\ANNO_ACCELERATOR
```

Expected: FAIL with a message containing:

```text
gpu-cuda
```

- [ ] **Step 6: Commit diagnostics CLI**

Run:

```powershell
git add crates/anno-rag-bin/src/main.rs
git commit -m "feat: add anno-rag GPU diagnostics command"
```

## Task 4: Embedder Device Selection

**Files:**
- Modify: `crates/anno-rag/src/embed.rs`

- [ ] **Step 1: Add failing unit test for the public label helper**

Add this test in `embed.rs` tests:

```rust
    #[test]
    fn cpu_device_label_is_stable() {
        assert_eq!(Embedder::device_label_for_test(&Device::Cpu), "cpu");
    }
```

Run:

```powershell
cargo test -p anno-rag cpu_device_label_is_stable --no-default-features
```

Expected: FAIL with:

```text
no function or associated item named `device_label_for_test`
```

- [ ] **Step 2: Add helper and wire accelerator decision**

In `impl Embedder`, add:

```rust
    #[cfg(test)]
    fn device_label_for_test(device: &Device) -> &'static str {
        crate::accelerator::device_label(device)
    }

    /// Device label used by diagnostics and tests.
    #[must_use]
    pub fn device_label(&self) -> &'static str {
        crate::accelerator::device_label(&self.device)
    }
```

In both load paths, replace:

```rust
                let device = Device::Cpu;
```

and:

```rust
        let device = Device::Cpu;
```

with:

```rust
                let requested =
                    crate::accelerator::AcceleratorPreference::from_env_or(cfg.accelerator)?;
                let decision = crate::accelerator::resolve(requested)?;
                let device = crate::accelerator::candle_device(&decision)?;
```

and:

```rust
        let requested = crate::accelerator::AcceleratorPreference::from_env_or(cfg.accelerator)?;
        let decision = crate::accelerator::resolve(requested)?;
        let device = crate::accelerator::candle_device(&decision)?;
```

- [ ] **Step 3: Run focused tests**

Run:

```powershell
cargo test -p anno-rag cpu_device_label_is_stable --no-default-features
cargo test -p anno-rag accelerator --no-default-features
```

Expected: PASS.

- [ ] **Step 4: Run targeted check**

Run:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: exit 0.

- [ ] **Step 5: Commit embedder selection**

Run:

```powershell
git add crates/anno-rag/src/embed.rs
git commit -m "feat: select anno-rag embedder accelerator device"
```

## Task 5: Detector CUDA Provider Selection

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Add failing config-helper tests**

In `detect.rs`, add tests:

```rust
    #[test]
    fn cpu_detector_uses_cpu_provider() {
        let cfg = detector_onnx_config_for(crate::accelerator::AcceleratorPreference::Cpu)
            .expect("cpu config");
        assert!(cfg.use_cpu_provider);
        assert!(!cfg.prefer_cuda);
        assert!(!cfg.prefer_coreml);
    }

    #[test]
    fn cuda_detector_requires_cuda_feature() {
        if !cfg!(feature = "gpu-cuda") {
            let err = detector_onnx_config_for(crate::accelerator::AcceleratorPreference::Cuda)
                .expect_err("cuda unavailable");
            assert!(err.to_string().contains("gpu-cuda"));
        }
    }
```

Run:

```powershell
cargo test -p anno-rag cpu_detector_uses_cpu_provider --no-default-features
```

Expected: FAIL because `detector_onnx_config_for` does not exist.

- [ ] **Step 2: Add ONNX config helper**

Near `NER_MODEL_ID` in `detect.rs`, add:

```rust
fn detector_onnx_config_for(
    preference: crate::accelerator::AcceleratorPreference,
) -> Result<anno::backends::hf_loader::OnnxSessionConfig> {
    let requested = crate::accelerator::AcceleratorPreference::from_env_or(preference)?;
    let mut onnx = anno::backends::hf_loader::OnnxSessionConfig::default();
    match requested {
        crate::accelerator::AcceleratorPreference::Cpu => {
            onnx.prefer_cuda = false;
            onnx.prefer_coreml = false;
        }
        crate::accelerator::AcceleratorPreference::Auto => {
            onnx.prefer_cuda = cfg!(feature = "gpu-cuda");
        }
        crate::accelerator::AcceleratorPreference::Cuda => {
            if !cfg!(feature = "gpu-cuda") {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=cuda requires a binary built with feature gpu-cuda".into(),
                ));
            }
            onnx.prefer_cuda = true;
        }
        crate::accelerator::AcceleratorPreference::Metal => {
            if !cfg!(feature = "gpu-metal") {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=metal requires a binary built with feature gpu-metal".into(),
                ));
            }
            onnx.prefer_coreml = false;
        }
    }
    Ok(onnx)
}
```

- [ ] **Step 3: Change detector construction to accept config**

Change:

```rust
    pub fn new() -> Result<Self> {
```

to:

```rust
    pub fn new(cfg: &crate::config::AnnoRagConfig) -> Result<Self> {
        let onnx = detector_onnx_config_for(cfg.accelerator)?;
        let model_cfg = anno::backends::gliner2_fastino::GLiNER2FastinoConfig::default()
            .with_onnx(onnx);
```

Change the local load:

```rust
                let ner = anno::backends::gliner2_fastino::GLiNER2Fastino::from_local(&model_path)
```

to:

```rust
                let ner = anno::backends::gliner2_fastino::GLiNER2Fastino::from_local_with_config(
                    &model_path,
                    model_cfg.clone(),
                )
```

Change the HF load:

```rust
        let ner = GLiNER2Fastino::from_pretrained(NER_MODEL_ID)
```

to:

```rust
        let ner = GLiNER2Fastino::from_pretrained_with_config(NER_MODEL_ID, model_cfg)
```

- [ ] **Step 4: Update pipeline lazy detector init**

In `crates/anno-rag/src/pipeline.rs`, change:

```rust
        let d = Arc::new(Detector::new()?);
```

to:

```rust
        let d = Arc::new(Detector::new(&self.cfg)?);
```

Update direct tests that call `Detector::new()` to pass `&AnnoRagConfig::default()`.

- [ ] **Step 5: Run focused tests**

Run:

```powershell
cargo test -p anno-rag cpu_detector_uses_cpu_provider --no-default-features
cargo test -p anno-rag cuda_detector_requires_cuda_feature --no-default-features
```

Expected: PASS.

- [ ] **Step 6: Run targeted check**

Run:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: exit 0.

- [ ] **Step 7: Commit detector CUDA selection**

Run:

```powershell
git add crates/anno-rag/src/detect.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat: wire CUDA preference into anno-rag detector"
```

## Task 6: Apple Metal Detector Backend

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/mod.rs`
- Modify: `crates/anno-rag/src/detect.rs`
- Modify: `crates/anno-rag/src/download_models.rs`

- [ ] **Step 1: Add failing public constructor test in `anno`**

In `crates/anno/src/backends/gliner2_fastino_candle/mod.rs`, add under tests:

```rust
    #[test]
    fn from_local_with_device_rejects_missing_weights() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = GLiNER2FastinoCandle::from_local_with_device(dir.path(), &Device::Cpu)
            .expect_err("empty dir must fail");
        assert!(err.to_string().contains("model.safetensors"));
    }
```

Run:

```powershell
cargo test -p anno from_local_with_device_rejects_missing_weights --features gliner2-fastino-candle
```

Expected: FAIL because `from_local_with_device` is not public.

- [ ] **Step 2: Expose device-aware Candle constructors**

In `impl GLiNER2FastinoCandle`, add:

```rust
    /// Load from a local directory with an explicit Candle device.
    pub fn from_local_with_device(model_dir: &Path, device: &Device) -> crate::Result<Self> {
        Self::from_local_on_device(model_dir, device)
    }

    /// Load from HuggingFace Hub with an explicit Candle device.
    pub fn from_pretrained_with_device(
        model_id: &str,
        device: &Device,
    ) -> crate::Result<Self> {
        let api = crate::backends::hf_loader::hf_api()
            .map_err(|e| crate::Error::Backend(format!("hf_api: {e}")))?;
        let repo = api.model(model_id.to_string());

        let _tokenizer =
            crate::backends::hf_loader::download_model_file(&repo, &["tokenizer.json"])
                .map_err(|e| crate::Error::Backend(format!("download tokenizer: {e}")))?;
        let _config = crate::backends::hf_loader::download_model_file(&repo, &["config.json"])
            .map_err(|e| crate::Error::Backend(format!("download config: {e}")))?;
        let _encoder_config =
            crate::backends::hf_loader::download_model_file(&repo, &["encoder_config/config.json"])
                .map_err(|e| crate::Error::Backend(format!("download encoder_config: {e}")))?;
        let weights_path = crate::backends::hf_loader::download_model_file(
            &repo,
            &["model.safetensors", "pytorch_model.bin"],
        )
        .map_err(|e| crate::Error::Backend(format!("download weights: {e}")))?;

        let snapshot_dir = weights_path
            .parent()
            .ok_or_else(|| crate::Error::Backend("snapshot dir resolution".into()))?;
        let mut model = Self::from_local_with_device(snapshot_dir, device)?;
        model.model_id = model_id.to_string();
        Ok(model)
    }
```

Change existing `from_pretrained` to delegate:

```rust
    pub fn from_pretrained(model_id: &str) -> crate::Result<Self> {
        Self::from_pretrained_with_device(model_id, &Device::Cpu)
    }
```

- [ ] **Step 3: Add Detector backend enum**

In `detect.rs`, replace:

```rust
pub struct Detector {
    ner: GLiNER2Fastino,
}
```

with:

```rust
enum NerBackend {
    Onnx(GLiNER2Fastino),
    #[cfg(feature = "gpu-metal")]
    Candle(anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle),
}

impl NerBackend {
    fn extract_with_types(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> anno::Result<Vec<anno::Entity>> {
        match self {
            Self::Onnx(model) => model.extract_with_types(text, labels, threshold),
            #[cfg(feature = "gpu-metal")]
            Self::Candle(model) => model.extract_with_types(text, labels, threshold),
        }
    }
}

pub struct Detector {
    ner: NerBackend,
}
```

Update calls from:

```rust
            .ner
            .extract_with_types(text, labels, threshold)
```

to:

```rust
            .ner
            .extract_with_types(text, labels, threshold)
```

The call text stays the same because `NerBackend` exposes the same helper.

- [ ] **Step 4: Add Candle model constants and construction path**

In `detect.rs`:

```rust
/// Candle/PyTorch GLiNER2 repo used for Apple Metal detector acceleration.
pub const CANDLE_NER_MODEL_ID: &str = "fastino/gliner2-multi-v1";
const CANDLE_NER_MODEL_DIR: &str = "gliner2-multi-v1-candle";
```

At the start of `Detector::new`, after resolving the accelerator decision:

```rust
        let requested = crate::accelerator::AcceleratorPreference::from_env_or(cfg.accelerator)?;
        let decision = crate::accelerator::resolve(requested)?;
        if matches!(
            decision.selected,
            crate::accelerator::SelectedAccelerator::Metal
        ) {
            return Self::new_candle_metal(cfg, &decision);
        }
```

Add the helper:

```rust
    #[cfg(feature = "gpu-metal")]
    fn new_candle_metal(
        _cfg: &crate::config::AnnoRagConfig,
        decision: &crate::accelerator::AcceleratorDecision,
    ) -> Result<Self> {
        let device = crate::accelerator::candle_device(decision)?;
        if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
            let model_path = PathBuf::from(models_dir).join(CANDLE_NER_MODEL_DIR);
            if model_path.exists() {
                let ner = anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle::from_local_with_device(
                    &model_path,
                    &device,
                )
                .map_err(|e| Error::Detect(format!("gliner2_fastino_candle load (local): {e}")))?;
                return Ok(Self {
                    ner: NerBackend::Candle(ner),
                });
            }
        }
        let ner = anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle::from_pretrained_with_device(
            CANDLE_NER_MODEL_ID,
            &device,
        )
        .map_err(|e| Error::Detect(format!("gliner2_fastino_candle load: {e}")))?;
        Ok(Self {
            ner: NerBackend::Candle(ner),
        })
    }

    #[cfg(not(feature = "gpu-metal"))]
    fn new_candle_metal(
        _cfg: &crate::config::AnnoRagConfig,
        _decision: &crate::accelerator::AcceleratorDecision,
    ) -> Result<Self> {
        Err(Error::Config(
            "ANNO_ACCELERATOR=metal requires a binary built with feature gpu-metal".into(),
        ))
    }
```

Wrap existing ONNX detector construction in `NerBackend::Onnx(ner)`.

- [ ] **Step 5: Extend offline model download layout**

In `download_models.rs`, add:

```rust
/// Candle/PyTorch GLiNER2 repo used by Metal sidecars.
const CANDLE_NER_MODEL_ID: &str = "fastino/gliner2-multi-v1";
```

Add to `download` behind feature:

```rust
    #[cfg(feature = "gpu-metal")]
    download_candle_ner(&models_dir).await?;
```

Add:

```rust
#[cfg(feature = "gpu-metal")]
async fn download_candle_ner(models_dir: &Path) -> Result<()> {
    let candle_dir = models_dir.join("gliner2-multi-v1-candle");
    tokio::fs::create_dir_all(&candle_dir).await?;
    let api =
        hf_hub::api::tokio::Api::new().map_err(|e| Error::Detect(format!("hf-hub init: {e}")))?;
    let repo = api.model(CANDLE_NER_MODEL_ID.to_string());
    for file in [
        "tokenizer.json",
        "config.json",
        "encoder_config/config.json",
        "model.safetensors",
    ] {
        let src = repo
            .get(file)
            .await
            .map_err(|e| Error::Detect(format!("{file} fetch: {e}")))?;
        let dest = candle_dir.join(file);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&src, dest).await?;
    }
    println!("  Candle NER model        ... ok");
    Ok(())
}
```

- [ ] **Step 6: Run checks**

Run:

```powershell
cargo test -p anno from_local_with_device_rejects_missing_weights --features gliner2-fastino-candle
cargo test -p anno-rag cpu_detector_uses_cpu_provider --no-default-features
```

Expected: PASS.

On Apple Silicon, run:

```bash
cargo check -p anno-rag-bin --features gpu-metal
ANNO_ACCELERATOR=metal cargo run -p anno-rag-bin --features gpu-metal -- diagnose-gpu
```

Expected: `selected` is `metal`.

- [ ] **Step 7: Commit Metal detector path**

Run:

```powershell
git add crates/anno/src/backends/gliner2_fastino_candle/mod.rs crates/anno-rag/src/detect.rs crates/anno-rag/src/download_models.rs
git commit -m "feat: add Metal detector backend path"
```

## Task 7: Accelerated Release Packaging Scripts

**Files:**
- Create: `scripts/release/package-accelerated-windows.ps1`
- Create: `scripts/release/package-accelerated-unix.sh`
- Create: `scripts/release/checksums-accelerated.sh`

- [ ] **Step 1: Add Windows packaging script**

Create `scripts/release/package-accelerated-windows.ps1`:

```powershell
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Tag,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Target,

    [Parameter(Mandatory = $true)]
    [ValidateSet("cuda")]
    [string]$Flavor
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Test-AssetComponent {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Value
    )
    if ($Value -notmatch '^[A-Za-z0-9._-]+$') {
        throw "Invalid $Name`: must match ^[A-Za-z0-9._-]+$"
    }
    if ($Value -notmatch '[A-Za-z0-9]') {
        throw "Invalid $Name`: must contain at least one ASCII alphanumeric character"
    }
}

Test-AssetComponent -Name "Tag" -Value $Tag
Test-AssetComponent -Name "Target" -Value $Target
Test-AssetComponent -Name "Flavor" -Value $Flavor

$ScriptPath = $PSCommandPath
if (-not $ScriptPath) {
    $ScriptPath = $MyInvocation.MyCommand.Path
}

$ReleaseDir = Split-Path -Parent $ScriptPath
$ScriptsDir = Split-Path -Parent $ReleaseDir
$RepoRoot = Split-Path -Parent $ScriptsDir

$PackageName = "hacienda-$Tag-$Target-$Flavor"
$DistDir = Join-Path -Path $RepoRoot -ChildPath "dist"
$StagingDir = Join-Path -Path $DistDir -ChildPath $PackageName
$ExamplesDir = Join-Path -Path $StagingDir -ChildPath "examples"
$ZipPath = Join-Path -Path $DistDir -ChildPath "$PackageName.zip"

$RequiredFiles = @(
    "target/$Target/release/anno-rag.exe",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "env.example",
    "docs/release/accelerated-gpu-builds.md",
    "docs/release/examples/claude_desktop_config.windows.json",
    "docs/release/examples/claude_desktop_config.macos.json"
)

$MissingFiles = @(foreach ($RelativePath in $RequiredFiles) {
    $FullPath = Join-Path -Path $RepoRoot -ChildPath $RelativePath
    if (-not (Test-Path -LiteralPath $FullPath -PathType Leaf)) {
        $RelativePath
    }
})

if ($MissingFiles.Count -gt 0) {
    $MissingList = $MissingFiles -join [Environment]::NewLine
    throw "Cannot create accelerated Windows package. Missing required file(s):$([Environment]::NewLine)$MissingList"
}

New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
if (Test-Path -LiteralPath $StagingDir) {
    Remove-Item -LiteralPath $StagingDir -Recurse -Force
}
if (Test-Path -LiteralPath $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}

New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null
New-Item -ItemType Directory -Path $ExamplesDir -Force | Out-Null

foreach ($RelativePath in $RequiredFiles) {
    $SourcePath = Join-Path -Path $RepoRoot -ChildPath $RelativePath
    $DestinationDir = $StagingDir
    if ($RelativePath -like "docs/release/examples/*.json") {
        $DestinationDir = $ExamplesDir
    }
    Copy-Item -LiteralPath $SourcePath -Destination $DestinationDir
}

Compress-Archive -Path (Join-Path -Path $StagingDir -ChildPath "*") -DestinationPath $ZipPath -Force
Write-Output $ZipPath
```

- [ ] **Step 2: Add Unix packaging script**

Create `scripts/release/package-accelerated-unix.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 TAG TARGET FLAVOR" >&2
  exit 2
fi

tag="$1"
target="$2"
flavor="$3"

validate_asset_component() {
  local name="$1"
  local value="$2"
  if [[ ! "${value}" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "Invalid ${name}: must match ^[A-Za-z0-9._-]+$" >&2
    exit 2
  fi
  if [[ ! "${value}" =~ [A-Za-z0-9] ]]; then
    echo "Invalid ${name}: must contain at least one ASCII alphanumeric character" >&2
    exit 2
  fi
}

validate_asset_component "TAG" "${tag}"
validate_asset_component "TARGET" "${target}"
validate_asset_component "FLAVOR" "${flavor}"

if [[ "${flavor}" != "metal" ]]; then
  echo "Unsupported Unix accelerated flavor: ${flavor}" >&2
  exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"

package_name="hacienda-${tag}-${target}-${flavor}"
dist_dir="${repo_root}/dist"
staging_dir="${dist_dir}/${package_name}"
tarball_path="${dist_dir}/${package_name}.tar.gz"

executables=("target/${target}/release/anno-rag")
required_files=(
  "README.md"
  "LICENSE-MIT"
  "LICENSE-APACHE"
  "env.example"
  "docs/release/accelerated-gpu-builds.md"
  "docs/release/examples/claude_desktop_config.windows.json"
  "docs/release/examples/claude_desktop_config.macos.json"
)

missing=()
not_executable=()

for relative_path in "${executables[@]}"; do
  full_path="${repo_root}/${relative_path}"
  if [[ ! -f "${full_path}" ]]; then
    missing+=("${relative_path}")
  elif [[ ! -x "${full_path}" ]]; then
    not_executable+=("${relative_path}")
  fi
done

for relative_path in "${required_files[@]}"; do
  if [[ ! -f "${repo_root}/${relative_path}" ]]; then
    missing+=("${relative_path}")
  fi
done

if (( ${#missing[@]} > 0 )); then
  {
    echo "Cannot create accelerated Unix package. Missing required file(s):"
    printf '  %s\n' "${missing[@]}"
  } >&2
  exit 1
fi

if (( ${#not_executable[@]} > 0 )); then
  {
    echo "Cannot create accelerated Unix package. Required executable(s) are not executable:"
    printf '  %s\n' "${not_executable[@]}"
  } >&2
  exit 1
fi

mkdir -p "${dist_dir}"
rm -rf -- "${staging_dir}"
rm -f -- "${tarball_path}"
mkdir -p "${staging_dir}/examples"

cp -- "${repo_root}/target/${target}/release/anno-rag" "${staging_dir}/"
cp -- "${repo_root}/README.md" "${staging_dir}/"
cp -- "${repo_root}/LICENSE-MIT" "${staging_dir}/"
cp -- "${repo_root}/LICENSE-APACHE" "${staging_dir}/"
cp -- "${repo_root}/env.example" "${staging_dir}/"
cp -- "${repo_root}/docs/release/accelerated-gpu-builds.md" "${staging_dir}/"
cp -- "${repo_root}/docs/release/examples/claude_desktop_config.windows.json" "${staging_dir}/examples/"
cp -- "${repo_root}/docs/release/examples/claude_desktop_config.macos.json" "${staging_dir}/examples/"

tar -C "${dist_dir}" -czf "${tarball_path}" "${package_name}"
echo "${tarball_path}"
```

- [ ] **Step 3: Add accelerated checksum script**

Create `scripts/release/checksums-accelerated.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"
dist_dir="${repo_root}/dist"
checksum_path="${dist_dir}/SHA256SUMS-accelerated.txt"

if [[ ! -d "${dist_dir}" ]]; then
  echo "Cannot write accelerated checksums. dist directory does not exist: ${dist_dir}" >&2
  exit 1
fi

shopt -s nullglob
archives=("${dist_dir}"/*-metal.tar.gz "${dist_dir}"/*-cuda.zip)
shopt -u nullglob

if (( ${#archives[@]} == 0 )); then
  echo "Cannot write accelerated checksums. No accelerated archives found in ${dist_dir}" >&2
  exit 1
fi

IFS=$'\n' archives=($(printf '%s\n' "${archives[@]}" | sort))
unset IFS

if command -v sha256sum >/dev/null 2>&1; then
  (
    cd "${dist_dir}"
    sha256sum -- "${archives[@]##*/}" > "${checksum_path}"
  )
elif command -v shasum >/dev/null 2>&1; then
  (
    cd "${dist_dir}"
    shasum -a 256 -- "${archives[@]##*/}" > "${checksum_path}"
  )
else
  echo "Cannot write accelerated checksums. Neither sha256sum nor shasum is available." >&2
  exit 1
fi

cat -- "${checksum_path}"
```

- [ ] **Step 4: Validate script syntax**

Run:

```powershell
powershell -NoProfile -Command "& { .\scripts\release\package-accelerated-windows.ps1 -Tag 'v0.0.0-test' -Target 'x86_64-pc-windows-msvc' -Flavor 'metal' }"
```

Expected: FAIL with:

```text
Cannot validate argument on parameter 'Flavor'
```

Run:

```powershell
bash -n scripts/release/package-accelerated-unix.sh
bash -n scripts/release/checksums-accelerated.sh
```

Expected: PASS.

- [ ] **Step 5: Commit packaging scripts**

Run:

```powershell
git add scripts/release/package-accelerated-windows.ps1 scripts/release/package-accelerated-unix.sh scripts/release/checksums-accelerated.sh
git commit -m "ci: add accelerated release packaging scripts"
```

## Task 8: Manual Accelerated Release Workflow

**Files:**
- Create: `.github/workflows/release-accelerated.yml`

- [ ] **Step 1: Add the workflow**

Create `.github/workflows/release-accelerated.yml`:

```yaml
name: Release Accelerated Binaries

on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Existing v* tag to package"
        required: true
        type: string
      publish:
        description: "Attach artifacts to the GitHub Release after smoke tests pass"
        required: true
        type: boolean
        default: false

permissions:
  contents: write

env:
  RELEASE_TAG: ${{ inputs.tag }}

jobs:
  build-metal:
    name: Build Apple Silicon Metal
    runs-on: macos-14
    steps:
      - name: Guard release tag
        shell: bash
        run: |
          if [[ "$RELEASE_TAG" != v* ]]; then
            echo "RELEASE_TAG must start with v: $RELEASE_TAG" >&2
            exit 1
          fi
          git ls-remote --exit-code --tags https://github.com/${{ github.repository }} "refs/tags/${RELEASE_TAG}"

      - uses: actions/checkout@v4
        with:
          ref: refs/tags/${{ env.RELEASE_TAG }}

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin

      - name: Install protoc
        run: brew list protobuf >/dev/null 2>&1 || brew install protobuf

      - uses: Swatinem/rust-cache@v2
        with:
          cache-all-crates: true
          key: release-accelerated-metal-aarch64-apple-darwin

      - name: Build anno-rag Metal binary
        run: cargo build --release -p anno-rag-bin --target aarch64-apple-darwin --features gpu-metal

      - name: Diagnose Metal
        run: ANNO_ACCELERATOR=metal ./target/aarch64-apple-darwin/release/anno-rag diagnose-gpu

      - name: Smoke Metal inference
        run: |
          smoke_dir="$(mktemp -d)"
          mkdir -p "${smoke_dir}/corpus" "${smoke_dir}/out"
          printf 'Maitre Julie Martin conseille la societe Horizon.\n' > "${smoke_dir}/corpus/sample.txt"
          ANNO_RAG_DATA_DIR="${smoke_dir}/data" ./target/aarch64-apple-darwin/release/anno-rag download-models --dir "${smoke_dir}/data/models"
          ANNO_MODELS_DIR="${smoke_dir}/data/models" \
            ANNO_RAG_DATA_DIR="${smoke_dir}/data" \
            ANNO_ACCELERATOR=metal \
            ./target/aarch64-apple-darwin/release/anno-rag ingest "${smoke_dir}/corpus" --output "${smoke_dir}/out"

      - name: Package Metal archive
        run: ./scripts/release/package-accelerated-unix.sh "$RELEASE_TAG" "aarch64-apple-darwin" "metal"

      - name: Upload Metal archive
        uses: actions/upload-artifact@v4
        with:
          name: release-aarch64-apple-darwin-metal
          path: dist/*-metal.tar.gz
          if-no-files-found: error

  build-cuda:
    name: Build Windows CUDA
    runs-on: [self-hosted, windows, cuda]
    steps:
      - name: Guard release tag
        shell: bash
        run: |
          if [[ "$RELEASE_TAG" != v* ]]; then
            echo "RELEASE_TAG must start with v: $RELEASE_TAG" >&2
            exit 1
          fi
          git ls-remote --exit-code --tags https://github.com/${{ github.repository }} "refs/tags/${RELEASE_TAG}"

      - uses: actions/checkout@v4
        with:
          ref: refs/tags/${{ env.RELEASE_TAG }}

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc

      - name: Install protoc
        shell: pwsh
        run: choco install protoc -y --no-progress

      - uses: Swatinem/rust-cache@v2
        with:
          cache-all-crates: true
          key: release-accelerated-cuda-x86_64-pc-windows-msvc

      - name: Align MSVC CRT
        shell: bash
        run: |
          echo "RUSTFLAGS=--cap-lints allow -C link-arg=/NODEFAULTLIB:libcmt" >> "$GITHUB_ENV"
          echo "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS=--cap-lints allow -C link-arg=/NODEFAULTLIB:libcmt" >> "$GITHUB_ENV"
          echo "CFLAGS_x86_64-pc-windows-msvc=/MD" >> "$GITHUB_ENV"
          echo "CFLAGS_x86_64_pc_windows_msvc=/MD" >> "$GITHUB_ENV"
          echo "CXXFLAGS_x86_64-pc-windows-msvc=/MD" >> "$GITHUB_ENV"
          echo "CXXFLAGS_x86_64_pc_windows_msvc=/MD" >> "$GITHUB_ENV"

      - name: Build anno-rag CUDA binary
        run: cargo build --release -p anno-rag-bin --target x86_64-pc-windows-msvc --features gpu-cuda

      - name: Diagnose CUDA
        shell: pwsh
        run: |
          $env:ANNO_ACCELERATOR = "cuda"
          .\target\x86_64-pc-windows-msvc\release\anno-rag.exe diagnose-gpu

      - name: Smoke CUDA inference
        shell: pwsh
        run: |
          $SmokeDir = Join-Path -Path $env:RUNNER_TEMP -ChildPath ([System.Guid]::NewGuid().ToString())
          $CorpusDir = Join-Path -Path $SmokeDir -ChildPath "corpus"
          $OutputDir = Join-Path -Path $SmokeDir -ChildPath "out"
          $DataDir = Join-Path -Path $SmokeDir -ChildPath "data"
          $ModelsDir = Join-Path -Path $DataDir -ChildPath "models"
          New-Item -ItemType Directory -Path $CorpusDir,$OutputDir,$DataDir -Force | Out-Null
          Set-Content -LiteralPath (Join-Path -Path $CorpusDir -ChildPath "sample.txt") -Value "Maitre Julie Martin conseille la societe Horizon." -Encoding UTF8
          .\target\x86_64-pc-windows-msvc\release\anno-rag.exe download-models --dir $ModelsDir
          $env:ANNO_MODELS_DIR = $ModelsDir
          $env:ANNO_RAG_DATA_DIR = $DataDir
          $env:ANNO_ACCELERATOR = "cuda"
          .\target\x86_64-pc-windows-msvc\release\anno-rag.exe ingest $CorpusDir --output $OutputDir

      - name: Package CUDA archive
        shell: pwsh
        run: .\scripts\release\package-accelerated-windows.ps1 -Tag "$env:RELEASE_TAG" -Target "x86_64-pc-windows-msvc" -Flavor "cuda"

      - name: Upload CUDA archive
        uses: actions/upload-artifact@v4
        with:
          name: release-x86_64-pc-windows-msvc-cuda
          path: dist/*-cuda.zip
          if-no-files-found: error

  publish:
    name: Publish accelerated artifacts
    runs-on: ubuntu-latest
    needs: [build-metal, build-cuda]
    if: ${{ inputs.publish }}
    steps:
      - uses: actions/checkout@v4
        with:
          ref: refs/tags/${{ env.RELEASE_TAG }}

      - name: Download accelerated archives
        uses: actions/download-artifact@v4
        with:
          pattern: release-*
          path: dist
          merge-multiple: true

      - name: Generate accelerated checksums
        run: ./scripts/release/checksums-accelerated.sh

      - name: Publish accelerated release assets
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ env.RELEASE_TAG }}
          generate_release_notes: true
          files: |
            dist/*-metal.tar.gz
            dist/*-cuda.zip
            dist/SHA256SUMS-accelerated.txt
```

- [ ] **Step 2: Validate YAML shape**

Run:

```powershell
@'
from pathlib import Path
import yaml
path = Path(".github/workflows/release-accelerated.yml")
data = yaml.safe_load(path.read_text())
assert data["name"] == "Release Accelerated Binaries"
assert "build-metal" in data["jobs"]
assert "build-cuda" in data["jobs"]
assert "publish" in data["jobs"]
'@ | python -
```

Expected: exit 0.

- [ ] **Step 3: Commit workflow**

Run:

```powershell
git add .github/workflows/release-accelerated.yml
git commit -m "ci: add manual accelerated release workflow"
```

## Task 9: Docs and README Link

**Files:**
- Create: `docs/release/accelerated-gpu-builds.md`
- Modify: `README.md`

- [ ] **Step 1: Add GPU build docs**

Create `docs/release/accelerated-gpu-builds.md`:

```markdown
# Accelerated GPU Builds

The default Hacienda / anno-rag release remains the CPU build. Use an accelerated sidecar only when the machine has matching hardware and drivers.

## Artifacts

| Artifact suffix | Platform | Hardware |
| --- | --- | --- |
| `aarch64-apple-darwin-metal` | Apple Silicon macOS | Apple Metal |
| `x86_64-pc-windows-msvc-cuda` | Windows x64 | NVIDIA CUDA |

## Runtime Selection

Set `ANNO_ACCELERATOR` before starting `anno-rag`:

```text
auto   try the compiled accelerator, then fall back to CPU
cpu    force CPU
metal  require Apple Metal and fail if unavailable
cuda   require NVIDIA CUDA and fail if unavailable
```

## Diagnostics

Run:

```bash
anno-rag diagnose-gpu
```

The command prints JSON with the compiled features, requested accelerator, selected embedder device, selected detector provider, and any CPU fallback reason.

## Windows CUDA

The CUDA build is for NVIDIA GPUs. Install a compatible NVIDIA driver and CUDA runtime before using it. If `ANNO_ACCELERATOR=cuda` fails, use the CPU build or set `ANNO_ACCELERATOR=cpu`.

## Apple Metal

The Metal build is for Apple Silicon macOS. If `ANNO_ACCELERATOR=metal` fails, use the CPU build or set `ANNO_ACCELERATOR=cpu`.
```

- [ ] **Step 2: Link the docs from README**

In `README.md`, near the release install section, add:

```markdown
GPU sidecar builds are documented in [docs/release/accelerated-gpu-builds.md](docs/release/accelerated-gpu-builds.md). The default release remains CPU-first; use the Metal or CUDA archives only on matching hardware.
```

- [ ] **Step 3: Validate docs references**

Run:

```powershell
rg -n "accelerated-gpu-builds|diagnose-gpu|ANNO_ACCELERATOR" README.md docs/release/accelerated-gpu-builds.md
```

Expected: matches in both files.

- [ ] **Step 4: Commit docs**

Run:

```powershell
git add docs/release/accelerated-gpu-builds.md README.md
git commit -m "docs: document accelerated GPU builds"
```

## Task 10: Final Verification

**Files:**
- Read-only: full diff.

- [ ] **Step 1: Check changed scope**

Run:

```powershell
git diff --name-status origin/main..HEAD
```

Expected changed paths are limited to:

```text
docs/superpowers/specs/2026-06-01-anno-apple-windows-gpu-builds-design.md
docs/superpowers/plans/2026-06-01-anno-apple-windows-gpu-builds-implementation.md
crates/anno-rag/Cargo.toml
crates/anno-rag-mcp/Cargo.toml
crates/anno-rag-bin/Cargo.toml
crates/anno-rag/src/accelerator.rs
crates/anno-rag/src/lib.rs
crates/anno-rag/src/config.rs
crates/anno-rag/src/embed.rs
crates/anno-rag/src/detect.rs
crates/anno-rag/src/pipeline.rs
crates/anno-rag/src/download_models.rs
crates/anno-rag-bin/src/main.rs
crates/anno/src/backends/gliner2_fastino_candle/mod.rs
.github/workflows/release-accelerated.yml
scripts/release/package-accelerated-windows.ps1
scripts/release/package-accelerated-unix.sh
scripts/release/checksums-accelerated.sh
docs/release/accelerated-gpu-builds.md
README.md
```

- [ ] **Step 2: Run local targeted tests**

Run:

```powershell
cargo test -p anno-rag accelerator --no-default-features
cargo test -p anno-rag old_config_defaults_accelerator_to_auto --no-default-features
cargo test -p anno-rag cpu_device_label_is_stable --no-default-features
cargo test -p anno-rag cpu_detector_uses_cpu_provider --no-default-features
cargo test -p anno-rag cuda_detector_requires_cuda_feature --no-default-features
cargo test -p anno-rag-bin parses_diagnose_gpu_command --no-default-features
```

Expected: all PASS.

- [ ] **Step 3: Run local targeted checks**

Run:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check
```

Expected: both exit 0.

- [ ] **Step 4: Verify feature metadata**

Run:

```powershell
cargo metadata --no-deps --format-version 1 -p anno-rag-bin --features gpu-cuda
cargo metadata --no-deps --format-version 1 -p anno-rag-bin --features gpu-metal
```

Expected: both exit 0.

- [ ] **Step 5: Verify diagnostics behavior**

Run:

```powershell
cargo run -p anno-rag-bin --no-default-features -- diagnose-gpu
```

Expected JSON contains:

```json
"requested": "auto"
```

Run:

```powershell
$env:ANNO_ACCELERATOR = "cuda"
cargo run -p anno-rag-bin --no-default-features -- diagnose-gpu
Remove-Item Env:\ANNO_ACCELERATOR
```

Expected: non-zero exit with a message containing:

```text
gpu-cuda
```

- [ ] **Step 6: Validate release scripts**

Run:

```powershell
bash -n scripts/release/package-accelerated-unix.sh
bash -n scripts/release/checksums-accelerated.sh
powershell -NoProfile -Command "& { .\scripts\release\package-accelerated-windows.ps1 -Tag 'v0.0.0-test' -Target 'x86_64-pc-windows-msvc' -Flavor 'metal' }"
```

Expected: the two Bash syntax checks pass; the PowerShell command fails at parameter validation because `metal` is not valid for the Windows script.

- [ ] **Step 7: Refresh GitNexus and confirm repository status**

Run:

```powershell
npx gitnexus analyze
npx gitnexus status
```

Expected: index exists and is fresh.

- [ ] **Step 8: Commit verification-only follow-up if needed**

If verification required no edits, do not create an empty commit. If verification found fixes in the accelerated release workflow, stage and commit the concrete touched files, for example:

```powershell
git add .github/workflows/release-accelerated.yml scripts/release/checksums-accelerated.sh
git commit -m "fix: stabilize accelerated GPU build plumbing"
```

Use the exact paths that were actually fixed, and leave unrelated files unstaged.

## Hardware Verification Gates

These commands are required before publishing accelerated artifacts, even if local development completed.

Apple Silicon runner:

```bash
cargo build --release -p anno-rag-bin --target aarch64-apple-darwin --features gpu-metal
ANNO_ACCELERATOR=metal ./target/aarch64-apple-darwin/release/anno-rag diagnose-gpu
smoke_dir="$(mktemp -d)"
mkdir -p "${smoke_dir}/corpus" "${smoke_dir}/out"
printf 'Maitre Julie Martin conseille la societe Horizon.\n' > "${smoke_dir}/corpus/sample.txt"
ANNO_RAG_DATA_DIR="${smoke_dir}/data" ./target/aarch64-apple-darwin/release/anno-rag download-models --dir "${smoke_dir}/data/models"
ANNO_MODELS_DIR="${smoke_dir}/data/models" ANNO_RAG_DATA_DIR="${smoke_dir}/data" ANNO_ACCELERATOR=metal ./target/aarch64-apple-darwin/release/anno-rag ingest "${smoke_dir}/corpus" --output "${smoke_dir}/out"
```

Expected JSON:

```json
"selected": "metal"
```

Windows NVIDIA CUDA runner:

```powershell
cargo build --release -p anno-rag-bin --target x86_64-pc-windows-msvc --features gpu-cuda
$env:ANNO_ACCELERATOR = "cuda"
.\target\x86_64-pc-windows-msvc\release\anno-rag.exe diagnose-gpu
$SmokeDir = Join-Path -Path $env:TEMP -ChildPath ([System.Guid]::NewGuid().ToString())
$CorpusDir = Join-Path -Path $SmokeDir -ChildPath "corpus"
$OutputDir = Join-Path -Path $SmokeDir -ChildPath "out"
$DataDir = Join-Path -Path $SmokeDir -ChildPath "data"
$ModelsDir = Join-Path -Path $DataDir -ChildPath "models"
New-Item -ItemType Directory -Path $CorpusDir,$OutputDir,$DataDir -Force | Out-Null
Set-Content -LiteralPath (Join-Path -Path $CorpusDir -ChildPath "sample.txt") -Value "Maitre Julie Martin conseille la societe Horizon." -Encoding UTF8
.\target\x86_64-pc-windows-msvc\release\anno-rag.exe download-models --dir $ModelsDir
$env:ANNO_MODELS_DIR = $ModelsDir
$env:ANNO_RAG_DATA_DIR = $DataDir
$env:ANNO_ACCELERATOR = "cuda"
.\target\x86_64-pc-windows-msvc\release\anno-rag.exe ingest $CorpusDir --output $OutputDir
```

Expected JSON:

```json
"selected": "cuda"
```

Do not set `publish: true` in `release-accelerated.yml` until both hardware gates pass on the release tag.
