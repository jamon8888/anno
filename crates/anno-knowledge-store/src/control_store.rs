//! SQLite control store and FTS search operations.

use crate::fts_query::build_fts_query;
use crate::migrations::run_migrations;
use crate::Result;
use anno_knowledge_core::{
    ChunkId, KnowledgeSearchHit, KnowledgeSearchRequest, KnowledgeStatus, ObjectId, ObjectType,
    PartId, RevisionId, SourceKind,
};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

/// Synchronous SQLite store. MCP service code keeps calls short and local.
pub struct KnowledgeControlStore {
    conn: Mutex<Connection>,
}

/// Input used by tests and later source/indexer code to store a pseudonymized chunk.
#[derive(Debug, Clone)]
pub struct TestChunkInput {
    /// Chunk id.
    pub chunk_id: ChunkId,
    /// Object id.
    pub object_id: ObjectId,
    /// Revision id.
    pub revision_id: RevisionId,
    /// Part id.
    pub part_id: PartId,
    /// Source family.
    pub source_kind: SourceKind,
    /// Object family.
    pub object_type: ObjectType,
    /// Pseudonymized title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized body chunk.
    pub body_pseudo: String,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: serde_json::Value,
}

impl KnowledgeControlStore {
    /// Open or create the knowledge SQLite database.
    ///
    /// # Errors
    /// Returns store errors if the path cannot be created or SQLite fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Return local status without loading ML models.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn status(&self) -> Result<KnowledgeStatus> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        Ok(KnowledgeStatus {
            sources: count(&conn, "knowledge_sources")?,
            accounts: count(&conn, "source_accounts")?,
            scopes: count(&conn, "source_scopes")?,
            objects: count(&conn, "knowledge_objects")?,
            chunks: count(&conn, "knowledge_chunks")?,
            failures: count_failed_objects(&conn)?,
            models_loaded: false,
        })
    }

    /// Insert one pseudonymized chunk. Public for the first slice; reused by source/indexer code in later phases.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn insert_test_chunk(&self, input: TestChunkInput) -> Result<()> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let now = Utc::now().to_rfc3339();
        let metadata = serde_json::to_string(&input.metadata_pseudo_json)?;
        let title_pseudo = input.title_pseudo.clone();
        let body_pseudo = input.body_pseudo.clone();
        let body_len = body_pseudo.chars().count() as u32;
        let chunk_id = input.chunk_id.as_string();
        let object_id = input.object_id.as_string();
        let revision_id = input.revision_id.as_string();
        let part_id = input.part_id.as_string();
        let source_kind = serde_json::to_value(input.source_kind)?
            .as_str()
            .expect("source kind serializes to string")
            .to_string();
        let object_type = serde_json::to_value(input.object_type)?
            .as_str()
            .expect("object type serializes to string")
            .to_string();

        conn.execute(
            "INSERT OR IGNORE INTO knowledge_objects \
             (object_id, source_id, account_id, scope_id, external_id, object_type, \
              title_pseudo, metadata_pseudo_json, source_url_policy, source_updated_at, state) \
             VALUES (?1, 'test-source', 'test-account', 'test-scope', ?1, ?2, ?3, ?4, NULL, ?5, 'pseudonymized')",
            params![
                &object_id,
                &object_type,
                &title_pseudo,
                &metadata,
                &now,
            ],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_revisions \
             (revision_id, object_id, provider_version, observed_at) VALUES (?1, ?2, 'test', ?3)",
            params![&revision_id, &object_id, &now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_parts \
             (part_id, object_id, part_type, title_pseudo, metadata_pseudo_json, extracted_chars) \
             VALUES (?1, ?2, 'file_body', ?3, ?4, ?5)",
            params![&part_id, &object_id, &title_pseudo, &metadata, body_len],
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO knowledge_chunks \
             (chunk_id, object_id, revision_id, part_id, source_kind, object_type, title_pseudo, \
             body_pseudo, metadata_pseudo_json, chunk_idx, char_start, char_end, indexed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0, ?10, ?11)",
            params![
                &chunk_id,
                &object_id,
                &revision_id,
                &part_id,
                &source_kind,
                &object_type,
                &title_pseudo,
                &body_pseudo,
                &metadata,
                body_len,
                &now,
            ],
        )?;
        conn.execute(
            "INSERT INTO knowledge_objects_fts \
             (chunk_id, object_id, revision_id, source_kind, object_type, title_pseudo, body_pseudo, metadata_pseudo) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &chunk_id,
                &object_id,
                &revision_id,
                &source_kind,
                &object_type,
                &title_pseudo,
                &body_pseudo,
                &metadata,
            ],
        )?;
        Ok(())
    }

    /// Run local FTS search. This must not load models.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn search_fast(&self, request: &KnowledgeSearchRequest) -> Result<Vec<KnowledgeSearchHit>> {
        let Some(fts_query) = build_fts_query(&request.query) else {
            return Ok(Vec::new());
        };
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT chunk_id, object_id, revision_id, source_kind, object_type, title_pseudo, \
                    snippet(knowledge_objects_fts, 6, '[', ']', '...', 20) AS snippet, \
                    bm25(knowledge_objects_fts) AS score \
             FROM knowledge_objects_fts \
             WHERE knowledge_objects_fts MATCH ?1 \
             ORDER BY score \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, request.top_k as i64], |row| {
            let source_kind_text: String = row.get(3)?;
            let object_type_text: String = row.get(4)?;
            Ok(KnowledgeSearchHit {
                chunk_id: ChunkId::new(parse_uuid(row.get::<_, String>(0)?)?),
                object_id: ObjectId::new(parse_uuid(row.get::<_, String>(1)?)?),
                revision_id: RevisionId::new(parse_uuid(row.get::<_, String>(2)?)?),
                source_kind: parse_source_kind(&source_kind_text)?,
                object_type: parse_object_type(&object_type_text)?,
                title_pseudo: row.get(5)?,
                snippet_pseudo: row.get(6)?,
                score: row.get::<_, f64>(7)? as f32,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

fn count(conn: &Connection, table: &str) -> Result<u64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count as u64)
}

fn count_failed_objects(conn: &Connection) -> Result<u64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_objects WHERE state IN ('failed_retryable', 'failed_permanent')",
        [],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

fn parse_uuid(value: String) -> std::result::Result<uuid::Uuid, rusqlite::Error> {
    uuid::Uuid::parse_str(&value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_source_kind(value: &str) -> std::result::Result<SourceKind, rusqlite::Error> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_object_type(value: &str) -> std::result::Result<ObjectType, rusqlite::Error> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_knowledge_core::{
        ChunkId, KnowledgeSearchRequest, ObjectId, ObjectType, PartId, RevisionId, SourceKind,
        SourceKindForId,
    };

    #[test]
    fn open_creates_database_and_empty_status() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("knowledge.sqlite3");

        let store = KnowledgeControlStore::open(&db_path).expect("open store");
        let status = store.status().expect("status");

        assert_eq!(status.sources, 0);
        assert_eq!(status.objects, 0);
        assert_eq!(status.chunks, 0);
        assert!(!status.models_loaded);
    }

    #[test]
    fn fast_search_returns_pseudonymized_chunks() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("knowledge.sqlite3");
        let store = KnowledgeControlStore::open(&db_path).expect("open store");

        let object_id =
            ObjectId::from_external(SourceKindForId::LocalFolder, "local", "folder-a", "file-a");
        let revision_id = RevisionId::from_parts("file-a", "v1");
        let part_id = PartId::from_parts("file-a", "body");
        let chunk_id = ChunkId::from_parts(revision_id, part_id, 0);

        store
            .insert_test_chunk(TestChunkInput {
                chunk_id,
                object_id,
                revision_id,
                part_id,
                source_kind: SourceKind::LocalFolder,
                object_type: ObjectType::File,
                title_pseudo: Some("Document PERSON_1".to_string()),
                body_pseudo: "Le contrat de PERSON_1 contient CLAUSE_1.".to_string(),
                metadata_pseudo_json: serde_json::json!({"path": "FILE_1"}),
            })
            .expect("insert chunk");

        let hits = store
            .search_fast(&KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, chunk_id);
        assert!(hits[0].snippet_pseudo.contains("contrat"));
        assert!(!hits[0].snippet_pseudo.contains("Dupont"));
    }
}
