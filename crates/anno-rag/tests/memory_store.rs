//! T3 integration test — Memory round-trip through LanceDB.
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
