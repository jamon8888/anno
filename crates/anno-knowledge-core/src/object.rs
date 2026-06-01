//! Knowledge object, part, revision, and state types.

use crate::ids::{AccountId, ObjectId, PartId, RevisionId, ScopeId, SourceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Logical object type from a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    /// An email message.
    Email,
    /// A file or document.
    File,
    /// A chat or messaging post.
    ChatMessage,
    /// A wiki or notebook page.
    Page,
}

/// Extracted part type inside an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartType {
    /// The main body of an email.
    EmailBody,
    /// The body of an attachment extracted from an email.
    AttachmentBody,
    /// The body of a standalone file.
    FileBody,
    /// Structured metadata extracted from the object.
    Metadata,
}

/// Current processing state for an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectState {
    /// Object found in source but not yet extracted.
    Discovered,
    /// Text extracted; awaiting privacy gateway pseudonymization.
    ExtractedPendingPrivacy,
    /// Pseudonymized and ready for vector indexing.
    Pseudonymized,
    /// Indexed into the local vector store.
    VectorIndexed,
    /// Processing deferred due to local resource budget.
    DeferredBudget,
    /// Failed with a transient error; eligible for retry.
    FailedRetryable,
    /// Failed with a permanent error; will not be retried.
    FailedPermanent,
    /// Erased by user request (GDPR Art. 17).
    Forgotten,
}

/// One logical source object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeObject {
    /// Unique identifier for this object.
    pub object_id: ObjectId,
    /// Source this object was fetched from.
    pub source_id: SourceId,
    /// Account used to access this object.
    pub account_id: AccountId,
    /// Scope within which this object was discovered.
    pub scope_id: ScopeId,
    /// Provider-issued identifier for this object.
    pub external_id: String,
    /// Logical type of this object.
    pub object_type: ObjectType,
    /// Raw (non-pseudonymized) title, if available.
    pub title_raw: Option<String>,
    /// Raw provider metadata as a JSON blob.
    pub metadata_raw: serde_json::Value,
    /// Source URL for deep-linking back to the original.
    pub source_url: Option<String>,
    /// When the provider last modified this object.
    pub source_updated_at: DateTime<Utc>,
    /// Current processing state.
    pub state: ObjectState,
}

/// One extracted part of a source object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePart {
    /// Unique identifier for this part.
    pub part_id: PartId,
    /// Parent object identifier.
    pub object_id: ObjectId,
    /// Type of extracted content.
    pub part_type: PartType,
    /// Pseudonymized title for this part, if available.
    pub title_pseudo: Option<String>,
    /// Pseudonymized metadata as a JSON blob.
    pub metadata_pseudo: serde_json::Value,
    /// Number of characters in the extracted text.
    pub extracted_chars: u32,
}

/// Object revision based on provider version or content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRevision {
    /// Unique identifier for this revision.
    pub revision_id: RevisionId,
    /// Object this revision belongs to.
    pub object_id: ObjectId,
    /// Provider-issued version token (etag, hash, sequence number).
    pub provider_version: String,
    /// When this revision was observed by the sync engine.
    pub observed_at: DateTime<Utc>,
}
