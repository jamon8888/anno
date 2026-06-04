//! Legal maintenance helpers that do not load models or initialize Pipeline.

use anno_rag::{
    config::AnnoRagConfig,
    legal::{
        kg::{LanceGraphStore, LegalKnowledgeGraph},
        status::EnrichmentStatusStore,
        store::LegalStore,
    },
    store::Store,
};
use uuid::Uuid;

/// Lightweight legal maintenance service for MCP status/sources/forget.
pub struct LegalMaintenanceService {
    store: Store,
    legal_store: LegalStore,
    legal_kg: LanceGraphStore,
    enrichment_status: EnrichmentStatusStore,
}

impl LegalMaintenanceService {
    /// Open all legal stores needed for maintenance.
    ///
    /// # Errors
    /// Returns store, legal enrichment, graph, or status-store open errors.
    pub async fn open(cfg: &AnnoRagConfig) -> anno_rag::Result<Self> {
        Ok(Self {
            store: Store::open(cfg).await?,
            legal_store: LegalStore::open(cfg).await?,
            legal_kg: LanceGraphStore::open(cfg).await?,
            enrichment_status: EnrichmentStatusStore::open(cfg).await?,
        })
    }

    /// Count all chunks in the main RAG store.
    ///
    /// # Errors
    /// Returns store errors on LanceDB count failure.
    pub async fn count_chunks(&self) -> anno_rag::Result<u64> {
        self.store.count_chunks().await
    }

    /// List distinct indexed legal folder paths from the main RAG store.
    ///
    /// # Errors
    /// Returns store errors on LanceDB query/decode failure.
    pub async fn list_indexed_folder_paths(&self) -> anno_rag::Result<Vec<String>> {
        self.store.list_indexed_folder_paths().await
    }

    /// Resolve a stable MCP folder id back to a folder path.
    ///
    /// # Errors
    /// Returns store errors while listing folder paths.
    pub async fn resolve_folder_id(
        &self,
        id: &str,
        to_id: impl Fn(&str) -> String,
    ) -> anno_rag::Result<Option<String>> {
        let paths = self.list_indexed_folder_paths().await?;
        Ok(paths.into_iter().find(|path| to_id(path) == id))
    }

    /// Delete legal state whose `source_path` is inside `path`.
    ///
    /// # Errors
    /// Returns store/legal/graph/status errors on cascade or chunk deletion failure.
    pub async fn forget_folder_path(&self, path: &str) -> anno_rag::Result<u64> {
        let doc_ids = self.store.doc_ids_for_source_subtree(path).await?;
        self.delete_auxiliary_doc_rows(&doc_ids).await?;
        let report = self.store.delete_folder_rows(path).await?;
        Ok(report.removed_chunks)
    }

    /// Delete legal state for exact document ids.
    ///
    /// # Errors
    /// Returns store/legal/graph/status errors on cascade or chunk deletion failure.
    pub async fn forget_doc_ids(&self, doc_ids: &[Uuid]) -> anno_rag::Result<u64> {
        self.delete_auxiliary_doc_rows(doc_ids).await?;
        self.store.delete_doc_id_rows(doc_ids).await
    }

    async fn delete_auxiliary_doc_rows(&self, doc_ids: &[Uuid]) -> anno_rag::Result<()> {
        for doc_id in doc_ids {
            self.legal_store.delete_doc(*doc_id).await?;
            self.legal_kg.delete_doc(*doc_id).await?;
            self.enrichment_status.delete_doc(*doc_id).await?;
        }
        Ok(())
    }
}
