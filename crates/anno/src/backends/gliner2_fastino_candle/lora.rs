//! LoRA adapter loader + merge_into_base.
//!
//! Stub for M3 — concrete implementation in M7.
//!
//! ## Reference
//!
//! - PEFT layer.py: <https://github.com/huggingface/peft/blob/main/src/peft/tuners/lora/layer.py>
//! - LoRA paper: arXiv:2106.09685
//!
//! ## Merge formula
//!
//! `W_merged = W_base + (alpha / r) * (lora_B @ lora_A)`
//!
//! Per-module: walk safetensors keys, group by module path, multiply,
//! scale, add. Done at `load_adapter` time; nothing per-forward.
