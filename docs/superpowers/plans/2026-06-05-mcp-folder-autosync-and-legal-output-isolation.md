# MCP Folder Auto-Sync And Legal Output Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Anno MCP treat a connected client folder as a living corpus without silently searching stale indexes, while keeping generated legal outputs outside the indexed source tree.

**Architecture:** Keep v1 pragmatic: do not add a permanent filesystem watcher and do not rewrite the legal/knowledge storage engines. First harden generated-output exclusion and move corpus-scoped legal outputs under `data_dir/corpora/<corpus_id>/outputs/legal-anon/`; then add explicit `sync_corpus`, corpus freshness state, and bounded knowledge auto-sync before selected-corpus search when models are already loaded. Legal sync stays explicit because it can trigger embeddings, legal enrichment, graph writes, and longer model work.

**Tech Stack:** Rust 2021, rmcp, Tokio, rusqlite, `anno-corpus-core`, `anno-corpus-store`, `anno-knowledge-store`, `anno-source-local`, `anno-rag`, existing `scripts/dev-fast.ps1` targeted build loop.

**Spec:** [`docs/superpowers/specs/2026-06-05-mcp-folder-autosync-and-legal-output-isolation-design.md`](../specs/2026-06-05-mcp-folder-autosync-and-legal-output-isolation-design.md)

---

## Scope Check

This plan covers one cross-cutting MCP behavior because the pieces are coupled:

- output isolation must land before opportunistic sync becomes more active;
- `sync_corpus` needs corpus bindings and knowledge source ids;
- search freshness must be returned by the same unified MCP search path that already enforces corpus scope.

Do not implement future Outlook/Notion connectors in this plan. The plan only leaves the source-sync contract compatible with those connectors.

Use a clean implementation worktree when executing. The current branch may already contain spec/doc edits; do not revert unrelated changes.

## File Map

Modify:

- `crates/anno-source-local/src/folder.rs` - stronger generated-artifact directory/file exclusion for knowledge local-folder discovery without suppressing ordinary client folders named `outputs`.
- `crates/anno-rag/src/pipeline.rs` - stronger generated-artifact exclusion for legal recursive ingest without suppressing ordinary client folders named `outputs`.
- `crates/anno-corpus-core/src/model.rs` - small sync/freshness enums shared by corpus store and MCP.
- `crates/anno-corpus-store/src/migrations.rs` - add corpus sync state table.
- `crates/anno-corpus-store/src/store.rs` - read/write corpus sync state and freshness.
- `crates/anno-rag-mcp/src/corpus.rs` - expose freshness in corpus health.
- `crates/anno-rag-mcp/src/lib.rs` - legal output root, `sync_corpus` tool, freshness fields in unified search, bounded opportunistic sync.
- `crates/anno-rag-mcp/src/health.rs` - advertise `sync_corpus`.

Create:

- `crates/anno-rag-mcp/src/corpus_sync.rs` - MCP sync parameter/result models and output parsing helpers.

Do not change:

- Claude Desktop config.
- Release packaging.
- tabular storage schema.
- non-folder source connector implementations.

## Build And Test Commands

Always check for active Rust builds first:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

Targeted checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-source-local -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-corpus-store -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Targeted unit tests:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-source-local
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```

Never run `cargo test --workspace` or `cargo build --release` locally for this plan.

## Fast Execution Strategy

This repo is slow when Cargo cache shape changes or when broad tests run. Execute with a verification ladder:

1. **Per edit:** run only the exact test named in the task, for example `cargo test -p anno-rag-mcp sync_corpus_unknown_corpus_returns_structured_error -- --nocapture`.
2. **Per task:** run one targeted `scripts\dev-fast.ps1 -Package <crate> -Mode check -Profile dev-fast` for the crate touched by that task.
3. **Per phase checkpoint:** run grouped `test-local.ps1` only after a cluster is complete:
   - checkpoint A after Tasks 1-2: `anno-source-local`, `anno-rag`, `anno-rag-mcp`;
   - checkpoint B after Tasks 3-6: `anno-corpus-store`, `anno-rag-mcp`;
   - checkpoint C after Tasks 7-9: `anno-rag-mcp`;
   - checkpoint D after docs/final verification: smoke only if a local binary is already available or the targeted `anno-rag-bin` dev-fast build is warm.
4. **Never during ordinary task work:** `cargo test --workspace`, release builds, all-feature builds, target cleanup, or switching profiles/features.

Subagent dispatch should also preserve speed:

- Task 0 stays local to the controller because it is the gate for branch state, active cargo processes, and GitNexus.
- Code-writing workers run sequentially for Tasks 1-9 because several tasks touch `crates/anno-rag-mcp/src/lib.rs`; parallel writes there would cost more in conflict resolution than they save.
- Read-only explorer agents may run in parallel for sidecar questions such as smoke command discovery, doc references, or existing test names.
- Reviewers do not run full test suites. Spec reviewers inspect the diff against the task. Code-quality reviewers run only the exact failing/passing tests already named by the task unless they identify a concrete risk that needs one additional targeted test.
- Workers must report every command they ran and avoid broad verification unless the task explicitly reaches a checkpoint.

Use hot-cache discipline:

- keep `-Profile dev-fast` for all `dev-fast.ps1` checks;
- do not change `RUSTFLAGS`, target triples, feature sets, or build profiles mid-task;
- check `Get-Process cargo,rustc` before each targeted check;
- when a command is slow, inspect whether another Rust process is active before assuming the code change caused it.

---

### Task 0: Pre-Flight And Impact Checks

**Files:** none.

- [ ] **Step 1: Confirm branch and dirty state**

Run:

```powershell
git status --short --branch
```

Expected: note existing doc/spec edits. Do not revert unrelated files.

- [ ] **Step 2: Confirm no local build is already running**

Run:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

Expected: no long-running `cargo` or `rustc`. If a dist or release build is running, wait or stop before targeted checks.

- [ ] **Step 3: Run GitNexus status**

Run:

```powershell
npx gitnexus status
```

Expected: index is present. If stale, run `npx gitnexus analyze` before code edits.

- [ ] **Step 4: Impact analysis for edited symbols**

Run:

```powershell
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
npx gitnexus impact --repo anno sync_local_scope --direction upstream
npx gitnexus impact --repo anno legal_ingest_candidate_paths --direction upstream
npx gitnexus impact --repo anno CorpusStore --direction upstream
```

Expected: record blast radius before editing. If any result is HIGH or CRITICAL, stop and report before code changes.

---

### Task 1: Harden Generated Artifact Exclusions

**Files:**
- Modify: `crates/anno-source-local/src/folder.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Write failing knowledge-source exclusion coverage**

In `crates/anno-source-local/src/folder.rs`, extend `skips_anno_generated_outputs`:

```rust
        fs::create_dir_all(dir.path().join("nested").join("anon")).expect("nested anon dir");
        fs::write(
            dir.path().join("nested").join("anon").join("source.md"),
            b"# generated nested anon",
        )
        .expect("nested anon output");
        fs::create_dir_all(dir.path().join("outputs")).expect("outputs dir");
        fs::write(
            dir.path().join("outputs").join("client-output.md"),
            b"# legitimate client output",
        )
        .expect("client output");
        fs::create_dir_all(dir.path().join(".anno")).expect(".anno dir");
        fs::write(dir.path().join(".anno").join("state.md"), b"# generated")
            .expect(".anno file");
        fs::create_dir_all(dir.path().join(".anno-rag")).expect(".anno-rag dir");
        fs::write(dir.path().join(".anno-rag").join("state.md"), b"# generated")
            .expect(".anno-rag file");
```

Keep the final assertion:

```rust
        assert_eq!(names, vec!["client-output.md", "source.md"]);
```

- [ ] **Step 2: Run the knowledge-source test and verify it fails**

Run:

```powershell
cargo test -p anno-source-local skips_anno_generated_outputs -- --nocapture
```

Expected: FAIL because nested `anon`, `.anno`, or `.anno-rag` files are still discovered, while `outputs/client-output.md` remains discoverable.

- [ ] **Step 3: Implement stronger knowledge-source filters**

Replace `is_generated_anno_dir` in `crates/anno-source-local/src/folder.rs`:

```rust
fn is_generated_anno_dir(root: &Path, path: &Path) -> bool {
    if path == root {
        return false;
    }
    path.strip_prefix(root)
        .ok()
        .map(|relative| {
            relative.components().any(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .map(is_generated_anno_dir_name)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn is_generated_anno_dir_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "anon" | ".anno" | ".anno-rag"
    )
}
```

Keep `is_generated_anno_file` unchanged unless the test shows a missed `.anon.` file case.

- [ ] **Step 4: Write failing legal walker coverage**

In `crates/anno-rag/src/pipeline.rs`, extend `legal_ingest_candidate_paths_skip_anno_generated_outputs`:

```rust
        std::fs::create_dir_all(dir.path().join("nested").join("anon")).expect("nested anon dir");
        std::fs::write(
            dir.path().join("nested").join("anon").join("ignored.md"),
            b"# generated",
        )
        .expect("nested anon generated");
        std::fs::create_dir_all(dir.path().join("outputs")).expect("outputs dir");
        std::fs::write(
            dir.path().join("outputs").join("kept.md"),
            b"# legitimate client output",
        )
        .expect("client outputs");
        std::fs::create_dir_all(dir.path().join(".anno")).expect(".anno dir");
        std::fs::write(dir.path().join(".anno").join("ignored.md"), b"# generated")
            .expect(".anno generated");
        std::fs::create_dir_all(dir.path().join(".anno-rag")).expect(".anno-rag dir");
        std::fs::write(dir.path().join(".anno-rag").join("ignored.md"), b"# generated")
            .expect(".anno-rag generated");
```

Keep the expected names:

```rust
        assert_eq!(names, vec!["contract.md", "kept.md", "source.md"]);
```

- [ ] **Step 5: Run the legal walker test and verify it fails**

Run:

```powershell
cargo test -p anno-rag legal_ingest_candidate_paths_skip_anno_generated_outputs -- --nocapture
```

Expected: FAIL because at least one generated directory is still scanned.

- [ ] **Step 6: Implement stronger legal generated-output filters**

Change `legal_ingest_candidate_paths` to pass the source root into the generated-output helper:

```rust
        .filter_entry(|entry| !is_anno_generated_output(entry.path(), folder, output_dir))
```

Change the file check:

```rust
        if !is_supported_ingest_path(path) || is_anno_generated_output(path, folder, output_dir) {
            continue;
        }
```

Replace `is_anno_generated_output`:

```rust
fn is_anno_generated_output(path: &Path, source_root: &Path, output_dir: &Path) -> bool {
    if path.starts_with(output_dir) {
        return true;
    }
    if path != source_root
        && path
            .strip_prefix(source_root)
            .ok()
            .map(|relative| {
                relative.components().any(|component| {
                    component
                        .as_os_str()
                        .to_str()
                        .map(is_generated_anno_dir_name)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().contains(".anon."))
        .unwrap_or(false)
}

fn is_generated_anno_dir_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "anon" | ".anno" | ".anno-rag"
    )
}
```

- [ ] **Step 7: Verify tests pass**

Run:

```powershell
cargo test -p anno-source-local skips_anno_generated_outputs -- --nocapture
cargo test -p anno-rag legal_ingest_candidate_paths_skip_anno_generated_outputs -- --nocapture
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-source-local/src/folder.rs crates/anno-rag/src/pipeline.rs
git commit -m "fix: skip generated anno artifacts during local ingest"
```

---

### Task 2: Move Corpus-Scoped Legal Outputs Outside Source Roots

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add failing MCP unit test**

Add this test near the existing `index_*` tests in `crates/anno-rag-mcp/src/lib.rs`:

```rust
#[test]
fn corpus_legal_output_dir_is_internal_and_corpus_scoped() {
    let cfg = AnnoRagConfig {
        data_dir: std::path::PathBuf::from("C:/anno-data"),
        ..AnnoRagConfig::default()
    };
    let corpus_id = anno_corpus_core::CorpusId::from_normalized_root("C:/clients/matter-a");

    let out = corpus_legal_output_dir(&cfg, corpus_id);

    assert_eq!(
        out,
        std::path::PathBuf::from("C:/anno-data")
            .join("corpora")
            .join(corpus_id.as_string())
            .join("outputs")
            .join("legal-anon")
    );
    assert!(!out.starts_with("C:/clients/matter-a"));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```powershell
cargo test -p anno-rag-mcp corpus_legal_output_dir_is_internal_and_corpus_scoped -- --nocapture
```

Expected: FAIL because `corpus_legal_output_dir` does not exist.

- [ ] **Step 3: Add the helper**

In `crates/anno-rag-mcp/src/lib.rs`, add near other private helpers:

```rust
fn corpus_legal_output_dir(
    cfg: &AnnoRagConfig,
    corpus_id: anno_corpus_core::CorpusId,
) -> std::path::PathBuf {
    cfg.data_dir
        .join("corpora")
        .join(corpus_id.as_string())
        .join("outputs")
        .join("legal-anon")
}
```

- [ ] **Step 4: Use the helper in `legal_ingest_impl`**

Replace:

```rust
        let out = folder.join("anon");
```

with:

```rust
        let out = corpus_id
            .map(|corpus_id| corpus_legal_output_dir(self.cfg.as_ref(), corpus_id))
            .unwrap_or_else(|| folder.join("anon"));
```

Keep the legacy no-corpus path as `folder/anon` so the deprecated `legal_ingest` behavior remains compatible.

- [ ] **Step 5: Include output ownership in the legal ingest response**

Change `LegalIngestResult`:

```rust
#[derive(Serialize)]
struct LegalIngestResult {
    ingested: usize,
    folder: String,
    output_root: String,
    output_scope: String,
}
```

Change the response construction:

```rust
                serde_json::to_value(LegalIngestResult {
                    ingested: summary.ingested,
                    folder: p.folder,
                    output_root: out.display().to_string(),
                    output_scope: if corpus_id.is_some() {
                        "corpus_internal".to_string()
                    } else {
                        "legacy_source_anon".to_string()
                    },
                })
```

- [ ] **Step 6: Verify targeted tests**

Run:

```powershell
cargo test -p anno-rag-mcp corpus_legal_output_dir_is_internal_and_corpus_scoped -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "fix: isolate corpus legal outputs from source folders"
```

---

### Task 3: Add Corpus Sync State And Freshness Types

**Files:**
- Modify: `crates/anno-corpus-core/src/model.rs`
- Modify: `crates/anno-corpus-store/src/migrations.rs`
- Modify: `crates/anno-corpus-store/src/store.rs`

- [ ] **Step 1: Add shared freshness enums**

In `crates/anno-corpus-core/src/model.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusFreshness {
    Fresh,
    MaybeStale,
    Stale,
    Unknown,
}

impl CorpusFreshness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::MaybeStale => "maybe_stale",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusSyncOutputKind {
    KnowledgeFast,
    LegalSemantic,
}

impl CorpusSyncOutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::KnowledgeFast => "knowledge_fast",
            Self::LegalSemantic => "legal_semantic",
        }
    }
}
```

- [ ] **Step 2: Write failing corpus-store migration test**

In `crates/anno-corpus-store/src/migrations.rs`, extend the schema test to assert:

```rust
        assert!(names.contains(&"corpus_sync_state".to_string()));
```

Run:

```powershell
cargo test -p anno-corpus-store migrations_create_expected_tables -- --nocapture
```

Expected: FAIL because the table does not exist.

- [ ] **Step 3: Add the sync state table**

In `crates/anno-corpus-store/src/migrations.rs`, add after `corpus_index_runs`:

```sql
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
```

- [ ] **Step 4: Add store row and methods**

In `crates/anno-corpus-store/src/store.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusSyncStateRow {
    pub corpus_id: CorpusId,
    pub freshness: String,
    pub last_sync_started_at: Option<String>,
    pub last_sync_finished_at: Option<String>,
    pub last_seen_file_count: Option<u64>,
    pub last_seen_root_mtime: Option<String>,
    pub last_summary_json: serde_json::Value,
}
```

Add methods in `impl CorpusStore`:

```rust
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
```

- [ ] **Step 5: Add store round-trip test**

In `crates/anno-corpus-store/src/store.rs`, add:

```rust
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
```

- [ ] **Step 6: Verify**

Run:

```powershell
cargo test -p anno-corpus-store sync_state_round_trips_freshness_and_summary -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-corpus-store -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-corpus-core/src/model.rs crates/anno-corpus-store/src/migrations.rs crates/anno-corpus-store/src/store.rs
git commit -m "feat: track corpus sync freshness state"
```

---

### Task 4: Add `sync_corpus` MCP Tool For `knowledge_fast`

**Files:**
- Create: `crates/anno-rag-mcp/src/corpus_sync.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`

- [ ] **Step 1: Create sync parameter/result models**

Create `crates/anno-rag-mcp/src/corpus_sync.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SyncCorpusParams {
    pub corpus_id: String,
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    #[serde(default = "default_outputs")]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub max_files: Option<usize>,
    #[serde(default)]
    pub max_millis: Option<u64>,
}

fn default_outputs() -> Vec<String> {
    vec!["knowledge_fast".to_string()]
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncCorpusResult {
    pub ok: bool,
    pub corpus_id: String,
    pub freshness: String,
    pub sources: SyncSourceSummary,
    pub knowledge: serde_json::Value,
    pub legal: serde_json::Value,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncSourceSummary {
    pub bound_sources: usize,
    pub synced_sources: usize,
    pub skipped_sources: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestedOutputs {
    pub knowledge_fast: bool,
    pub legal_semantic: bool,
}

pub fn parse_requested_outputs(outputs: &[String]) -> Result<RequestedOutputs, String> {
    let mut requested = RequestedOutputs {
        knowledge_fast: false,
        legal_semantic: false,
    };
    for output in outputs {
        match output.as_str() {
            "knowledge_fast" => requested.knowledge_fast = true,
            "legal_semantic" => requested.legal_semantic = true,
            other => {
                return Err(format!(
                    "unsupported output '{other}'. Expected knowledge_fast or legal_semantic"
                ));
            }
        }
    }
    if !requested.knowledge_fast && !requested.legal_semantic {
        requested.knowledge_fast = true;
    }
    Ok(requested)
}
```

- [ ] **Step 2: Wire the module and advertised tool**

In `crates/anno-rag-mcp/src/lib.rs`, add:

```rust
mod corpus_sync;
```

In `crates/anno-rag-mcp/src/health.rs`, add `"sync_corpus"` after `"index"`:

```rust
        "index",
        "sync_corpus",
        "search",
```

Update the tool-order test:

```rust
        assert_eq!(names[0], "index");
        assert_eq!(names[1], "sync_corpus");
        assert_eq!(names[2], "search");
        assert_eq!(names[3], "sources");
```

- [ ] **Step 3: Add failing output parser tests**

In `crates/anno-rag-mcp/src/corpus_sync.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_outputs_defaults_empty_to_knowledge_fast() {
        let outputs = parse_requested_outputs(&[]).expect("parse");
        assert!(outputs.knowledge_fast);
        assert!(!outputs.legal_semantic);
    }

    #[test]
    fn parse_outputs_rejects_unknown_output() {
        let err = parse_requested_outputs(&["deep_magic".to_string()]).expect_err("unknown");
        assert!(err.contains("unsupported output"));
    }
}
```

Run:

```powershell
cargo test -p anno-rag-mcp parse_outputs -- --nocapture
```

Expected: PASS after the module is created.

- [ ] **Step 4: Add `sync_corpus_impl`**

In `impl AnnoRagServer`, add:

```rust
    async fn sync_corpus_impl(
        &self,
        p: crate::corpus_sync::SyncCorpusParams,
    ) -> Result<crate::corpus_sync::SyncCorpusResult, String> {
        let corpus_id = crate::corpus::parse_corpus_id(&p.corpus_id)?;
        let requested = crate::corpus_sync::parse_requested_outputs(&p.outputs)?;
        let corpus = self.corpus().await.map_err(|e| e.to_string())?;
        if !corpus.corpus_exists(corpus_id).map_err(|e| e.to_string())? {
            return Err(format!("unknown corpus_id: {}", p.corpus_id));
        }

        let bound_sources = corpus
            .store()
            .binding_ids_for_corpus_kind(
                corpus_id,
                anno_corpus_core::CorpusBindingKind::KnowledgeSource,
            )
            .map_err(|e| e.to_string())?;
        let selected_sources: Vec<String> = match p.sources {
            Some(sources) => bound_sources
                .iter()
                .filter(|source_id| sources.iter().any(|wanted| wanted == *source_id))
                .cloned()
                .collect(),
            None => bound_sources.clone(),
        };

        let started_at = chrono::Utc::now().to_rfc3339();
        let mut warnings = Vec::new();
        let mut total = crate::indexer::SyncSummary::default();

        if requested.knowledge_fast {
            for source_id in &selected_sources {
                match self
                    .knowledge_sync_impl(KnowledgeSyncParams {
                        source_id: Some(source_id.clone()),
                    })
                    .await
                {
                    Ok(summary) => {
                        total.seen += summary.seen;
                        total.skipped_unchanged += summary.skipped_unchanged;
                        total.extracted += summary.extracted;
                        total.pseudonymized += summary.pseudonymized;
                        total.fts_ready += summary.fts_ready;
                        total.forgotten += summary.forgotten;
                        total.failed += summary.failed;
                        total.truncated |= summary.truncated;
                    }
                    Err(error) => warnings.push(format!("knowledge source {source_id}: {error}")),
                }
            }
        }

        let legal = if requested.legal_semantic {
            serde_json::json!({"ran": false, "reason": "legal_semantic not enabled in this phase"})
        } else {
            serde_json::json!({"ran": false, "reason": "output not requested"})
        };
        let freshness = if total.failed == 0 && !total.truncated && warnings.is_empty() {
            "fresh"
        } else {
            "maybe_stale"
        };
        let finished_at = chrono::Utc::now().to_rfc3339();
        let summary = serde_json::json!({
            "knowledge_fast": total,
            "legal": legal,
            "warnings": warnings,
        });
        corpus
            .store()
            .upsert_sync_state(
                corpus_id,
                freshness,
                Some(&started_at),
                Some(&finished_at),
                Some(total.seen),
                None,
                &summary,
            )
            .map_err(|e| e.to_string())?;

        Ok(crate::corpus_sync::SyncCorpusResult {
            ok: true,
            corpus_id: corpus_id.as_string(),
            freshness: freshness.to_string(),
            sources: crate::corpus_sync::SyncSourceSummary {
                bound_sources: bound_sources.len(),
                synced_sources: selected_sources.len(),
                skipped_sources: bound_sources.len().saturating_sub(selected_sources.len()),
            },
            knowledge: serde_json::to_value(total).map_err(|e| e.to_string())?,
            legal,
            warnings,
        })
    }
```

This first version accepts `max_files` and `max_millis` in the API but still uses the existing knowledge sync default budget. Task 5 tightens the budget plumbing.

- [ ] **Step 5: Add the MCP tool**

In the `#[tool_router] impl AnnoRagServer` block, add:

```rust
    #[tool(
        description = "Synchronize a selected corpus. Defaults to knowledge_fast; legal_semantic must be requested explicitly."
    )]
    async fn sync_corpus(
        &self,
        Parameters(p): Parameters<crate::corpus_sync::SyncCorpusParams>,
    ) -> String {
        match self.sync_corpus_impl(p).await {
            Ok(result) => serde_json::to_string_pretty(&result)
                .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
        }
    }
```

- [ ] **Step 6: Add routing test for unknown corpus**

In `crates/anno-rag-mcp/src/lib.rs`, add:

```rust
#[tokio::test]
async fn sync_corpus_unknown_corpus_returns_structured_error() {
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
    let out = server
        .sync_corpus(Parameters(crate::corpus_sync::SyncCorpusParams {
            corpus_id: uuid::Uuid::new_v4().to_string(),
            sources: None,
            outputs: vec!["knowledge_fast".to_string()],
            max_files: None,
            max_millis: None,
        }))
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"].as_str().unwrap().contains("unknown corpus_id"));
}
```

- [ ] **Step 7: Verify**

Run:

```powershell
cargo test -p anno-rag-mcp parse_outputs -- --nocapture
cargo test -p anno-rag-mcp sync_corpus_unknown_corpus_returns_structured_error -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag-mcp/src/corpus_sync.rs crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs
git commit -m "feat: add corpus sync mcp tool"
```

---

### Task 5: Plumb Bounded Knowledge Sync Budgets

**Files:**
- Modify: `crates/anno-rag-mcp/src/indexer.rs`
- Modify: `crates/anno-rag-mcp/src/knowledge.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add sync options**

In `crates/anno-rag-mcp/src/indexer.rs`, add:

```rust
#[derive(Debug, Clone, Copy)]
pub struct SyncOptions {
    pub max_files: usize,
    pub max_millis: Option<u64>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        let budget = DiscoverBudget::default();
        Self {
            max_files: budget.max_files,
            max_millis: None,
        }
    }
}
```

- [ ] **Step 2: Change `sync_local_scope` signature**

Change:

```rust
pub async fn sync_local_scope(
    store: &KnowledgeControlStore,
    pipeline: &Pipeline,
    cfg: &AnnoRagConfig,
    source: &SourceRow,
    scope: &ScopeRow,
) -> Result<SyncSummary, String> {
```

to:

```rust
pub async fn sync_local_scope(
    store: &KnowledgeControlStore,
    pipeline: &Pipeline,
    cfg: &AnnoRagConfig,
    source: &SourceRow,
    scope: &ScopeRow,
    options: SyncOptions,
) -> Result<SyncSummary, String> {
```

Replace:

```rust
    let budget = DiscoverBudget::default();
```

with:

```rust
    let budget = DiscoverBudget {
        max_files: options.max_files,
        ..DiscoverBudget::default()
    };
    let started = std::time::Instant::now();
```

Inside the object loop, before extraction, add:

```rust
        if options
            .max_millis
            .is_some_and(|limit| started.elapsed().as_millis() as u64 >= limit)
        {
            summary.truncated = true;
            break;
        }
```

- [ ] **Step 3: Update `KnowledgeService::sync`**

In `crates/anno-rag-mcp/src/knowledge.rs`, change the import:

```rust
use crate::indexer::{sync_local_scope, SyncOptions, SyncSummary};
```

Change method signature:

```rust
    pub async fn sync(
        &self,
        pipeline: &anno_rag::pipeline::Pipeline,
        cfg: &AnnoRagConfig,
        source_id: Option<&str>,
        options: SyncOptions,
    ) -> Result<SyncSummary, String> {
```

Change the call:

```rust
                let s = sync_local_scope(&self.store, pipeline, cfg, source, scope, options).await?;
```

- [ ] **Step 4: Preserve legacy `knowledge_sync` defaults**

In `knowledge_sync_impl`, change:

```rust
            .sync(pipeline, self.cfg.as_ref(), p.source_id.as_deref())
```

to:

```rust
            .sync(
                pipeline,
                self.cfg.as_ref(),
                p.source_id.as_deref(),
                crate::indexer::SyncOptions::default(),
            )
```

- [ ] **Step 5: Use budgets in `sync_corpus_impl`**

In `sync_corpus_impl`, replace each `knowledge_sync_impl` call with direct knowledge service sync:

```rust
                let service = self.knowledge().await.map_err(|e| e.to_string())?;
                let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
                let options = crate::indexer::SyncOptions {
                    max_files: p.max_files.unwrap_or_else(|| crate::indexer::SyncOptions::default().max_files),
                    max_millis: p.max_millis,
                };
                match service
                    .sync(
                        pipeline,
                        self.cfg.as_ref(),
                        Some(source_id.as_str()),
                        options,
                    )
                    .await
```

- [ ] **Step 6: Add budget test**

In `crates/anno-rag-mcp/src/indexer.rs`, add:

```rust
#[test]
fn sync_options_default_matches_discovery_budget() {
    let options = SyncOptions::default();
    let budget = DiscoverBudget::default();
    assert_eq!(options.max_files, budget.max_files);
    assert_eq!(options.max_millis, None);
}
```

- [ ] **Step 7: Verify**

Run:

```powershell
cargo test -p anno-rag-mcp sync_options_default_matches_discovery_budget -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag-mcp/src/indexer.rs crates/anno-rag-mcp/src/knowledge.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat: bound corpus knowledge sync work"
```

---

### Task 6: Surface Freshness In Corpus Health And Search

**Files:**
- Modify: `crates/anno-rag-mcp/src/corpus.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Extend corpus health wire type**

In `CorpusHealthWire`, add:

```rust
    /// Freshness from the last sync state row.
    pub freshness: String,
```

In `CorpusService::health`, load sync state:

```rust
        let freshness = self
            .store
            .sync_state(corpus_id)?
            .map(|state| state.freshness)
            .unwrap_or_else(|| "unknown".to_string());
```

Return it:

```rust
            freshness,
```

- [ ] **Step 2: Add freshness helper in MCP server**

In `impl AnnoRagServer`, add:

```rust
    async fn freshness_for_effective(
        &self,
        effective: &anno_corpus_core::EffectiveCorpus,
    ) -> Result<(bool, String), String> {
        let anno_corpus_core::EffectiveCorpus::Single(corpus_id) = effective else {
            return Ok((false, "cross_corpus".to_string()));
        };
        let service = self.corpus().await.map_err(|e| e.to_string())?;
        let state = service
            .store()
            .sync_state(*corpus_id)
            .map_err(|e| e.to_string())?;
        let freshness = state
            .map(|state| state.freshness)
            .unwrap_or_else(|| "unknown".to_string());
        Ok((freshness == "fresh", freshness))
    }
```

- [ ] **Step 3: Add freshness fields to unified search response**

In `search_impl_routing`, after resolving `effective`, add:

```rust
        let (index_fresh, freshness) = match self.freshness_for_effective(&effective).await {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!("freshness failed: {error}"));
                (false, "unknown".to_string())
            }
        };
```

Add fields to the final JSON:

```rust
            "index_fresh": index_fresh,
            "freshness": freshness,
            "sync": {
                "attempted": false,
                "reason": "not_requested"
            },
```

- [ ] **Step 4: Add search freshness test**

In `crates/anno-rag-mcp/src/lib.rs`, add:

```rust
#[tokio::test]
async fn search_reports_unknown_freshness_for_unsynced_single_corpus() {
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
    let corpus = server.corpus().await.expect("corpus");
    let registered = corpus
        .register_index_root("c:/clients/a", "general")
        .expect("register");

    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".to_string(),
            top_k: 5,
            mode: Some("fast".to_string()),
            scope: Some("knowledge".to_string()),
            filters: None,
            corpus_id: Some(registered.corpus_id.as_string()),
            allow_cross_corpus: false,
        })
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["index_fresh"], false);
    assert_eq!(parsed["freshness"], "unknown");
}
```

- [ ] **Step 5: Verify**

Run:

```powershell
cargo test -p anno-rag-mcp search_reports_unknown_freshness_for_unsynced_single_corpus -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag-mcp/src/corpus.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat: report corpus index freshness in mcp responses"
```

---

### Task 7: Add Bounded Opportunistic Knowledge Sync Before Search

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add helper that avoids model cold start**

In `impl AnnoRagServer`, add:

```rust
    async fn maybe_sync_knowledge_before_search(
        &self,
        effective: &anno_corpus_core::EffectiveCorpus,
        scope: &str,
        warnings: &mut Vec<String>,
    ) -> serde_json::Value {
        if !(scope == "all" || scope == "knowledge") {
            return serde_json::json!({"attempted": false, "reason": "scope_not_knowledge"});
        }
        let anno_corpus_core::EffectiveCorpus::Single(corpus_id) = effective else {
            return serde_json::json!({"attempted": false, "reason": "cross_corpus"});
        };
        if self.pipeline_arc().is_none() {
            return serde_json::json!({
                "attempted": false,
                "reason": "models_not_loaded"
            });
        }
        let p = crate::corpus_sync::SyncCorpusParams {
            corpus_id: corpus_id.as_string(),
            sources: None,
            outputs: vec!["knowledge_fast".to_string()],
            max_files: Some(25),
            max_millis: Some(750),
        };
        match self.sync_corpus_impl(p).await {
            Ok(result) => serde_json::json!({
                "attempted": true,
                "freshness": result.freshness,
                "warnings": result.warnings,
            }),
            Err(error) => {
                warnings.push(format!("opportunistic sync failed: {error}"));
                serde_json::json!({
                    "attempted": true,
                    "error": error
                })
            }
        }
    }
```

- [ ] **Step 2: Call helper before search reads indexes**

In `search_impl_routing`, after resolving `effective` and before the knowledge/ legal search blocks, add:

```rust
        let sync_status = self
            .maybe_sync_knowledge_before_search(&effective, &scope, &mut warnings)
            .await;
```

Replace the final `"sync"` field from Task 6:

```rust
            "sync": sync_status,
```

- [ ] **Step 3: Add no-cold-start test**

In `crates/anno-rag-mcp/src/lib.rs`, add:

```rust
#[tokio::test]
async fn search_does_not_opportunistically_load_pipeline_when_models_are_cold() {
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
    let corpus = server.corpus().await.expect("corpus");
    let registered = corpus
        .register_index_root("c:/clients/a", "general")
        .expect("register");

    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".to_string(),
            top_k: 5,
            mode: Some("fast".to_string()),
            scope: Some("knowledge".to_string()),
            filters: None,
            corpus_id: Some(registered.corpus_id.as_string()),
            allow_cross_corpus: false,
        })
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["sync"]["attempted"], false);
    assert_eq!(parsed["sync"]["reason"], "models_not_loaded");
    assert!(server.pipeline_arc().is_none());
}
```

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p anno-rag-mcp search_does_not_opportunistically_load_pipeline_when_models_are_cold -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat: add bounded opportunistic knowledge sync"
```

---

### Task 8: Add Explicit `legal_semantic` Sync Support

**Files:**
- Modify: `crates/anno-corpus-store/src/store.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add legal folder metadata read support**

In `crates/anno-corpus-store/src/store.rs`, add a `CorpusStore` method:

```rust
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
```

- [ ] **Step 2: Add metadata round-trip test**

In `crates/anno-corpus-store/src/store.rs`, add:

```rust
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
```

- [ ] **Step 3: Store legal source path in binding metadata**

In `legal_ingest_impl`, change the legal binding metadata:

```rust
&serde_json::json!({
    "label": label.clone(),
    "source_path": p.folder.clone()
}),
```

- [ ] **Step 4: Add source-folder resolver**

In `crates/anno-rag-mcp/src/lib.rs`, add:

```rust
fn source_folder_for_legal_binding(
    store: &anno_corpus_store::CorpusStore,
    corpus_id: anno_corpus_core::CorpusId,
    folder_id: &str,
) -> anno_corpus_store::Result<Option<String>> {
    Ok(store
        .binding_metadata(
            corpus_id,
            anno_corpus_core::CorpusBindingKind::LegalFolder,
            folder_id,
        )?
        .and_then(|metadata| {
            metadata
                .get("source_path")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        }))
}
```

- [ ] **Step 5: Resolve legal folder bindings inside `sync_corpus_impl`**

After knowledge sync in `sync_corpus_impl`, replace the Task 4 legal response branch with:

```rust
        let legal = if requested.legal_semantic {
            let legal_folders = corpus
                .store()
                .binding_ids_for_corpus_kind(
                    corpus_id,
                    anno_corpus_core::CorpusBindingKind::LegalFolder,
                )
                .map_err(|e| e.to_string())?;
            let mut ingested = 0usize;
            let mut legal_warnings = Vec::new();
            for folder_id in legal_folders {
                let Some(folder) = source_folder_for_legal_binding(corpus.store(), corpus_id, &folder_id)
                    .map_err(|e| e.to_string())?
                else {
                    legal_warnings.push(format!("legal folder binding {folder_id} has no source path"));
                    continue;
                };
                match self
                    .legal_ingest_impl(
                        LegalIngestParams {
                            folder,
                            recursive: true,
                        },
                        Some(corpus_id),
                    )
                    .await
                {
                    Ok(value) => {
                        ingested += value
                            .get("ingested")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0) as usize;
                    }
                    Err(error) => legal_warnings.push(error),
                }
            }
            serde_json::json!({
                "ran": true,
                "ingested": ingested,
                "warnings": legal_warnings,
            })
        } else {
            serde_json::json!({"ran": false, "reason": "output not requested"})
        };
```

- [ ] **Step 6: Verify**

Run:

```powershell
cargo test -p anno-corpus-store binding_metadata_round_trips_source_path -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-corpus-store -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-corpus-store/src/store.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat: support explicit legal corpus sync"
```

---

### Task 9: Add MCP Smoke Coverage For The Living Folder Behavior

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add regression test for stale search metadata**

Add:

```rust
#[tokio::test]
async fn search_marks_index_not_fresh_before_sync_corpus() {
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
    let corpus = server.corpus().await.expect("corpus");
    let registered = corpus
        .register_index_root("c:/clients/living-folder", "general")
        .expect("register");

    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "new document".to_string(),
            top_k: 5,
            mode: Some("fast".to_string()),
            scope: Some("knowledge".to_string()),
            filters: None,
            corpus_id: Some(registered.corpus_id.as_string()),
            allow_cross_corpus: false,
        })
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["index_fresh"], false);
    assert!(parsed["freshness"].as_str().is_some());
}
```

- [ ] **Step 2: Add regression test for multi-corpus guard**

Add:

```rust
#[tokio::test]
async fn search_still_requires_corpus_when_multiple_corpora_exist_after_freshness_changes() {
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
    let corpus = server.corpus().await.expect("corpus");
    corpus
        .register_index_root("c:/clients/a", "general")
        .expect("register a");
    corpus
        .register_index_root("c:/clients/b", "general")
        .expect("register b");

    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".to_string(),
            top_k: 5,
            mode: Some("fast".to_string()),
            scope: Some("knowledge".to_string()),
            filters: None,
            corpus_id: None,
            allow_cross_corpus: false,
        })
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"].as_str().unwrap().contains("corpus"));
}
```

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p anno-rag-mcp living-folder -- --nocapture
cargo test -p anno-rag-mcp search_still_requires_corpus_when_multiple_corpora_exist_after_freshness_changes -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "test: cover corpus freshness mcp search behavior"
```

---

### Task 10: Documentation And MCP Tool Inventory

**Files:**
- Modify: `docs/developers/mcp-tools.md`
- Modify: `README.md`
- Modify if present: `docs/developers/cli.md`

- [ ] **Step 1: Find current docs references**

Run:

```powershell
rg -n "knowledge_sync|sync_corpus|index_fresh|corpus_health|legal_ingest|folder/anon|legal-anon" README.md docs
```

Expected: list of docs to update. Only update existing docs locations.

- [ ] **Step 2: Add MCP tool docs**

In `docs/developers/mcp-tools.md`, add this entry near `index`:

````markdown
### `sync_corpus`

Synchronizes a selected corpus. By default it refreshes the `knowledge_fast` output only. Legal semantic refresh must be requested explicitly with `outputs=["legal_semantic"]` or `outputs=["knowledge_fast","legal_semantic"]`.

Example payload:

```json
{
  "corpus_id": "00000000-0000-0000-0000-000000000000",
  "outputs": ["knowledge_fast"],
  "max_files": 25,
  "max_millis": 750
}
```

The response includes `freshness`, source counts, knowledge summary, legal summary, and warnings. `freshness="fresh"` means the bounded sync completed without failed files or truncation.
````

- [ ] **Step 3: Document search freshness**

Add near unified `search` docs:

```markdown
Search responses include corpus freshness metadata:

- `index_fresh=true` means the selected corpus was synced successfully.
- `index_fresh=false` means Anno answered from the existing index and the caller should consider `sync_corpus`.
- `sync.attempted=false` with `reason="models_not_loaded"` means Anno avoided a hidden model cold start.
```

- [ ] **Step 4: Document legal output isolation**

Add:

```markdown
For corpus-scoped indexing, generated legal anonymized files are stored under Anno's data directory, not under the client source folder. Use the explicit export workflow when generated anonymized files need to be copied to a user-chosen destination.
```

- [ ] **Step 5: Verify stale docs scan**

Run:

```powershell
rg -n "folder/anon|include_legal|projection|projections|watcher" README.md docs
```

Expected: no stale `folder/anon` claim for corpus-scoped ingest; `watcher` appears only in the design/spec context that says no permanent watcher in v1.

- [ ] **Step 6: Commit**

```powershell
git add README.md docs/developers/mcp-tools.md docs/developers/cli.md
git commit -m "docs: describe corpus sync and freshness"
```

---

### Task 11: Final Verification

**Files:** none unless a verification failure reveals a bug.

- [ ] **Step 1: Run targeted crate checks**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-source-local -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-corpus-store -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: all exit code 0.

- [ ] **Step 2: Run targeted unit tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-source-local
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-corpus-store
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```

Expected: all exit code 0.

- [ ] **Step 3: Run MCP smoke if a local binary is available**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode build -Profile dev-fast
```

Expected: exit code 0 without recompiling the whole workspace.

Then run the existing MCP smoke entrypoint used by this repo, or list tools through the local binary if no smoke script is present:

```powershell
rg -n "mcp.*smoke|tools/list|anno_health" scripts docs crates/anno-rag-mcp
```

Expected: use the repo-local smoke command found by the search. `sync_corpus` must appear in tool inventory and `anno_health` must remain callable.

- [ ] **Step 4: GitNexus detect changes**

Run:

```powershell
npx gitnexus detect-changes
```

If the command is unavailable in the installed GitNexus CLI, run:

```powershell
npx gitnexus status
git diff --stat HEAD
```

Expected: changes are limited to the files in this plan.

- [ ] **Step 5: Final commit if verification changed docs or tests**

Only commit if Step 1-4 required additional edits:

```powershell
git add <changed-files>
git commit -m "fix: finalize corpus sync verification"
```

---

## Acceptance Checklist

- [ ] Recursive local discovery skips `anon`, `.anno`, `.anno-rag`, and `.anon.*` generated files while preserving ordinary client `outputs` folders.
- [ ] Corpus-scoped legal ingest writes generated files under `data_dir/corpora/<corpus_id>/outputs/legal-anon/`.
- [ ] Legacy `legal_ingest` without corpus remains backward-compatible.
- [ ] `sync_corpus` is listed in MCP tools and defaults to `knowledge_fast`.
- [ ] `sync_corpus(outputs=["knowledge_fast"])` catches up changed or added files through existing knowledge sync.
- [ ] `sync_corpus(outputs=["legal_semantic"])` is explicit and does not run during normal fast search.
- [ ] Search responses include `index_fresh`, `freshness`, and `sync`.
- [ ] Opportunistic sync does not cold-start models on a fast search.
- [ ] Multi-corpus guard behavior remains intact.
- [ ] Docs explain that the folder should feel live without a permanent watcher.
