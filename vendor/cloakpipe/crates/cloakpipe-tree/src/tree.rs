//! Tree data structures for hierarchical document indexing.
//!
//! A TreeIndex represents a document as a navigable hierarchy of nodes,
//! similar to an intelligent table of contents optimized for LLM reasoning.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A complete tree index for one document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeIndex {
    /// Unique identifier for this tree index.
    pub id: String,
    /// Original document filename/path.
    pub source: String,
    /// Document-level description/summary.
    pub description: Option<String>,
    /// Total number of pages in the source document.
    pub total_pages: usize,
    /// The root nodes of the tree (top-level chapters/sections).
    pub children: Vec<TreeNode>,
    /// When this index was created.
    pub created_at: DateTime<Utc>,
    /// Which LLM model was used for summarization.
    pub model: String,
    /// Whether summaries have been pseudonymized.
    pub pseudonymized: bool,
}

/// A single node in the tree hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    /// Node identifier (e.g., "1", "1.2", "1.2.3").
    pub id: String,
    /// Section title extracted from the document.
    pub title: String,
    /// LLM-generated summary of this section's content.
    pub summary: Option<NodeSummary>,
    /// Page range this node covers [start, end] (1-indexed, inclusive).
    pub pages: (usize, usize),
    /// Approximate token count of the raw text in this node.
    pub token_count: Option<usize>,
    /// Child nodes (subsections).
    pub children: Vec<TreeNode>,
    /// Depth level in the tree (0 = root children).
    pub depth: usize,
}

/// Summary information for a tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSummary {
    /// The summary text (may be pseudonymized).
    pub text: String,
    /// Key topics/entities mentioned in this section.
    pub key_topics: Vec<String>,
    /// Whether this summary has been pseudonymized.
    pub pseudonymized: bool,
}

/// Result of a tree search — identifies which nodes are relevant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Ordered list of relevant node IDs (most relevant first).
    pub node_ids: Vec<String>,
    /// The LLM's reasoning trace for why these nodes were selected.
    pub reasoning: String,
    /// Confidence score (0.0–1.0) if available.
    pub confidence: Option<f64>,
}

/// Extracted content from a tree node (for RAG context).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedContent {
    /// The node ID this content came from.
    pub node_id: String,
    /// The full text content of this section.
    pub text: String,
    /// Page range.
    pub pages: (usize, usize),
    /// The node title (for citation/traceability).
    pub title: String,
}

impl TreeIndex {
    /// Create a new empty tree index for a document.
    pub fn new(source: &str, model: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source: source.to_string(),
            description: None,
            total_pages: 0,
            children: Vec::new(),
            created_at: Utc::now(),
            model: model.to_string(),
            pseudonymized: false,
        }
    }

    /// Find a node by its ID (e.g., "1.2.3").
    pub fn find_node(&self, id: &str) -> Option<&TreeNode> {
        fn search<'a>(nodes: &'a [TreeNode], id: &str) -> Option<&'a TreeNode> {
            for node in nodes {
                if node.id == id {
                    return Some(node);
                }
                if let Some(found) = search(&node.children, id) {
                    return Some(found);
                }
            }
            None
        }
        search(&self.children, id)
    }

    /// Get all node IDs and titles (for presenting to the LLM during search).
    pub fn navigation_map(&self) -> Vec<NavigationEntry> {
        let mut entries = Vec::new();
        fn collect(nodes: &[TreeNode], entries: &mut Vec<NavigationEntry>) {
            for node in nodes {
                entries.push(NavigationEntry {
                    id: node.id.clone(),
                    title: node.title.clone(),
                    summary: node.summary.as_ref().map(|s| s.text.clone()),
                    depth: node.depth,
                    pages: node.pages,
                    has_children: !node.children.is_empty(),
                });
                collect(&node.children, entries);
            }
        }
        collect(&self.children, &mut entries);
        entries
    }

    /// Count total nodes in the tree.
    pub fn node_count(&self) -> usize {
        fn count(nodes: &[TreeNode]) -> usize {
            nodes.iter().map(|n| 1 + count(&n.children)).sum()
        }
        count(&self.children)
    }

    /// Get the maximum depth of the tree.
    pub fn max_depth(&self) -> usize {
        fn depth(nodes: &[TreeNode]) -> usize {
            nodes.iter().map(|n| {
                if n.children.is_empty() {
                    n.depth
                } else {
                    depth(&n.children)
                }
            }).max().unwrap_or(0)
        }
        depth(&self.children)
    }
}

/// A simplified node entry for LLM navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationEntry {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub depth: usize,
    pub pages: (usize, usize),
    pub has_children: bool,
}

impl std::fmt::Display for NavigationEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let indent = "  ".repeat(self.depth);
        let summary = self.summary.as_deref().unwrap_or("");
        write!(
            f,
            "{}[{}] {} (pages {}-{}): {}",
            indent, self.id, self.title, self.pages.0, self.pages.1, summary
        )
    }
}
