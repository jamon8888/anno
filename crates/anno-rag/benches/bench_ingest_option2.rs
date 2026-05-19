//! Manual ingest performance harness for Option 2.
//!
//! This is intentionally a custom `harness = false` bench rather than a
//! Criterion benchmark: the goal is one machine-readable run per matrix point,
//! not statistical micro-benchmark iteration inside one process.
#![allow(clippy::unwrap_used, missing_docs)]

mod common;

use anno::OnnxSessionConfig;
use anno_rag::config::AnnoRagConfig;
use anno_rag::detect::Detector;
use anno_rag::embed::Embedder;
use anno_rag::ingest::{self, ExtractedChunk};
use anno_rag::store::{ChunkRecord, Store};
use anno_rag::vault::Vault;
use anyhow::{anyhow, bail, Context, Result};
use cloakpipe_core::DetectedEntity;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchKind {
    NerOnly,
    FullIngest,
}

impl BenchKind {
    fn parse(raw: &str) -> Result<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "ner_only" | "ner-only" | "ner" => Ok(Self::NerOnly),
            "full_ingest" | "full-ingest" | "full" => Ok(Self::FullIngest),
            other => {
                bail!("unsupported ANNO_RAG_INGEST_BENCH={other}; expected ner_only or full_ingest")
            }
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::NerOnly => "ner_only",
            Self::FullIngest => "full_ingest",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provider {
    Cpu,
    CoreMl,
}

impl Provider {
    fn parse(raw: &str) -> Result<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "cpu" => Ok(Self::Cpu),
            "coreml" | "core_ml" | "core-ml" => Ok(Self::CoreMl),
            other => bail!("unsupported ANNO_RAG_INGEST_PROVIDER={other}; expected cpu or coreml"),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::CoreMl => "coreml",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
struct BenchOptions {
    bench: BenchKind,
    corpus: PathBuf,
    provider: Provider,
    pool: usize,
    intra_threads: usize,
    warm: bool,
    index_tail: bool,
}

impl BenchOptions {
    fn from_env() -> Result<Self> {
        let bench = BenchKind::parse(&env_string("ANNO_RAG_INGEST_BENCH", "ner_only"))?;
        let corpus =
            env_path("ANNO_RAG_INGEST_BENCH_CORPUS").unwrap_or_else(common::bench_corpus_dir);
        let provider = Provider::parse(&env_string("ANNO_RAG_INGEST_PROVIDER", "cpu"))?;
        let pool = env_usize("ANNO_RAG_INGEST_POOL", 1)?;
        let intra_threads = env_usize("ANNO_RAG_INGEST_INTRA_THREADS", 0)?;
        let warm = env_bool("ANNO_RAG_INGEST_WARM", true)?;
        let index_tail = env_bool("ANNO_RAG_INGEST_INDEX_TAIL", false)?;

        if pool == 0 {
            bail!("ANNO_RAG_INGEST_POOL must be >= 1");
        }
        if pool > 1 && intra_threads == 0 {
            bail!("pool > 1 requires ANNO_RAG_INGEST_INTRA_THREADS > 0");
        }
        if provider == Provider::CoreMl {
            if !cfg!(target_os = "macos") {
                bail!("provider=coreml requires macOS");
            }
            #[cfg(not(feature = "gliner2-fastino-coreml"))]
            bail!("provider=coreml requires --features gliner2-fastino-coreml");
        }
        if !corpus.exists() {
            bail!("bench corpus does not exist: {}", corpus.display());
        }

        Ok(Self {
            bench,
            corpus,
            provider,
            pool,
            intra_threads,
            warm,
            index_tail,
        })
    }

    fn onnx_config(&self) -> OnnxSessionConfig {
        let mut cfg = OnnxSessionConfig::default();
        cfg.num_threads = self.intra_threads;
        if self.provider == Provider::CoreMl {
            cfg.prefer_coreml = true;
            cfg.use_cpu_provider = true;
        }
        cfg
    }
}

#[derive(Debug)]
struct PhaseMs {
    extract: u128,
    ner: u128,
    vault: u128,
    embed: u128,
    store: u128,
    anon_write: u128,
    index: u128,
    fts: u128,
}

impl PhaseMs {
    const fn zero() -> Self {
        Self {
            extract: 0,
            ner: 0,
            vault: 0,
            embed: 0,
            store: 0,
            anon_write: 0,
            index: 0,
            fts: 0,
        }
    }
}

#[derive(Debug)]
struct PerfResult {
    docs: usize,
    chunks: usize,
    elapsed: Duration,
    rss_peak_bytes: u64,
    phases: PhaseMs,
}

#[derive(Debug)]
struct BenchDoc {
    path: PathBuf,
    source_path: String,
    folder_path: String,
    doc_id: Uuid,
    chunks: Vec<ExtractedChunk>,
}

struct RssSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    handle: thread::JoinHandle<()>,
}

impl RssSampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(common::current_rss_bytes()));
        let worker_stop = Arc::clone(&stop);
        let worker_peak = Arc::clone(&peak);
        let handle = thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let rss = common::current_rss_bytes();
                update_peak(&worker_peak, rss);
                thread::sleep(Duration::from_millis(50));
            }
            update_peak(&worker_peak, common::current_rss_bytes());
        });
        Self { stop, peak, handle }
    }

    fn finish(self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.join();
        let rss = common::current_rss_bytes();
        update_peak(&self.peak, rss);
        self.peak.load(Ordering::Relaxed)
    }
}

fn update_peak(peak: &AtomicU64, value: u64) {
    let mut observed = peak.load(Ordering::Relaxed);
    while value > observed {
        match peak.compare_exchange_weak(observed, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => observed = next,
        }
    }
}

fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let opts = BenchOptions::from_env()?;
    let rt = tokio::runtime::Runtime::new().context("create tokio runtime")?;
    let result = match opts.bench {
        BenchKind::NerOnly => rt.block_on(run_ner_only(&opts))?,
        BenchKind::FullIngest => rt.block_on(run_full_ingest(&opts))?,
    };
    print_result(&opts, &result);
    Ok(())
}

async fn run_ner_only(opts: &BenchOptions) -> Result<PerfResult> {
    let cfg = AnnoRagConfig::default();
    let extract_started = Instant::now();
    let docs = collect_docs(&opts.corpus, &cfg).await?;
    let extract_ms = extract_started.elapsed().as_millis();
    let texts = flatten_texts(&docs);

    let detectors = if opts.warm {
        let detectors = build_detectors(opts)?;
        warm_detectors(&detectors)?;
        Some(detectors)
    } else {
        None
    };

    let sampler = RssSampler::start();
    let started = Instant::now();
    let detectors = match detectors {
        Some(detectors) => detectors,
        None => build_detectors(opts)?,
    };
    let ner_started = Instant::now();
    let _entities = detect_parallel(&detectors, &texts).await?;
    let mut phases = PhaseMs::zero();
    phases.extract = extract_ms;
    phases.ner = ner_started.elapsed().as_millis();
    let elapsed = started.elapsed();
    let rss_peak_bytes = sampler.finish();

    Ok(PerfResult {
        docs: docs.len(),
        chunks: texts.len(),
        elapsed,
        rss_peak_bytes,
        phases,
    })
}

async fn run_full_ingest(opts: &BenchOptions) -> Result<PerfResult> {
    let tmp = tempfile::tempdir().context("tempdir")?;
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().join("data"),
        ..Default::default()
    };
    let vault = Vault::open(&cfg.vault_path(), [0u8; 32])?;
    let store = Store::open(&cfg).await?;
    let output_dir = tmp.path().join("outputs");

    let detectors = if opts.warm {
        let detectors = build_detectors(opts)?;
        warm_detectors(&detectors)?;
        Some(detectors)
    } else {
        None
    };
    let embedder = if opts.warm {
        let embedder = Embedder::load(&cfg).await?;
        let _ = embedder.embed_batch(&["warmup".to_string()])?;
        Some(embedder)
    } else {
        None
    };

    let sampler = RssSampler::start();
    let started = Instant::now();
    let mut phases = PhaseMs::zero();

    let extract_started = Instant::now();
    let docs = collect_docs(&opts.corpus, &cfg).await?;
    phases.extract = extract_started.elapsed().as_millis();

    let flat = flatten_chunks(&docs);
    let texts: Vec<String> = flat.iter().map(|(_, chunk)| chunk.text.clone()).collect();

    let detectors = match detectors {
        Some(detectors) => detectors,
        None => build_detectors(opts)?,
    };
    let ner_started = Instant::now();
    let entities = detect_parallel(&detectors, &texts).await?;
    phases.ner = ner_started.elapsed().as_millis();

    let vault_started = Instant::now();
    let mut pseudo_chunks = Vec::with_capacity(texts.len());
    let mut pseudo_by_doc = vec![Vec::new(); docs.len()];
    for (flat_idx, (doc_idx, _chunk)) in flat.iter().enumerate() {
        let pseudo = vault
            .pseudonymize(&texts[flat_idx], &entities[flat_idx])
            .await?;
        pseudo_by_doc[*doc_idx].push(pseudo.clone());
        pseudo_chunks.push(pseudo);
    }
    phases.vault = vault_started.elapsed().as_millis();

    let embedder = match embedder {
        Some(embedder) => embedder,
        None => Embedder::load(&cfg).await?,
    };
    let embed_started = Instant::now();
    let vectors = embedder.embed_batch(&pseudo_chunks)?;
    if vectors.len() != pseudo_chunks.len() {
        bail!(
            "vectors len {} != chunks len {}",
            vectors.len(),
            pseudo_chunks.len()
        );
    }
    phases.embed = embed_started.elapsed().as_millis();

    let store_started = Instant::now();
    let mut records = Vec::with_capacity(flat.len());
    for (flat_idx, (doc_idx, chunk)) in flat.iter().enumerate() {
        let doc = &docs[*doc_idx];
        records.push(ChunkRecord {
            doc_id: doc.doc_id,
            source_path: doc.source_path.clone(),
            folder_path: doc.folder_path.clone(),
            chunk_idx: chunk.idx,
            text_pseudo: pseudo_chunks[flat_idx].clone(),
            page: chunk.page,
            char_start: chunk.char_start,
            char_end: chunk.char_end,
            vector: vectors[flat_idx].clone(),
        });
    }
    store.upsert(records).await?;
    phases.store = store_started.elapsed().as_millis();

    let write_started = Instant::now();
    write_anon_outputs(&docs, &pseudo_by_doc, &output_dir)?;
    phases.anon_write = write_started.elapsed().as_millis();

    if opts.index_tail {
        let index_started = Instant::now();
        let _ = store.maybe_build_index(cfg.vector_index_threshold).await?;
        phases.index = index_started.elapsed().as_millis();

        let fts_started = Instant::now();
        let _ = store.maybe_build_fts_index().await?;
        phases.fts = fts_started.elapsed().as_millis();
    }

    let elapsed = started.elapsed();
    let rss_peak_bytes = sampler.finish();

    Ok(PerfResult {
        docs: docs.len(),
        chunks: texts.len(),
        elapsed,
        rss_peak_bytes,
        phases,
    })
}

fn build_detectors(opts: &BenchOptions) -> Result<Vec<Arc<Detector>>> {
    (0..opts.pool)
        .map(|_| Detector::new_with_onnx_config(opts.onnx_config()).map(Arc::new))
        .collect::<anno_rag::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn warm_detectors(detectors: &[Arc<Detector>]) -> Result<()> {
    for detector in detectors {
        let _ = detector.detect("Madame Marie Durand signe a Paris.")?;
    }
    Ok(())
}

async fn detect_parallel(
    detectors: &[Arc<Detector>],
    texts: &[String],
) -> Result<Vec<Vec<DetectedEntity>>> {
    if detectors.is_empty() {
        bail!("detector pool is empty");
    }
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    if detectors.len() == 1 {
        return texts
            .iter()
            .map(|text| detectors[0].detect(text).map_err(anyhow::Error::from))
            .collect();
    }

    let mut buckets = vec![Vec::<(usize, String)>::new(); detectors.len()];
    for (idx, text) in texts.iter().cloned().enumerate() {
        buckets[idx % detectors.len()].push((idx, text));
    }

    let mut handles = Vec::with_capacity(detectors.len());
    for (detector, bucket) in detectors.iter().cloned().zip(buckets) {
        handles.push(tokio::task::spawn_blocking(
            move || -> Result<Vec<(usize, Vec<DetectedEntity>)>> {
                let mut out = Vec::with_capacity(bucket.len());
                for (idx, text) in bucket {
                    out.push((idx, detector.detect(&text)?));
                }
                Ok(out)
            },
        ));
    }

    let mut ordered = Vec::with_capacity(texts.len());
    ordered.resize_with(texts.len(), || None);
    for handle in handles {
        for (idx, entities) in handle.await.context("detector worker join")?? {
            ordered[idx] = Some(entities);
        }
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(idx, item)| item.ok_or_else(|| anyhow!("missing detector output for chunk {idx}")))
        .collect()
}

async fn collect_docs(corpus: &Path, cfg: &AnnoRagConfig) -> Result<Vec<BenchDoc>> {
    let mut docs = Vec::new();
    for path in collect_supported_files(corpus)? {
        let extracted = ingest::extract(&path, cfg)
            .await
            .with_context(|| format!("extract {}", path.display()))?;
        let folder_path = path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        docs.push(BenchDoc {
            path,
            source_path: extracted.source_path,
            folder_path,
            doc_id: Uuid::now_v7(),
            chunks: extracted.chunks,
        });
    }
    Ok(docs)
}

fn collect_supported_files(corpus: &Path) -> Result<Vec<PathBuf>> {
    let walker = if corpus.is_dir() {
        walkdir::WalkDir::new(corpus).into_iter()
    } else {
        walkdir::WalkDir::new(corpus).max_depth(1).into_iter()
    };
    let mut paths = Vec::new();
    for entry in walker {
        let entry = entry.with_context(|| format!("walk {}", corpus.display()))?;
        if entry.file_type().is_file() && is_supported(entry.path()) {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn is_supported(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "pdf"
            | "docx"
            | "pptx"
            | "xlsx"
            | "xls"
            | "xlsb"
            | "xlsm"
            | "txt"
            | "md"
            | "rst"
            | "html"
            | "htm"
            | "rtf"
            | "epub"
            | "odt"
            | "ods"
            | "odp"
            | "eml"
            | "msg"
            | "xml"
            | "csv"
            | "tsv"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "go"
            | "rb"
            | "php"
            | "swift"
            | "kt"
            | "scala"
            | "sql"
    )
}

fn flatten_texts(docs: &[BenchDoc]) -> Vec<String> {
    docs.iter()
        .flat_map(|doc| doc.chunks.iter().map(|chunk| chunk.text.clone()))
        .collect()
}

fn flatten_chunks(docs: &[BenchDoc]) -> Vec<(usize, ExtractedChunk)> {
    docs.iter()
        .enumerate()
        .flat_map(|(doc_idx, doc)| {
            doc.chunks
                .iter()
                .cloned()
                .map(move |chunk| (doc_idx, chunk))
        })
        .collect()
}

fn write_anon_outputs(
    docs: &[BenchDoc],
    pseudo_by_doc: &[Vec<String>],
    output_dir: &Path,
) -> Result<()> {
    std::fs::create_dir_all(output_dir).context("create output dir")?;
    for (doc, chunks) in docs.iter().zip(pseudo_by_doc) {
        let stem = doc
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("doc");
        let out_path = output_dir.join(format!("{stem}.anon.md"));
        std::fs::write(out_path, chunks.join("\n\n")).context("write anon output")?;
    }
    Ok(())
}

fn print_result(opts: &BenchOptions, result: &PerfResult) {
    let elapsed_ms = result.elapsed.as_millis();
    let elapsed_s = result.elapsed.as_secs_f64();
    let docs_per_s = rate(result.docs, elapsed_s);
    let chunks_per_s = rate(result.chunks, elapsed_s);
    let rss_peak_mb = result.rss_peak_bytes / (1024 * 1024);
    println!(
        "INGEST_PERF os={} provider={} bench={} docs={} chunks={} pool={} intra_threads={} warm={} elapsed_ms={} docs_per_s={:.4} chunks_per_s={:.4} rss_peak_mb={} extract_ms={} ner_ms={} vault_ms={} embed_ms={} store_ms={} anon_write_ms={} index_ms={} fts_ms={}",
        std::env::consts::OS,
        opts.provider,
        opts.bench.as_str(),
        result.docs,
        result.chunks,
        opts.pool,
        opts.intra_threads,
        opts.warm,
        elapsed_ms,
        docs_per_s,
        chunks_per_s,
        rss_peak_mb,
        result.phases.extract,
        result.phases.ner,
        result.phases.vault,
        result.phases.embed,
        result.phases.store,
        result.phases.anon_write,
        result.phases.index,
        result.phases.fts
    );
}

fn rate(count: usize, elapsed_s: f64) -> f64 {
    if elapsed_s <= f64::EPSILON {
        0.0
    } else {
        count as f64 / elapsed_s
    }
}

fn env_string(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn env_usize(name: &str, default: usize) -> Result<usize> {
    match std::env::var(name) {
        Ok(raw) => raw
            .parse::<usize>()
            .with_context(|| format!("parse {name}={raw} as usize")),
        Err(_) => Ok(default),
    }
}

fn env_bool(name: &str, default: bool) -> Result<bool> {
    match std::env::var(name) {
        Ok(raw) => match raw.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => Ok(true),
            "0" | "false" | "no" | "n" | "off" => Ok(false),
            _ => bail!("parse {name}={raw} as bool"),
        },
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bench_kind_aliases() {
        assert_eq!(BenchKind::parse("ner").unwrap(), BenchKind::NerOnly);
        assert_eq!(
            BenchKind::parse("full-ingest").unwrap(),
            BenchKind::FullIngest
        );
    }

    #[test]
    fn parses_provider_aliases() {
        assert_eq!(Provider::parse("cpu").unwrap(), Provider::Cpu);
        assert_eq!(Provider::parse("core-ml").unwrap(), Provider::CoreMl);
    }

    #[test]
    fn rate_handles_zero_elapsed() {
        assert_eq!(rate(10, 0.0), 0.0);
    }
}
