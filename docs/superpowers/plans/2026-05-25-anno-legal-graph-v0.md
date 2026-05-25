# Anno Legal Graph v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the no-op legal graph backend with a typed local SQLite graph that persists legal nodes/edges and powers the five named legal graph intents.

**Architecture:** Keep `LegalKnowledgeGraph` as the stable application boundary. Rename the current no-op `LanceGraphStore` into a real local store implementation backed by `rusqlite`, while leaving MCP contracts intact. `legal::query` will dispatch typed intents through backend methods instead of relying on arbitrary Cypher.

**Tech Stack:** Rust, `anno-rag`, `rusqlite`, `serde_json`, `uuid`, `chrono`, `async-trait`, existing `LegalKnowledgeGraph` trait, existing MCP legal tools.

---

## Scope Check

This plan implements Legal Graph v0 only. It does not implement `lance-graph` v1, generic Cypher, UI changes, or new legal extraction rules. It produces working, testable software independently: graph batches are persisted, named intents return rows, and delete/validation behavior is covered.

## File Structure

- Modify `crates/anno-rag/Cargo.toml`: add `rusqlite` to `anno-rag` dependencies through the workspace dependency.
- Modify `crates/anno-rag/src/legal/kg.rs`: implement SQLite-backed graph persistence, node/edge serialization, delete, link hints, compact, and typed query helpers.
- Modify `crates/anno-rag/src/legal/query.rs`: route `GraphIntent` through typed graph helper methods instead of free-form Cypher templates.
- Modify `crates/anno-rag/src/pipeline.rs`: update stale no-op documentation comments and preserve current ingest dual-write calls.
- Add `crates/anno-rag/tests/legal_graph_v0.rs`: backend conformance, intent, delete, and validation tests.
- Update `docs/superpowers/specs/2026-05-25-anno-legal-graph-v0-design.md` only if implementation decisions differ from the spec.

---

## Task 1: Add SQLite Dependency to `anno-rag`

**Files:**
- Modify: `crates/anno-rag/Cargo.toml`
- Verify: `Cargo.lock`

- [ ] **Step 1: Add the dependency**

In `crates/anno-rag/Cargo.toml`, add `rusqlite` near the other workspace dependencies:

```toml
rusqlite            = { workspace = true }
```

The dependency already exists in the workspace root as:

```toml
rusqlite = { version = "0.37", features = ["bundled"] }
```

- [ ] **Step 2: Refresh the lockfile**

Run:

```powershell
cargo check -p anno-rag --lib
```

Expected: PASS. `Cargo.lock` should not gain a new high-risk tree because `rusqlite` is already present in the workspace.

- [ ] **Step 3: Commit**

```powershell
git add crates/anno-rag/Cargo.toml Cargo.lock
git commit -m "deps(legal): enable sqlite graph backend"
```

---

## Task 2: Write Backend Conformance Tests First

**Files:**
- Add: `crates/anno-rag/tests/legal_graph_v0.rs`

- [ ] **Step 1: Create failing tests**

Create `crates/anno-rag/tests/legal_graph_v0.rs`:

```rust
use anno_rag::config::AnnoRagConfig;
use anno_rag::legal::kg::{EdgeBatch, EdgeWrite, LegalKnowledgeGraph, NodeBatch, NodeWrite, SqliteLegalGraphStore};
use anno_rag::legal::query::{run_intent, GraphIntent};
use anno_rag::legal::types::{ArticleRef, PartyKind};
use chrono::Utc;
use std::collections::HashMap;
use tempfile::TempDir;
use uuid::Uuid;

fn cfg_in(dir: &TempDir) -> AnnoRagConfig {
    let mut cfg = AnnoRagConfig::default();
    cfg.data_dir = dir.path().join("data");
    cfg.index_dir = dir.path().join("index.lance");
    cfg
}

async fn store() -> (TempDir, SqliteLegalGraphStore) {
    let dir = TempDir::new().expect("tempdir");
    let cfg = cfg_in(&dir);
    let kg = SqliteLegalGraphStore::open(&cfg).await.expect("open sqlite legal graph");
    (dir, kg)
}

fn seeded_contract_graph(doc_id: Uuid, chunk_id: Uuid) -> (NodeBatch, EdgeBatch) {
    let party_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"org:acme");
    let obligation_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"obligation:pay:acme");
    let article_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"code_civil:1103");

    let mut nodes = NodeBatch::new();
    nodes.add_document(
        doc_id,
        Some("contract".into()),
        Some("civil".into()),
        Some("fr".into()),
        Some(Utc::now()),
        Some("dossier-1".into()),
    );
    nodes.add_chunk(chunk_id, doc_id, 10, 80, Some(1));
    nodes.nodes.push(NodeWrite::Party {
        party_id,
        kind: PartyKind::Organization,
        canonical_name: "org:acme".into(),
        normalized_form: "org:acme".into(),
        siren: None,
    });
    nodes.nodes.push(NodeWrite::Obligation {
        obligation_id,
        kind: "payment".into(),
        text_pseudo: "payer la facture sous trente jours".into(),
    });
    nodes.nodes.push(NodeWrite::Article {
        article_id,
        article: ArticleRef {
            code: "code_civil".into(),
            article_num: "1103".into(),
        },
    });

    let mut edges = EdgeBatch::new();
    edges.edges.push(EdgeWrite {
        from_label: "Document",
        from_key: doc_id.to_string(),
        to_label: "Chunk",
        to_key: chunk_id.to_string(),
        edge_type: "HAS_CHUNK",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Party",
        from_key: party_id.to_string(),
        to_label: "Document",
        to_key: doc_id.to_string(),
        edge_type: "PARTY_TO",
        props: HashMap::from([("role".into(), "client".into())]),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Party",
        from_key: party_id.to_string(),
        to_label: "Obligation",
        to_key: obligation_id.to_string(),
        edge_type: "BOUND_BY",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Obligation",
        to_key: obligation_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::from([("byte_start".into(), "10".into()), ("byte_end".into(), "80".into())]),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Document",
        from_key: doc_id.to_string(),
        to_label: "Article",
        to_key: article_id.to_string(),
        edge_type: "REFERENCES",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Article",
        to_key: article_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });

    (nodes, edges)
}

#[tokio::test]
async fn sqlite_graph_persists_nodes_and_edges() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);

    kg.upsert_batch(&nodes, &edges).await.expect("upsert graph");

    let result = run_intent(
        &kg,
        GraphIntent::PartyDossier {
            party: "org:acme".into(),
        },
    )
    .await
    .expect("party dossier");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].get("doc_id").map(String::as_str), Some(doc_id.to_string().as_str()));
    assert_eq!(result.rows[0].get("chunk_id").map(String::as_str), Some(chunk_id.to_string().as_str()));
    assert_eq!(result.rows[0].get("role").map(String::as_str), Some("client"));
}

#[tokio::test]
async fn obligations_owed_by_returns_source_chunk() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert graph");

    let result = run_intent(
        &kg,
        GraphIntent::ObligationsOwedBy {
            party: "org:acme".into(),
        },
    )
    .await
    .expect("obligations");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].get("obligation_kind").map(String::as_str), Some("payment"));
    assert_eq!(result.rows[0].get("chunk_id").map(String::as_str), Some(chunk_id.to_string().as_str()));
}

#[tokio::test]
async fn delete_doc_removes_document_scoped_edges() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert graph");

    kg.delete_doc(doc_id).await.expect("delete doc");

    let result = run_intent(
        &kg,
        GraphIntent::PartyDossier {
            party: "org:acme".into(),
        },
    )
    .await
    .expect("party dossier after delete");

    assert!(result.rows.is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```powershell
cargo test -p anno-rag --test legal_graph_v0
```

Expected: FAIL because `SqliteLegalGraphStore` does not exist yet.

- [ ] **Step 3: Commit the failing tests**

```powershell
git add crates/anno-rag/tests/legal_graph_v0.rs
git commit -m "test(legal): define graph v0 backend behavior"
```

---

## Task 3: Implement SQLite Store Schema and Batch Upsert

**Files:**
- Modify: `crates/anno-rag/src/legal/kg.rs`

- [ ] **Step 1: Add imports and the new store type**

At the top of `kg.rs`, extend imports:

```rust
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
```

Replace the no-op `LanceGraphStore` section with this real store type while keeping a compatibility alias:

```rust
/// SQLite-backed legal knowledge graph handle.
pub struct SqliteLegalGraphStore {
    db_path: PathBuf,
}

/// Backwards-compatible name used by existing pipeline wiring.
pub type LanceGraphStore = SqliteLegalGraphStore;
```

- [ ] **Step 2: Add schema initialization**

Add this implementation block:

```rust
impl SqliteLegalGraphStore {
    /// Open or create the legal KG SQLite database under `cfg.index_path()/legal_kg.sqlite`.
    ///
    /// # Errors
    /// Returns [`Error::Graph`] if directories, SQLite open, or schema setup fail.
    pub async fn open(cfg: &crate::config::AnnoRagConfig) -> Result<Self> {
        let root = cfg.index_path();
        std::fs::create_dir_all(&root).map_err(|e| Error::Graph(format!("mkdir legal_kg: {e}")))?;
        let db_path = root.join("legal_kg.sqlite");
        let store = Self { db_path };
        store.with_conn(Self::init_schema)?;
        Ok(store)
    }

    /// SQLite database path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.db_path
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| Error::Graph(format!("sqlite open {}: {e}", self.db_path.display())))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| Error::Graph(format!("sqlite journal_mode: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| Error::Graph(format!("sqlite foreign_keys: {e}")))?;
        f(&conn)
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS legal_nodes (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                doc_id TEXT,
                chunk_id TEXT,
                normalized_key TEXT NOT NULL,
                display TEXT,
                props_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_legal_nodes_label_key
                ON legal_nodes(label, normalized_key);
            CREATE INDEX IF NOT EXISTS idx_legal_nodes_doc
                ON legal_nodes(doc_id);
            CREATE INDEX IF NOT EXISTS idx_legal_nodes_chunk
                ON legal_nodes(chunk_id);

            CREATE TABLE IF NOT EXISTS legal_edges (
                id TEXT PRIMARY KEY,
                from_label TEXT NOT NULL,
                from_key TEXT NOT NULL,
                to_label TEXT NOT NULL,
                to_key TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                doc_id TEXT,
                chunk_id TEXT,
                props_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_legal_edges_unique
                ON legal_edges(from_label, from_key, edge_type, to_label, to_key, doc_id, chunk_id);
            CREATE INDEX IF NOT EXISTS idx_legal_edges_from
                ON legal_edges(from_label, from_key, edge_type);
            CREATE INDEX IF NOT EXISTS idx_legal_edges_to
                ON legal_edges(to_label, to_key, edge_type);
            CREATE INDEX IF NOT EXISTS idx_legal_edges_doc
                ON legal_edges(doc_id);
            CREATE INDEX IF NOT EXISTS idx_legal_edges_chunk
                ON legal_edges(chunk_id);
            "#,
        )
        .map_err(|e| Error::Graph(format!("sqlite schema: {e}")))?;
        Ok(())
    }
}
```

- [ ] **Step 3: Add serialization helpers**

Add these helpers below the implementation block:

```rust
#[derive(Debug)]
struct NodeRow {
    id: String,
    label: &'static str,
    doc_id: Option<String>,
    chunk_id: Option<String>,
    normalized_key: String,
    display: Option<String>,
    props_json: String,
}

fn node_to_row(node: &NodeWrite) -> Result<NodeRow> {
    let props = match node {
        NodeWrite::Document {
            doc_id,
            doc_type,
            legal_domain,
            jurisdiction,
            document_date,
            dossier_id,
        } => serde_json::json!({
            "doc_id": doc_id.to_string(),
            "doc_type": doc_type,
            "legal_domain": legal_domain,
            "jurisdiction": jurisdiction,
            "document_date": document_date.map(|d| d.to_rfc3339()),
            "dossier_id": dossier_id,
        }),
        NodeWrite::Chunk {
            chunk_id,
            doc_id,
            byte_start,
            byte_end,
            page,
        } => serde_json::json!({
            "chunk_id": chunk_id.to_string(),
            "doc_id": doc_id.to_string(),
            "byte_start": byte_start,
            "byte_end": byte_end,
            "page": page,
        }),
        NodeWrite::Party {
            party_id,
            kind,
            canonical_name,
            normalized_form,
            siren,
        } => serde_json::json!({
            "party_id": party_id.to_string(),
            "kind": format!("{kind:?}"),
            "canonical_name": canonical_name,
            "normalized_form": normalized_form,
            "siren": siren,
        }),
        NodeWrite::Article { article_id, article } => serde_json::json!({
            "article_id": article_id.to_string(),
            "code": article.code,
            "article_num": article.article_num,
            "normalized_ref": article.normalized_ref(),
        }),
        NodeWrite::Obligation {
            obligation_id,
            kind,
            text_pseudo,
        } => serde_json::json!({
            "obligation_id": obligation_id.to_string(),
            "kind": kind,
            "text_pseudo": text_pseudo,
        }),
        other => serde_json::to_value(format!("{other:?}"))
            .map_err(|e| Error::Graph(format!("node props serialize: {e}")))?,
    };

    let (id, label, doc_id, chunk_id, normalized_key, display) = match node {
        NodeWrite::Document { doc_id, .. } => (
            doc_id.to_string(),
            "Document",
            Some(doc_id.to_string()),
            None,
            doc_id.to_string(),
            Some(doc_id.to_string()),
        ),
        NodeWrite::Chunk { chunk_id, doc_id, .. } => (
            chunk_id.to_string(),
            "Chunk",
            Some(doc_id.to_string()),
            Some(chunk_id.to_string()),
            chunk_id.to_string(),
            Some(chunk_id.to_string()),
        ),
        NodeWrite::Party {
            party_id,
            canonical_name,
            normalized_form,
            ..
        } => (
            party_id.to_string(),
            "Party",
            None,
            None,
            normalized_form.clone(),
            Some(canonical_name.clone()),
        ),
        NodeWrite::Article { article_id, article } => (
            article_id.to_string(),
            "Article",
            None,
            None,
            article.normalized_ref(),
            Some(article.normalized_ref()),
        ),
        NodeWrite::Obligation {
            obligation_id,
            kind,
            ..
        } => (
            obligation_id.to_string(),
            "Obligation",
            None,
            None,
            obligation_id.to_string(),
            Some(kind.clone()),
        ),
        NodeWrite::Validation {
            validation_id,
            chunk_id,
            ..
        } => (
            validation_id.to_string(),
            "Validation",
            None,
            Some(chunk_id.to_string()),
            validation_id.to_string(),
            Some("validation".to_string()),
        ),
        NodeWrite::Court { court_id, .. } => (
            court_id.clone(),
            "Court",
            None,
            None,
            court_id.clone(),
            Some(court_id.clone()),
        ),
        NodeWrite::Amount { amount_id, scope, .. } => (
            amount_id.to_string(),
            "Amount",
            None,
            None,
            amount_id.to_string(),
            Some(scope.clone()),
        ),
        NodeWrite::Event { event_id, kind, .. } => (
            event_id.to_string(),
            "Event",
            None,
            None,
            event_id.to_string(),
            Some(kind.clone()),
        ),
        NodeWrite::Risk { risk_id, category, .. } => (
            risk_id.to_string(),
            "Risk",
            None,
            None,
            risk_id.to_string(),
            Some(category.clone()),
        ),
        NodeWrite::MandatoryClauseCheck {
            check_id,
            requirement,
            ..
        } => (
            check_id.to_string(),
            "MandatoryClauseCheck",
            None,
            None,
            check_id.to_string(),
            Some(requirement.clone()),
        ),
    };

    Ok(NodeRow {
        id,
        label,
        doc_id,
        chunk_id,
        normalized_key,
        display,
        props_json: props.to_string(),
    })
}

fn edge_id(edge: &EdgeWrite, doc_id: Option<&str>, chunk_id: Option<&str>) -> String {
    let raw = format!(
        "{}:{}:{}:{}:{}:{}:{}",
        edge.from_label,
        edge.from_key,
        edge.edge_type,
        edge.to_label,
        edge.to_key,
        doc_id.unwrap_or(""),
        chunk_id.unwrap_or("")
    );
    Uuid::new_v5(&Uuid::NAMESPACE_OID, raw.as_bytes()).to_string()
}
```

- [ ] **Step 4: Implement `upsert_batch`**

Replace the no-op trait implementation with:

```rust
#[async_trait]
impl LegalKnowledgeGraph for SqliteLegalGraphStore {
    async fn upsert_batch(&self, nodes: &NodeBatch, edges: &EdgeBatch) -> Result<()> {
        self.with_conn(|conn| {
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| Error::Graph(format!("sqlite tx: {e}")))?;
            let now = Utc::now().to_rfc3339();

            for node in &nodes.nodes {
                let row = node_to_row(node)?;
                tx.execute(
                    r#"
                    INSERT INTO legal_nodes
                        (id, label, doc_id, chunk_id, normalized_key, display, props_json, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                    ON CONFLICT(label, normalized_key) DO UPDATE SET
                        doc_id = COALESCE(excluded.doc_id, legal_nodes.doc_id),
                        chunk_id = COALESCE(excluded.chunk_id, legal_nodes.chunk_id),
                        display = COALESCE(excluded.display, legal_nodes.display),
                        props_json = excluded.props_json
                    "#,
                    params![
                        row.id,
                        row.label,
                        row.doc_id,
                        row.chunk_id,
                        row.normalized_key,
                        row.display,
                        row.props_json,
                        now
                    ],
                )
                .map_err(|e| Error::Graph(format!("upsert node: {e}")))?;
            }

            for edge in &edges.edges {
                let doc_id = infer_edge_doc_id(&tx, edge)?;
                let chunk_id = infer_edge_chunk_id(edge);
                let props_json = serde_json::to_string(&edge.props)
                    .map_err(|e| Error::Graph(format!("edge props serialize: {e}")))?;
                tx.execute(
                    r#"
                    INSERT INTO legal_edges
                        (id, from_label, from_key, to_label, to_key, edge_type, doc_id, chunk_id, props_json, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                    ON CONFLICT(from_label, from_key, edge_type, to_label, to_key, doc_id, chunk_id)
                    DO UPDATE SET props_json = excluded.props_json
                    "#,
                    params![
                        edge_id(edge, doc_id.as_deref(), chunk_id.as_deref()),
                        edge.from_label,
                        edge.from_key,
                        edge.to_label,
                        edge.to_key,
                        edge.edge_type,
                        doc_id,
                        chunk_id,
                        props_json,
                        now
                    ],
                )
                .map_err(|e| Error::Graph(format!("upsert edge: {e}")))?;
            }

            tx.commit()
                .map_err(|e| Error::Graph(format!("sqlite commit: {e}")))?;
            Ok(())
        })
    }

    async fn delete_doc(&self, doc_id: Uuid) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM legal_edges WHERE doc_id = ?1", params![doc_id.to_string()])
                .map_err(|e| Error::Graph(format!("delete doc edges: {e}")))?;
            conn.execute(
                "DELETE FROM legal_nodes WHERE doc_id = ?1 OR chunk_id IN (SELECT normalized_key FROM legal_nodes WHERE doc_id = ?1 AND label = 'Chunk')",
                params![doc_id.to_string()],
            )
            .map_err(|e| Error::Graph(format!("delete doc nodes: {e}")))?;
            Ok(())
        })
    }

    async fn link_cross_documents(&self, doc_id: Uuid, hints: &[CrossDocLinkHint]) -> Result<()> {
        let mut edges = EdgeBatch::new();
        for hint in hints {
            if let Some(target_doc_id) = hint.target_doc_id {
                edges.edges.push(EdgeWrite {
                    from_label: "Document",
                    from_key: doc_id.to_string(),
                    to_label: "Document",
                    to_key: target_doc_id.to_string(),
                    edge_type: hint.edge_type,
                    props: HashMap::from([(
                        "matching_quote".to_string(),
                        hint.matching_quote.clone().unwrap_or_default(),
                    )]),
                });
            }
        }
        self.upsert_batch(&NodeBatch::new(), &edges).await
    }

    async fn compact(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch("PRAGMA optimize; VACUUM;")
                .map_err(|e| Error::Graph(format!("sqlite compact: {e}")))?;
            Ok(())
        })
    }

    async fn cypher(
        &self,
        _query: &str,
        _params: HashMap<String, String>,
    ) -> Result<Vec<HashMap<String, String>>> {
        Err(Error::Graph(
            "Legal Graph v0 does not accept arbitrary Cypher; use named graph intents".into(),
        ))
    }
}
```

Add helper functions used above:

```rust
fn infer_edge_chunk_id(edge: &EdgeWrite) -> Option<String> {
    if edge.from_label == "Chunk" {
        Some(edge.from_key.clone())
    } else if edge.to_label == "Chunk" {
        Some(edge.to_key.clone())
    } else {
        edge.props.get("chunk_id").cloned()
    }
}

fn infer_edge_doc_id(conn: &Connection, edge: &EdgeWrite) -> Result<Option<String>> {
    if edge.from_label == "Document" {
        return Ok(Some(edge.from_key.clone()));
    }
    if edge.to_label == "Document" {
        return Ok(Some(edge.to_key.clone()));
    }
    if let Some(doc_id) = edge.props.get("doc_id") {
        return Ok(Some(doc_id.clone()));
    }
    let chunk_id = infer_edge_chunk_id(edge);
    if let Some(chunk_id) = chunk_id {
        let doc_id = conn
            .query_row(
                "SELECT doc_id FROM legal_nodes WHERE label = 'Chunk' AND normalized_key = ?1",
                params![chunk_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| Error::Graph(format!("infer edge doc_id: {e}")))?;
        return Ok(doc_id);
    }
    Ok(None)
}
```

- [ ] **Step 5: Run backend tests**

Run:

```powershell
cargo test -p anno-rag --test legal_graph_v0 sqlite_graph_persists_nodes_and_edges
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/kg.rs
git commit -m "feat(legal): persist graph batches in sqlite"
```

---

## Task 4: Add Typed Intent Query Methods

**Files:**
- Modify: `crates/anno-rag/src/legal/kg.rs`
- Modify: `crates/anno-rag/src/legal/query.rs`

- [ ] **Step 1: Extend the trait with typed methods**

In `LegalKnowledgeGraph`, add these methods before `cypher`:

```rust
    /// Query documents/chunks related to one normalized party.
    async fn party_dossier(&self, party: &str) -> Result<Vec<HashMap<String, String>>>;

    /// Query obligations owed by one normalized party.
    async fn obligations_owed_by(&self, party: &str) -> Result<Vec<HashMap<String, String>>>;

    /// Query documents/chunks that cite a normalized article reference.
    async fn citation_chain(&self, article_ref: &str) -> Result<Vec<HashMap<String, String>>>;

    /// Query timeline events for a dossier.
    async fn procedural_timeline(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>>;

    /// Query appeal links rooted at one document.
    async fn appeal_chain(&self, doc_id: &str, max_depth: u32) -> Result<Vec<HashMap<String, String>>>;
```

Update `InMemoryKG` test implementation to return `Ok(Vec::new())` for each new method.

- [ ] **Step 2: Implement typed methods for SQLite**

Add these methods inside the `impl LegalKnowledgeGraph for SqliteLegalGraphStore` block:

```rust
    async fn party_dossier(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            query_maps(
                conn,
                r#"
                SELECT
                    d.normalized_key AS doc_id,
                    c.normalized_key AS chunk_id,
                    party_edge.props_json AS party_props,
                    d.props_json AS doc_props,
                    c.props_json AS chunk_props
                FROM legal_nodes p
                JOIN legal_edges party_edge
                    ON party_edge.from_label = 'Party'
                   AND party_edge.from_key = p.id
                   AND party_edge.edge_type = 'PARTY_TO'
                JOIN legal_nodes d
                    ON d.label = 'Document'
                   AND d.normalized_key = party_edge.to_key
                LEFT JOIN legal_edges chunk_edge
                    ON chunk_edge.from_label = 'Document'
                   AND chunk_edge.from_key = d.normalized_key
                   AND chunk_edge.edge_type = 'HAS_CHUNK'
                LEFT JOIN legal_nodes c
                    ON c.label = 'Chunk'
                   AND c.normalized_key = chunk_edge.to_key
                WHERE p.label = 'Party'
                  AND p.normalized_key = ?1
                ORDER BY json_extract(d.props_json, '$.document_date'), c.normalized_key
                "#,
                [party],
                |mut row| {
                    let role = json_prop(&row.remove("party_props"), "role");
                    row.insert("role".into(), role.unwrap_or_default());
                    Ok(row)
                },
            )
        })
    }

    async fn obligations_owed_by(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            query_maps(
                conn,
                r#"
                SELECT
                    o.normalized_key AS obligation_id,
                    json_extract(o.props_json, '$.kind') AS obligation_kind,
                    json_extract(o.props_json, '$.text_pseudo') AS text_pseudo,
                    source_edge.chunk_id AS chunk_id,
                    source_edge.doc_id AS doc_id
                FROM legal_nodes p
                JOIN legal_edges bound
                    ON bound.from_label = 'Party'
                   AND bound.from_key = p.id
                   AND bound.edge_type = 'BOUND_BY'
                JOIN legal_nodes o
                    ON o.label = 'Obligation'
                   AND o.normalized_key = bound.to_key
                LEFT JOIN legal_edges source_edge
                    ON source_edge.to_label = 'Obligation'
                   AND source_edge.to_key = o.normalized_key
                   AND source_edge.edge_type = 'MENTIONS'
                WHERE p.label = 'Party'
                  AND p.normalized_key = ?1
                ORDER BY source_edge.doc_id, source_edge.chunk_id
                "#,
                [party],
                Ok,
            )
        })
    }

    async fn citation_chain(&self, article_ref: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            query_maps(
                conn,
                r#"
                SELECT
                    ref_edge.from_key AS doc_id,
                    mention_edge.from_key AS chunk_id,
                    a.normalized_key AS article_ref
                FROM legal_nodes a
                JOIN legal_edges ref_edge
                    ON ref_edge.to_label = 'Article'
                   AND ref_edge.to_key = a.id
                   AND ref_edge.edge_type = 'REFERENCES'
                LEFT JOIN legal_edges mention_edge
                    ON mention_edge.to_label = 'Article'
                   AND mention_edge.to_key = a.id
                   AND mention_edge.edge_type = 'MENTIONS'
                WHERE a.label = 'Article'
                  AND a.normalized_key = ?1
                ORDER BY ref_edge.from_key, mention_edge.from_key
                "#,
                [article_ref],
                Ok,
            )
        })
    }

    async fn procedural_timeline(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            query_maps(
                conn,
                r#"
                SELECT
                    d.normalized_key AS doc_id,
                    e.normalized_key AS event_id,
                    json_extract(e.props_json, '$.kind') AS event_kind,
                    json_extract(e.props_json, '$.event_date') AS event_date,
                    mention.chunk_id AS chunk_id
                FROM legal_nodes d
                JOIN legal_edges has_chunk
                    ON has_chunk.from_label = 'Document'
                   AND has_chunk.from_key = d.normalized_key
                   AND has_chunk.edge_type = 'HAS_CHUNK'
                JOIN legal_edges mention
                    ON mention.from_label = 'Chunk'
                   AND mention.from_key = has_chunk.to_key
                   AND mention.edge_type = 'MENTIONS'
                JOIN legal_nodes e
                    ON e.label = 'Event'
                   AND e.normalized_key = mention.to_key
                WHERE json_extract(d.props_json, '$.dossier_id') = ?1
                ORDER BY json_extract(e.props_json, '$.event_date'), e.normalized_key
                "#,
                [dossier_id],
                Ok,
            )
        })
    }

    async fn appeal_chain(&self, doc_id: &str, max_depth: u32) -> Result<Vec<HashMap<String, String>>> {
        let depth = max_depth.clamp(1, 25);
        self.with_conn(|conn| {
            let sql = format!(
                r#"
                WITH RECURSIVE appeal_chain(root_doc, prior_doc, depth) AS (
                    SELECT from_key, to_key, 1
                    FROM legal_edges
                    WHERE from_label = 'Document'
                      AND to_label = 'Document'
                      AND edge_type = 'APPEALS'
                      AND from_key = ?1
                    UNION ALL
                    SELECT ac.root_doc, e.to_key, ac.depth + 1
                    FROM appeal_chain ac
                    JOIN legal_edges e
                      ON e.from_label = 'Document'
                     AND e.to_label = 'Document'
                     AND e.edge_type = 'APPEALS'
                     AND e.from_key = ac.prior_doc
                    WHERE ac.depth < {depth}
                )
                SELECT root_doc AS doc_id, prior_doc, depth
                FROM appeal_chain
                ORDER BY depth
                "#
            );
            query_maps(conn, &sql, [doc_id], Ok)
        })
    }
```

Add query helpers below the serialization helpers:

```rust
fn query_maps<const N: usize>(
    conn: &Connection,
    sql: &str,
    params_array: [&str; N],
    map: impl Fn(HashMap<String, String>) -> Result<HashMap<String, String>>,
) -> Result<Vec<HashMap<String, String>>> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| Error::Graph(format!("prepare graph query: {e}")))?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| (*s).to_string()).collect();
    let mut rows = stmt
        .query(rusqlite::params_from_iter(params_array))
        .map_err(|e| Error::Graph(format!("run graph query: {e}")))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| Error::Graph(format!("read graph row: {e}")))?
    {
        let mut item = HashMap::new();
        for (idx, name) in names.iter().enumerate() {
            let value = row.get::<_, Option<String>>(idx).unwrap_or_default();
            if let Some(value) = value {
                item.insert(name.clone(), value);
            }
        }
        out.push(map(item)?);
    }
    Ok(out)
}

fn json_prop(json: &Option<String>, key: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(json.as_ref()?).ok()?;
    value.get(key)?.as_str().map(ToOwned::to_owned)
}
```

- [ ] **Step 3: Route `GraphIntent` through typed methods**

Replace `run_intent` in `crates/anno-rag/src/legal/query.rs` with:

```rust
pub async fn run_intent(
    kg: &dyn LegalKnowledgeGraph,
    intent: GraphIntent,
) -> Result<GraphQueryResult> {
    let rows = match intent {
        GraphIntent::PartyDossier { party } => kg.party_dossier(&party).await?,
        GraphIntent::ObligationsOwedBy { party } => kg.obligations_owed_by(&party).await?,
        GraphIntent::CitationChain { article_ref } => kg.citation_chain(&article_ref).await?,
        GraphIntent::ProceduralTimeline { dossier_id } => {
            kg.procedural_timeline(&dossier_id).await?
        }
        GraphIntent::AppealChain { doc_id, max_depth } => {
            kg.appeal_chain(&doc_id, max_depth).await?
        }
    };
    Ok(GraphQueryResult { rows })
}
```

Update the module comment to:

```rust
//! Five named graph traversals dispatched through [`LegalKnowledgeGraph`].
//! Legal Graph v0 deliberately does not expose arbitrary Cypher.
```

- [ ] **Step 4: Run intent tests**

Run:

```powershell
cargo test -p anno-rag --test legal_graph_v0
cargo test -p anno-rag legal::query --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/kg.rs crates/anno-rag/src/legal/query.rs crates/anno-rag/tests/legal_graph_v0.rs
git commit -m "feat(legal): execute typed graph intents"
```

---

## Task 5: Cover Validation, Privacy, and Delete Behavior

**Files:**
- Modify: `crates/anno-rag/tests/legal_graph_v0.rs`
- Modify: `crates/anno-rag/src/legal/kg.rs`

- [ ] **Step 1: Add validation persistence test**

Append this test to `legal_graph_v0.rs`:

```rust
#[tokio::test]
async fn validation_nodes_are_persisted_with_chunk_provenance() {
    let (_dir, kg) = store().await;
    let chunk_id = Uuid::now_v7();
    let validation_id = Uuid::now_v7();

    let mut nodes = NodeBatch::new();
    nodes.nodes.push(NodeWrite::Validation {
        validation_id,
        chunk_id,
        field_name: "obligation:payment".into(),
        action: "confirm".into(),
        corrected_value: None,
        note: Some("checked against source".into()),
        validated_at: Utc::now(),
        actor: Some("reviewer@example.test".into()),
    });
    let mut edges = EdgeBatch::new();
    edges.edges.push(EdgeWrite {
        from_label: "Validation",
        from_key: validation_id.to_string(),
        to_label: "Chunk",
        to_key: chunk_id.to_string(),
        edge_type: "VALIDATES",
        props: HashMap::new(),
    });

    kg.upsert_batch(&nodes, &edges).await.expect("upsert validation");

    let rows = kg
        .debug_nodes_by_label("Validation")
        .await
        .expect("debug validation rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("chunk_id").map(String::as_str), Some(chunk_id.to_string().as_str()));
    assert!(rows[0].get("props_json").expect("props").contains("confirm"));
}
```

- [ ] **Step 2: Add privacy test**

Append:

```rust
#[tokio::test]
async fn party_graph_stores_normalized_form_not_raw_person_name() {
    let (_dir, kg) = store().await;
    let party_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"pii:TOKEN_123");
    let mut nodes = NodeBatch::new();
    nodes.nodes.push(NodeWrite::Party {
        party_id,
        kind: PartyKind::Person,
        canonical_name: "pii:TOKEN_123".into(),
        normalized_form: "pii:TOKEN_123".into(),
        siren: None,
    });

    kg.upsert_batch(&nodes, &EdgeBatch::new()).await.expect("upsert party");
    let rows = kg.debug_nodes_by_label("Party").await.expect("debug party rows");

    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("props_json").expect("props").contains("pii:TOKEN_123"));
    assert!(!rows[0].get("props_json").expect("props").contains("Jean Dupont"));
}
```

- [ ] **Step 3: Add a hidden debug helper for integration tests**

Because integration tests cannot access `#[cfg(test)]` methods from the compiled library, add this hidden inspection helper to `SqliteLegalGraphStore`:

```rust
#[doc(hidden)]
pub async fn debug_nodes_by_label(&self, label: &str) -> Result<Vec<HashMap<String, String>>> {
    self.with_conn(|conn| {
        query_maps(
            conn,
            "SELECT id, label, doc_id, chunk_id, normalized_key, display, props_json FROM legal_nodes WHERE label = ?1 ORDER BY id",
            [label],
            Ok,
        )
    })
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag --test legal_graph_v0
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/kg.rs crates/anno-rag/tests/legal_graph_v0.rs
git commit -m "test(legal): cover graph validation and privacy"
```

---

## Task 6: Update Pipeline and Documentation Comments

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/legal/kg.rs`
- Modify: `crates/anno-rag/Cargo.toml`

- [ ] **Step 1: Replace stale no-op comments**

In `pipeline.rs`, replace:

```rust
/// Lance-graph knowledge graph (Phase 1: directory-backed no-op; real writes in Stage D).
```

with:

```rust
/// Local legal knowledge graph used for relationship-backed legal intents.
```

Replace:

```rust
// Graph dual-write (Phase 1: no-op LanceGraphStore; real writes in Stage D).
```

with:

```rust
// Graph dual-write: persists typed legal nodes/edges for named intents.
```

Replace `legal_graph_query` docs:

```rust
/// Dispatches a parameterized Cypher query (Phase 1: always returns an
/// empty row set from the no-op `LanceGraphStore`; real execution wired
/// in Stage D).
```

with:

```rust
/// Dispatches a named legal graph intent through the local typed graph backend.
```

- [ ] **Step 2: Replace stale Cargo comment**

In `crates/anno-rag/Cargo.toml`, replace:

```toml
# Legal RAG (Phase 1+2) — added 2026-05-23. The current KG store is a
# no-op abstraction; do not pull lance-graph until Stage D wires execution.
```

with:

```toml
# Legal Graph v0 uses a local typed SQLite backend. Do not add lance-graph
# until the optional v1 backend has a dedicated dependency/API audit.
```

- [ ] **Step 3: Run docs and focused checks**

Run:

```powershell
cargo fmt --all -- --check
cargo check -p anno-rag --lib
cargo test -p anno-rag --test legal_graph_v0
```

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/legal/kg.rs crates/anno-rag/Cargo.toml
git commit -m "docs(legal): describe graph v0 backend"
```

---

## Task 7: Full Verification and Supply-Chain Check

**Files:**
- Verify only unless formatting changes are required.

- [ ] **Step 1: Format**

Run:

```powershell
cargo fmt --all -- --check
```

Expected: PASS.

- [ ] **Step 2: Check workspace targets**

Run:

```powershell
cargo check --workspace --all-targets
```

Expected: PASS.

- [ ] **Step 3: Run focused graph and legal tests**

Run:

```powershell
cargo test -p anno-rag --test legal_graph_v0
cargo test -p anno-rag legal::kg --lib
cargo test -p anno-rag legal::query --lib
```

Expected: PASS.

- [ ] **Step 4: Run clippy using the repo's CI feature set**

On Windows, use the same CRT flags already needed by this workspace:

```powershell
$env:RUSTFLAGS='-C target-feature=-crt-static'
$env:CFLAGS_x86_64_pc_windows_msvc='-MD'
$env:CXXFLAGS_x86_64_pc_windows_msvc='-MD'
cargo clippy --workspace --all-targets --features "eval discourse" -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Run dependency policy**

Run:

```powershell
cargo deny check
```

Expected: PASS. If a new advisory appears, do not ignore it in `deny.toml` unless a safe upgrade or dependency removal is impossible and the reason is documented.

- [ ] **Step 6: Run GitNexus change detection**

Run:

```powershell
npx gitnexus analyze
npx gitnexus status
```

Expected: index is fresh for the current commit. Then run:

```powershell
npx gitnexus query -r anno-pr-review-worktree "legal graph sqlite graph backend named intents" --limit 10
```

Expected: results include `legal/kg.rs`, `legal/query.rs`, and `pipeline.rs`.

- [ ] **Step 7: Commit any verification-only formatting changes**

If formatting changed files:

```powershell
git add crates/anno-rag/src/legal/kg.rs crates/anno-rag/src/legal/query.rs crates/anno-rag/src/pipeline.rs
git commit -m "chore(legal): format graph v0 backend"
```

If no files changed, do not create a commit.

---

## Self-Review Notes

- Spec coverage: product goals map to typed intents; architecture maps to SQLite backend; non-goals are preserved by rejecting arbitrary Cypher in v0; testing requirements map to Tasks 2, 5, and 7.
- Dependency risk: v0 uses existing workspace `rusqlite`; `lance-graph` remains out of scope.
- Type consistency: the plan consistently uses `SqliteLegalGraphStore` and preserves `LanceGraphStore` as a compatibility type alias for current pipeline code.
- Open implementation detail: `debug_nodes_by_label` is documented as `#[doc(hidden)]` because integration tests need access. Remove it when a public inspection API is added.
