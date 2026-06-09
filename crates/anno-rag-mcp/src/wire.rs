//! Wire response types (serialization structs) for the anno-rag MCP server.

use anno_rag::config::MemoryNerMode;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct SearchHitWire {
    pub(crate) doc_id: String,
    pub(crate) chunk_id: String,
    pub(crate) corpus_id: Option<String>,
    pub(crate) document_label: Option<String>,
    pub(crate) chunk_idx: u32,
    pub(crate) text_pseudo: String,
    pub(crate) page: Option<u32>,
    pub(crate) char_start: u32,
    pub(crate) char_end: u32,
    pub(crate) score: f32,
}

#[derive(Serialize)]
pub(crate) struct SearchResult {
    pub(crate) hits: Vec<SearchHitWire>,
}

#[derive(Serialize)]
pub(crate) struct RehydrateResult {
    pub(crate) text: String,
    pub(crate) tokens_rehydrated: usize,
}

#[derive(Serialize)]
pub(crate) struct DetectResult {
    pub(crate) entities: Vec<EntityInfo>,
}

#[derive(Serialize)]
pub(crate) struct EntityInfo {
    pub(crate) original: String,
    pub(crate) category: String,
    pub(crate) confidence: f64,
    pub(crate) source: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Serialize)]
pub(crate) struct VaultStatsResult {
    pub(crate) total_mappings: usize,
    pub(crate) categories: std::collections::HashMap<String, u32>,
}

#[derive(Serialize)]
pub(crate) struct MemorySaveResultWire {
    pub(crate) id: String,
    pub(crate) stored_text: String,
    pub(crate) redacted_text: String,
    pub(crate) token_count: usize,
    pub(crate) ner_mode: MemoryNerMode,
}

#[derive(Serialize)]
pub(crate) struct MemoryInvalidateResultWire {
    pub(crate) id: String,
    pub(crate) invalidated: bool,
    pub(crate) valid_to: String,
}

#[derive(Serialize)]
pub(crate) struct MemoryHitWire {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) kind: String,
    pub(crate) created_at: String,
    pub(crate) entity_refs: Vec<String>,
    pub(crate) score: f32,
}

#[derive(Serialize)]
pub(crate) struct MemoryRecallResultWire {
    pub(crate) hits: Vec<MemoryHitWire>,
}

#[derive(Serialize)]
pub(crate) struct MemoryForgetResultWire {
    pub(crate) forgotten_ids: Vec<String>,
    pub(crate) vault_tokens_purged: usize,
    pub(crate) note: String,
}

#[derive(Serialize)]
pub(crate) struct MemoryListResultWire {
    pub(crate) items: Vec<MemoryHitWire>,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct DownloadModelsResult {
    pub(crate) status: String,
    pub(crate) path: String,
    pub(crate) message: String,
}
