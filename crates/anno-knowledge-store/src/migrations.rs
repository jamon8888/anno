//! SQLite schema migrations for knowledge control and FTS data.

use crate::Result;
use rusqlite::Connection;

const SCHEMA_VERSION: i64 = 2;

/// Apply all schema migrations to the given connection.
///
/// Safe to call multiple times — migrations are guarded by `PRAGMA user_version`
/// and all DDL uses `CREATE … IF NOT EXISTS`.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        migrate_v1(conn)?;
        conn.pragma_update(None, "user_version", 1_i64)?;
    }
    if version < 2 {
        migrate_v2(conn)?;
        conn.pragma_update(None, "user_version", 2_i64)?;
    }
    Ok(())
}

/// Create all v1 tables, indexes, and FTS virtual table.
fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS knowledge_sources (
            source_id TEXT PRIMARY KEY NOT NULL,
            kind TEXT NOT NULL,
            display_label_pseudo TEXT NOT NULL,
            created_at TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS source_accounts (
            account_id TEXT PRIMARY KEY NOT NULL,
            source_id TEXT NOT NULL REFERENCES knowledge_sources(source_id) ON DELETE CASCADE,
            provider_subject TEXT NOT NULL,
            tenant_id TEXT,
            display_label_pseudo TEXT NOT NULL,
            scopes_granted_json TEXT NOT NULL,
            auth_ref TEXT,
            created_at TEXT NOT NULL,
            last_seen_at TEXT
        );

        CREATE TABLE IF NOT EXISTS source_scopes (
            scope_id TEXT PRIMARY KEY NOT NULL,
            account_id TEXT NOT NULL REFERENCES source_accounts(account_id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            provider_key TEXT NOT NULL,
            display_label_pseudo TEXT NOT NULL,
            sync_policy_json TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS knowledge_objects (
            object_id TEXT PRIMARY KEY NOT NULL,
            source_id TEXT NOT NULL,
            account_id TEXT NOT NULL,
            scope_id TEXT NOT NULL,
            external_id TEXT NOT NULL,
            object_type TEXT NOT NULL,
            title_pseudo TEXT,
            metadata_pseudo_json TEXT NOT NULL,
            source_url_policy TEXT,
            source_updated_at TEXT NOT NULL,
            state TEXT NOT NULL,
            last_error TEXT,
            UNIQUE(source_id, account_id, scope_id, external_id)
        );

        CREATE TABLE IF NOT EXISTS knowledge_revisions (
            revision_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT NOT NULL REFERENCES knowledge_objects(object_id) ON DELETE CASCADE,
            provider_version TEXT NOT NULL,
            observed_at TEXT NOT NULL,
            UNIQUE(object_id, provider_version)
        );

        CREATE TABLE IF NOT EXISTS knowledge_parts (
            part_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT NOT NULL REFERENCES knowledge_objects(object_id) ON DELETE CASCADE,
            part_type TEXT NOT NULL,
            title_pseudo TEXT,
            metadata_pseudo_json TEXT NOT NULL,
            extracted_chars INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_chunks (
            chunk_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT NOT NULL REFERENCES knowledge_objects(object_id) ON DELETE CASCADE,
            revision_id TEXT NOT NULL REFERENCES knowledge_revisions(revision_id) ON DELETE CASCADE,
            part_id TEXT NOT NULL REFERENCES knowledge_parts(part_id) ON DELETE CASCADE,
            source_kind TEXT NOT NULL,
            object_type TEXT NOT NULL,
            title_pseudo TEXT,
            body_pseudo TEXT NOT NULL,
            metadata_pseudo_json TEXT NOT NULL,
            chunk_idx INTEGER NOT NULL,
            char_start INTEGER NOT NULL,
            char_end INTEGER NOT NULL,
            indexed_at TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_objects_fts USING fts5(
            chunk_id UNINDEXED,
            object_id UNINDEXED,
            revision_id UNINDEXED,
            source_kind UNINDEXED,
            object_type UNINDEXED,
            title_pseudo,
            body_pseudo,
            metadata_pseudo,
            tokenize = 'unicode61 remove_diacritics 1'
        );

        CREATE TABLE IF NOT EXISTS sync_runs (
            sync_run_id TEXT PRIMARY KEY NOT NULL,
            source_id TEXT,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            status TEXT NOT NULL,
            objects_seen INTEGER NOT NULL DEFAULT 0,
            objects_changed INTEGER NOT NULL DEFAULT 0,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS index_jobs (
            job_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT,
            job_type TEXT NOT NULL,
            status TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            not_before TEXT,
            last_error TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_object
            ON knowledge_chunks(object_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_revision
            ON knowledge_chunks(revision_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_objects_state
            ON knowledge_objects(state);
        "#,
    )?;
    Ok(())
}

/// v2: add progress + corpus tracking columns to `index_jobs`.
///
/// SQLite `ALTER TABLE ADD COLUMN` is append-only and cannot be re-run, so
/// each add is guarded by a probe of `PRAGMA table_info`.
fn migrate_v2(conn: &Connection) -> Result<()> {
    let existing: Vec<String> = conn
        .prepare("PRAGMA table_info(index_jobs)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let adds = [
        ("corpus_id", "ALTER TABLE index_jobs ADD COLUMN corpus_id TEXT"),
        (
            "files_done",
            "ALTER TABLE index_jobs ADD COLUMN files_done INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "files_total",
            "ALTER TABLE index_jobs ADD COLUMN files_total INTEGER NOT NULL DEFAULT 0",
        ),
        ("created_at", "ALTER TABLE index_jobs ADD COLUMN created_at TEXT"),
        ("updated_at", "ALTER TABLE index_jobs ADD COLUMN updated_at TEXT"),
    ];
    for (col, sql) in adds {
        if !existing.iter().any(|c| c == col) {
            conn.execute(sql, [])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrations_create_expected_tables_and_fts() {
        let conn = Connection::open_in_memory().expect("open memory db");

        run_migrations(&conn).expect("migrate");

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type IN ('table', 'view') AND name LIKE 'knowledge_%' \
                 ORDER BY name",
            )
            .expect("prepare");
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect");

        assert!(names.contains(&"knowledge_sources".to_string()));
        assert!(names.contains(&"knowledge_objects".to_string()));
        assert!(names.contains(&"knowledge_chunks".to_string()));
        assert!(names.contains(&"knowledge_objects_fts".to_string()));
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().expect("open memory db");

        run_migrations(&conn).expect("first migrate");
        run_migrations(&conn).expect("second migrate");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, 2);
    }

    #[test]
    fn migrate_v2_adds_job_progress_columns() {
        let conn = Connection::open_in_memory().expect("open memory db");
        run_migrations(&conn).expect("migrate");

        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(index_jobs)")
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect");

        for expected in ["corpus_id", "files_done", "files_total", "created_at", "updated_at"] {
            assert!(cols.contains(&expected.to_string()), "missing column {expected}");
        }

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, 2);
    }
}
