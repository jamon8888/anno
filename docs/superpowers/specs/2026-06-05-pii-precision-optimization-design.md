# PII Precision Optimization — Hybrid Defense + Calibration Design

**Date** : 2026-06-05
**Status** : Draft, awaiting user review
**Branch** : `feat/gdpr-full-coverage` (continuation)

## Executive summary

This spec defines a hybrid pipeline that maximises PII detection precision and
recall for full GDPR coverage (Articles 4, 9, 10) without changing the
underlying GLiNER2 model. It combines two complementary strategies :

- **Defense in depth** on deterministic layers (regex, French heuristics,
  validators). Each layer covers a specific failure mode of GLiNER2.
- **Calibration-driven optimisation** on ML layers (GLiNER2 multi-task,
  discourse centering, entropy filter). Each addition is measured against
  the existing annotated French corpus.

The design exploits three existing but unused anno capabilities :

- `pii_eval.rs` with 35 annotated French documents
- `CalibrationEvaluator` + `EntropyFilter` in `anno-eval/calibration.rs`
- `discourse::centering` module for language-agnostic anaphora resolution

It also exploits two recent GLiNER2 advances :

- Multi-task schema composition (one forward pass for NER + classification +
  structured extraction) — paper [arXiv 2507.18546](https://arxiv.org/html/2507.18546v1)
- 2048-token context window (vs 512 previously)

## Goals

1. Cover the 23 GDPR labels (Art. 4 basic + identifiers, Art. 9 special
   categories, Art. 10 criminal records) at F1 ≥ target per category
2. Reduce false-negative rate on Art. 9/10 categories (recall ≥ 0.80)
3. Provide runtime quality signal without a benchmark loop (shadow scoring
   on annotated docs, calibration drift detection)
4. Keep latency ≤ 120 ms per 10k-char chunk
5. Make every layer rollbackable via feature flag

## Non-goals

- Fine-tuning GLiNER2 on a French domain corpus (deferred — would require a
  significantly larger annotated dataset)
- Coreference via a French neural model (deferred — centering rule-based
  approach is sufficient as a first step)
- Cross-document entity coalescing (the vault already handles token stability)

## Architecture

```
                            ┌──────────────────┐
                            │   Input chunk    │
                            └────────┬─────────┘
                                     │
            ┌────────────────────────┼────────────────────────┐
            ↓                        ↓                        ↓
┌───────────────────────┐  ┌────────────────────┐  ┌────────────────────────┐
│  DETERMINISTIC STACK  │  │     ML STACK       │  │  DISCOURSE LAYER       │
│                       │  │                    │  │                        │
│  L1: FR regex         │  │  GLiNER2 single-   │  │  Centering propagates  │
│  (NIR, SIRET, IBAN,   │  │  pass schema :     │  │  Person labels to FR   │
│   phone, email,       │  │  - NER 23 GDPR     │  │  pronouns (il, elle,   │
│   honorifics)         │  │  - doc_type class. │  │   ledit, l'intéressé)  │
│                       │  │  - structured ID   │  │                        │
│  L2: FR heuristics    │  │                    │  │  Virtual pronoun spans │
│  (org suffixes,       │  │                    │  │  inherit antecedent    │
│   addresses, dates,   │  │                    │  │  canonical_id          │
│   intl IBAN mod-97)   │  │                    │  │                        │
└──────────┬────────────┘  └─────────┬──────────┘  └──────────┬─────────────┘
           │                         │                        │
           └─────────────────────────┼────────────────────────┘
                                     ↓
                       ┌──────────────────────────┐
                       │   ENTROPY FILTER         │
                       │   per-span disagreement  │
                       │   > 0.4 → review queue   │
                       └────────────┬─────────────┘
                                    ↓
                       ┌──────────────────────────┐
                       │   VALIDATORS             │
                       │   Luhn, mod-97, NIR key, │
                       │   date range, IP parse,  │
                       │   postal code, IDs cross │
                       │   -check via structure   │
                       └────────────┬─────────────┘
                                    ↓
                       ┌──────────────────────────┐
                       │   AGGREGATOR             │
                       │   sort + dedup_overlaps  │
                       │   priority : Pattern >   │
                       │   Heuristic > Ner >      │
                       │   Centering              │
                       └────────────┬─────────────┘
                                    ↓
                       ┌──────────────────────────┐
                       │   SHADOW SCORER (async)  │
                       │   if doc ∈ annotated     │
                       │   → emit per-category F1 │
                       └────────────┬─────────────┘
                                    ↓
                       ┌──────────────────────────┐
                       │   Final entities         │
                       │   + audit telemetry      │
                       │   + review queue items   │
                       └──────────────────────────┘
```

## Components

### 1. Deterministic stack

#### Layer 1 — FR regex (existing, extended)

Existing patterns in `crates/anno-rag/src/detect.rs::FrPatterns` are kept.
Additions :

- `rib_fr` — banking code structure (5+5+11+2)
- `tva_intra_fr` — `FR<2 digits><9 digits SIREN>`
- `plate_fr` — French vehicle plates `AA-123-AA`
- `mrz_passport` — OCR-A pattern typical of passport machine-readable zones
- Foreign social security numbers (DE, IT, BE, ES variants)
- French judicial dossier numbers `RG n° XX/XXXXX`

#### Layer 2 — French heuristics backend (new)

New crate module `crates/anno/src/backends/heuristic_fr/`. Implements
the `Model` trait so it composes with `StackedNER`.

Rules :

- **ORG suffixes** — `SAS`, `SARL`, `EURL`, `SA`, `SASU`, `SNC`, `SCS`, `SCA`,
  `SCI`, `SCM`, `SCP`, `SCOP`, `SEM`, `EPIC`, `GIE`, `EIRL`,
  `Société Civile`, `Société Civile Immobilière`, `Auto-entrepreneur`,
  `Micro-entreprise`. Pattern : `<Capitalized words 1-5> <SUFFIX>[,.]`.
  Confidence 0.85.
- **Addresses** — voie keywords (`rue`, `avenue`, `boulevard`, `place`,
  `impasse`, `allée`, `chemin`, `route`, `quai`, `cours`, `passage`,
  `square`, `esplanade`, `promenade`, `voie`, `sentier`, `villa`). Pattern :
  `<number> <VOIE>[\s,] <Capitalized words 1-4>` with adjacent
  postal-code attachment when present.
- **French dates** — `MOIS_FR` lexicon + numeric formats. Categorised as
  `date_of_birth` when within ±50 chars of `né(e) le`, `date de naissance`,
  `anniversaire`.
- **International IBAN** — all ISO 3166-1 alpha-2 country codes with known
  IBAN length, mod-97 validated.

#### Layer 5 — Validators (new)

`crates/anno-rag/src/validators.rs`.

```rust
pub enum ValidationResult {
    Accept,
    Reject { reason: &'static str },
    AdjustConfidence(f32),
}

pub trait EntityValidator: Send + Sync {
    fn label(&self) -> &str;
    fn validate(&self, e: &DetectedEntity, ctx: &str) -> ValidationResult;
}
```

Implementations :

| Validator | Label(s) | Rule |
|---|---|---|
| `LuhnValidator` | `bank_account`, `card_number`, `SIRET` | Luhn checksum |
| `Iban97Validator` | `IBAN_FR`, `iban` | mod-97 on BBAN representation |
| `NirControlKeyValidator` | `NIR` | Control key `97 - (n mod 97)` |
| `DateRangeValidator` | `date_of_birth` | Year between 1900 and current |
| `IpAddressValidator` | `ip_address` | Parse via `std::net::IpAddr` |
| `EmailRfcValidator` | `email_address` | RFC 5321 light |
| `PostalCodeValidator` | `postal_code` | 01000-98999 (FR metropolitan + DOM) |
| `StructuredIdCrossCheck` | `national_id`, `tax_id` | Invokes `extract_structure` on the context to confirm the ID type |

Validators run after the aggregator. Rejections are emitted as
`RejectionEvent` to the audit telemetry (counts only, never the text).

### 2. ML stack

#### Single-pass multi-task schema (new API to build)

Currently every `extract_*` method calls `transformer.transform(text, &[task])`
with a single-element slice (verified at `crates/anno/src/backends/gliner2_fastino/mod.rs`
lines 331, 561, 619, 686, 744). The architecture supports multi-task
because `transform` accepts `&[SchemaTask]` and produces one
`ProcessedRecord` with combined tokens — the encoder runs once on the
concatenated prompt-and-text input — but no public method exposes this.

The design adds a new public method on `GLiNER2Fastino` that exercises
this path :

```rust
// New method to add in crates/anno/src/backends/gliner2_fastino/mod.rs
pub fn extract_composite(
    &self,
    text: &str,
    tasks: &[SchemaTask],
    label_thresholds: &HashMap<&str, f32>,
) -> Result<CompositeResult> {
    let record = self.transformer.transform(text, tasks)?;
    // For each TaskMapping in record.tasks, dispatch to the appropriate
    // pipeline (decode_entities, decode_classifications, decode_structure)
    // reusing the shared encoder output.
    // ...
}

pub struct CompositeResult {
    pub entities: Vec<Entity>,
    pub classifications: HashMap<String, (String, f32)>,
    pub structures: Vec<ExtractedStructure>,
}
```

Caller side in `detect_for_ingest` :

```rust
let tasks = vec![
    SchemaTask::EntitiesDescribed(GDPR_NER_LABELS_DESCRIBED.into()),
    SchemaTask::Classifications("doc_type".into(), vec![
        "contract".into(), "case_file".into(), "lease".into(),
        "employment".into(), "medical_record".into(),
        "tax_document".into(), "correspondence".into(),
    ]),
    SchemaTask::Structures("identifiers".into(), vec![
        ("siret".into(),          FieldType::String),
        ("national_id".into(),    FieldType::String),  // Choice not supported Phase 2
        ("iban_country".into(),   FieldType::String),
    ]),
];
let composite = ner.extract_composite(text, &tasks, &thresholds)?;
```

Note: `FieldType::Choice` is documented as Phase 2 not-shipped at
`crates/anno/src/backends/gliner2_fastino/mod.rs:62` — falls
back to single-best-span decoding. Use `String` for now and parse the
output text against an allowlist post-hoc.

Expected gains :

- Forward passes per chunk : 2-3 → **1** (one shared encoder run)
- Encoder reuse confirmed by the `ProcessedRecord` structure :
  combined `input_ids` / `attention_mask` for all task prompts
- Doc-type aware threshold adjustment via
  `DOC_TYPE_THRESHOLD_ADJUSTMENTS: HashMap<DocType, HashMap<Label, f32>>`
  applied as a post-decode filter

Implementation cost : moderate. Requires adapting `run_pipeline_dispatch`
to iterate over all `task_map` entries in `record.tasks`, dispatching
to the right scorer (`scorer` for Entities, `classifier` for
Classifications, `count_lstm_fixed`+`scorer` for Structures) on the same
encoder output. The Candle backend
(`crates/anno/src/backends/gliner2_fastino_candle/`)
must implement the same method for parity on Metal.

#### Discourse centering for French pronouns

The existing `crates/anno/src/discourse/centering.rs` provides
**low-level primitives only** : `track_centers(utterances, config) ->
Vec<CenteringState>` at `crates/anno/src/discourse/centering.rs:615`
takes a list of utterances (each itself a list of `ForwardCenter`) and
returns per-utterance backward-looking centers (`Cb`) and transition
types. **There is no built-in pronoun-resolution API.**

The design must build a pronoun-resolution layer on top of these
primitives. New module
`crates/anno-rag/src/discourse_pii.rs` :

```rust
// Module-level pronoun lexicon (new)
const PRONOUNS_FR: &[&str] = &[
    "il", "elle", "ils", "elles", "on",
    "lui", "leur", "le", "la", "les", "se",
    "son", "sa", "ses", "leurs",
    "celui-ci", "celle-ci", "ceux-ci", "celles-ci",
    "ce dernier", "cette dernière", "ces derniers",
    "ledit", "ladite", "lesdits", "lesdites",
    "l'intéressé", "l'intéressée", "le requérant", "la requérante",
    "le demandeur", "la demanderesse", "le défendeur", "la défenderesse",
    "le bailleur", "le preneur", "le mandant", "le mandataire",
];

pub fn resolve_pronouns(
    text: &str,
    entities: &[DetectedEntity],
) -> Result<Vec<VirtualPronounSpan>> {
    // 1. Segment text into utterances via DiscourseScope::analyze
    let scope = anno::discourse::types::DiscourseScope::analyze(text);
    // 2. For each utterance, build Vec<ForwardCenter> from entities that
    //    overlap with the utterance span (using existing entity offsets
    //    as the canonical mention anchors).
    let utterances: Vec<Vec<ForwardCenter>> = build_utterances(&scope, entities);
    // 3. Run track_centers to get the per-utterance Cb chain.
    let states = anno::discourse::centering::track_centers(
        &utterances,
        &CenteringConfig::default(),
    );
    // 4. For each utterance, scan for FR pronouns and project the Cb of
    //    the current state onto each pronoun match.
    let mut virtual_spans = vec![];
    for (i, sentence) in scope.sentences().iter().enumerate() {
        let state = &states[i];
        let Some(cb_entity_id) = state.cb else { continue };
        let antecedent = entities.iter().find(|e| e.id == cb_entity_id);
        let Some(ante) = antecedent else { continue };
        if ante.category != EntityCategory::Person { continue }
        for pronoun_hit in find_pronouns_in(sentence, PRONOUNS_FR) {
            virtual_spans.push(VirtualPronounSpan {
                start: pronoun_hit.start,
                end: pronoun_hit.end,
                canonical_id: ante.canonical_id.clone(),
                confidence: 0.75,
                source: DetectionSource::Centering,
            });
        }
    }
    Ok(virtual_spans)
}
```

Helpers that must be written :

- `build_utterances(scope, entities)` — maps `Entity` byte spans to
  `ForwardCenter { entity_id, surface, salience }` per utterance
- `find_pronouns_in(sentence, lexicon)` — case-insensitive matcher with
  word boundaries
- `DiscourseScope::sentences()` — **verify this method exists** on
  `DiscourseScope`. If only `.analyze()` is public, sentence
  segmentation must be implemented locally (regex on `[.!?]\s+[A-ZÀÂ...]`).

Resolved pronouns produce `VirtualPronounSpan` entries that inherit the
antecedent's `canonical_id`. The vault uses the same token for the pronoun
as for the root entity, preventing leakage through unmasked anaphora.

Source priority is set so centering loses to all other sources in
overlap-deduplication :

```rust
fn source_priority(s: &DetectionSource) -> u8 {
    match s {
        Pattern   => 0,
        Heuristic => 1,
        Ner       => 2,
        Centering => 3,
    }
}
```

#### Entropy filter

Per-span entropy is computed on overlapping detections from multiple
sources using `EntropyFilter::strict()` (threshold 0.4).

Policy by label class :

| Label class | Action on high entropy |
|---|---|
| Art. 9 / Art. 10 | `FlagForReview` (keep highest confidence + emit review event) |
| Other GDPR | `TakeHighest` (silent disambiguation) |

Configurable via `ANNO_ENTROPY_POLICY`.

### 3. Calibration and observability

#### Shadow scoring

Annotated corpus loaded once at startup, keyed by SHA-256 of document
content. After `detect_for_ingest` returns, if the doc hash matches an
annotated entry, `pii_eval::score_detections` runs asynchronously via a
`tokio::mpsc` channel and emits a `ShadowScoreEvent` to the
`anno_rag::detect::shadow_score` tracing target.

The event contains TP/FP/FN counts and precision/recall/F1 per category.
It never contains the original text or the detected values.

#### Calibration runtime

A global `CalibrationEvaluator` accumulates `(confidence, is_correct)`
observations for any document with ground truth. The
`calibration_report` MCP tool exposes the current reliability diagram.
A `CalibrationDriftAlert` is emitted when ECE > 0.1 on a label for N
consecutive documents.

#### Audit telemetry extension

The existing `emit_detect_audit` adds the following fields :

- `from_heuristic: usize`
- `from_centering: usize`
- `rejected_by_validators: BTreeMap<String, usize>` (per reason)
- `flagged_high_entropy: usize`
- `multi_task_doc_type: Option<(String, f32)>`
- `chunking_strategy: &'static str`

#### Review queue

Append-only JSONL file at `ANNO_REVIEW_QUEUE_PATH`
(default `.anno-review-queue.jsonl`).

Each entry contains :

- Timestamp, doc_id, kind (`HighEntropy` / `RejectedValidator` /
  `CalibrationDrift`)
- Label, span byte offsets, context hash (sha256)
- Sources, confidences, entropy

Never the actual text. Human resolutions are written to
`.anno-review-decisions.jsonl` and can be replayed to tune thresholds.

### 4. Chunking adjustment

Move from 10k-char chunks (200-char overlap) to ~24k-char chunks
(500-char overlap) to exploit the 2048-token GLiNER2 context window.

Implemented via `backends::chunking::ChunkConfig::long_document()` in
`anno-rag-rag/src/ingest.rs`.

### 5. Feature flag granularity

```bash
ANNO_GDPR_LAYERS=basic        # Existing pipeline only (rollback)
ANNO_GDPR_LAYERS=defense      # + L2 heuristics + validators
ANNO_GDPR_LAYERS=shadow       # + shadow scoring + entropy filter
ANNO_GDPR_LAYERS=full         # + centering + multi-task composition
ANNO_GDPR_LAYERS=full,review  # full + review queue active

# Defaults : "defense" in prod, "full" in dev
```

## Validation plan

### Corpus extension (35 → 150 docs)

| Tranche | Volume | Provenance | Coverage |
|---|---|---|---|
| Existing `pii_annotations.toml` | 35 | Repo | NIR, SIRET, IBAN, phone, email, person, org, location |
| Synthetic Art. 4 identifiers | 60 | New module `tests/fixtures/synthetic_gdpr.rs` | national_id, tax_id, bank_account, ip_address, username, device_id |
| Synthetic Art. 9 | 30 | Same | health, genetic, biometric, racial, political, religious, sexual, trade_union |
| Synthetic Art. 10 | 10 | Same | criminal records, judicial proceedings |
| Adversarial hand-crafted | 15 | Manual | OCR noise, homoglyphs, ambiguities (e.g. Marie = person vs common noun) |

Synthetic generator uses FR-locale faker pools for syntactically valid PII
values, template substitution with computed byte offsets, and 10–30 %
noise injection.

### Test levels

1. **Unit tests** — per layer, in each crate's `tests.rs` modules
2. **Integration tests** — `crates/anno-rag/tests/gdpr_pipeline.rs`,
   one test per feature flag level
3. **Snapshot tests** — `insta` crate, one snapshot per corpus document,
   format `(doc_id, label, byte_offset, source, confidence)`
4. **Property tests** — proptest, structural invariants (no panic on
   arbitrary UTF-8, valid char boundaries, pseudo-then-rehydrate identity)

### Target metrics

| Category | Current F1 | Target F1 |
|---|---|---|
| Art. 4 structured (NIR, SIRET, IBAN, phone, email) | ~0.95 | ≥ 0.98 |
| Art. 4 person/org/location | ~0.75 | ≥ 0.85 |
| Art. 4 identifiers (national_id, tax_id, bank_account) | n/a | ≥ 0.70 |
| Art. 9 health/biometric/genetic | n/a | ≥ 0.60 (recall ≥ 0.80) |
| Art. 9 political/religious/racial/sexual/trade_union | n/a | ≥ 0.55 (recall ≥ 0.80) |
| Art. 10 criminal | n/a | ≥ 0.65 |

### Performance targets

| Metric | Current | Target |
|---|---|---|
| Latency per chunk (10k chars) | ~80 ms | ≤ 120 ms |
| Forward passes per chunk | 2-3 | 1 |
| Resident memory | ~600 MB | ≤ 700 MB |
| Throughput (docs/sec, single thread) | ~12 | ≥ 8 |

Tolerance : up to +50 % latency acceptable if F1 gain on Art. 9
exceeds 10 points.

## Rollout phases

| Phase | Week | Scope |
|---|---|---|
| A | 1 | Heuristics FR backend + validators + extended audit, `defense` in dev |
| B | 2 | Corpus extension + shadow scoring + snapshot tests + benchmark baseline |
| C | 3 | Multi-task composition + entropy filter + centering FR, `full` in dev / `defense` in prod |
| D | 4 | Calibration runtime + review queue + MCP tools + `full` in prod |

Each phase is an independently mergeable PR with rollback via feature flag.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Multi-task composition introduces regressions | Phase C gated behind feature flag, shadow scoring detects drops |
| French pronoun lexicon causes false coreference | `Centering` source priority loses to all others, virtual spans flagged separately in audit |
| Synthetic corpus drift from real distribution | 15 adversarial hand-crafted docs + extension to real docs over time |
| Review queue floods on Art. 9 | Configurable entropy threshold per label class, default `FlagForReview` only on Art. 9/10 |
| Calibration runtime memory growth | Reservoir sampling (10k observations max per label) |
| Snapshot tests block PRs on noise | Snapshots reviewed in PR diff, `cargo insta review` workflow documented |
| `extract_composite` does not exist — must be written | Phase C deliverable. Includes Candle-backend parity at `crates/anno/src/backends/gliner2_fastino_candle/` so Apple Metal builds match. |
| `DiscourseScope::sentences()` may not exist | Verify in Phase C kick-off. Fallback : local sentence splitter using `regex` crate. |
| `FieldType::Choice` not shipped in Phase 2 | Use `FieldType::String` and parse the output against an allowlist post-hoc. Document the gap so a future Phase upgrade can drop the parser. |

## Code-reality audit (2026-06-05)

The following claims were verified against the actual codebase before
finalising this spec :

- `pii_annotations.toml` contains 35 entries (verified)
- `centering::track_centers` exists at line 615 (verified)
- `DiscourseScope::analyze` exists at types.rs:791 (verified)
- `EntropyFilter::strict()` and `confidence_entropy` exist (verified)
- `SchemaTransformer::transform` accepts `&[SchemaTask]` (verified —
  multi-task is supported at the transformer level)

Identified gaps that this spec acknowledges :

- No `extract_composite` method exists. Must be written.
- No `resolve_pronoun` method exists. The centering module exposes
  `track_centers` only; the pronoun-mapping layer must be built on top.
- `FieldType::Choice` is documented as not-shipped in Phase 2.
- `DiscourseScope::sentences()` not verified — may need a local
  sentence splitter.

## Out of scope

- Domain fine-tuning of GLiNER2 on a French legal/medical corpus
  (the LOGICAL paper's 0.98 F1 path — requires significantly larger
  annotated dataset)
- Replacing GLiNER2 with `gliner2-privacy-filter-PII-multi` (ONNX not
  available, would require model conversion)
- Cross-document entity coalescing (vault handles token stability already)
- Neural French coreference model integration

## References

- GLiNER2 paper — [arXiv 2507.18546](https://arxiv.org/html/2507.18546v1)
- GLiNER2-PII paper — [arXiv 2605.09973](https://arxiv.org/abs/2605.09973)
- LOGICAL paper (clinical PII fine-tuning) —
  [arXiv 2510.19346](https://arxiv.org/abs/2510.19346)
- Existing components :
  - `crates/anno-rag/src/pii_eval.rs` (35-doc FR scorer)
  - `crates/anno-eval/src/eval/calibration.rs` (EntropyFilter, ECE)
  - `crates/anno/src/discourse/centering.rs` (centering theory)
  - `crates/anno/src/backends/gliner2_fastino/` (schema-driven inference)
