//! Vision-OCR client — image→text transcription. Sibling to [`crate::llm::LlmClient`]
//! (text→JSON). Backends in [`vllm_server`] (on-prem GPU) and [`local_gguf`]
//! (desktop llama.cpp server); routing in [`routing`]. Both run inside the
//! customer's trust boundary — no third-party egress (Spec B §4.3).
//!
//! Trait definition and value types live in `anno_rag::vlm` to avoid a circular
//! dependency (anno-rag-tabular already depends on anno-rag). Re-exported here
//! for backward compat so existing backends need no import changes.

pub mod local_gguf;
pub mod routing;
pub mod vllm_server;

// Re-export trait and value types from anno-rag so backends can implement them
// without the circular dep that would arise if anno-rag depended on anno-rag-tabular.
pub use anno_rag::vlm::{
    vlm_quality_score, PageImage, Transcription, VlmOcrClient, VLM_OCR_PROMPT_FR,
};
