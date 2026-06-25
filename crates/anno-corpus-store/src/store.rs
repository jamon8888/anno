//! SQLite corpus registry operations.

use crate::{migrations, Error, Result};
use anno_corpus_core::{
    roots_overlap, ContentId, CorpusBindingKind, CorpusId, CorpusProfile, CorpusRoot,
    DocumentInstanceId,
};
use chrono::Utc;
use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

/// Synchronous SQLite corpus registry.
pub struct CorpusStore {
    conn: Mutex<Connection>,
}

/// Result of registering a corpus root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterCorpusResult {
    /// Stable corpus id.
    pub corpus_id: CorpusId,
    /// Normalized corpus root.
    pub normalized_root: String,
    /// Pseudonymous display label.
    pub label_pseudo: String,
}

/// Persisted corpus summary row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusRow {
    /// Stable corpus id.
    pub corpus_id: CorpusId,
    /// Pseudonymous display label.
    pub label_pseudo: String,
    /// Human-readable alias (user-supplied or auto). Option for rows read
    /// before the auto-alias backfill runs.
    pub alias: Option<String>,
    /// Registry health field.
    pub health: String,
}

/// Persisted corpus binding row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusBindingRow {
    /// Bound corpus id.
    pub corpus_id: CorpusId,
    /// Binding family.
    pub binding_kind: CorpusBindingKind,
    /// Binding identifier.
    pub binding_id: String,
}

/// Persisted corpus document row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusDocumentRow {
    /// Owning corpus id.
    pub corpus_id: CorpusId,
    /// Document instance id.
    pub document_id: DocumentInstanceId,
    /// Backend family.
    pub backend_kind: String,
    /// Stable content id.
    pub content_id: String,
}

/// Persisted corpus source/output sync state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusSyncStateRow {
    /// Owning corpus id.
    pub corpus_id: CorpusId,
    /// Freshness value serialized by `anno-corpus-core`.
    pub freshness: String,
    /// Last sync start timestamp.
    pub last_sync_started_at: Option<String>,
    /// Last successful or attempted sync finish timestamp.
    pub last_sync_finished_at: Option<String>,
    /// Last observed source file count.
    pub last_seen_file_count: Option<u64>,
    /// Last observed source root mtime.
    pub last_seen_root_mtime: Option<String>,
    /// Opaque sync summary for MCP responses.
    pub last_summary_json: serde_json::Value,
}

impl CorpusStore {
    /// Open or create the corpus registry database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        migrations::migrate(&conn)?;
        Self::backfill_aliases(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn backfill_aliases(conn: &Connection) -> Result<()> {
        let ids: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT corpus_id FROM corpora WHERE alias IS NULL ORDER BY created_at, corpus_id",
            )?;
            let collected = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            collected
        };
        for id in ids {
            let next: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(CAST(SUBSTR(alias, 8) AS INTEGER)), 0) FROM corpora",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            conn.execute(
                "UPDATE corpora SET alias = ?2 WHERE corpus_id = ?1",
                params![id, format!("corpus-{:02}", next + 1)],
            )?;
        }
        Ok(())
    }

    /// Register a normalized corpus root.
    pub fn register_root(
        &self,
        raw_root: &str,
        profiles: &[CorpusProfile],
    ) -> Result<RegisterCorpusResult> {
        let root = CorpusRoot::from_raw(raw_root)?;
        let corpus_id = CorpusId::from_normalized_root(&root.normalized_path);
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");

        let mut stmt = conn.prepare("SELECT corpus_id, normalized_root FROM corpora")?;
        let existing_roots = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for existing_root in existing_roots {
            let (existing_id, existing_root) = existing_root?;
            if existing_root != root.normalized_path
                && roots_overlap(&existing_root, &root.normalized_path)
            {
                return Err(Error::Overlap {
                    corpus_id: CorpusId::new(parse_uuid(existing_id)?),
                });
            }
        }

        let profiles_json = serde_json::to_string(profiles)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO corpora \
             (corpus_id, normalized_root, raw_root, label_pseudo, profiles_json, health, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'ok', ?6, ?6) \
             ON CONFLICT(corpus_id) DO UPDATE SET \
                raw_root = excluded.raw_root, \
                profiles_json = excluded.profiles_json, \
                updated_at = excluded.updated_at",
            params![
                corpus_id.as_string(),
                &root.normalized_path,
                &root.raw_path,
                &root.label_pseudo,
                &profiles_json,
                &now,
            ],
        )?;

        // Assign an auto-alias `corpus-NN` only if this corpus has none yet.
        // Re-registration of an existing root keeps any prior (user or auto) alias.
        let existing_alias: Option<String> = conn
            .query_row(
                "SELECT alias FROM corpora WHERE corpus_id = ?1",
                params![corpus_id.as_string()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if existing_alias.is_none() {
            let next: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(CAST(SUBSTR(alias, 8) AS INTEGER)), 0) FROM corpora",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let auto = format!("corpus-{:02}", next + 1);
            conn.execute(
                "UPDATE corpora SET alias = ?2 WHERE corpus_id = ?1",
                params![corpus_id.as_string(), auto],
            )?;
        }

        Ok(RegisterCorpusResult {
            corpus_id,
            normalized_root: root.normalized_path,
            label_pseudo: root.label_pseudo,
        })
    }

    /// Add or update a binding for a corpus.
    pub fn add_binding(
        &self,
        corpus_id: CorpusId,
        binding_kind: CorpusBindingKind,
        binding_id: &str,
        metadata: &serde_json::Value,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let metadata_json = serde_json::to_string(metadata)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO corpus_bindings \
             (corpus_id, binding_kind, binding_id, metadata_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                corpus_id.as_string(),
                binding_kind_text(binding_kind),
                binding_id,
                &metadata_json,
                &now,
            ],
        )?;
        Ok(())
    }

    /// Return metadata for a corpus binding.
    pub fn binding_metadata(
        &self,
        corpus_id: CorpusId,
        binding_kind: CorpusBindingKind,
        binding_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let mut stmt = conn.prepare(
            "SELECT metadata_json FROM corpus_bindings \
             WHERE corpus_id = ?1 AND binding_kind = ?2 AND binding_id = ?3",
        )?;
        let mut rows = stmt.query(params![
            corpus_id.as_string(),
            binding_kind_text(binding_kind),
            binding_id,
        ])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let metadata_json: String = row.get(0)?;
        Ok(Some(serde_json::from_str(&metadata_json)?))
    }

    /// Add or update a document observed in a corpus.
    pub fn add_document(
        &self,
        corpus_id: CorpusId,
        document_id: DocumentInstanceId,
        backend_kind: &str,
        source_path: &str,
        relative_path: Option<&str>,
        content_id: &ContentId,
        metadata: &serde_json::Value,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let source_path_hash = sha256_hex(source_path.as_bytes());
        let relative_path_hash = relative_path.map(|path| sha256_hex(path.as_bytes()));
        let metadata_json = serde_json::to_string(metadata)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO corpus_documents \
             (corpus_id, document_id, backend_kind, source_path_hash, relative_path_hash, relative_path, content_id, metadata_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                corpus_id.as_string(),
                document_id.as_string(),
                backend_kind,
                &source_path_hash,
                &relative_path_hash,
                relative_path,
                content_id.as_str(),
                &metadata_json,
                &now,
            ],
        )?;
        Ok(())
    }

    /// Count registered corpora.
    pub fn corpus_count(&self) -> Result<usize> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM corpora", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Record (or update) the readable relative path for a document, enabling
    /// handle resolution. Inserts a minimal row if the document is not present.
    pub fn record_document_path(
        &self,
        corpus_id: CorpusId,
        document_id: DocumentInstanceId,
        relative_path: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        conn.execute(
            "INSERT INTO corpus_documents \
                (corpus_id, document_id, backend_kind, source_path_hash, content_id, \
                 metadata_json, relative_path, created_at) \
             VALUES (?1, ?2, 'legal', '', '', '{}', ?3, ?4) \
             ON CONFLICT(corpus_id, document_id, backend_kind) \
             DO UPDATE SET relative_path = excluded.relative_path",
            params![
                corpus_id.as_string(),
                document_id.as_string(),
                relative_path,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Resolve a document id from its corpus + readable relative path.
    pub fn document_id_by_relative_path(
        &self,
        corpus_id: CorpusId,
        relative_path: &str,
    ) -> Result<Option<DocumentInstanceId>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT document_id FROM corpus_documents \
             WHERE corpus_id = ?1 AND relative_path = ?2 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![corpus_id.as_string(), relative_path])?;
        match rows.next()? {
            Some(row) => Ok(Some(DocumentInstanceId::new(parse_uuid(
                row.get::<_, String>(0)?,
            )?))),
            None => Ok(None),
        }
    }

    /// List registered corpora without raw filesystem paths.
    pub fn list_corpora(&self) -> Result<Vec<CorpusRow>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT corpus_id, label_pseudo, alias, health \
             FROM corpora ORDER BY created_at, corpus_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CorpusRow {
                corpus_id: CorpusId::new(parse_uuid(row.get::<_, String>(0)?)?),
                label_pseudo: row.get(1)?,
                alias: row.get(2)?,
                health: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Return whether a corpus id exists.
    pub fn corpus_exists(&self, corpus_id: CorpusId) -> Result<bool> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        corpus_exists(&conn, corpus_id)
    }

    /// Return the only registered corpus id.
    pub fn single_corpus_id(&self) -> Result<CorpusId> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let mut stmt = conn.prepare("SELECT corpus_id FROM corpora ORDER BY corpus_id LIMIT 2")?;
        let rows = stmt.query_map([], |row| parse_uuid(row.get::<_, String>(0)?))?;
        let ids = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        match ids.as_slice() {
            [id] => Ok(CorpusId::new(*id)),
            [] => Err(Error::UnknownCorpus("no corpus registered".to_string())),
            _ => Err(Error::UnknownCorpus(
                "multiple corpora registered".to_string(),
            )),
        }
    }

    /// Set (or replace) the human-readable alias for a corpus.
    /// Errors (via rusqlite UNIQUE violation) if the alias is already taken.
    pub fn set_alias(&self, corpus_id: CorpusId, alias: &str) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        conn.execute(
            "UPDATE corpora SET alias = ?2, updated_at = ?3 WHERE corpus_id = ?1",
            params![corpus_id.as_string(), alias, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Resolve a corpus id by its alias. Returns `None` if no corpus has it.
    pub fn lookup_by_alias(&self, alias: &str) -> Result<Option<CorpusId>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let mut stmt = conn.prepare("SELECT corpus_id FROM corpora WHERE alias = ?1")?;
        let mut rows = stmt.query(params![alias])?;
        match rows.next()? {
            Some(row) => Ok(Some(CorpusId::new(parse_uuid(row.get::<_, String>(0)?)?))),
            None => Ok(None),
        }
    }

    /// Return binding ids for a corpus and binding kind.
    pub fn binding_ids_for_corpus_kind(
        &self,
        corpus_id: CorpusId,
        binding_kind: CorpusBindingKind,
    ) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let mut stmt = conn.prepare(
            "SELECT binding_id FROM corpus_bindings \
             WHERE corpus_id = ?1 AND binding_kind = ?2 \
             ORDER BY binding_id",
        )?;
        let rows = stmt.query_map(
            params![corpus_id.as_string(), binding_kind_text(binding_kind)],
            |row| row.get::<_, String>(0),
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Return all backend bindings for a corpus.
    pub fn bindings_for_corpus(&self, corpus_id: CorpusId) -> Result<Vec<CorpusBindingRow>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let mut stmt = conn.prepare(
            "SELECT binding_kind, binding_id FROM corpus_bindings \
             WHERE corpus_id = ?1 ORDER BY binding_kind, binding_id",
        )?;
        let rows = stmt.query_map(params![corpus_id.as_string()], |row| {
            Ok(CorpusBindingRow {
                corpus_id,
                binding_kind: parse_binding_kind(&row.get::<_, String>(0)?)?,
                binding_id: row.get(1)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Return the corpus that owns a binding.
    pub fn corpus_for_binding(
        &self,
        binding_kind: CorpusBindingKind,
        binding_id: &str,
    ) -> Result<CorpusId> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT corpus_id FROM corpus_bindings \
             WHERE binding_kind = ?1 AND binding_id = ?2 \
             ORDER BY corpus_id LIMIT 2",
        )?;
        let rows = stmt.query_map(
            params![binding_kind_text(binding_kind), binding_id],
            |row| parse_uuid(row.get::<_, String>(0)?),
        )?;
        let ids = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        match ids.as_slice() {
            [id] => Ok(CorpusId::new(*id)),
            [] => Err(Error::UnknownCorpus(format!(
                "no corpus binding for {binding_id}"
            ))),
            _ => Err(Error::UnknownCorpus(format!(
                "multiple corpus bindings for {binding_id}"
            ))),
        }
    }

    /// Delete the corpus registry rows for one corpus.
    pub fn delete_corpus_registry_rows(&self, corpus_id: CorpusId) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        conn.execute(
            "DELETE FROM corpora WHERE corpus_id = ?1",
            params![corpus_id.as_string()],
        )?;
        Ok(())
    }

    /// Return document UUIDs for a corpus.
    pub fn document_ids_for_corpus(
        &self,
        corpus_id: CorpusId,
        backend_kind: &str,
    ) -> Result<Vec<uuid::Uuid>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let mut stmt = conn.prepare(
            "SELECT document_id FROM corpus_documents \
             WHERE corpus_id = ?1 AND backend_kind = ?2 \
             ORDER BY document_id",
        )?;
        let rows = stmt.query_map(params![corpus_id.as_string(), backend_kind], |row| {
            parse_uuid(row.get::<_, String>(0)?)
        })?;
        let ids = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Add or update sync state for one corpus.
    pub fn upsert_sync_state(
        &self,
        corpus_id: CorpusId,
        freshness: &str,
        started_at: Option<&str>,
        finished_at: Option<&str>,
        file_count: Option<u64>,
        root_mtime: Option<&str>,
        summary: &serde_json::Value,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let now = Utc::now().to_rfc3339();
        let summary_json = serde_json::to_string(summary)?;
        conn.execute(
            "INSERT INTO corpus_sync_state \
             (corpus_id, freshness, last_sync_started_at, last_sync_finished_at, last_seen_file_count, last_seen_root_mtime, last_summary_json, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(corpus_id) DO UPDATE SET \
                freshness = excluded.freshness, \
                last_sync_started_at = excluded.last_sync_started_at, \
                last_sync_finished_at = excluded.last_sync_finished_at, \
                last_seen_file_count = excluded.last_seen_file_count, \
                last_seen_root_mtime = excluded.last_seen_root_mtime, \
                last_summary_json = excluded.last_summary_json, \
                updated_at = excluded.updated_at",
            params![
                corpus_id.as_string(),
                freshness,
                started_at,
                finished_at,
                file_count.map(|value| value as i64),
                root_mtime,
                &summary_json,
                &now,
            ],
        )?;
        Ok(())
    }

    /// Return the last recorded sync state for one corpus.
    pub fn sync_state(&self, corpus_id: CorpusId) -> Result<Option<CorpusSyncStateRow>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let mut stmt = conn.prepare(
            "SELECT freshness, last_sync_started_at, last_sync_finished_at, last_seen_file_count, last_seen_root_mtime, last_summary_json \
             FROM corpus_sync_state WHERE corpus_id = ?1",
        )?;
        let mut rows = stmt.query(params![corpus_id.as_string()])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let last_summary_json: String = row.get(5)?;
        Ok(Some(CorpusSyncStateRow {
            corpus_id,
            freshness: row.get(0)?,
            last_sync_started_at: row.get(1)?,
            last_sync_finished_at: row.get(2)?,
            last_seen_file_count: row
                .get::<_, Option<i64>>(3)?
                .map(|value| value.max(0) as u64),
            last_seen_root_mtime: row.get(4)?,
            last_summary_json: serde_json::from_str(&last_summary_json)?,
        }))
    }
}

fn ensure_corpus_exists(conn: &Connection, corpus_id: CorpusId) -> Result<()> {
    let exists = corpus_exists(conn, corpus_id)?;
    if exists {
        Ok(())
    } else {
        Err(Error::UnknownCorpus(corpus_id.as_string()))
    }
}

fn corpus_exists(conn: &Connection, corpus_id: CorpusId) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM corpora WHERE corpus_id = ?1)",
        params![corpus_id.as_string()],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn binding_kind_text(binding_kind: CorpusBindingKind) -> &'static str {
    match binding_kind {
        CorpusBindingKind::KnowledgeSource => "knowledge_source",
        CorpusBindingKind::LegalFolder => "legal_folder",
        CorpusBindingKind::LegalDocument => "legal_document",
        CorpusBindingKind::TabularReview => "tabular_review",
    }
}

fn parse_binding_kind(value: &str) -> std::result::Result<CorpusBindingKind, rusqlite::Error> {
    match value {
        "knowledge_source" => Ok(CorpusBindingKind::KnowledgeSource),
        "legal_folder" => Ok(CorpusBindingKind::LegalFolder),
        "legal_document" => Ok(CorpusBindingKind::LegalDocument),
        "tabular_review" => Ok(CorpusBindingKind::TabularReview),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown corpus binding kind: {other}"),
            )),
        )),
    }
}

fn sha256_hex(input: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(input);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn parse_uuid(value: String) -> std::result::Result<Uuid, rusqlite::Error> {
    Uuid::parse_str(&value)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use anno_corpus_core::CorpusRoot;

    fn store() -> (tempfile::TempDir, CorpusStore) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CorpusStore::open(dir.path().join("corpora.sqlite3")).expect("open store");
        (dir, store)
    }

    #[test]
    fn open_creates_missing_parent_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("nested").join("corpora.sqlite3");

        let _store = CorpusStore::open(&db_path).expect("open nested store");

        assert!(db_path.exists());
    }

    #[test]
    fn migrations_create_empty_store() {
        let (_dir, store) = store();

        assert_eq!(store.corpus_count().expect("count corpora"), 0);
    }

    #[test]
    fn register_root_is_stable() {
        let (_dir, store) = store();

        let first = store
            .register_root("C:\\Clients\\Acme\\", &[CorpusProfile::Legal])
            .expect("register first");
        let second = store
            .register_root(" c:/CLIENTS/acme/// ", &[CorpusProfile::Legal])
            .expect("register second");

        assert_eq!(first, second);
        assert_eq!(first.normalized_root, "c:/clients/acme");
        assert!(first.label_pseudo.starts_with("corpus_"));
        assert_eq!(store.corpus_count().expect("count corpora"), 1);
    }

    #[test]
    fn register_root_rejects_overlap() {
        let (_dir, store) = store();
        let existing = store
            .register_root("C:/Clients/Acme", &[CorpusProfile::Legal])
            .expect("register existing");

        let err = store
            .register_root("C:/Clients/Acme/Submatter", &[CorpusProfile::Legal])
            .expect_err("overlap");

        match err {
            Error::Overlap { corpus_id } => assert_eq!(corpus_id, existing.corpus_id),
            other => panic!("expected overlap, got {other:?}"),
        }
    }

    #[test]
    fn binding_and_document_round_trip() {
        let (_dir, store) = store();
        let registered = store
            .register_root("C:/Clients/Acme", &[CorpusProfile::Legal])
            .expect("register");
        let root = CorpusRoot::from_raw("C:/Clients/Acme").expect("root");
        let content_id = ContentId::from_bytes(b"contract body");
        let document_id =
            DocumentInstanceId::from_parts(registered.corpus_id, "contract.pdf", &content_id);

        store
            .add_binding(
                registered.corpus_id,
                CorpusBindingKind::LegalFolder,
                "matter-folder-1",
                &serde_json::json!({ "kind": "fixture" }),
            )
            .expect("add binding");
        assert!(store
            .corpus_exists(registered.corpus_id)
            .expect("corpus exists"));
        assert_eq!(
            store.single_corpus_id().expect("single corpus"),
            registered.corpus_id
        );
        assert_eq!(
            store
                .binding_ids_for_corpus_kind(registered.corpus_id, CorpusBindingKind::LegalFolder)
                .expect("bindings"),
            vec!["matter-folder-1".to_string()]
        );
        assert_eq!(
            store
                .corpus_for_binding(CorpusBindingKind::LegalFolder, "matter-folder-1")
                .expect("binding corpus"),
            registered.corpus_id
        );
        store
            .add_document(
                registered.corpus_id,
                document_id,
                "local_file",
                "C:/Clients/Acme/contract.pdf",
                Some("contract.pdf"),
                &content_id,
                &serde_json::json!({ "root": root.normalized_path }),
            )
            .expect("add document");

        assert_eq!(
            store
                .document_ids_for_corpus(registered.corpus_id, "local_file")
                .expect("document ids"),
            vec![document_id.as_uuid()]
        );
        let bindings = store
            .bindings_for_corpus(registered.corpus_id)
            .expect("all bindings");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].binding_kind, CorpusBindingKind::LegalFolder);
        assert_eq!(bindings[0].binding_id, "matter-folder-1");

        store
            .delete_corpus_registry_rows(registered.corpus_id)
            .expect("delete corpus registry");
        assert_eq!(store.corpus_count().expect("count corpora"), 0);
    }

    #[test]
    fn binding_metadata_round_trips_source_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("corpora.sqlite3")).expect("open store");
        let registered = store
            .register_root("c:/clients/matter", &[CorpusProfile::Legal])
            .expect("register");

        store
            .add_binding(
                registered.corpus_id,
                CorpusBindingKind::LegalFolder,
                "folder-a",
                &serde_json::json!({"source_path": "c:/clients/matter"}),
            )
            .expect("binding");

        let metadata = store
            .binding_metadata(
                registered.corpus_id,
                CorpusBindingKind::LegalFolder,
                "folder-a",
            )
            .expect("metadata")
            .expect("exists");
        assert_eq!(metadata["source_path"], "c:/clients/matter");
    }

    #[test]
    fn list_corpora_exposes_alias_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        store
            .register_root(
                dir.path().join("folderA").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .expect("register");
        let rows = store.list_corpora().expect("list");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].alias.is_some(), "alias field populated");
    }

    #[test]
    fn register_root_auto_assigns_sequential_alias() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        store
            .register_root(
                dir.path().join("a").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .expect("a");
        store
            .register_root(
                dir.path().join("b").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .expect("b");
        let rows = store.list_corpora().expect("list");
        let aliases: Vec<String> = rows.iter().filter_map(|r| r.alias.clone()).collect();
        assert!(
            aliases.contains(&"corpus-01".to_string()),
            "first auto-alias"
        );
        assert!(
            aliases.contains(&"corpus-02".to_string()),
            "second auto-alias"
        );
    }

    #[test]
    fn reregister_same_root_keeps_alias() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let path = dir.path().join("a");
        let first = store
            .register_root(path.to_str().unwrap(), &[CorpusProfile::All])
            .expect("first");
        store
            .set_alias(first.corpus_id, "matter-7")
            .expect("user alias");
        store
            .register_root(path.to_str().unwrap(), &[CorpusProfile::All])
            .expect("second");
        assert_eq!(
            store.lookup_by_alias("matter-7").expect("lookup"),
            Some(first.corpus_id)
        );
    }

    #[test]
    fn set_and_lookup_alias_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let reg = store
            .register_root(
                dir.path().join("folderA").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .expect("register");
        store
            .set_alias(reg.corpus_id, "2026-0042")
            .expect("set alias");
        let found = store.lookup_by_alias("2026-0042").expect("lookup");
        assert_eq!(found, Some(reg.corpus_id));
        assert_eq!(store.lookup_by_alias("nope").expect("lookup miss"), None);
    }

    #[test]
    fn set_alias_rejects_duplicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let a = store
            .register_root(
                dir.path().join("a").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .expect("register a");
        let b = store
            .register_root(
                dir.path().join("b").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .expect("register b");
        store.set_alias(a.corpus_id, "dup").expect("first alias ok");
        assert!(
            store.set_alias(b.corpus_id, "dup").is_err(),
            "duplicate rejected"
        );
    }

    #[test]
    fn document_id_by_relative_path_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let reg = store
            .register_root(
                dir.path().join("a").to_str().unwrap(),
                &[CorpusProfile::All],
            )
            .expect("register");
        let doc = DocumentInstanceId::new(uuid::Uuid::nil());
        store
            .record_document_path(reg.corpus_id, doc, "contrats/x.txt")
            .expect("record");
        let found = store
            .document_id_by_relative_path(reg.corpus_id, "contrats/x.txt")
            .expect("lookup");
        assert_eq!(found, Some(doc));
        assert_eq!(
            store
                .document_id_by_relative_path(reg.corpus_id, "missing")
                .expect("miss"),
            None
        );
    }

    #[test]
    fn sync_state_round_trips_freshness_and_summary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("corpora.sqlite3")).expect("open store");
        let registered = store
            .register_root("c:/clients/matter", &[CorpusProfile::Knowledge])
            .expect("register");

        store
            .upsert_sync_state(
                registered.corpus_id,
                "fresh",
                Some("2026-06-05T10:00:00Z"),
                Some("2026-06-05T10:00:01Z"),
                Some(3),
                Some("2026-06-05T09:59:00Z"),
                &serde_json::json!({"knowledge_fast": {"indexed": 1}}),
            )
            .expect("upsert sync state");

        let row = store
            .sync_state(registered.corpus_id)
            .expect("load state")
            .expect("state exists");
        assert_eq!(row.freshness, "fresh");
        assert_eq!(row.last_seen_file_count, Some(3));
        assert_eq!(row.last_summary_json["knowledge_fast"]["indexed"], 1);
    }
}
