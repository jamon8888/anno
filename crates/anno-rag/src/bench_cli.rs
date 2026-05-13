//! `anno-rag bench --corpus <dir>` — reproduce SLO measurements on a user corpus.
//!
//! Emits a markdown table to stdout showing cold-start, idle RSS, ingest
//! throughput, search latency, and peak RSS — checked against the v0.5
//! performance budget.

use crate::error::Result;
use crate::{AnnoRagConfig, Pipeline};
use std::path::Path;
use std::time::Instant;

const QUERIES: &[&str] = &[
    "résiliation contrat",
    "clause confidentialité",
    "préavis de fin",
    "objet du contrat",
    "modalités de paiement",
];

/// Run the bench and print a markdown report to stdout.
///
/// # Errors
/// Returns the underlying [`Error`](crate::error::Error) if pipeline construction,
/// ingest, or search fails.
pub async fn run(corpus: &Path) -> Result<()> {
    let tmp = tempfile::tempdir().map_err(crate::error::Error::from)?;
    let mut cfg = AnnoRagConfig::default();
    cfg.data_dir = tmp.path().to_path_buf();
    let key = [0u8; 32];

    let t0 = Instant::now();
    let p = Pipeline::new(cfg, key).await?;
    let cold = t0.elapsed();
    let rss_idle = current_rss_mb();

    let t1 = Instant::now();
    let n = p.ingest_folder(corpus, true, &tmp.path().join("out")).await?;
    let ingest = t1.elapsed();
    let rss_after_ingest = current_rss_mb();

    let mut search_lat_us = Vec::with_capacity(QUERIES.len());
    for q in QUERIES {
        let t = Instant::now();
        let _ = p.search(q, 10).await?;
        search_lat_us.push(t.elapsed().as_micros());
    }
    search_lat_us.sort_unstable();
    let p50 = search_lat_us[search_lat_us.len() / 2];
    let p95 = search_lat_us[search_lat_us.len() * 95 / 100];
    let rss_peak = current_rss_mb();

    println!("# anno-rag bench results\n");
    println!("Corpus: `{}`\n", corpus.display());
    println!("| Metric | Value | SLO |");
    println!("|---|---|---|");
    println!("| Cold-start | {:.2}s | <2s |", cold.as_secs_f64());
    println!("| Idle RSS | {} MB | <200 MB |", rss_idle);
    if n > 0 {
        println!(
            "| Ingest ({} docs) | {:.1}s ({:.1}s/doc) | <10s/doc |",
            n,
            ingest.as_secs_f64(),
            ingest.as_secs_f64() / n as f64
        );
    } else {
        println!("| Ingest | (no documents) | — |");
    }
    println!("| Search p50 | {} µs | — |", p50);
    println!("| Search p95 | {} µs | <200000 µs |", p95);
    println!("| Peak RSS | {} MB | <1500 MB |", rss_peak);
    println!("| RSS after ingest | {} MB | — |", rss_after_ingest);
    Ok(())
}

fn current_rss_mb() -> u64 {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_memory()),
    );
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).map(|p| p.memory() / 1024 / 1024).unwrap_or(0)
}
