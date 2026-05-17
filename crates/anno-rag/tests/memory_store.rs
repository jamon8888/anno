//! T3 integration test — Memory round-trip through LanceDB.
#![allow(clippy::unwrap_used)]
//!
//! Ignored by default: LanceDB table creation takes ~30s on the first run
//! and dominates the per-task feedback loop. Run explicitly with
//! `cargo test -p anno-rag --test memory_store -- --ignored` to exercise.

use anno_rag::config::AnnoRagConfig;
use anno_rag::memory::{Memory, MemoryId, MemoryKind, TokenRef};
use anno_rag::store::Store;
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
#[ignore = "lancedb table creation takes ~30s — same justification as open_creates_chunks_table"]
async fn insert_then_get_round_trips() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        embed_dim: 384,
        memory_embedding_dim: 384,
        ..Default::default()
    };
    let store = Store::open(&cfg).await.expect("open");

    let id = MemoryId::new();
    let now = Utc::now();
    let m = Memory {
        id: id.clone(),
        session_id: Some("s1".into()),
        kind: MemoryKind::Fact,
        text: "le dossier PERSON_a4f3".into(),
        created_at: now,
        accessed_at: now,
        valid_from: now,
        valid_to: None,
        embedding: vec![0.1f32; 384],
        token_refs: vec![TokenRef {
            label: "PERSON".into(),
            token: "PERSON_a4f3".into(),
        }],
        entity_refs: vec![],
    };
    store.memory_insert(&m).await.expect("memory_insert");

    let got = store
        .memory_get(&id)
        .await
        .expect("memory_get")
        .expect("must exist");
    assert_eq!(got.text, m.text);
    assert_eq!(got.session_id, m.session_id);
    assert_eq!(got.kind, m.kind);
}

#[tokio::test]
#[ignore = "needs at least one row before scalar indexes can be built — same lance/lancedb invariant the chunks IVF observes"]
async fn scalar_indexes_created_after_setup() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        embed_dim: 384,
        memory_embedding_dim: 384,
        ..Default::default()
    };
    let store = Store::open(&cfg).await.expect("open");

    // LanceDB rejects index creation on empty tables, so seed one row first.
    let now = Utc::now();
    let m = Memory {
        id: MemoryId::new(),
        session_id: Some("s1".into()),
        kind: MemoryKind::Fact,
        text: "seed".into(),
        created_at: now,
        accessed_at: now,
        valid_from: now,
        valid_to: None,
        embedding: vec![0.0f32; 384],
        token_refs: vec![TokenRef {
            label: "PERSON".into(),
            token: "PERSON_seed".into(),
        }],
        entity_refs: vec![],
    };
    store.memory_insert(&m).await.expect("seed insert");

    store
        .setup_memory_indexes()
        .await
        .expect("setup_memory_indexes");
    let indexes = store
        .memory_list_indexes()
        .await
        .expect("memory_list_indexes");
    let columns: Vec<String> = indexes.iter().flat_map(|i| i.columns.clone()).collect();
    for expected in [
        "created_at",
        "session_id",
        "kind",
        "token_refs",
        "entity_refs",
    ] {
        assert!(
            columns.iter().any(|c| c == expected),
            "missing index on {expected}; columns = {columns:?}"
        );
    }

    // Idempotent — second call must succeed and not create duplicates.
    store
        .setup_memory_indexes()
        .await
        .expect("setup_memory_indexes is idempotent");
}
