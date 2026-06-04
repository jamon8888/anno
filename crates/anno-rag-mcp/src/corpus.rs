//! MCP-facing corpus registry facade.

use anno_corpus_core::{CorpusGuardError, CorpusId, CorpusProfile, EffectiveCorpus};
use anno_corpus_store::{CorpusStore, RegisterCorpusResult};
use anno_rag::config::AnnoRagConfig;
use serde::Serialize;
use std::path::PathBuf;

/// Public corpus shape returned by MCP tools.
#[derive(Debug, Clone, Serialize)]
pub struct CorpusWire {
    /// Stable corpus id.
    pub corpus_id: String,
    /// Pseudonymous display label.
    pub label: String,
    /// Current corpus health.
    pub health: String,
}

/// MCP corpus health details.
#[derive(Debug, Clone, Serialize)]
pub struct CorpusHealthWire {
    /// Stable corpus id.
    pub corpus_id: String,
    /// Registry health field.
    pub health: String,
    /// Count of knowledge source bindings.
    pub knowledge_sources: usize,
    /// Count of legal document bindings.
    pub legal_documents: usize,
    /// Count of tabular review bindings.
    pub tabular_reviews: usize,
}

/// Lazy corpus registry service.
pub struct CorpusService {
    store: CorpusStore,
}

impl CorpusService {
    /// Open the local corpus registry from Anno config.
    pub fn open(cfg: &AnnoRagConfig) -> anno_corpus_store::Result<Self> {
        Ok(Self {
            store: CorpusStore::open(corpus_db_path(cfg))?,
        })
    }

    /// Register an index root for the given MCP profile.
    pub fn register_index_root(
        &self,
        path: &str,
        profile: &str,
    ) -> anno_corpus_store::Result<RegisterCorpusResult> {
        let profiles = match profile {
            "general" => vec![CorpusProfile::Knowledge],
            "legal" => vec![CorpusProfile::Legal],
            "all" => vec![CorpusProfile::All],
            _ => vec![CorpusProfile::All],
        };
        self.store.register_root(path, &profiles)
    }

    /// Expose the underlying registry for MCP handlers.
    pub fn store(&self) -> &CorpusStore {
        &self.store
    }

    /// Return whether a corpus exists.
    pub fn corpus_exists(&self, corpus_id: CorpusId) -> anno_corpus_store::Result<bool> {
        self.store.corpus_exists(corpus_id)
    }

    /// Return one corpus wire row, or `None` if the id is unknown.
    pub fn get(&self, corpus_id: CorpusId) -> anno_corpus_store::Result<Option<CorpusWire>> {
        let rows = self.list()?;
        Ok(rows
            .into_iter()
            .find(|row| row.corpus_id == corpus_id.as_string()))
    }

    /// List all corpus rows.
    pub fn list(&self) -> anno_corpus_store::Result<Vec<CorpusWire>> {
        Ok(self
            .store
            .list_corpora()?
            .into_iter()
            .map(|row| CorpusWire {
                corpus_id: row.corpus_id.as_string(),
                label: row.label_pseudo,
                health: row.health,
            })
            .collect())
    }

    /// Return health counts for one existing corpus.
    pub fn health(&self, corpus_id: CorpusId) -> anno_corpus_store::Result<CorpusHealthWire> {
        let corpus = self
            .get(corpus_id)?
            .ok_or_else(|| anno_corpus_store::Error::UnknownCorpus(corpus_id.as_string()))?;
        let bindings = self.store.bindings_for_corpus(corpus_id)?;
        let knowledge_sources = bindings
            .iter()
            .filter(|binding| {
                binding.binding_kind == anno_corpus_core::CorpusBindingKind::KnowledgeSource
            })
            .count();
        let tabular_reviews = bindings
            .iter()
            .filter(|binding| {
                binding.binding_kind == anno_corpus_core::CorpusBindingKind::TabularReview
            })
            .count();
        let legal_documents = self.store.document_ids_for_corpus(corpus_id, "legal")?.len();
        Ok(CorpusHealthWire {
            corpus_id: corpus.corpus_id,
            health: corpus.health,
            knowledge_sources,
            legal_documents,
            tabular_reviews,
        })
    }

    /// Resolve the corpus that must apply to one MCP operation.
    pub fn resolve_effective(
        &self,
        corpus_id: Option<&str>,
        allow_cross_corpus: bool,
    ) -> Result<EffectiveCorpus, CorpusGuardError> {
        let count = self
            .store
            .corpus_count()
            .map_err(|_| CorpusGuardError::NoCorpus)?;
        if let Some(value) = corpus_id {
            let parsed = parse_corpus_id(value)
                .map_err(|_| CorpusGuardError::UnknownCorpus(value.to_string()))?;
            let exists = self
                .store
                .corpus_exists(parsed)
                .map_err(|_| CorpusGuardError::UnknownCorpus(value.to_string()))?;
            if exists {
                return Ok(EffectiveCorpus::Single(parsed));
            }
            return Err(CorpusGuardError::UnknownCorpus(value.to_string()));
        }
        if allow_cross_corpus {
            return Ok(EffectiveCorpus::CrossCorpus);
        }
        match count {
            0 => Err(CorpusGuardError::NoCorpus),
            1 => {
                let one = self
                    .store
                    .single_corpus_id()
                    .map_err(|_| CorpusGuardError::NoCorpus)?;
                Ok(EffectiveCorpus::Single(one))
            }
            _ => Err(CorpusGuardError::CorpusRequired),
        }
    }
}

/// Path to the local corpus registry database.
pub fn corpus_db_path(cfg: &AnnoRagConfig) -> PathBuf {
    cfg.data_dir.join("corpus.sqlite3")
}

/// Parse a user-supplied corpus id.
pub fn parse_corpus_id(value: &str) -> Result<CorpusId, String> {
    uuid::Uuid::parse_str(value)
        .map(CorpusId::new)
        .map_err(|e| format!("bad corpus_id: {e}"))
}
