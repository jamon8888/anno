# Design: Decompose `anno-eval` `loader.rs` into a `loader/` module tree

**Date:** 2026-06-15
**Status:** Approved (brainstorming) — pending implementation plan
**Scope:** `crates/anno-eval/src/eval/loader.rs` only

## Context

`crates/anno-eval/src/eval/loader.rs` is 9,865 lines — the largest file in the
workspace and the single worst maintainability hotspot. Its bulk is one
`impl DatasetLoader` block spanning lines 1380–6954 (5,574 lines, 85 methods)
plus ~2,860 lines of inline tests.

`anno-eval` is the evaluation harness and is **not published** to crates.io.
The public surface of `DatasetLoader` (its `pub fn` entry points) does **not**
change in this refactor, so external blast radius is effectively zero; the risk
is purely intra-crate.

This decomposition is also a deliberate rehearsal for the larger `anno` crate
carve-out (separating `pii`/`rag`/`discourse` into their own crates), which is
the next architectural step but is deferred until in-flight PRs land and a 0.11
version is cut. The pure-core / thin-orchestration seam established here is the
same seam that move will need.

## The three jobs tangled in one impl

The 5,574-line `impl DatasetLoader` mixes three distinct responsibilities:

1. **Orchestration / cache / config** (~15 methods): `new`, `with_cache_dir`,
   `load`, `load_or_download`, `load_coref`, `load_relation`, `cache_path`,
   `is_cached`, manifest handling, `status`, `load_all_cached`.
2. **Acquisition / IO** (~20 methods): S3 up/download, HF Hub resolution and
   download, `download_attempt(_bytes)`, `compute_sha256`, byte-cap guards.
   These are genuinely stateful (`self.config`, `self.cache_dir`, S3 settings)
   and feature-gated (`#[cfg]` on s3 / hf-hub).
3. **Parsers** (~43 methods): one `parse_*` per dataset/format, grouping into
   NER / coref / relation / classification / event families.

## Key finding driving the approach

A `self`-usage scan of every `parse_*` method shows **~40 of ~43 parsers never
reference `self`** — they take `&self` purely by convention. Only the
dispatcher (`parse_content_str`, `parse_content_impl`) and
`parse_hf_api_response` use `self`, and the last one only because it *calls*
sibling parsers (the `&self` falls away once those become free functions).

The dataset parsers are therefore pure functions in disguise.

## Chosen approach: B — Pure parser functions

Convert the self-free parsers into free functions grouped by task family; keep a
thin dispatcher; split the genuinely stateful acquisition layer into relocated
`impl DatasetLoader` blocks. **Hybrid by design**: pure functions where the code
is pure, split-impl where it is stateful.

Approaches considered and rejected:

- **A — Move, don't change** (keep all methods on `DatasetLoader`, split impl
  across files). Lowest churn but wastes the self-free property — parsers stay
  un-unit-testable and coupling is only cosmetically reduced.
- **C — Trait + registry** (`DatasetParser` trait + dispatch map). Over-engineered:
  parsers have heterogeneous return types (`LoadedDataset` vs
  `Vec<CorefDocument>` vs `Vec<RelationDocument>`) so one trait doesn't fit
  cleanly, and datasets are known at compile time so the registry indirection
  buys nothing.

## Target layout

`loader.rs` → a `loader/` directory. Line counts are estimates from the mapped
method ranges; no file exceeds ~1,500 lines.

```
eval/loader/
  mod.rs            ~400   DatasetLoader struct + Default; public entry points only:
                           new, with_cache_dir, load, load_or_download, load_coref,
                           load_or_download_coref, load_relation, load_or_download_relation,
                           load_all_cached, status, cache_path, is_cached,
                           cache_dir, s3_enabled/bucket
  types.rs          ~700   LoadableDatasetId (+Deref/From/FromStr/TryFrom), DatasetParsePlan,
                           CacheManifest(Entry), TemporalMetadata, AnnotatedToken/Sentence,
                           DatasetMetadata, DataSource, LoadedDataset, DatasetStats, RelationDocument
  cache.rs          ~250   manifest read/write (update_manifest, cached_manifest_entries),
                           cache_path_for, is_cached_for, compute_sha256, byte-cap enforcement
  acquire/
    mod.rs          ~300   download_with_resolved_url + acquisition orchestration
    hf_hub.rs       ~700   extract_hf_dataset_name, hf_rows_url, resolve_hf_config_split_prefer,
                           download_hf_dataset_file_from_hub (+ no-feature fallback),
                           download_hf_dataset_paginated, try_hf_hub_download
    s3.rs           ~400   download_from_s3, upload_to_s3, download_manifest_entry_from_s3,
                           upload_cached_dataset_to_s3
    http.rs         ~350   download_attempt, download_attempt_bytes,
                           max_download_bytes, enforce_max_download_bytes
  parse/
    mod.rs          ~250   DatasetId -> parser dispatch (was parse_content_impl);
                           is_hf_api_response; get_temporal_metadata
    util.rs         ~300   parse_bio_tag, map_entity_type, spans_from_array, overlaps,
                           extract_tag_names_from_features, extract_class_names_from_features
    ner.rs          ~1100  conll, conllu, jsonl_ner, tsv_ner, csv_ner, wikiann, tweetner7,
                           bc5cdr, ncbi_disease, cadec_hf_api, cadec_jsonl, hf_api_response
    coref.rs        ~700   litbank, litbank_coref, gap, preco_jsonl, ecb_plus
    relation.rs     ~700   docred, docred_relations, chisiec, chisiec_relations, google_re_corpus
    classification.rs ~600 trec, agnews, dbpedia14, yahoo_answers, afrisenti, afriqa,
                           masakhanews, tweettopic
    event.rs        ~600   maven, maven_arg, casie, rams
```

## Signature changes

- **Parsers** (self-free): `fn parse_conll(&self, content, id)` becomes
  `pub(crate) fn parse_conll(content: &str, id: DatasetId) -> Result<LoadedDataset>`.
  Coref/relation parsers keep their respective return types
  (`Vec<CorefDocument>`, `Vec<RelationDocument>`).
- **`parse_hf_api_response`**: re-point its internal calls to the new free
  functions; drop `&self` once it no longer needs it.
- **Dispatcher**: `parse_content_impl` becomes `parse::dispatch(content, id)` — a
  thin `match` on `DatasetId`. `parse_content_str` stays a `&self` public method
  that delegates to the dispatcher.
- **Acquisition + cache** (stateful, feature-gated): keep `&self`, relocate into
  `acquire/*.rs` and `cache.rs` as additional `impl DatasetLoader` blocks (Rust
  allows split inherent impls within one crate).

## Migration & verification (behaviour-preserving)

Incremental — the crate compiles and tests pass green after **every** step; no
big-bang rewrite:

1. `types.rs` — pure type moves, safest first.
2. `parse/util.rs` — shared helpers.
3. parse families one at a time, **moving each parser's existing `#[cfg(test)]`
   tests alongside it**.
4. `acquire/*` — cfg-gated, done last so feature combinations are isolated.
5. `cache.rs`, then `mod.rs` collapses to the orchestration shell.

The existing ~2,860 lines of tests are the **characterization net**: they move
with their parsers and must pass *unchanged except for import paths*. If any test
needs a real edit, that is a signal that behaviour changed — stop and
investigate.

Tooling per project conventions:

- Dev loop: `scripts/test-local.ps1 -Package anno-eval`.
- Before PR: `cargo fmt` (committed separately) + `cargo clippy --jobs 2 ... -D warnings`.
- Run GitNexus impact analysis on `DatasetLoader` / `loader.rs` before moving.

## Success criteria

- `loader.rs` removed; replaced by the `loader/` module tree.
- No file exceeds ~1,500 lines.
- All existing `anno-eval` tests pass, changed only in import paths.
- ~40 dataset parsers are callable and unit-testable as free functions.
- `cargo clippy -D warnings` and `cargo fmt --check` clean.
- `anno-eval` public API byte-identical (verify by diffing `pub` signatures).

## Explicit non-goals (YAGNI)

- Not touching `dataset_registry.rs` (separate data-as-code refactor; already
  partly underway in `dataset_registry_src/`).
- Not changing any public `DatasetLoader` signature, parsing behaviour, or
  dataset coverage.
- Not the trait/registry approach (rejected C).
- Not the other refactor candidates from the architecture review (crate
  carve-out, error-struct fields, unwrap/SAFETY hardening, `grounded/mod.rs`).
