//! Source, account, and scope domain types.

use crate::ids::{AccountId, ScopeId, SourceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported knowledge source families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A local filesystem folder.
    LocalFolder,
    /// Microsoft Outlook email.
    MicrosoftOutlook,
    /// Microsoft OneDrive file storage.
    MicrosoftOneDrive,
    /// Microsoft SharePoint document library.
    MicrosoftSharePoint,
    /// Gmail email.
    Gmail,
    /// Google Drive file storage.
    GoogleDrive,
    /// Slack messaging.
    Slack,
    /// Notion workspace.
    Notion,
}

/// Configured source integration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSource {
    /// Unique identifier for this source.
    pub source_id: SourceId,
    /// Kind of the source.
    pub kind: SourceKind,
    /// Pseudonymized display label shown in UI.
    pub display_label_pseudo: String,
    /// When this source was registered.
    pub created_at: DateTime<Utc>,
    /// Whether this source is currently enabled for sync.
    pub enabled: bool,
}

/// Account or local identity inside a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAccount {
    /// Unique identifier for this account.
    pub account_id: AccountId,
    /// Source this account belongs to.
    pub source_id: SourceId,
    /// Opaque provider-issued subject identifier.
    pub provider_subject: String,
    /// Optional tenant identifier (e.g. Azure AD tenant).
    pub tenant_id: Option<String>,
    /// Pseudonymized display label shown in UI.
    pub display_label_pseudo: String,
    /// OAuth or permission scopes granted for this account.
    pub scopes_granted: Vec<String>,
    /// Opaque reference to stored credentials (e.g. keychain key).
    pub auth_ref: Option<String>,
    /// When this account was first registered.
    pub created_at: DateTime<Utc>,
    /// When this account was last successfully contacted.
    pub last_seen_at: Option<DateTime<Utc>>,
}

/// Scope type selected inside a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    /// A local filesystem folder.
    LocalFolder,
    /// An email folder (inbox, sent, etc.).
    MailFolder,
    /// A cloud drive folder.
    DriveFolder,
    /// A messaging channel.
    Channel,
    /// A hierarchical page tree (e.g. Notion workspace).
    PageTree,
}

/// Sync policy for a selected scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPolicy {
    /// Whether this scope is actively synced.
    pub enabled: bool,
    /// Maximum number of pages to process per sync run.
    pub max_pages_per_run: u32,
    /// Whether to download and index file attachments.
    pub include_attachments: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_pages_per_run: 5,
            include_attachments: false,
        }
    }
}

/// Selectable area inside a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceScope {
    /// Unique identifier for this scope.
    pub scope_id: ScopeId,
    /// Account this scope belongs to.
    pub account_id: AccountId,
    /// Kind of this scope.
    pub kind: ScopeKind,
    /// Opaque provider-issued key for this scope.
    pub provider_key: String,
    /// Pseudonymized display label shown in UI.
    pub display_label_pseudo: String,
    /// Sync policy applied to this scope.
    pub sync_policy: SyncPolicy,
    /// Whether this scope is currently enabled.
    pub enabled: bool,
}
