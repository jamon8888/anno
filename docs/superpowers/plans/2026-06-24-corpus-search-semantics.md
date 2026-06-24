# Corpus Search Semantics (Spec A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make anno's local-folder search work per-client (cloisonné) and cross-client (explicit), with human-readable corpus aliases and document handles instead of raw UUIDs.

**Architecture:** Build bottom-up across 4 phases. Phase 1 adds an `alias` column + auto-alias to the corpus registry (`anno-corpus-store`). Phase 2 updates resolution (`resolve_effective` truth table + `resolve_doc_ref` handles) in `anno-corpus-store`/`anno-rag-mcp`. Phase 3 makes the CLI `ingest` register a corpus. Phase 4 wires path-prefix filtering, cross-corpus provenance, and the structured disambiguation response into the MCP search surface.

**Tech Stack:** Rust, rusqlite (SQLite), `rmcp` MCP tool macros, `clap` (CLI), `tokio`.

**Spec:** `docs/superpowers/specs/2026-06-24-corpus-search-semantics-design.md`

---

## File Structure

| File | Responsibility | Phase |
|------|----------------|-------|
| `crates/anno-corpus-store/src/migrations.rs` | Idempotent `alias` column + partial unique index | 1 |
| `crates/anno-corpus-store/src/store.rs` | Auto-alias generation, `set_alias`, `lookup_by_alias`, back-fill, `alias` on `CorpusRow` | 1 |
| `crates/anno-corpus-store/src/error.rs` | (reuse `UnknownCorpus`) | 1 |
| `crates/anno-rag-mcp/src/corpus.rs` | `resolve_effective` truth table; accept alias; `resolve_doc_ref` | 2 |
| `crates/anno-rag-bin/src/main.rs` | CLI `ingest` registers corpus (`--profile`, `--alias`) | 3 |
| `crates/anno-rag-mcp/src/lib.rs` | Disambiguation response; `path_prefix`; `corpus_id`+`handle` in hits | 4 |
| `crates/anno-rag/src/store.rs` / `pipeline.rs` | `path_prefix` chunk filter; propagate `corpus_id`/`handle` to `SearchHit` | 4 |

---

## Phase 1 — Corpus registry: alias foundation

### Task 1: Idempotent `alias` column migration

**Files:**
- Modify: `crates/anno-corpus-store/src/migrations.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `migrations.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: FAIL — `alias column present` assertion fails (column missing).

- [ ] **Step 3: Add the idempotent column migration**

In `migrate()`, after the `execute_batch(...)?;` block (before `Ok(())`), append:

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: PASS (both `migrate_adds_alias_column_idempotently` and `migrations_create_expected_tables`).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-corpus-store/src/migrations.rs
git commit -m "feat(corpus-store): add idempotent alias column migration"
```

---

### Task 2: Expose `alias` on `CorpusRow` and `list_corpora`

**Files:**
- Modify: `crates/anno-corpus-store/src/store.rs:35` (`CorpusRow`), `:248` (`list_corpora`)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `store.rs`:

```rust
    #[test]
    fn list_corpora_exposes_alias_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        store
            .register_root(dir.path().join("folderA").to_str().unwrap(), &[CorpusProfile::All])
            .expect("register");
        let rows = store.list_corpora().expect("list");
        assert_eq!(rows.len(), 1);
        // alias is auto-assigned (Task 4); here we assert the field exists and is Some.
        assert!(rows[0].alias.is_some(), "alias field populated");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: FAIL — `no field 'alias' on CorpusRow` (compile error).

- [ ] **Step 3: Add `alias` to `CorpusRow` and select it**

In `CorpusRow` (line 35), add field:

```rust
pub struct CorpusRow {
    /// Stable corpus id.
    pub corpus_id: CorpusId,
    /// Pseudonymous display label.
    pub label_pseudo: String,
    /// Human-readable alias (user-supplied or auto `corpus-NN`). Always Some
    /// after Task 4 back-fill; Option for pre-migration rows read mid-upgrade.
    pub alias: Option<String>,
    /// Registry health field.
    pub health: String,
}
```

In `list_corpora()` (line 248), update the query and mapping:

```rust
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
```

> Note: this test will only pass once Task 4 auto-assigns the alias. If running tasks strictly in order, expect this test RED until Task 4. Mark it `#[ignore]` with reason `"alias populated in Task 4"` now, and remove the ignore in Task 4 Step 1.

- [ ] **Step 4: Run test to verify it compiles (ignored)**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: PASS (compiles; `list_corpora_exposes_alias_field` shows as ignored).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-corpus-store/src/store.rs
git commit -m "feat(corpus-store): expose alias on CorpusRow + list_corpora"
```

---

### Task 3: `lookup_by_alias` + `set_alias`

**Files:**
- Modify: `crates/anno-corpus-store/src/store.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `store.rs`:

```rust
    #[test]
    fn set_and_lookup_alias_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let reg = store
            .register_root(dir.path().join("folderA").to_str().unwrap(), &[CorpusProfile::All])
            .expect("register");
        store.set_alias(reg.corpus_id, "2026-0042").expect("set alias");
        let found = store.lookup_by_alias("2026-0042").expect("lookup");
        assert_eq!(found, Some(reg.corpus_id));
        assert_eq!(store.lookup_by_alias("nope").expect("lookup miss"), None);
    }

    #[test]
    fn set_alias_rejects_duplicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let a = store
            .register_root(dir.path().join("a").to_str().unwrap(), &[CorpusProfile::All])
            .expect("register a");
        let b = store
            .register_root(dir.path().join("b").to_str().unwrap(), &[CorpusProfile::All])
            .expect("register b");
        store.set_alias(a.corpus_id, "dup").expect("first alias ok");
        assert!(store.set_alias(b.corpus_id, "dup").is_err(), "duplicate rejected");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: FAIL — `no method named set_alias` (compile error).

- [ ] **Step 3: Implement `set_alias` and `lookup_by_alias`**

Add to `impl CorpusStore` (near `single_corpus_id`):

```rust
    /// Set (or replace) the human-readable alias for a corpus.
    /// Returns `Error::from` the rusqlite UNIQUE violation if the alias is taken.
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: PASS (`set_and_lookup_alias_roundtrip`, `set_alias_rejects_duplicate`).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-corpus-store/src/store.rs
git commit -m "feat(corpus-store): set_alias + lookup_by_alias"
```

---

### Task 4: Auto-alias `corpus-NN` on register + back-fill on open

**Files:**
- Modify: `crates/anno-corpus-store/src/store.rs` (`register_root`, `open`)

- [ ] **Step 1: Un-ignore Task 2's test and add auto-alias tests**

Remove the `#[ignore]` attribute added in Task 2. Add:

```rust
    #[test]
    fn register_root_auto_assigns_sequential_alias() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        store.register_root(dir.path().join("a").to_str().unwrap(), &[CorpusProfile::All]).expect("a");
        store.register_root(dir.path().join("b").to_str().unwrap(), &[CorpusProfile::All]).expect("b");
        let rows = store.list_corpora().expect("list");
        let aliases: Vec<String> = rows.iter().filter_map(|r| r.alias.clone()).collect();
        assert!(aliases.contains(&"corpus-01".to_string()), "first auto-alias");
        assert!(aliases.contains(&"corpus-02".to_string()), "second auto-alias");
    }

    #[test]
    fn reregister_same_root_keeps_alias() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let path = dir.path().join("a");
        let first = store.register_root(path.to_str().unwrap(), &[CorpusProfile::All]).expect("first");
        store.set_alias(first.corpus_id, "matter-7").expect("user alias");
        // Re-register same root: must NOT overwrite the user alias.
        store.register_root(path.to_str().unwrap(), &[CorpusProfile::All]).expect("second");
        assert_eq!(store.lookup_by_alias("matter-7").expect("lookup"), Some(first.corpus_id));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: FAIL — `first auto-alias` assertion fails (alias still NULL).

- [ ] **Step 3: Assign auto-alias in `register_root`; back-fill in `open`**

In `register_root`, after the `INSERT ... ON CONFLICT` execute (after line 144, before `Ok(RegisterCorpusResult {`):

```rust
        // Assign an auto-alias `corpus-NN` only if this corpus has none yet.
        // Re-registration of an existing root keeps any prior (user or auto) alias.
        let existing_alias: Option<String> = conn
            .query_row(
                "SELECT alias FROM corpora WHERE corpus_id = ?1",
                params![corpus_id.as_string()],
                |row| row.get(0),
            )
            .optional()?;
        if existing_alias.is_none() {
            let next: i64 = conn.query_row(
                "SELECT COUNT(*) FROM corpora WHERE alias IS NOT NULL",
                [],
                |row| row.get(0),
            )?;
            let auto = format!("corpus-{:02}", next + 1);
            conn.execute(
                "UPDATE corpora SET alias = ?2 WHERE corpus_id = ?1",
                params![corpus_id.as_string(), auto],
            )?;
        }
```

Ensure `use rusqlite::OptionalExtension;` is imported at the top of `store.rs` (for `.optional()`).

For back-fill of pre-migration corpora, add a private helper and call it from `open()` after `migrate(&conn)?`:

```rust
    fn backfill_aliases(conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(
            "SELECT corpus_id FROM corpora WHERE alias IS NULL ORDER BY created_at, corpus_id",
        )?;
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for id in ids {
            let next: i64 = conn.query_row(
                "SELECT COUNT(*) FROM corpora WHERE alias IS NOT NULL",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "UPDATE corpora SET alias = ?2 WHERE corpus_id = ?1",
                params![id, format!("corpus-{:02}", next + 1)],
            )?;
        }
        Ok(())
    }
```

In `open()`, after `migrations::migrate(&conn)?;`:

```rust
        Self::backfill_aliases(&conn)?;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: PASS — all Phase 1 tests including `register_root_auto_assigns_sequential_alias`, `reregister_same_root_keeps_alias`, `list_corpora_exposes_alias_field`.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-corpus-store/src/store.rs
git commit -m "feat(corpus-store): auto-alias corpus-NN on register + back-fill on open"
```

---

## Phase 2 — Resolution semantics

### Task 5: `resolve_effective` accepts alias + count==0 truth table

**Files:**
- Modify: `crates/anno-rag-mcp/src/corpus.rs:135-170`

- [ ] **Step 1: Write the failing test**

Add a `tests` module at the bottom of `corpus.rs` (or extend the existing one):

```rust
#[cfg(test)]
mod resolve_tests {
    use super::*;
    use anno_corpus_core::{CorpusProfile, EffectiveCorpus};

    fn svc(dir: &std::path::Path) -> CorpusService {
        CorpusService { store: CorpusStore::open(dir.join("c.sqlite3")).expect("open") }
    }

    #[test]
    fn zero_corpus_no_cross_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        assert!(matches!(s.resolve_effective(None, false), Err(CorpusGuardError::NoCorpus)));
    }

    #[test]
    fn zero_corpus_with_cross_is_cross() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        assert_eq!(s.resolve_effective(None, true).unwrap(), EffectiveCorpus::CrossCorpus);
    }

    #[test]
    fn resolves_by_alias() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        let reg = s.store.register_root(dir.path().join("a").to_str().unwrap(), &[CorpusProfile::All]).unwrap();
        s.store.set_alias(reg.corpus_id, "2026-0042").unwrap();
        assert_eq!(
            s.resolve_effective(Some("2026-0042"), false).unwrap(),
            EffectiveCorpus::Single(reg.corpus_id)
        );
    }
}
```

> If `CorpusService { store }` cannot be constructed in tests because the field is private, add `#[cfg(test)] pub(crate) fn from_store(store: CorpusStore) -> Self { Self { store } }` to `impl CorpusService` and use it.

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: FAIL — `zero_corpus_with_cross_is_cross` (today count==0 returns NoCorpus even with cross), and `resolves_by_alias` (alias not looked up).

- [ ] **Step 3: Rewrite `resolve_effective`**

Replace the body of `resolve_effective` (lines 135-170) with:

```rust
    pub fn resolve_effective(
        &self,
        corpus_ref: Option<&str>,
        allow_cross_corpus: bool,
    ) -> Result<EffectiveCorpus, CorpusGuardError> {
        // 1. Explicit reference wins over everything: try UUID, then alias.
        if let Some(value) = corpus_ref {
            if let Ok(parsed) = parse_corpus_id(value) {
                if self.store.corpus_exists(parsed).map_err(|_| CorpusGuardError::NoCorpus)? {
                    return Ok(EffectiveCorpus::Single(parsed));
                }
            }
            if let Some(by_alias) =
                self.store.lookup_by_alias(value).map_err(|_| CorpusGuardError::NoCorpus)?
            {
                return Ok(EffectiveCorpus::Single(by_alias));
            }
            return Err(CorpusGuardError::UnknownCorpus(value.to_string()));
        }
        // 2. Explicit "search everything" short-circuits the count.
        if allow_cross_corpus {
            return Ok(EffectiveCorpus::CrossCorpus);
        }
        // 3. Implicit: depends on how many corpora exist.
        let count = self.store.corpus_count().map_err(|_| CorpusGuardError::NoCorpus)?;
        match count {
            0 => Err(CorpusGuardError::NoCorpus),
            1 => Ok(EffectiveCorpus::Single(
                self.store.single_corpus_id().map_err(|_| CorpusGuardError::NoCorpus)?,
            )),
            _ => Err(CorpusGuardError::CorpusRequired),
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS (all three resolve tests).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-mcp/src/corpus.rs
git commit -m "feat(mcp): resolve_effective accepts alias + cross-corpus on empty registry"
```

---

### Task 6: `resolve_doc_ref` — UUID-or-handle document resolution

**Files:**
- Modify: `crates/anno-rag-mcp/src/corpus.rs`

- [ ] **Step 1: Write the failing test**

Add to `resolve_tests`:

```rust
    #[test]
    fn resolve_doc_ref_passthrough_uuid() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        let uuid = "a9ea6215-c656-5629-b75a-7054b3d6d911";
        // No corpus needed: a syntactically valid UUID resolves to itself.
        assert_eq!(s.resolve_doc_ref(uuid).unwrap(), uuid.to_string());
    }

    #[test]
    fn resolve_doc_ref_unknown_alias_errors() {
        let dir = tempfile::tempdir().unwrap();
        let s = svc(dir.path());
        assert!(s.resolve_doc_ref("ghost/contrats/x.txt").is_err());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: FAIL — `no method named resolve_doc_ref`.

- [ ] **Step 3: Implement `resolve_doc_ref`**

Add to `impl CorpusService`. It returns the stringified document UUID. For a handle `alias/relative_path`, it resolves the corpus by alias then recomputes the scoped document id:

```rust
    /// Resolve a document reference that is EITHER a UUID (passthrough) OR a
    /// readable handle `alias/relative_path`. Returns the stringified doc UUID.
    pub fn resolve_doc_ref(&self, doc_ref: &str) -> Result<String, CorpusGuardError> {
        // UUID passthrough.
        if uuid::Uuid::parse_str(doc_ref).is_ok() {
            return Ok(doc_ref.to_string());
        }
        // Handle form: split alias from the relative path at the first '/'.
        let (alias, relative) = doc_ref
            .split_once('/')
            .ok_or_else(|| CorpusGuardError::UnknownCorpus(doc_ref.to_string()))?;
        let corpus_id = self
            .store
            .lookup_by_alias(alias)
            .map_err(|_| CorpusGuardError::UnknownCorpus(alias.to_string()))?
            .ok_or_else(|| CorpusGuardError::UnknownCorpus(alias.to_string()))?;
        // Look up the document by (corpus_id, relative_path) in the registry.
        self.store
            .document_id_by_relative_path(corpus_id, relative)
            .map_err(|_| CorpusGuardError::UnknownCorpus(doc_ref.to_string()))?
            .map(|id| id.as_string())
            .ok_or_else(|| CorpusGuardError::UnknownCorpus(doc_ref.to_string()))
    }
```

> Dependency: `document_id_by_relative_path` is added in Task 7. Until then this won't compile — that's expected; Tasks 6 and 7 land together (commit at end of Task 7). Skip the standalone commit here; run only the compile check in Step 4 of Task 7.

- [ ] **Step 4: (Deferred)**

Compile + test together with Task 7.

---

### Task 7: `document_id_by_relative_path` in the registry

**Files:**
- Modify: `crates/anno-corpus-store/src/store.rs`, `crates/anno-corpus-store/src/migrations.rs`

Background: `corpus_documents` stores `relative_path_hash`, not the plaintext path (`migrations.rs:37`). To resolve a handle, we need a reverse lookup. Add a nullable `relative_path` column alongside the existing hash so handles resolve without weakening the hash-based privacy elsewhere.

- [ ] **Step 1: Write the failing test**

Add to `store.rs` tests:

```rust
    #[test]
    fn document_id_by_relative_path_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CorpusStore::open(dir.path().join("c.sqlite3")).expect("open");
        let reg = store
            .register_root(dir.path().join("a").to_str().unwrap(), &[CorpusProfile::All])
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
            store.document_id_by_relative_path(reg.corpus_id, "missing").expect("miss"),
            None
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Expected: FAIL — `no method named record_document_path`.

- [ ] **Step 3: Add column + methods**

In `migrations.rs`, in the same `if !has_alias`-style guarded block pattern, add a second guard for `relative_path` on `corpus_documents`:

```rust
    let has_relpath: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('corpus_documents') WHERE name = 'relative_path'")?
        .exists([])?;
    if !has_relpath {
        conn.execute_batch(
            "ALTER TABLE corpus_documents ADD COLUMN relative_path TEXT;\n\
             CREATE INDEX IF NOT EXISTS idx_corpus_documents_relpath \
             ON corpus_documents(corpus_id, relative_path);",
        )?;
    }
```

In `store.rs`, add:

```rust
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
            Some(row) => Ok(Some(DocumentInstanceId::new(parse_uuid(row.get::<_, String>(0)?)?))),
            None => Ok(None),
        }
    }
```

- [ ] **Step 4: Run tests (both crates) to verify they pass**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store`
Then: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS — `document_id_by_relative_path_roundtrip`, plus Task 6's `resolve_doc_ref_passthrough_uuid` and `resolve_doc_ref_unknown_alias_errors` now compile and pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-corpus-store/src/store.rs crates/anno-corpus-store/src/migrations.rs crates/anno-rag-mcp/src/corpus.rs
git commit -m "feat: document handle resolution (resolve_doc_ref + relative_path lookup)"
```

---

## Phase 3 — CLI ingest registers a corpus

### Task 8: `--profile` / `--alias` flags on `ingest` + corpus registration

**Files:**
- Modify: `crates/anno-rag-bin/src/main.rs:38` (Ingest variant), `:291-300` (handler)

- [ ] **Step 1: Add flags to the `Ingest` clap variant**

In the `Cmd::Ingest` variant (line 38), add two fields (match the existing field style):

```rust
    Ingest {
        folder: PathBuf,
        #[arg(long)]
        recursive: bool,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        config: Option<PathBuf>,
        /// Index profile: all (default), legal, or general.
        #[arg(long, default_value = "all")]
        profile: String,
        /// Optional human-readable corpus alias (e.g. a matter number).
        #[arg(long)]
        alias: Option<String>,
    },
```

- [ ] **Step 2: Register the corpus before ingesting**

Replace the `Cmd::Ingest { .. }` handler body (lines 291-300):

```rust
        Cmd::Ingest {
            folder,
            recursive,
            output,
            config: _,
            profile,
            alias,
        } => {
            let out = output.unwrap_or_else(|| cfg.outputs_dir());
            // Register the folder as a corpus so search resolution and handles work.
            let svc = anno_rag_mcp::corpus::CorpusService::open(&cfg)?;
            let folder_str = folder.to_string_lossy();
            let reg = svc.register_index_root(&folder_str, &profile)?;
            if let Some(alias) = alias.as_deref() {
                svc.store().set_alias(reg.corpus_id, alias)?;
            }
            let n = pipeline.ingest_folder(&folder, recursive, &out).await?;
            println!(
                "ingested {n} documents → {} (corpus {})",
                out.display(),
                reg.corpus_id.as_string()
            );
        }
```

> If `anno_rag_mcp::corpus::CorpusService` / `register_index_root` are not `pub`, make them `pub` (they already are per `corpus.rs`). Confirm `anno-rag-bin/Cargo.toml` depends on `anno-rag-mcp`; if not, add it.

- [ ] **Step 3: Build to verify it compiles**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check`
Expected: PASS (compiles).

- [ ] **Step 4: Manual smoke verification**

Run (in a scratch dir with one `.txt`):
`E:\cargo-target\dev-fast\anno-rag.exe ingest <scratch> --alias test-0001`
Then confirm a corpus exists by listing via the MCP `sources`/`corpus_list` or by checking `corpus.sqlite3`. Expected: one corpus row with `alias = test-0001`.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-bin/src/main.rs crates/anno-rag-bin/Cargo.toml
git commit -m "feat(cli): ingest registers a corpus with --profile/--alias"
```

---

## Phase 4 — Search surface: provenance, handles, path_prefix, disambiguation

### Task 9: Add `corpus_id` + `handle` to `SearchHit`

**Files:**
- Modify: `crates/anno-rag/src/store.rs` or wherever `SearchHit` is defined (grep first), `crates/anno-rag-mcp/src/legal.rs` (`LegalSearchHitWire`)

- [ ] **Step 1: Locate `SearchHit`**

Run: `rg "struct SearchHit" crates/anno-rag/src`
Then read the struct. Add two optional fields:

```rust
    /// Owning corpus id, when the search was corpus-scoped or cross-corpus.
    pub corpus_id: Option<String>,
    /// Readable document handle `alias/relative_path`, when resolvable.
    pub handle: Option<String>,
```

Default them to `None` at every existing construction site (compile errors will point to each).

- [ ] **Step 2: Expose them on `LegalSearchHitWire`**

In `crates/anno-rag-mcp/src/legal.rs:83`, extend:

```rust
#[derive(Serialize)]
pub(crate) struct LegalSearchHitWire {
    pub(crate) chunk_id: String,
    pub(crate) doc_id: String,
    pub(crate) text_pseudo: String,
    pub(crate) score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) corpus_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) handle: Option<String>,
}
```

Update the mapping in `legal_search_impl_with_effective` (`lib.rs` ~line 922) to populate `corpus_id`/`handle` (set `None` for now; Task 10 fills `handle`).

- [ ] **Step 3: Build to verify it compiles**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -AllAffected -Mode check`
Expected: PASS after all construction sites set the new fields.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/src crates/anno-rag-mcp/src/legal.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat: SearchHit carries corpus_id + handle (provenance scaffolding)"
```

---

### Task 10: Populate `handle` from alias + relative_path on hits

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`legal_search_impl_with_effective`)

- [ ] **Step 1: Write the failing integration test**

In `crates/anno-rag-mcp/tests/` add `handles.rs` (follow `health.rs` test harness patterns):

```rust
// Pseudocode-level intent — adapt to the existing test harness in tests/health.rs:
// 1. Register a corpus with alias "case-1" and record a document at "contrats/x.txt".
// 2. Run legal_search cross-corpus.
// 3. Assert at least one hit has handle == Some("case-1/contrats/x.txt").
```

> Read `tests/health.rs` first to reuse its server/fixture setup; mirror it exactly. If a full MCP round-trip is too heavy, instead unit-test a pure helper `fn build_handle(alias: Option<&str>, relative_path: Option<&str>) -> Option<String>` extracted into `corpus.rs`, and test that directly.

- [ ] **Step 2: Extract + test the pure helper**

In `corpus.rs`:

```rust
/// Compose a readable document handle from a corpus alias and a relative path.
/// Returns `None` when either part is missing (caller falls back to doc_id).
#[must_use]
pub fn build_handle(alias: Option<&str>, relative_path: Option<&str>) -> Option<String> {
    match (alias, relative_path) {
        (Some(a), Some(p)) if !a.is_empty() && !p.is_empty() => Some(format!("{a}/{p}")),
        _ => None,
    }
}
```

Test in `resolve_tests`:

```rust
    #[test]
    fn build_handle_composes_or_none() {
        assert_eq!(super::build_handle(Some("case-1"), Some("contrats/x.txt")), Some("case-1/contrats/x.txt".into()));
        assert_eq!(super::build_handle(None, Some("x")), None);
        assert_eq!(super::build_handle(Some("a"), None), None);
    }
```

- [ ] **Step 3: Wire `build_handle` into the hit mapping**

In `legal_search_impl_with_effective`, when mapping each hit to `LegalSearchHitWire`, look up the corpus alias (via `self.corpus().await` → `get(corpus_id)` → `alias`) and the document's `relative_path` (via `document_id_by_relative_path` reverse — or carry `relative_path` on the hit if the store already has it), then:

```rust
                handle: crate::corpus::build_handle(alias.as_deref(), relative_path.as_deref()),
                corpus_id: corpus_id.map(|c| c.as_string()),
```

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS (`build_handle_composes_or_none` + the handle integration/unit test).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-mcp/src/corpus.rs crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/tests/handles.rs
git commit -m "feat(mcp): populate readable document handles on legal search hits"
```

---

### Task 11: Legal tools accept handle in place of UUID

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (handlers for `legal_extract_contract`, `legal_risk_review`, `legal_timeline`)

- [ ] **Step 1: Write the failing test**

Add to `tests/handles.rs`:

```rust
// Intent: legal_extract_contract called with a handle "case-1/contrats/x.txt"
// returns the same result as calling it with the doc's UUID.
// Reuse the corpus + document fixture from Task 10.
```

- [ ] **Step 2: Resolve the ref at the top of each handler**

In `legal_extract_contract` (impl around `LegalExtractContractParams`), before using `doc_id`:

```rust
        let doc_id = match self.corpus().await {
            Ok(svc) => svc.resolve_doc_ref(&params.doc_id).unwrap_or(params.doc_id.clone()),
            Err(_) => params.doc_id.clone(),
        };
```

Apply the same pattern to `legal_risk_review` (`scope_id`) and `legal_timeline` (`dossier_id`).

- [ ] **Step 3: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS — handle and UUID produce identical extraction.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/tests/handles.rs
git commit -m "feat(mcp): legal tools accept document handle or UUID"
```

---

### Task 12: Structured disambiguation response on `CorpusRequired`

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`legal_search_impl` / `search` error mapping)

- [ ] **Step 1: Write the failing test**

Add to `tests/handles.rs`:

```rust
// Intent: with 2 corpora registered and no corpus_id / allow_cross_corpus,
// legal_search returns a JSON object with:
//   status == "corpus_required"
//   available[] listing each corpus's alias
//   a hint mentioning allow_cross_corpus
```

- [ ] **Step 2: Map `CorpusRequired` to a structured response**

Where `resolve_effective` is awaited in `legal_search_impl` (line 827), catch `CorpusRequired` specifically and build a JSON response instead of an opaque error:

```rust
        let effective = match self
            .resolve_effective_corpus(p.corpus_id.as_deref(), p.allow_cross_corpus)
            .await
        {
            Ok(eff) => eff,
            Err(e) if e.contains("multiple corpora") => {
                let available = self
                    .corpus()
                    .await
                    .map_err(|e| e.to_string())?
                    .list()
                    .map_err(|e| e.to_string())?;
                return Ok(serde_json::json!({
                    "status": "corpus_required",
                    "message": "Plusieurs dossiers indexés. Précisez un dossier ou demandez une recherche transversale.",
                    "available": available.iter().map(|c| serde_json::json!({
                        "corpus_id": c.corpus_id,
                        "alias": c.alias,
                        "label": c.label,
                        "health": c.health,
                    })).collect::<Vec<_>>(),
                    "hint": "Relancez avec corpus_id/alias, ou allow_cross_corpus: true pour un contrôle de conflits.",
                }));
            }
            Err(e) => return Err(e),
        };
```

> Adjust the error discriminant to match how `resolve_effective_corpus` surfaces `CorpusRequired` (it currently `.map_err(|e| e.to_string())`). Prefer matching on the typed `CorpusGuardError::CorpusRequired` by threading the typed error through rather than string-matching — refactor `resolve_effective_corpus` to return the typed error if practical.

- [ ] **Step 3: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS — disambiguation JSON with alias list.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/tests/handles.rs
git commit -m "feat(mcp): structured corpus_required disambiguation response"
```

---

### Task 13: `path_prefix` sub-folder filter

**Files:**
- Modify: `crates/anno-rag-mcp/src/legal.rs` (`LegalSearchParams`), `crates/anno-rag/src/pipeline.rs` (legal search filter), `crates/anno-rag/src/store.rs` (chunk filter)

- [ ] **Step 1: Add `path_prefix` to params**

In `LegalSearchParams` (legal.rs), add:

```rust
    /// Restrict to chunks whose relative_path starts with this prefix
    /// (e.g. "contrats"). Empty/None = no restriction.
    #[serde(default)]
    pub path_prefix: Option<String>,
```

Add the matching field to `LegalSearchFilters` in `anno-rag/src/legal/types.rs` and thread it through `legal_search*` in `pipeline.rs`.

- [ ] **Step 2: Write the failing test**

In `anno-rag/src/store.rs` tests (or pipeline tests), index two chunks with relative paths `contrats/a.txt` and `correspondance/b.txt`, search with `path_prefix = "contrats"`, assert only the first returns.

```rust
    // Adapt to the existing Store test harness: build a tiny LanceDB fixture,
    // upsert two chunks with differing relative_path metadata, search with the
    // prefix filter, assert the result set is restricted to "contrats/...".
```

- [ ] **Step 3: Apply the filter in the store query**

In the chunk search path, after retrieving candidate chunks, retain only those whose `relative_path` starts with `"{prefix}/"` (or equals the prefix). Apply at the LanceDB predicate level if supported, else as a post-filter:

```rust
        if let Some(prefix) = filters.path_prefix.as_deref().filter(|p| !p.is_empty()) {
            hits.retain(|h| {
                h.relative_path
                    .as_deref()
                    .is_some_and(|rp| rp == prefix || rp.starts_with(&format!("{prefix}/")))
            });
        }
```

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS — only `contrats/...` chunks returned.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src crates/anno-rag-mcp/src/legal.rs
git commit -m "feat: path_prefix sub-folder filter on legal search"
```

---

### Task 14: Final integration check + fmt + clippy

**Files:** none (verification)

- [ ] **Step 1: Format**

Run: `cargo fmt`
Then stage only formatting: `git add -A && git commit -m "style: cargo fmt"` (separate commit per project convention).

- [ ] **Step 2: Clippy (jobs 2)**

Run: `cargo clippy --package anno-corpus-store --package anno-rag-mcp --package anno-rag-bin --jobs 2 -- -D warnings`
Fix any lints inline.

- [ ] **Step 3: Targeted affected-crate check**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -AllAffected`
Expected: PASS across all affected crates.

- [ ] **Step 4: End-to-end manual smoke (Claude Desktop or CLI)**

- `ingest` two scratch folders with `--alias case-1` and `--alias case-2`.
- `legal_search` with no corpus → expect `corpus_required` listing both aliases.
- `legal_search` with `allow_cross_corpus: true` → hits carry distinct `corpus_id` + `handle`.
- `legal_extract_contract` with a `handle` → same result as with the UUID.
- `legal_search` with `path_prefix` → restricted to the sub-folder.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "test: end-to-end corpus search semantics verification"
```

---

## Self-Review notes

- **Spec coverage:** §4.1 → Task 8; §4.2 → Tasks 1–4; §4.3 → Task 5; §4.4 → Task 12; §4.5 → Task 13; §4.6 → Tasks 9–10; §4.7 → Tasks 6–7, 10–11. All covered.
- **Known soft spots flagged inline** (not placeholders — explicit "read X first" or "adapt to harness"): Task 9 Step 1 (locate `SearchHit`), Task 10 Step 1 (reuse `tests/health.rs` harness), Task 13 Step 2/3 (`relative_path` must be present on chunk hits — verify the Store carries it; if not, add it in Task 13 before the filter). These require reading the existing test/store harness, which the executing agent must do per-task.
- **Type consistency:** `alias: Option<String>` on `CorpusRow`; `resolve_doc_ref(&str) -> Result<String, CorpusGuardError>`; `build_handle(Option<&str>, Option<&str>) -> Option<String>` — used consistently across Tasks 2–11.
- **Open dependency to confirm during Task 13:** chunk `relative_path` availability in the Store search result. If absent, Task 13 grows a precursor step to thread it from ingestion metadata.
