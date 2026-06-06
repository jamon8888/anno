pub mod client;
pub mod legal_signals;
pub mod normalizers;
pub mod offsets;
pub mod prompt;

/// Default local extraction model (GLiNER2/Fastino multi-v1).
///
/// Matches the canonical NER model id used elsewhere in the workspace
/// (`anno-rag::download_models::CANDLE_NER_MODEL_ID`).
pub const DEFAULT_LOCAL_MODEL: &str = "fastino/gliner2-multi-v1";
