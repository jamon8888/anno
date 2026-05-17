//! LoCoMo sanity harness — walks `tests/fixtures/locomo_subset/scenarios.toml`
//! and asserts every v0.1 + v0.2 memory feature on a deterministic planted
//! graph.
//!
//! **NOT a statistical accuracy benchmark.** Smoke coverage only — catches
//! regressions in `save_memory` / `recall_memory` / `graph_recall` /
//! `forget_memory` / `list_memories` / `invalidate_memory` plumbing.
//!
//! `#[ignore]`'d because each scenario spins a fresh Pipeline + LanceDB
//! (~30 s cold per scenario × 10 scenarios = ~5 min). Run with:
//!
//! ```text
//! cargo test -p anno-rag --release --test locomo_smoke -- --ignored --nocapture
//! ```

use anno_rag::config::AnnoRagConfig;
use anno_rag::memory::{MemoryId, MemoryKind};
use anno_rag::pipeline::Pipeline;
use chrono::{Duration, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use tempfile::TempDir;

// ----------------------------------------------------------------------------
// TOML shapes (mirror docs/.../locomo_subset/scenarios.toml).
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Fixture {
    scenario: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    memories: Vec<MemoryEntry>,
    #[serde(default)]
    queries: Vec<Op>,
}

#[derive(Debug, Deserialize)]
struct MemoryEntry {
    text: String,
    kind: String,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Op {
    // The `operation` field discriminates explicit ops; absence = default recall.
    Tagged(TaggedOp),
    Recall(RecallOp),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation")]
enum TaggedOp {
    #[serde(rename = "graph_recall")]
    GraphRecall {
        seed_entity: String,
        #[serde(default = "default_max_hops")]
        max_hops: u8,
        #[serde(default = "default_per_hop_limit")]
        per_hop_limit: usize,
        expected_entity_substring: String,
    },
    #[serde(rename = "forget_memory_by_index")]
    ForgetByIndex { forget_index: usize },
    #[serde(rename = "list_memories")]
    ListMemories {
        #[serde(default = "default_list_limit")]
        limit: usize,
        expected_page_sizes: Vec<usize>,
    },
}

#[derive(Debug, Deserialize)]
struct RecallOp {
    text: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    kinds: Option<Vec<String>>,
    #[serde(default)]
    as_of_offset_ms: Option<i64>,
    #[serde(default)]
    graph_expand: bool,
    #[serde(default)]
    expected_memory_indexes: Vec<usize>,
    #[serde(default)]
    expected_text_contains: Option<String>,
}

fn default_top_k() -> usize {
    3
}
fn default_max_hops() -> u8 {
    2
}
fn default_per_hop_limit() -> usize {
    50
}
fn default_list_limit() -> usize {
    20
}

fn parse_kind(s: &str) -> MemoryKind {
    match s {
        "fact" => MemoryKind::Fact,
        "preference" => MemoryKind::Preference,
        "reference" => MemoryKind::Reference,
        "context" => MemoryKind::Context,
        other => panic!("unknown kind in fixture: {other}"),
    }
}

fn fresh_cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        embed_dim: 384,
        memory_embedding_dim: 384,
        ..Default::default()
    }
}

// ----------------------------------------------------------------------------
// One-scenario driver.
// ----------------------------------------------------------------------------

async fn run_scenario(s: &Scenario) {
    println!("=== {} — {} ===", s.id, s.description);

    let tmp = TempDir::new().expect("tempdir");
    let p = Pipeline::new(fresh_cfg(tmp.path()), [0u8; 32])
        .await
        .expect("Pipeline::new");

    // Save every memory; record the assigned ids in order.
    let mut ids: Vec<MemoryId> = Vec::with_capacity(s.memories.len());
    let mut last_save_at = Utc::now();
    for m in &s.memories {
        let saved = p
            .save_memory(&m.text, Some(parse_kind(&m.kind)), m.session_id.clone())
            .await
            .unwrap_or_else(|e| panic!("{} save_memory: {e}", s.id));
        ids.push(saved.id);
        last_save_at = Utc::now();
        // Small spacing so v7 ids stay strictly monotonic across saves.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    for (qi, op) in s.queries.iter().enumerate() {
        match op {
            Op::Tagged(TaggedOp::GraphRecall {
                seed_entity,
                max_hops,
                per_hop_limit,
                expected_entity_substring,
            }) => {
                let r = p
                    .graph_recall(seed_entity, *max_hops, *per_hop_limit, None)
                    .await
                    .unwrap_or_else(|e| panic!("{} graph_recall: {e}", s.id));
                let needle = expected_entity_substring.to_lowercase();
                let hit = r
                    .nodes
                    .iter()
                    .any(|n| n.display.to_lowercase().contains(&needle));
                assert!(
                    hit,
                    "{}: graph_recall did not reach '{}'. nodes = {:?}",
                    s.id,
                    expected_entity_substring,
                    r.nodes
                        .iter()
                        .map(|n| n.display.clone())
                        .collect::<Vec<_>>()
                );
            }
            Op::Tagged(TaggedOp::ForgetByIndex { forget_index }) => {
                let id = ids
                    .get(*forget_index)
                    .unwrap_or_else(|| panic!("{}: forget_index out of range", s.id))
                    .clone();
                let r = p
                    .forget_memory(Some(id), None, 1, false)
                    .await
                    .unwrap_or_else(|e| panic!("{} forget_memory: {e}", s.id));
                assert_eq!(
                    r.forgotten_ids.len(),
                    1,
                    "{}: forget did not remove exactly one row",
                    s.id
                );
            }
            Op::Tagged(TaggedOp::ListMemories {
                limit,
                expected_page_sizes,
            }) => {
                let mut cursor: Option<String> = None;
                let mut got_sizes: Vec<usize> = Vec::new();
                loop {
                    let page = p
                        .list_memories(None, None, *limit, cursor.clone())
                        .await
                        .unwrap_or_else(|e| panic!("{} list_memories: {e}", s.id));
                    got_sizes.push(page.items.len());
                    match page.next_cursor {
                        Some(c) => cursor = Some(c),
                        None => break,
                    }
                    if got_sizes.len() > expected_page_sizes.len() + 1 {
                        panic!(
                            "{}: list_memories produced more pages than expected (got {:?})",
                            s.id, got_sizes
                        );
                    }
                }
                assert_eq!(
                    &got_sizes, expected_page_sizes,
                    "{}: list_memories page sizes",
                    s.id
                );
            }
            Op::Recall(q) => {
                let kinds = q
                    .kinds
                    .as_ref()
                    .map(|v| v.iter().map(|k| parse_kind(k)).collect::<Vec<_>>());
                let as_of = q
                    .as_of_offset_ms
                    .map(|ms| last_save_at + Duration::milliseconds(ms));
                let hits = p
                    .recall_memory(
                        &q.text,
                        q.top_k,
                        q.session_id.clone(),
                        kinds,
                        as_of,
                        q.graph_expand,
                    )
                    .await
                    .unwrap_or_else(|e| panic!("{} query #{} recall_memory: {e}", s.id, qi));

                // Assertion: every expected_memory_index must appear in the hits.
                let id_strs: HashMap<String, usize> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (id.as_string(), i))
                    .collect();
                let hit_indexes: Vec<usize> = hits
                    .iter()
                    .filter_map(|h| id_strs.get(&h.id).copied())
                    .collect();
                for want in &q.expected_memory_indexes {
                    assert!(
                        hit_indexes.contains(want),
                        "{} query #{}: expected memory index {} in hits, got {:?}",
                        s.id,
                        qi,
                        want,
                        hit_indexes
                    );
                }
                if let Some(needle) = &q.expected_text_contains {
                    let any = hits.iter().any(|h| h.text.contains(needle));
                    assert!(
                        any,
                        "{} query #{}: expected text containing '{}' in any hit; hits = {:?}",
                        s.id,
                        qi,
                        needle,
                        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Entry point.
// ----------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "drives full Pipeline + LanceDB per scenario; ~5 min total. Opt-in via --ignored --release."]
async fn locomo_smoke_runs_all_scenarios() {
    let toml_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/locomo_subset/scenarios.toml");
    let raw = std::fs::read_to_string(&toml_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", toml_path.display()));
    let fixture: Fixture =
        toml::from_str(&raw).unwrap_or_else(|e| panic!("parse scenarios.toml: {e}"));

    assert!(
        !fixture.scenario.is_empty(),
        "scenarios.toml has no [[scenario]] entries"
    );

    for s in &fixture.scenario {
        run_scenario(s).await;
    }

    println!("\n✓ {} scenarios passed", fixture.scenario.len());
}
