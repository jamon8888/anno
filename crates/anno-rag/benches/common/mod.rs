//! Shared helpers for benches.
#![allow(dead_code)] // each bench uses a subset

use anno_rag::pipeline::Pipeline;
use anno_rag::AnnoRagConfig;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// Path to the bench fixture corpus relative to crate root.
pub fn bench_corpus_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bench_corpus")
}

/// Build a Pipeline pointing at a fresh temp data dir. Caller owns the TempDir.
pub async fn pipeline_in_tempdir() -> (Pipeline, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let key = [0u8; 32];
    let p = Pipeline::new(cfg, key).await.expect("pipeline");
    (p, tmp)
}

/// Current process RSS in bytes, cross-platform. Returns 0 on lookup failure.
pub fn current_rss_bytes() -> u64 {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_memory()),
    );
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}
