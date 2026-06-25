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
    /// Freshness from the last sync state row.
    pub freshness: String,
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
        let legal_documents = self
            .store
            .document_ids_for_corpus(corpus_id, "legal")?
            .len();
        let freshness = self
            .store
            .sync_state(corpus_id)?
            .map(|state| state.freshness)
            .unwrap_or_else(|| "unknown".to_string());
        Ok(CorpusHealthWire {
            corpus_id: corpus.corpus_id,
            health: corpus.health,
            freshness,
            knowledge_sources,
            legal_documents,
            tabular_reviews,
        })
    }

    /// Resolve the corpus that must apply to one MCP operation.
    pub fn resolve_effective(
        &self,
        corpus_ref: Option<&str>,
        allow_cross_corpus: bool,
    ) -> Result<EffectiveCorpus, CorpusGuardError> {
        // 1. Explicit reference wins: try UUID, then alias.
        if let Some(value) = corpus_ref {
            if let Ok(parsed) = parse_corpus_id(value) {
                if self
                    .store
                    .corpus_exists(parsed)
                    .map_err(|_| CorpusGuardError::NoCorpus)?
                {
                    return Ok(EffectiveCorpus::Single(parsed));
                }
            }
            if let Some(by_alias) = self
                .store
                .lookup_by_alias(value)
                .map_err(|_| CorpusGuardError::NoCorpus)?
            {
                return Ok(EffectiveCorpus::Single(by_alias));
            }
            return Err(CorpusGuardError::UnknownCorpus(value.to_string()));
        }
        // 2. Explicit "search everything" short-circuits the count.
        if allow_cross_corpus {
            return Ok(EffectiveCorpus::CrossCorpus);
        }
        // 3. Implicit: depends on how many corpora exist.
        let count = self
            .store
            .corpus_count()
            .map_err(|_| CorpusGuardError::NoCorpus)?;
        match count {
            0 => Err(CorpusGuardError::NoCorpus),
            1 => Ok(EffectiveCorpus::Single(
                self.store
                    .single_corpus_id()
                    .map_err(|_| CorpusGuardError::NoCorpus)?,
            )),
            _ => Err(CorpusGuardError::CorpusRequired),
        }
    }

    /// Resolve a document reference that is EITHER a UUID (passthrough) OR a
    /// readable handle `alias/relative_path`. Returns the stringified doc UUID.
    pub fn resolve_doc_ref(&self, doc_ref: &str) -> Result<String, CorpusGuardError> {
        // UUID passthrough.
        if uuid::Uuid::parse_str(doc_ref).is_ok() {
            return Ok(doc_ref.to_string());
        }
        // Handle form: split alias from the relative path at the first '/'.
        let (alias, relative) = doc_ref
            .split_once('/')
            .ok_or_else(|| CorpusGuardError::UnknownCorpus(doc_ref.to_string()))?;
        let corpus_id = self
            .store
            .lookup_by_alias(alias)
            .map_err(|_| CorpusGuardError::UnknownCorpus(alias.to_string()))?
            .ok_or_else(|| CorpusGuardError::UnknownCorpus(alias.to_string()))?;
        self.store
            .document_id_by_relative_path(corpus_id, relative)
            .map_err(|_| CorpusGuardError::UnknownCorpus(doc_ref.to_string()))?
            .map(|id| id.as_string())
            .ok_or_else(|| CorpusGuardError::UnknownCorpus(doc_ref.to_string()))
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

#[cfg(test)]
impl CorpusService {
    pub(crate) fn from_store_for_test(store: CorpusStore) -> Self {
        Self { store }
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use anno_corpus_core::{CorpusProfile, EffectiveCorpus};

    fn svc(dir: &std::path::Path) -> CorpusService {
        CorpusService::from_store_for_test(CorpusStore::open(dir.join("c.sqlite3")).expect("open"))
    }

    #[test]
    fn zero_corpus_no_cross_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        assert!(matches!(
            s.resolve_effective(None, false),
            Err(CorpusGuardError::NoCorpus)
        ));
    }

    #[test]
    fn zero_corpus_with_cross_is_cross() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        assert_eq!(
            s.resolve_effective(None, true).unwrap(),
            EffectiveCorpus::CrossCorpus
        );
    }

    #[test]
    fn resolves_by_alias() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        let reg = s
            .store
            .register_root(
                dir.path().join("a").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .unwrap();
        s.store.set_alias(reg.corpus_id, "2026-0042").unwrap();
        assert_eq!(
            s.resolve_effective(Some("2026-0042"), false).unwrap(),
            EffectiveCorpus::Single(reg.corpus_id)
        );
    }

    #[test]
    fn resolve_doc_ref_passthrough_uuid() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        let uuid = "a9ea6215-c656-5629-b75a-7054b3d6d911";
        // A syntactically valid UUID resolves to itself (no corpus needed).
        assert_eq!(s.resolve_doc_ref(uuid).unwrap(), uuid.to_string());
    }

    #[test]
    fn resolve_doc_ref_resolves_handle() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        let reg = s
            .store
            .register_root(
                dir.path().join("a").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .unwrap();
        s.store.set_alias(reg.corpus_id, "case-1").unwrap();
        let doc = anno_corpus_core::DocumentInstanceId::new(uuid::Uuid::nil());
        s.store
            .record_document_path(reg.corpus_id, doc, "contrats/x.txt")
            .unwrap();
        assert_eq!(
            s.resolve_doc_ref("case-1/contrats/x.txt").unwrap(),
            doc.as_string()
        );
    }

    #[test]
    fn resolve_doc_ref_unknown_alias_errors() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        assert!(s.resolve_doc_ref("ghost/contrats/x.txt").is_err());
    }
}
