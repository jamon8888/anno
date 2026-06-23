//! SQLite control store and FTS search operations.

use crate::fts_query::build_fts_query;
use crate::migrations::run_migrations;
use crate::Result;
use anno_knowledge_core::{
    AccountId, ChunkId, KnowledgeSearchHit, KnowledgeSearchRequest, KnowledgeStatus, ObjectId,
    ObjectType, PartId, RevisionId, ScopeId, SourceId, SourceKind, SourceKindForId,
};
use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection};
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

/// Inputs to register a local folder as a source + synthetic account + scope.
#[derive(Debug, Clone)]
pub struct LocalFolderRegistration {
    /// Stable key for the source id (the folder path).
    pub stable_key: String,
    /// Pseudonymized source display label.
    pub source_label_pseudo: String,
    /// Pseudonymized scope display label.
    pub scope_label_pseudo: String,
    /// Provider key for the scope (the folder path).
    pub provider_key: String,
}

/// Result of registering a local folder.
#[derive(Debug, Clone)]
pub struct LocalFolderRegistered {
    /// New or existing source id.
    pub source_id: SourceId,
    /// New or existing synthetic account id.
    pub account_id: AccountId,
    /// New or existing scope id.
    pub scope_id: ScopeId,
}

/// A configured source row.
#[derive(Debug, Clone)]
pub struct SourceRow {
    /// Source id.
    pub source_id: SourceId,
    /// Source kind (serialized form).
    pub kind: String,
    /// Pseudonymized label.
    pub display_label_pseudo: String,
    /// Whether enabled.
    pub enabled: bool,
}

/// A configured scope row.
#[derive(Debug, Clone)]
pub struct ScopeRow {
    /// Scope id.
    pub scope_id: ScopeId,
    /// Owning account id.
    pub account_id: AccountId,
    /// Provider key (folder path).
    pub provider_key: String,
    /// Pseudonymized label.
    pub display_label_pseudo: String,
    /// Whether enabled.
    pub enabled: bool,
}

/// One pseudonymized chunk to commit.
#[derive(Debug, Clone)]
pub struct CommitChunk {
    /// Chunk id.
    pub chunk_id: ChunkId,
    /// Chunk index within the part.
    pub chunk_idx: u32,
    /// Pseudonymized title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized body.
    pub text_pseudo: String,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: String,
    /// Char start offset.
    pub char_start: u32,
    /// Char end offset.
    pub char_end: u32,
}

/// All rows for committing one object revision in a single transaction.
#[derive(Debug, Clone)]
pub struct CommitObjectInput {
    /// Object id.
    pub object_id: ObjectId,
    /// Source id.
    pub source_id: SourceId,
    /// Account id.
    pub account_id: AccountId,
    /// Scope id.
    pub scope_id: ScopeId,
    /// Revision id (content-based).
    pub revision_id: RevisionId,
    /// Part id (single FileBody part).
    pub part_id: PartId,
    /// External id (canonical path).
    pub external_id: String,
    /// Object family.
    pub object_type: ObjectType,
    /// Provider version (hex of content hash).
    pub provider_version: String,
    /// Pseudonymized title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: String,
    /// Source kind.
    pub source_kind: SourceKind,
    /// Pseudonymized chunks.
    pub chunks: Vec<CommitChunk>,
}

/// Lowercase hex of a 32-byte hash.
#[must_use]
pub fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
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

    /// Lock the connection mutex. Shared by the jobs module.
    pub(crate) fn conn_lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("knowledge sqlite mutex poisoned")
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

        let source_id = SourceId::from_parts(SourceKindForId::LocalFolder, "test-source");
        let account_id = AccountId::from_parts(source_id, "test-account");
        let scope_id = ScopeId::from_parts(account_id, "test-scope");

        conn.execute(
            "INSERT OR IGNORE INTO knowledge_objects \
             (object_id, source_id, account_id, scope_id, external_id, object_type, \
               title_pseudo, metadata_pseudo_json, source_url_policy, source_updated_at, state) \
              VALUES (?1, ?2, ?3, ?4, ?1, ?5, ?6, ?7, NULL, ?8, 'pseudonymized')",
            params![
                &object_id,
                source_id.as_string(),
                account_id.as_string(),
                scope_id.as_string(),
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

    /// Register (idempotently) a local folder as source + synthetic account + scope.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn register_local_folder(
        &self,
        reg: LocalFolderRegistration,
    ) -> Result<LocalFolderRegistered> {
        let source_id = SourceId::from_parts(SourceKindForId::LocalFolder, &reg.stable_key);
        let account_id = AccountId::from_parts(source_id, "local");
        let scope_id = ScopeId::from_parts(account_id, &reg.provider_key);
        let now = Utc::now().to_rfc3339();
        let kind = serde_json::to_value(SourceKind::LocalFolder)?
            .as_str()
            .expect("source kind serializes to string")
            .to_string();

        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_sources \
             (source_id, kind, display_label_pseudo, created_at, enabled) \
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![source_id.as_string(), kind, reg.source_label_pseudo, now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO source_accounts \
             (account_id, source_id, provider_subject, tenant_id, display_label_pseudo, \
              scopes_granted_json, auth_ref, created_at, last_seen_at) \
             VALUES (?1, ?2, 'local', NULL, ?3, '[]', NULL, ?4, NULL)",
            params![
                account_id.as_string(),
                source_id.as_string(),
                reg.source_label_pseudo,
                now
            ],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO source_scopes \
             (scope_id, account_id, kind, provider_key, display_label_pseudo, \
              sync_policy_json, enabled) \
             VALUES (?1, ?2, 'local_folder', ?3, ?4, ?5, 1)",
            params![
                scope_id.as_string(),
                account_id.as_string(),
                reg.provider_key,
                reg.scope_label_pseudo,
                "{\"enabled\":true,\"max_pages_per_run\":5,\"include_attachments\":false}",
            ],
        )?;
        Ok(LocalFolderRegistered {
            source_id,
            account_id,
            scope_id,
        })
    }

    /// List configured sources.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn list_sources(&self) -> Result<Vec<SourceRow>> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT source_id, kind, display_label_pseudo, enabled \
             FROM knowledge_sources ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SourceRow {
                source_id: SourceId::new(parse_uuid(row.get::<_, String>(0)?)?),
                kind: row.get(1)?,
                display_label_pseudo: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Return the source whose scope provider key exactly matches `provider_key`.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn source_by_provider_key(&self, provider_key: &str) -> Result<Option<SourceRow>> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT src.source_id, src.kind, src.display_label_pseudo, src.enabled \
             FROM source_scopes s \
             JOIN source_accounts a ON a.account_id = s.account_id \
             JOIN knowledge_sources src ON src.source_id = a.source_id \
             WHERE s.provider_key = ?1 \
             ORDER BY src.created_at \
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![provider_key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(SourceRow {
                source_id: SourceId::new(parse_uuid(row.get::<_, String>(0)?)?),
                kind: row.get(1)?,
                display_label_pseudo: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
            }))
        } else {
            Ok(None)
        }
    }

    /// Return enabled scopes for a source.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn enabled_scopes_for_source(&self, source_id: &SourceId) -> Result<Vec<ScopeRow>> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT s.scope_id, s.account_id, s.provider_key, s.display_label_pseudo, s.enabled \
             FROM source_scopes s \
             JOIN source_accounts a ON a.account_id = s.account_id \
             WHERE a.source_id = ?1 AND s.enabled = 1 \
             ORDER BY s.provider_key",
        )?;
        let sid = source_id.as_string();
        let rows = stmt.query_map(params![sid], |row| {
            Ok(ScopeRow {
                scope_id: ScopeId::new(parse_uuid(row.get::<_, String>(0)?)?),
                account_id: AccountId::new(parse_uuid(row.get::<_, String>(1)?)?),
                provider_key: row.get(2)?,
                display_label_pseudo: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// True when the object already has an `fts_ready` revision at this provider version.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn revision_is_fts_ready(
        &self,
        object_id: &ObjectId,
        provider_version: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_objects o \
             JOIN knowledge_revisions r ON r.object_id = o.object_id \
             WHERE o.object_id = ?1 AND r.provider_version = ?2 AND o.state = 'fts_ready'",
            params![object_id.as_string(), provider_version],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Commit one object revision: upsert object/revision/part, replace chunks
    /// and FTS rows, set state to `fts_ready`. One transaction.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn commit_object(&self, input: &CommitObjectInput) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let source_kind = serde_json::to_value(input.source_kind)?
            .as_str()
            .expect("source kind serializes to string")
            .to_string();
        let object_type = serde_json::to_value(input.object_type)?
            .as_str()
            .expect("object type serializes to string")
            .to_string();
        let oid = input.object_id.as_string();
        let rid = input.revision_id.as_string();
        let pid = input.part_id.as_string();

        let mut conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO knowledge_objects \
             (object_id, source_id, account_id, scope_id, external_id, object_type, \
              title_pseudo, metadata_pseudo_json, source_url_policy, source_updated_at, state, last_error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, 'fts_ready', NULL) \
             ON CONFLICT(object_id) DO UPDATE SET \
               title_pseudo = excluded.title_pseudo, \
               metadata_pseudo_json = excluded.metadata_pseudo_json, \
               source_updated_at = excluded.source_updated_at, \
               state = 'fts_ready', last_error = NULL",
            params![oid, input.source_id.as_string(), input.account_id.as_string(),
                    input.scope_id.as_string(), input.external_id, object_type,
                    input.title_pseudo, input.metadata_pseudo_json, now],
        )?;

        tx.execute(
            "INSERT OR IGNORE INTO knowledge_revisions \
             (revision_id, object_id, provider_version, observed_at) VALUES (?1, ?2, ?3, ?4)",
            params![rid, oid, input.provider_version, now],
        )?;

        tx.execute(
            "INSERT INTO knowledge_parts \
             (part_id, object_id, part_type, title_pseudo, metadata_pseudo_json, extracted_chars) \
             VALUES (?1, ?2, 'file_body', ?3, ?4, ?5) \
             ON CONFLICT(part_id) DO UPDATE SET \
               title_pseudo = excluded.title_pseudo, \
               metadata_pseudo_json = excluded.metadata_pseudo_json, \
               extracted_chars = excluded.extracted_chars",
            params![
                pid,
                oid,
                input.title_pseudo,
                input.metadata_pseudo_json,
                input
                    .chunks
                    .iter()
                    .map(|c| c.text_pseudo.chars().count() as i64)
                    .sum::<i64>()
            ],
        )?;

        // Replace prior chunks + FTS rows for this object.
        tx.execute(
            "DELETE FROM knowledge_objects_fts WHERE object_id = ?1",
            params![oid],
        )?;
        tx.execute(
            "DELETE FROM knowledge_chunks WHERE object_id = ?1",
            params![oid],
        )?;

        for c in &input.chunks {
            let cid = c.chunk_id.as_string();
            tx.execute(
                "INSERT INTO knowledge_chunks \
                 (chunk_id, object_id, revision_id, part_id, source_kind, object_type, title_pseudo, \
                  body_pseudo, metadata_pseudo_json, chunk_idx, char_start, char_end, indexed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![cid, oid, rid, pid, source_kind, object_type, c.title_pseudo,
                        c.text_pseudo, c.metadata_pseudo_json, c.chunk_idx, c.char_start, c.char_end, now],
            )?;
            tx.execute(
                "INSERT INTO knowledge_objects_fts \
                 (chunk_id, object_id, revision_id, source_kind, object_type, title_pseudo, body_pseudo, metadata_pseudo) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![cid, oid, rid, source_kind, object_type, c.title_pseudo, c.text_pseudo, c.metadata_pseudo_json],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Delete all objects (and their chunks + FTS rows) under a scope.
    /// Returns the number of objects removed.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn forget_scope(&self, scope_id: &ScopeId) -> Result<u64> {
        let sid = scope_id.as_string();
        let mut conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let tx = conn.transaction()?;
        // Remove FTS rows first (no cascade on the virtual table).
        tx.execute(
            "DELETE FROM knowledge_objects_fts WHERE object_id IN \
             (SELECT object_id FROM knowledge_objects WHERE scope_id = ?1)",
            params![sid],
        )?;
        let removed = tx.execute(
            "DELETE FROM knowledge_objects WHERE scope_id = ?1",
            params![sid],
        )? as u64;
        // knowledge_chunks/revisions/parts cascade via ON DELETE CASCADE.
        tx.commit()?;
        Ok(removed)
    }

    /// Delete a source and all objects, chunks, FTS rows, accounts, and scopes under it.
    /// Returns the number of objects removed.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn forget_source(&self, source_id: &SourceId) -> Result<u64> {
        let sid = source_id.as_string();
        let mut conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let tx = conn.transaction()?;
        // Remove FTS rows first (no cascade on the virtual table).
        tx.execute(
            "DELETE FROM knowledge_objects_fts WHERE object_id IN \
             (SELECT object_id FROM knowledge_objects WHERE source_id = ?1)",
            params![sid],
        )?;
        let removed = tx.execute(
            "DELETE FROM knowledge_objects WHERE source_id = ?1",
            params![sid],
        )? as u64;
        tx.execute(
            "DELETE FROM knowledge_sources WHERE source_id = ?1",
            params![sid],
        )?;
        // source_accounts/source_scopes and object children cascade via foreign keys.
        tx.commit()?;
        Ok(removed)
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
        let mut sql = String::from(
            "SELECT knowledge_objects_fts.chunk_id, knowledge_objects_fts.object_id, \
                    knowledge_objects_fts.revision_id, knowledge_objects_fts.source_kind, \
                    knowledge_objects_fts.object_type, knowledge_objects_fts.title_pseudo, \
                    snippet(knowledge_objects_fts, 6, '[', ']', '...', 20) AS snippet, \
                    bm25(knowledge_objects_fts) AS score, \
                    o.source_id, o.scope_id \
             FROM knowledge_objects_fts \
             JOIN knowledge_objects o ON o.object_id = knowledge_objects_fts.object_id \
             WHERE knowledge_objects_fts MATCH ?",
        );
        let mut bindings = Vec::<rusqlite::types::Value>::new();
        bindings.push(fts_query.into());
        if !request.source_ids.is_empty() {
            sql.push_str(" AND o.source_id IN (");
            sql.push_str(&repeat_vars(request.source_ids.len()));
            sql.push(')');
            for source_id in &request.source_ids {
                bindings.push(source_id.as_string().into());
            }
        }
        if !request.scope_ids.is_empty() {
            sql.push_str(" AND o.scope_id IN (");
            sql.push_str(&repeat_vars(request.scope_ids.len()));
            sql.push(')');
            for scope_id in &request.scope_ids {
                bindings.push(scope_id.as_string().into());
            }
        }
        sql.push_str(" ORDER BY score LIMIT ?");
        bindings.push((request.top_k as i64).into());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(bindings), |row| {
            let source_kind_text: String = row.get(3)?;
            let object_type_text: String = row.get(4)?;
            Ok(KnowledgeSearchHit {
                chunk_id: ChunkId::new(parse_uuid(row.get::<_, String>(0)?)?),
                object_id: ObjectId::new(parse_uuid(row.get::<_, String>(1)?)?),
                revision_id: RevisionId::new(parse_uuid(row.get::<_, String>(2)?)?),
                source_id: SourceId::new(parse_uuid(row.get::<_, String>(8)?)?),
                scope_id: ScopeId::new(parse_uuid(row.get::<_, String>(9)?)?),
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

fn repeat_vars(len: usize) -> String {
    (0..len).map(|_| "?").collect::<Vec<_>>().join(", ")
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

    #[test]
    fn add_local_folder_source_then_list_and_get_scopes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");

        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".to_string(),
                source_label_pseudo: "FOLDER_1".to_string(),
                scope_label_pseudo: "FOLDER_1".to_string(),
                provider_key: "C:/docs".to_string(),
            })
            .expect("register");

        let sources = store.list_sources().expect("list");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_id, reg.source_id);

        let scopes = store
            .enabled_scopes_for_source(&reg.source_id)
            .expect("scopes");
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].scope_id, reg.scope_id);
        assert_eq!(scopes[0].provider_key, "C:/docs");
        assert_eq!(scopes[0].account_id, reg.account_id);
    }

    #[test]
    fn register_local_folder_is_idempotent_on_same_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let a = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".to_string(),
                source_label_pseudo: "FOLDER_1".to_string(),
                scope_label_pseudo: "FOLDER_1".to_string(),
                provider_key: "C:/docs".to_string(),
            })
            .expect("register a");
        let b = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".to_string(),
                source_label_pseudo: "FOLDER_1".to_string(),
                scope_label_pseudo: "FOLDER_1".to_string(),
                provider_key: "C:/docs".to_string(),
            })
            .expect("register b");
        assert_eq!(a.source_id, b.source_id);
        assert_eq!(a.scope_id, b.scope_id);
        assert_eq!(store.list_sources().expect("list").len(), 1);
    }

    fn sample_chunk(
        _object_id: ObjectId,
        revision_id: RevisionId,
        part_id: PartId,
        idx: u32,
        body: &str,
    ) -> CommitChunk {
        CommitChunk {
            chunk_id: ChunkId::from_parts(revision_id, part_id, idx),
            chunk_idx: idx,
            title_pseudo: Some("Doc FOLDER_1".to_string()),
            text_pseudo: body.to_string(),
            metadata_pseudo_json: "{\"path\":\"FOLDER_1\"}".to_string(),
            char_start: 0,
            char_end: body.len() as u32,
        }
    }

    fn commit_input(
        scope_id: ScopeId,
        account_id: AccountId,
        source_id: SourceId,
        hash: [u8; 32],
        body: &str,
    ) -> CommitObjectInput {
        let object_id = ObjectId::from_external(
            SourceKindForId::LocalFolder,
            &source_id.as_string(),
            &scope_id.as_string(),
            "C:/docs/a.txt",
        );
        let revision_id = RevisionId::from_parts(&object_id.as_string(), &hex32(&hash));
        let part_id = PartId::from_parts(&object_id.as_string(), "file_body");
        CommitObjectInput {
            object_id,
            source_id,
            account_id,
            scope_id,
            revision_id,
            part_id,
            external_id: "C:/docs/a.txt".to_string(),
            object_type: ObjectType::File,
            provider_version: hex32(&hash),
            title_pseudo: Some("Doc FOLDER_1".to_string()),
            metadata_pseudo_json: "{\"path\":\"FOLDER_1\"}".to_string(),
            source_kind: SourceKind::LocalFolder,
            chunks: vec![sample_chunk(object_id, revision_id, part_id, 0, body)],
        }
    }

    #[test]
    fn commit_object_then_search_and_skip_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(),
                source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(),
                provider_key: "C:/docs".into(),
            })
            .expect("register");

        let hash = [1u8; 32];
        let input = commit_input(
            reg.scope_id,
            reg.account_id,
            reg.source_id,
            hash,
            "le contrat FOLDER_1",
        );

        assert!(!store
            .revision_is_fts_ready(&input.object_id, &input.provider_version)
            .expect("check"));
        store.commit_object(&input).expect("commit");
        assert!(store
            .revision_is_fts_ready(&input.object_id, &input.provider_version)
            .expect("check"));

        let hits = store
            .search_fast(&KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");
        assert_eq!(hits.len(), 1);

        let status = store.status().expect("status");
        assert_eq!(status.objects, 1);
        assert_eq!(status.chunks, 1);
    }

    #[test]
    fn search_fast_filters_by_source_id() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let a = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "c:/clients/a".into(),
                source_label_pseudo: "a".into(),
                scope_label_pseudo: "a".into(),
                provider_key: "c:/clients/a".into(),
            })
            .expect("register a");
        let b = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "c:/clients/b".into(),
                source_label_pseudo: "b".into(),
                scope_label_pseudo: "b".into(),
                provider_key: "c:/clients/b".into(),
            })
            .expect("register b");
        let input_a = commit_input(
            a.scope_id,
            a.account_id,
            a.source_id,
            [1u8; 32],
            "Contrat Alpha",
        );
        let input_b = commit_input(
            b.scope_id,
            b.account_id,
            b.source_id,
            [2u8; 32],
            "Contrat Beta",
        );
        store.commit_object(&input_a).expect("commit a");
        store.commit_object(&input_b).expect("commit b");

        let hits = store
            .search_fast(
                &KnowledgeSearchRequest::new("contrat")
                    .with_top_k(10)
                    .with_source_ids(vec![a.source_id]),
            )
            .expect("search");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source_id, a.source_id);
    }

    #[test]
    fn recommit_with_new_revision_replaces_chunks_no_duplicates() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(),
                source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(),
                provider_key: "C:/docs".into(),
            })
            .expect("register");

        let v1 = commit_input(
            reg.scope_id,
            reg.account_id,
            reg.source_id,
            [1u8; 32],
            "version une FOLDER_1",
        );
        store.commit_object(&v1).expect("commit v1");
        let v2 = commit_input(
            reg.scope_id,
            reg.account_id,
            reg.source_id,
            [2u8; 32],
            "version deux FOLDER_1",
        );
        store.commit_object(&v2).expect("commit v2");

        let status = store.status().expect("status");
        assert_eq!(status.objects, 1);
        assert_eq!(status.chunks, 1);
        let hits = store
            .search_fast(&KnowledgeSearchRequest::new("version").with_top_k(5))
            .expect("search");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn forget_scope_removes_objects_chunks_and_fts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(),
                source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(),
                provider_key: "C:/docs".into(),
            })
            .expect("register");
        let input = commit_input(
            reg.scope_id,
            reg.account_id,
            reg.source_id,
            [1u8; 32],
            "le contrat FOLDER_1",
        );
        store.commit_object(&input).expect("commit");
        assert_eq!(store.status().expect("status").objects, 1);

        let removed = store.forget_scope(&reg.scope_id).expect("forget");
        assert_eq!(removed, 1);

        let status = store.status().expect("status");
        assert_eq!(status.objects, 0);
        assert_eq!(status.chunks, 0);
        let hits = store
            .search_fast(&KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");
        assert!(hits.is_empty());
    }

    #[test]
    fn forget_source_removes_source_objects_chunks_and_fts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(),
                source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(),
                provider_key: "C:/docs".into(),
            })
            .expect("register");
        let input = commit_input(
            reg.scope_id,
            reg.account_id,
            reg.source_id,
            [1u8; 32],
            "le contrat FOLDER_1",
        );
        store.commit_object(&input).expect("commit");
        assert_eq!(store.list_sources().expect("sources").len(), 1);

        let removed = store.forget_source(&reg.source_id).expect("forget source");
        assert_eq!(removed, 1);

        assert!(store.list_sources().expect("sources").is_empty());
        let status = store.status().expect("status");
        assert_eq!(status.objects, 0);
        assert_eq!(status.chunks, 0);
        let hits = store
            .search_fast(&KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");
        assert!(hits.is_empty());
    }

    #[test]
    fn source_by_provider_key_finds_registered_local_folder() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(),
                source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(),
                provider_key: "C:/docs".into(),
            })
            .expect("register");

        let found = store
            .source_by_provider_key("C:/docs")
            .expect("lookup")
            .expect("source");
        assert_eq!(found.source_id, reg.source_id);
        assert!(store
            .source_by_provider_key("C:/missing")
            .expect("missing")
            .is_none());
    }
}
