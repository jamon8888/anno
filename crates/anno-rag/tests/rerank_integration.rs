//! Integration tests for the reranked search path. Heavy (model
//! download + LanceDB); ignored by default.
#![cfg(feature = "rerank")]

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "downloads model + opens LanceDB"]
async fn reranker_lazy_inits_only_on_demand() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");
    assert!(!p.reranker_loaded(), "reranker must not load at construction");
}
