//! v0.1 stub — types only. Logic lands in Tasks 2+.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Memory category. `Fact` and `Preference` are first-class for retrieval;
/// `Reference` and `Context` are reserved for v0.2's conflict-resolution and
/// transient-context paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

/// On-disk hit returned by `Store::memories_hybrid_search` — text is the
/// **tokenized** form (still PII-safe). The Pipeline layer rehydrates
/// before returning to the caller as [`MemoryHit`].
#[derive(Debug, Clone)]
pub struct MemoryHitRow {
    /// Stringified [`MemoryId`].
    pub id: String,
    /// Cowork session id (if any) — used by the pipeline's session filter.
    pub session_id: Option<String>,
    /// Pseudonymized text as stored on disk.
    pub text_tokenized: String,
    /// Category.
    pub kind: MemoryKind,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// Bi-temporal start (μs since epoch). Equals `created_at` until v0.2
    /// invalidation lands; v0.2 conflict resolver may override.
    pub valid_from_us: i64,
    /// Bi-temporal end (μs since epoch). `None` means still valid.
    pub valid_to_us: Option<i64>,
    /// Canonicalised entity references (`pii:...` + `ent:...`) — the
    /// LabelList index payload v0.2 uses for graph traversal.
    pub entity_refs: Vec<String>,
    /// Hybrid retrieval score (RRF; higher is better).
    pub score: f32,
}

/// Where a hit came from. v0.2 introduces a second path — graph-expansion
/// over `entity_refs` (LabelList scan) — alongside the hybrid arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HitProvenance {
    /// Vector + FTS RRF-reranked hit.
    Hybrid,
    /// LabelList-indexed 2-hop graph traversal hit.
    GraphExpand,
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
    /// Bi-temporal start, RFC 3339.
    pub valid_from: String,
    /// Bi-temporal end, RFC 3339. `None` while the row is still valid.
    pub valid_to: Option<String>,
    /// Canonicalised entity references (LabelList payload).
    pub entity_refs: Vec<String>,
    /// Hybrid retrieval score (higher = better; produced by `RRFReranker`).
    /// For `HitProvenance::GraphExpand`, this is a co-occurrence-derived
    /// score (v0.2 T5).
    pub score: f32,
    /// Which path produced this hit. See [`HitProvenance`].
    pub via: HitProvenance,
}

/// Entity-source tag for the graph-recall wire shape.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKindWire {
    /// Vault-tokenized PII entity (resolves only inside the tenant).
    PiiToken,
    /// Non-PII NER entity (canonical text form).
    NamedEntity,
}

/// One node in [`GraphRecallResult`]. `id` is the canonical entity key
/// (`pii:...` or `ent:...`); `display` is the human-readable form
/// (rehydrated for PII; the canonical tail for named entities).
#[derive(Debug, Clone, Serialize)]
pub struct EntityNode {
    /// Canonical entity id (`pii:LABEL:TOKEN` or `ent:TAG:value`).
    pub id: String,
    /// Display string — token rehydrated for PII, lowercase canonical
    /// tail for named entities.
    pub display: String,
    /// Source tag (PII vault vs NER).
    pub kind: EntityKindWire,
    /// Number of memories in the subgraph that reference this node.
    pub mention_count: u32,
}

/// One edge in [`GraphRecallResult`] — a memory `via` connecting two
/// entities. The same memory can produce multiple edges if it references
/// more than two entities.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryEdge {
    /// Source entity id.
    pub from: String,
    /// Memory id that carries the co-occurrence.
    pub via: String,
    /// Target entity id.
    pub to: String,
}

/// Output of `Pipeline::graph_recall` — the connected subgraph reached
/// from a seed entity in at most N hops over the `entity_refs` LabelList.
#[derive(Debug, Clone, Serialize)]
pub struct GraphRecallResult {
    /// Canonical seed entity id (the resolved form of the input).
    pub seed: String,
    /// For PII seeds, the rehydrated plaintext (if known). `None` for
    /// named-entity seeds and for unknown PII tokens.
    pub seed_resolved: Option<String>,
    /// Entity nodes reached in the traversal, with mention counts.
    pub nodes: Vec<EntityNode>,
    /// Co-occurrence edges between entities.
    pub edges: Vec<MemoryEdge>,
    /// Memories visited along the traversal, rehydrated. `via =
    /// HitProvenance::GraphExpand` on each.
    pub memories: Vec<MemoryHit>,
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
