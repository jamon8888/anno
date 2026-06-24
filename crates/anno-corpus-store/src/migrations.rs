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

        CREATE TABLE IF NOT EXISTS corpus_sync_state (
            corpus_id TEXT PRIMARY KEY REFERENCES corpora(corpus_id) ON DELETE CASCADE,
            freshness TEXT NOT NULL,
            last_sync_started_at TEXT,
            last_sync_finished_at TEXT,
            last_seen_file_count INTEGER,
            last_seen_root_mtime TEXT,
            last_summary_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_corpus_bindings_lookup
            ON corpus_bindings(binding_kind, binding_id);
        CREATE INDEX IF NOT EXISTS idx_corpus_documents_doc
            ON corpus_documents(document_id);
        "#,
    )?;
    // Additive migration: `alias` column for human-readable corpus references.
    // execute_batch can't use ALTER ADD COLUMN idempotently, so guard on pragma.
    let has_alias: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('corpora') WHERE name = 'alias'")?
        .exists([])?;
    if !has_alias {
        conn.execute_batch(
            "ALTER TABLE corpora ADD COLUMN alias TEXT;\n\
             CREATE UNIQUE INDEX IF NOT EXISTS idx_corpora_alias \
             ON corpora(alias) WHERE alias IS NOT NULL;",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_create_expected_tables() {
        let conn = Connection::open_in_memory().expect("open sqlite");

        migrate(&conn).expect("migrate");

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type = 'table' ORDER BY name",
            )
            .expect("prepare table query");
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query tables")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect names");
        assert!(names.contains(&"corpus_bindings".to_string()));
        assert!(names.contains(&"corpus_documents".to_string()));
        assert!(names.contains(&"corpus_index_runs".to_string()));
        assert!(names.contains(&"corpus_sync_state".to_string()));
        assert!(names.contains(&"corpora".to_string()));
    }

    #[test]
    fn migrate_adds_alias_column_idempotently() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        migrate(&conn).expect("migrate once");
        // Second run must not error (column already exists).
        migrate(&conn).expect("migrate twice");

        let mut stmt = conn
            .prepare("SELECT name FROM pragma_table_info('corpora')")
            .expect("prepare pragma");
        let cols = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query cols")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect cols");
        assert!(cols.contains(&"alias".to_string()), "alias column present");
    }
}
