use crate::ids::{CorpusId, DocumentInstanceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusBindingKind {
    KnowledgeSource,
    LegalFolder,
    LegalDocument,
    TabularReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusProfile {
    Knowledge,
    Legal,
    Tabular,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusFreshness {
    Fresh,
    MaybeStale,
    Stale,
    Unknown,
}

impl CorpusFreshness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::MaybeStale => "maybe_stale",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusSyncOutputKind {
    KnowledgeFast,
    LegalSemantic,
}

impl CorpusSyncOutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::KnowledgeFast => "knowledge_fast",
            Self::LegalSemantic => "legal_semantic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusSummary {
    pub corpus_id: CorpusId,
    pub label_pseudo: String,
    pub profiles: Vec<CorpusProfile>,
    pub health: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusDocumentRef {
    pub corpus_id: CorpusId,
    pub document_id: DocumentInstanceId,
    pub source_path_hash: String,
    pub relative_path_hash: Option<String>,
    pub content_id: String,
}
