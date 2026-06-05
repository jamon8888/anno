//! Knowledge sync orchestration: discovery -> extract -> pseudonymize -> store.

use anno_knowledge_core::{ChunkId, ObjectId, PartId, RevisionId, SourceKind, SourceKindForId};
use anno_knowledge_store::{
    hex32, CommitChunk, CommitObjectInput, KnowledgeControlStore, ScopeRow, SourceRow,
};
use anno_rag::config::AnnoRagConfig;
use anno_rag::knowledge_privacy::{ExtractedChunkInput, PrivacyIndexInput};
use anno_rag::pipeline::Pipeline;
use anno_source_local::{DiscoverBudget, LocalFolderSource};
use serde::Serialize;

/// Per-run result summary returned by `knowledge_sync`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncSummary {
    /// Files discovered this run (after budget).
    pub seen: u64,
    /// Skipped because already `fts_ready` at the current content hash.
    pub skipped_unchanged: u64,
    /// Files extracted by Kreuzberg.
    pub extracted: u64,
    /// Objects pseudonymized.
    pub pseudonymized: u64,
    /// Objects written to FTS.
    pub fts_ready: u64,
    /// Objects removed because the file disappeared.
    pub forgotten: u64,
    /// Objects that failed this run.
    pub failed: u64,
    /// True when the budget truncated the walk (deletion reconciliation skipped).
    pub truncated: bool,
}

/// Budget knobs for a knowledge sync run.
#[derive(Debug, Clone, Copy)]
pub struct SyncOptions {
    /// Maximum files to discover.
    pub max_files: usize,
    /// Optional wall-clock budget in milliseconds.
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

/// Sync one local-folder scope end to end.
///
/// # Errors
/// Returns a string error only for setup failures (store/scope access). Per-file
/// failures are counted in the summary, not returned as errors.
pub async fn sync_local_scope(
    store: &KnowledgeControlStore,
    pipeline: &Pipeline,
    cfg: &AnnoRagConfig,
    source: &SourceRow,
    scope: &ScopeRow,
    options: SyncOptions,
) -> Result<SyncSummary, String> {
    let mut summary = SyncSummary::default();
    let budget = DiscoverBudget {
        max_files: options.max_files,
        ..DiscoverBudget::default()
    };
    let started = std::time::Instant::now();
    let src = LocalFolderSource::new(&scope.provider_key);
    let discovered = src
        .discover(&budget)
        .map_err(|e| format!("discover: {e}"))?;
    summary.seen = discovered.len() as u64;
    summary.truncated = discovered.len() >= budget.max_files;

    for obj in &discovered {
        let object_id = ObjectId::from_external(
            SourceKindForId::LocalFolder,
            "local",
            &scope.provider_key,
            &obj.external_id,
        );
        let provider_version = hex32(&obj.content_hash);

        match store.revision_is_fts_ready(&object_id, &provider_version) {
            Ok(true) => {
                summary.skipped_unchanged += 1;
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "revision check failed");
                summary.failed += 1;
                continue;
            }
        }

        if options
            .max_millis
            .is_some_and(|limit| started.elapsed().as_millis() as u64 >= limit)
        {
            summary.truncated = true;
            break;
        }

        // Extract via Kreuzberg.
        let extracted = match anno_rag::ingest::extract(&obj.path, cfg).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "extraction failed");
                summary.failed += 1;
                continue;
            }
        };
        summary.extracted += 1;

        let revision_id = RevisionId::from_parts(&object_id.as_string(), &provider_version);
        let part_id = PartId::from_parts(&object_id.as_string(), "file_body");

        let privacy_input = PrivacyIndexInput {
            object_id: object_id.as_string(),
            revision_id: revision_id.as_string(),
            part_id: part_id.as_string(),
            title_raw: obj.title_raw.clone(),
            metadata_raw: obj.metadata_raw.clone(),
            chunks: extracted
                .chunks
                .iter()
                .map(|c| ExtractedChunkInput {
                    idx: c.idx,
                    text: c.text.clone(),
                    char_start: c.char_start,
                    char_end: c.char_end,
                })
                .collect(),
        };

        let pseudo = match pipeline.pseudonymize_knowledge_object(privacy_input).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "pseudonymization failed");
                summary.failed += 1;
                continue;
            }
        };
        summary.pseudonymized += 1;

        let chunks: Vec<CommitChunk> = pseudo
            .iter()
            .map(|p| CommitChunk {
                chunk_id: ChunkId::from_parts(revision_id, part_id, p.chunk_idx),
                chunk_idx: p.chunk_idx,
                title_pseudo: p.title_pseudo.clone(),
                text_pseudo: p.text_pseudo.clone(),
                metadata_pseudo_json: p.metadata_pseudo_json.clone(),
                char_start: p.char_start,
                char_end: p.char_end,
            })
            .collect();

        let commit = CommitObjectInput {
            object_id,
            source_id: source.source_id,
            account_id: scope.account_id,
            scope_id: scope.scope_id,
            revision_id,
            part_id,
            external_id: obj.external_id.clone(),
            object_type: obj.object_type,
            provider_version,
            title_pseudo: pseudo.first().and_then(|p| p.title_pseudo.clone()),
            metadata_pseudo_json: pseudo
                .first()
                .map(|p| p.metadata_pseudo_json.clone())
                .unwrap_or_else(|| "{}".into()),
            source_kind: SourceKind::LocalFolder,
            chunks,
        };

        match store.commit_object(&commit) {
            Ok(()) => summary.fts_ready += 1,
            Err(e) => {
                tracing::warn!(error = %e, "commit failed");
                summary.failed += 1;
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_summary_starts_zeroed() {
        let s = SyncSummary::default();
        assert_eq!(s.seen, 0);
        assert_eq!(s.fts_ready, 0);
        assert_eq!(s.failed, 0);
        assert!(!s.truncated);
    }

    #[test]
    fn sync_options_default_matches_discovery_budget() {
        let options = SyncOptions::default();
        let budget = DiscoverBudget::default();
        assert_eq!(options.max_files, budget.max_files);
        assert_eq!(options.max_millis, None);
    }
}
