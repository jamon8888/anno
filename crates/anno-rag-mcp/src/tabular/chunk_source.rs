//! Adapter: `Pipeline` → `ChunkSource` for the tabular extraction engine.

use anno_rag::pipeline::Pipeline;
use anno_rag_tabular::error::{Error as TabularError, Result as TabularResult};
use anno_rag_tabular::extract::{ChunkRef, ChunkSource};
use async_trait::async_trait;
use std::sync::Arc;

/// Wraps [`Pipeline`] so the tabular [`anno_rag_tabular::extract::Extractor`]
/// can retrieve pseudonymized chunks from the anno-rag index.
pub struct PipelineChunkSource(pub Arc<Pipeline>);

#[async_trait]
impl ChunkSource for PipelineChunkSource {
    async fn chunks_for_doc(&self, doc_id: uuid::Uuid) -> TabularResult<Vec<ChunkRef>> {
        let hits = self
            .0
            .chunks_by_doc(doc_id)
            .await
            .map_err(|e| TabularError::Extract {
                doc: doc_id.to_string(),
                col: "?".into(),
                source: Box::new(e),
            })?;
        Ok(hits
            .into_iter()
            .map(|h| ChunkRef {
                id: h.chunk_id,
                doc_id: h.doc_id,
                content: h.text_pseudo,
                page: h.page,
            })
            .collect())
    }

    async fn chunk_by_id(&self, chunk_id: uuid::Uuid) -> TabularResult<Option<ChunkRef>> {
        let hit = self
            .0
            .chunk_by_id(chunk_id)
            .await
            .map_err(|e| TabularError::Extract {
                doc: "?".into(),
                col: "?".into(),
                source: Box::new(e),
            })?;
        Ok(hit.map(|h| ChunkRef {
            id: h.chunk_id,
            doc_id: h.doc_id,
            content: h.text_pseudo,
            page: h.page,
        }))
    }
}
