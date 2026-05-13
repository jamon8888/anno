//! Extract full text content from selected tree nodes for RAG context.

use crate::tree::{TreeIndex, ExtractedContent};
use crate::indexer::ParsedPage;
use anyhow::Result;

/// Extracts full text from tree nodes identified by search.
pub struct ContentExtractor;

impl ContentExtractor {
    /// Extract content from specific nodes by ID.
    pub fn extract(
        tree: &TreeIndex,
        node_ids: &[String],
        pages: &[ParsedPage],
    ) -> Result<Vec<ExtractedContent>> {
        let mut results = Vec::new();

        for id in node_ids {
            if let Some(node) = tree.find_node(id) {
                let text: String = pages.iter()
                    .filter(|p| p.page_number >= node.pages.0 && p.page_number <= node.pages.1)
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");

                results.push(ExtractedContent {
                    node_id: id.clone(),
                    text,
                    pages: node.pages,
                    title: node.title.clone(),
                });
            }
        }

        Ok(results)
    }
}
