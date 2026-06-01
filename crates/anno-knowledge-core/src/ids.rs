//! Typed deterministic UUID identifiers for knowledge entities.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const KNOWLEDGE_NAMESPACE: Uuid = Uuid::from_u128(0x67a2_4aa5_6f8d_4f88_9c53_3d6f_0f42_9b21);

/// Minimal source kind enum used by deterministic ID builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKindForId {
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

impl SourceKindForId {
    fn as_stable_str(self) -> &'static str {
        match self {
            Self::LocalFolder => "local_folder",
            Self::MicrosoftOutlook => "microsoft_outlook",
            Self::MicrosoftOneDrive => "microsoft_onedrive",
            Self::MicrosoftSharePoint => "microsoft_sharepoint",
            Self::Gmail => "gmail",
            Self::GoogleDrive => "google_drive",
            Self::Slack => "slack",
            Self::Notion => "notion",
        }
    }
}

macro_rules! typed_id {
    ($name:ident) => {
        #[doc = concat!("Typed UUID wrapper for ", stringify!($name), ".")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Wraps a raw [`Uuid`].
            #[must_use]
            pub const fn new(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the underlying [`Uuid`].
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }

            /// Returns the UUID formatted as a hyphenated string.
            #[must_use]
            pub fn as_string(self) -> String {
                self.0.to_string()
            }
        }
    };
}

typed_id!(SourceId);
typed_id!(AccountId);
typed_id!(ScopeId);
typed_id!(ObjectId);
typed_id!(PartId);
typed_id!(RevisionId);
typed_id!(ChunkId);

fn stable_uuid(parts: &[&str]) -> Uuid {
    let stable = parts.join("\u{1f}");
    Uuid::new_v5(&KNOWLEDGE_NAMESPACE, stable.as_bytes())
}

impl SourceId {
    /// Derives a deterministic [`SourceId`] from a source kind and a stable key.
    #[must_use]
    pub fn from_parts(kind: SourceKindForId, stable_key: &str) -> Self {
        Self(stable_uuid(&["source", kind.as_stable_str(), stable_key]))
    }
}

impl AccountId {
    /// Derives a deterministic [`AccountId`] from a parent [`SourceId`] and a provider subject.
    #[must_use]
    pub fn from_parts(source_id: SourceId, provider_subject: &str) -> Self {
        Self(stable_uuid(&["account", &source_id.as_string(), provider_subject]))
    }
}

impl ScopeId {
    /// Derives a deterministic [`ScopeId`] from a parent [`AccountId`] and a provider key.
    #[must_use]
    pub fn from_parts(account_id: AccountId, provider_key: &str) -> Self {
        Self(stable_uuid(&["scope", &account_id.as_string(), provider_key]))
    }
}

impl ObjectId {
    /// Derives a deterministic [`ObjectId`] from source kind, account key, scope key, and external ID.
    #[must_use]
    pub fn from_external(
        kind: SourceKindForId,
        account_key: &str,
        scope_key: &str,
        external_id: &str,
    ) -> Self {
        Self(stable_uuid(&["object", kind.as_stable_str(), account_key, scope_key, external_id]))
    }
}

impl PartId {
    /// Derives a deterministic [`PartId`] from an object key and a part key.
    #[must_use]
    pub fn from_parts(object_key: &str, part_key: &str) -> Self {
        Self(stable_uuid(&["part", object_key, part_key]))
    }
}

impl RevisionId {
    /// Derives a deterministic [`RevisionId`] from an object key and a provider version token.
    #[must_use]
    pub fn from_parts(object_key: &str, provider_version: &str) -> Self {
        Self(stable_uuid(&["revision", object_key, provider_version]))
    }
}

impl ChunkId {
    /// Derives a deterministic [`ChunkId`] from a [`RevisionId`], [`PartId`], and chunk index.
    #[must_use]
    pub fn from_parts(revision_id: RevisionId, part_id: PartId, chunk_idx: u32) -> Self {
        Self(stable_uuid(&[
            "chunk",
            &revision_id.as_string(),
            &part_id.as_string(),
            &chunk_idx.to_string(),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_is_stable_for_same_kind_and_key() {
        let first = SourceId::from_parts(SourceKindForId::MicrosoftOutlook, "tenant-a:user-a");
        let second = SourceId::from_parts(SourceKindForId::MicrosoftOutlook, "tenant-a:user-a");

        assert_eq!(first, second);
        assert_eq!(first.as_uuid().get_version_num(), 5);
    }

    #[test]
    fn object_id_changes_when_scope_changes() {
        let inbox = ObjectId::from_external(
            SourceKindForId::MicrosoftOutlook,
            "account-a",
            "inbox",
            "immutable-message-id",
        );
        let sent = ObjectId::from_external(
            SourceKindForId::MicrosoftOutlook,
            "account-a",
            "sent",
            "immutable-message-id",
        );

        assert_ne!(inbox, sent);
    }

    #[test]
    fn chunk_id_is_stable_for_revision_part_and_index() {
        let revision = RevisionId::from_parts("object-a", "version-a");
        let part = PartId::from_parts("object-a", "body");

        let first = ChunkId::from_parts(revision, part, 7);
        let second = ChunkId::from_parts(revision, part, 7);

        assert_eq!(first, second);
    }
}
