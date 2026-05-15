//! v0.1 stub — types only. Logic lands in Tasks 2+.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Memory category. `Fact` and `Preference` are first-class for retrieval;
/// `Reference` and `Context` are reserved for v0.2's conflict-resolution and
/// transient-context paths.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum MemoryKind {
    /// Stable factual claim ("the cabinet has 12 lawyers").
    Fact,
    /// User/session preference ("the user prefers tables to prose").
    Preference,
    /// Pointer to a canonical entity that other memories cite.
    Reference,
    /// Transient session context (current dossier, current task).
    Context,
}

/// Unique memory id. v7 UUIDs sort lexicographically by time, which makes
/// `ORDER BY id` equivalent to `ORDER BY created_at` for paging.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub Uuid);

impl MemoryId {
    /// Fresh time-sortable id.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Display form (canonical UUID string).
    #[must_use]
    pub fn as_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

/// One pseudonym reference inside a memory's body. `label` is the cleartext
/// span the detector found ("Marie Dupont"); `token` is the vault token
/// emitted ("PERSON_42"). Used by [`Pipeline::forget_memory`] to cascade
/// erasure into the vault.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct TokenRef {
    /// Detector label or category (for human review).
    pub label: String,
    /// Pseudo-token in the vault.
    pub token: String,
}

/// One memory record. `text` is always the PII-tokenized form;
/// cleartext PII NEVER persists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique id.
    pub id: MemoryId,
    /// Cowork session id, if any.
    pub session_id: Option<String>,
    /// Category.
    pub kind: MemoryKind,
    /// PII-tokenized text. NEVER plaintext.
    pub text: String,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Most recent recall timestamp (UTC). Equals `created_at` on insert.
    pub accessed_at: DateTime<Utc>,
    /// v0.2 forward-compat. Populated as `created_at` in v0.1.
    pub valid_from: DateTime<Utc>,
    /// v0.2 forward-compat. Always `None` in v0.1.
    pub valid_to: Option<DateTime<Utc>>,
    /// e5-small embedding of `text` (dim = 384).
    pub embedding: Vec<f32>,
    /// Pseudonym references — drives the vault-cascade erasure path.
    pub token_refs: Vec<TokenRef>,
    /// v0.2 forward-compat. Always empty in v0.1.
    pub entity_refs: Vec<String>,
}

/// One hit returned by [`Pipeline::recall_memory`]. Text is rehydrated
/// (plaintext) at the boundary — the stored form on disk stays tokenized.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryHit {
    /// Stringified [`MemoryId`].
    pub id: String,
    /// Rehydrated text (vault tokens replaced by originals when known).
    pub text: String,
    /// Category.
    pub kind: MemoryKind,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// Hybrid retrieval score (higher = better; produced by `RRFReranker`).
    pub score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_kind_round_trip_json() {
        for k in [
            MemoryKind::Fact,
            MemoryKind::Preference,
            MemoryKind::Reference,
            MemoryKind::Context,
        ] {
            let s = serde_json::to_string(&k).unwrap();
            let back: MemoryKind = serde_json::from_str(&s).unwrap();
            assert_eq!(k, back);
        }
    }

    #[test]
    fn memory_id_is_time_sortable() {
        let a = MemoryId::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = MemoryId::new();
        assert!(a.as_string() < b.as_string(), "v7 ids are time-sortable");
    }

    #[test]
    fn token_ref_round_trip_json() {
        let r = TokenRef {
            label: "Marie Dupont".into(),
            token: "PERSON_42".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: TokenRef = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }
}
