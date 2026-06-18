//! Verifies that Pipeline::new does NOT load models. Only ingest/search/detect should.
#![allow(clippy::unwrap_used)]

use anno_rag::{AnnoRagConfig, Pipeline};

#[tokio::test]
async fn pipeline_new_does_not_load_embedder() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let p = Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline");

    let stats = p.vault_stats().await;
    assert_eq!(stats.total_mappings, 0);
    assert!(!p.embedder_loaded(), "embedder should be lazy");
    assert!(!p.detector_loaded(), "detector should be lazy");
}

#[tokio::test]
async fn warmup_loads_both_models() {
    let models_dir = match std::env::var("ANNO_MODELS_DIR") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => return,
    };
    let cfg = anno_rag::config::AnnoRagConfig {
        data_dir: models_dir.parent().unwrap_or(&models_dir).to_path_buf(),
        ..Default::default()
    };
    let key = [0u8; 32];
    let pipeline = std::sync::Arc::new(anno_rag::pipeline::Pipeline::new(cfg, key).await.unwrap());
    assert!(!pipeline.embedder_loaded());
    assert!(!pipeline.detector_loaded());

    let outcome = std::sync::Arc::clone(&pipeline).warmup().await;
    assert!(
        outcome.embedder_ok,
        "embedder failed: {:?}",
        outcome.embedder_error
    );
    assert!(
        outcome.detector_ok,
        "detector failed: {:?}",
        outcome.detector_error
    );
    assert!(pipeline.embedder_loaded());
    assert!(pipeline.detector_loaded());
}

#[tokio::test]
#[ignore = "requires HF cache populated; exercised in bench harness"]
async fn search_triggers_embedder_init() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let p = Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline");
    let _ = p.search("test", 5).await;
    assert!(p.embedder_loaded());
    assert!(p.detector_loaded());
}
