//! SQLite schema migrations for the corpus registry.

use crate::Result;
use rusqlite::Connection;

/// Apply all corpus registry schema migrations to the given connection.
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS corpora (
            corpus_id TEXT PRIMARY KEY,
            normalized_root TEXT NOT NULL UNIQUE,
            raw_root TEXT NOT NULL,
            label_pseudo TEXT NOT NULL,
            profiles_json TEXT NOT NULL,
            health TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS corpus_bindings (
            corpus_id TEXT NOT NULL REFERENCES corpora(corpus_id) ON DELETE CASCADE,
            binding_kind TEXT NOT NULL,
            binding_id TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(corpus_id, binding_kind, binding_id)
        );

        CREATE TABLE IF NOT EXISTS corpus_documents (
            corpus_id TEXT NOT NULL REFERENCES corpora(corpus_id) ON DELETE CASCADE,
            document_id TEXT NOT NULL,
            backend_kind TEXT NOT NULL,
            source_path_hash TEXT NOT NULL,
            relative_path_hash TEXT,
            content_id TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(corpus_id, document_id, backend_kind)
        );

        CREATE TABLE IF NOT EXISTS corpus_index_runs (
            run_id TEXT PRIMARY KEY,
            corpus_id TEXT NOT NULL REFERENCES corpora(corpus_id) ON DELETE CASCADE,
            profile TEXT NOT NULL,
            status TEXT NOT NULL,
            counters_json TEXT NOT NULL,
            failures_json TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_corpus_bindings_lookup
            ON corpus_bindings(binding_kind, binding_id);
        CREATE INDEX IF NOT EXISTS idx_corpus_documents_doc
            ON corpus_documents(document_id);
        "#,
    )?;
    Ok(())
}
