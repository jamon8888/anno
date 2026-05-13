//! Verifies that Pipeline::new does NOT load models. Only ingest/search/detect should.

use anno_rag::{AnnoRagConfig, Pipeline};

#[tokio::test]
async fn pipeline_new_does_not_load_embedder() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = AnnoRagConfig::default();
    cfg.data_dir = tmp.path().to_path_buf();
    let p = Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline");

    let stats = p.vault_stats().await;
    assert_eq!(stats.total_mappings, 0);
    assert!(!p.embedder_loaded(), "embedder should be lazy");
    assert!(!p.detector_loaded(), "detector should be lazy");
}

#[tokio::test]
#[ignore = "requires HF cache populated; exercised in bench harness"]
async fn search_triggers_embedder_init() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = AnnoRagConfig::default();
    cfg.data_dir = tmp.path().to_path_buf();
    let p = Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline");
    let _ = p.search("test", 5).await;
    assert!(p.embedder_loaded());
    assert!(p.detector_loaded());
}
