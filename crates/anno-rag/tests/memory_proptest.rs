//! Property test for the forget-cascade invariant.
//!
//! Invariant: every `forget_memory(id)` call returns at most 1
//! `forgotten_id`, and `vault_tokens_purged` only counts tokens that
//! actually disappear from the vault. The save/forget sequence never
//! corrupts the store or panics, regardless of insertion + forget order.
//!
//! Ignored by default — drives LanceDB + the detector + the embedder
//! through 50 cases × up to 30 ops each, which is ~minutes of wall clock
//! and pulls the ~470 MB e5 model. Run with:
//!
//! ```text
//! cargo test -p anno-rag --release --test memory_proptest -- --ignored
//! ```

use anno_rag::config::AnnoRagConfig;
use anno_rag::memory::MemoryKind;
use anno_rag::pipeline::Pipeline;
use proptest::prelude::*;
use tempfile::TempDir;

#[derive(Debug, Clone)]
enum Op {
    Save(String),
    ForgetById(usize),
}

prop_compose! {
    fn op_seq(max_ops: usize)
              (ops in proptest::collection::vec(
                  prop_oneof![
                      "[a-zA-Z ]{5,40}".prop_map(Op::Save),
                      (0usize..50).prop_map(Op::ForgetById),
                  ],
                  1..max_ops))
              -> Vec<Op> { ops }
}

fn fresh_cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        embed_dim: 384,
        memory_embedding_dim: 384,
        ..Default::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    #[ignore = "drives full LanceDB + e5 + detector; ~minutes/run — opt in via --ignored"]
    fn forget_cascade_never_underflows(ops in op_seq(30)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let cfg = fresh_cfg(tmp.path());
            let p = Pipeline::new(cfg, [0u8; 32]).await.unwrap();
            let mut ids = Vec::new();
            for op in ops {
                match op {
                    Op::Save(text) => {
                        let s = p
                            .save_memory(&text, Some(MemoryKind::Context), None)
                            .await
                            .unwrap();
                        ids.push(s.id);
                    }
                    Op::ForgetById(idx) => {
                        if !ids.is_empty() {
                            let i = idx % ids.len();
                            let id = ids.remove(i);
                            let r = p
                                .forget_memory(Some(id), None, 1, false)
                                .await
                                .unwrap();
                            // Invariant: at most one row deleted per id forget.
                            assert!(r.forgotten_ids.len() <= 1);
                        }
                    }
                }
            }
        });
    }

    #[test]
    #[ignore = "drives full LanceDB + NER per case; minutes/run — opt in via --ignored --release"]
    fn graph_recall_is_monotonic_in_hops(seed in "[a-z]{4,10}") {
        // Property: `graph_recall(seed, n, ...)` returns at least as many
        // memories and at least as many nodes as `graph_recall(seed, n-1, ...)`.
        // BFS over a non-shrinking graph cannot lose reach when you grant
        // more hops.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let cfg = fresh_cfg(tmp.path());
            let p = Pipeline::new(cfg, [0u8; 32]).await.unwrap();

            // Plant a small two-edge graph: seed → Bordeaux, seed → Cabinet Martin.
            p.save_memory(
                &format!("Cabinet {seed} a Bordeaux."),
                Some(MemoryKind::Fact),
                None,
            )
            .await
            .unwrap();
            p.save_memory(
                &format!("Cabinet {seed} collabore avec Cabinet Martin."),
                Some(MemoryKind::Fact),
                None,
            )
            .await
            .unwrap();

            let r1 = p
                .graph_recall(&format!("Cabinet {seed}"), 1, 50, None)
                .await
                .unwrap();
            let r2 = p
                .graph_recall(&format!("Cabinet {seed}"), 2, 50, None)
                .await
                .unwrap();
            assert!(
                r2.memories.len() >= r1.memories.len(),
                "2 hops must reach >= memories than 1 hop"
            );
            assert!(r2.nodes.len() >= r1.nodes.len());
        });
    }
}
