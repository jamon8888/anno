# Anno Tabular Local Legal Extraction Quality Design

**Date:** 2026-05-27
**Status:** Draft validated against current codebase
**Scope:** `anno-rag-tabular` local-first extraction quality using GLiNER2/Fastino, with optional LLM fallback
**Related:** `docs/superpowers/plans/2026-05-27-anno-tabular-local-legal-extraction-quality.md`

## Summary

Anno Tabular already provides the right extraction contract for legal review: typed cells, required citations, offset verification, and optional semantic support scoring. The next quality step is not to replace the LLM with GLiNER2 wholesale. The safe design is to use GLiNER2 as a local candidate generator for evidence-backed fields, then let Anno Tabular verify, abstain, or route difficult legal interpretation fields to an LLM or a human.

The target behavior is local-first and conservative:

- Fill simple extractive cells locally when a value and citation are strong.
- Locate legal clauses locally when the requested output is verbatim.
- Abstain rather than fabricate for absence, negation, weak support, or legal reasoning.
- Route only truly complex columns to an optional LLM.
- Keep raw document text local by default.

## Codebase Validation

The design is grounded in the current repository state:

- `crates/anno-rag-tabular/src/llm/mod.rs` defines `LlmClient`, a single `generate_structured(system, user, json_schema)` abstraction. This is the seam that can host a local tabular client.
- `crates/anno-rag-tabular/src/extract/mod.rs` owns `Extractor { llm: Arc<dyn LlmClient>, chunks: Arc<dyn ChunkSource> }`, so extraction is already backend-swappable.
- `crates/anno-rag-tabular/src/extract/batch.rs` builds one prompt with `[CHUNK::<uuid>]...[/CHUNK]` and `[COLUMN::<name>]prompt[/COLUMN]`, then calls `llm.generate_structured()`.
- `crates/anno-rag-tabular/src/schema/json_schema.rs` requires each emitted cell envelope to contain `value`, `reasoning`, and `citations`, with at least one citation.
- `crates/anno-rag-tabular/src/verify/offsets.rs` verifies byte-accurate citation round-trip against chunk text.
- `crates/anno-rag-tabular/src/verify/support.rs` defines semantic support scoring, but production cross-encoder wiring is explicitly not complete yet.
- `crates/anno/src/backends/gliner2_fastino/mod.rs` exposes public GLiNER2/Fastino APIs for `extract_with_label_descriptions`, `extract_structure`, and `classify`.
- `crates/anno-rag/src/detect.rs` already demonstrates the required char-offset to byte-offset conversion for GLiNER outputs on French text.
- `crates/anno-rag-mcp/Cargo.toml` still excludes `anno-rag-tabular`; MCP tabular tools are documented as a later phase, not present today.

## Problem

Legal extraction quality is not the same as generic entity extraction quality. A legal table mixes several task classes under the same `CellType`:

- `Text` can mean a party name, a SIREN, a short clause summary, or a legal right.
- `Verbatim` can mean a short quoted value or a whole clause.
- `Boolean` often requires proving both presence and absence across the document.
- `Enum` can be simple when labels are explicit, but hard when choices encode legal interpretation.

Because of this, routing by `CellType` alone is unsafe. The current templates confirm the issue. For example, `real-estate-v1` uses `text` for simple fields like `landlord`, `tenant`, and `premises_address`, but also for `tenant_break_rights` and `assignment_sublet`, which are clause/legal-right fields. `customer-contract-v1` uses booleans for auto-renewal, change of control, exclusivity, and MFN, all of which are higher-risk than a simple entity span.

## Goals

1. Add an explicit extraction-quality layer to Anno Tabular columns.
2. Enable GLiNER2 local extraction for fields that are genuinely extractive.
3. Preserve the existing citation-first contract.
4. Prefer abstention over low-confidence legal output.
5. Reduce LLM token usage by routing only complex columns to the LLM.
6. Keep the first implementation testable without requiring live GLiNER model downloads.

## Non-Goals

- Do not implement HTTP MCP transport in this design.
- Do not build the Svelte/AG Grid tabular UI here.
- Do not attempt full offline legal reasoning.
- Do not require a live Anthropic key for local-only extraction.
- Do not send raw document chunks to Anthropic in local-only mode.

## Extraction Modes

Columns need explicit extraction modes. `CellType` remains the value shape; `ExtractionMode` describes the safe extraction strategy.

```rust
pub enum ExtractionMode {
    Auto,
    LocalSpan,
    LocalClause,
    LocalClassifier,
    LlmRequired,
    Manual,
}
```

### `Auto`

Backward-compatible default for existing columns and old stored reviews. `Auto` behaves like today's extractor unless a caller explicitly chooses local-first routing.

### `LocalSpan`

Use for short values that should appear as a contiguous or near-contiguous text span:

- party names
- legal forms
- addresses
- dates
- amounts
- SIREN / registration numbers
- employee names
- job title
- jurisdiction names

Quality expectation: high precision when a candidate passes normalization and citation verification.

### `LocalClause`

Use for verbatim clauses or clause windows:

- jurisdiction clause
- confidentiality definition
- permitted use
- rent escalation
- termination clause
- assignment/subletting clause

Quality expectation: acceptable only when the cited span contains the clause text or a coherent clause window. GLiNER2 alone is insufficient; the extractor must combine GLiNER candidates with headings, keywords, and window expansion.

### `LocalClassifier`

Use for positive detection when evidence is explicit:

- lease type
- contract type
- governing law enum
- asset status
- explicit presence of a named clause

Quality rule: a positive label requires a supporting quote. A negative answer must usually abstain unless a template provides an explicit negative phrase pattern. Absence is not evidence.

### `LlmRequired`

Use when the field requires interpretation, synthesis, or legal judgment:

- repair obligations summary
- termination-for-cause conditions
- assignment restrictions requiring interpretation
- right of renewal status
- whether a clause has a legal effect despite different drafting
- risk review

Quality expectation: local extraction may provide candidate clauses, but final cell value needs LLM or human review.

### `Manual`

Use when the field should be user-filled only or where automation risk exceeds value.

## Column Metadata

Templates should support optional extraction metadata. The first useful shape:

```toml
[[column]]
name = "landlord"
prompt = "Landlord (bailleur) — full legal name and form."
type = "text"

[column.extraction]
mode = "local_span"
normalizer = "legal_name"
threshold = 0.45
labels = [
  { name = "bailleur", description = "Nom complet et forme juridique du bailleur" },
  { name = "lessor", description = "Full legal name and legal form of the landlord" },
]
keywords = ["bailleur", "entre les soussignes"]
```

For clause fields:

```toml
[column.extraction]
mode = "local_clause"
normalizer = "verbatim_clause"
threshold = 0.35
labels = [
  { name = "clause_destination", description = "Clause de destination des locaux loues" },
]
keywords = ["destination des lieux", "usage des locaux", "activite autorisee"]
window_before_chars = 250
window_after_chars = 1500
```

For classifier fields:

```toml
[column.extraction]
mode = "local_classifier"
normalizer = "enum"
threshold = 0.60
keywords = ["bail commercial", "bail derogatoire", "bail professionnel"]
```

## Quality Model

The system should score and decide in this order:

1. Candidate generation: GLiNER2, regex/rules, keyword locator, heading locator.
2. Normalization: date, currency, number, enum, legal name, country code.
3. Citation construction: exact byte offsets and quoted text.
4. Offset verification: existing deterministic verifier.
5. Semantic support: existing `SupportScorer` when a production scorer is wired.
6. Abstention: omit the column if evidence is weak, ambiguous, absent, or non-verifiable.

The default confidence policy:

- `High`: exact citation passes, normalization passes, support score >= 0.7 when available, and extraction mode is local-safe.
- `Medium`: exact citation passes, normalization passes, support scorer unavailable or score in 0.4..0.7.
- `Low`: verifier downgraded, semantic support is weak, or the candidate requires user review.
- No cell: abstained or routed elsewhere.

## Abstention Semantics

Current `extract_batch()` treats a missing column in an otherwise valid response as non-error. This should remain the primary abstention mechanism.

The local extractor must not emit fake citations to support `false`, `none`, or `null`. If the template wants a negative value, it needs either:

- an explicit negative quote in the source, or
- a future schema that represents `unknown`, `not_found`, and `not_applicable` separately from typed cell values.

Until then, omission is safer than emitting unsupported negatives.

## GLiNER2 Role

GLiNER2/Fastino should be used for:

- semantic labels with descriptions,
- multilingual legal entity spans,
- short value extraction,
- clause anchor detection,
- single-label classification as a candidate signal.

It should not be treated as:

- a legal reasoning engine,
- a substitute for clause interpretation,
- proof of absence,
- a verifier of the final answer.

## Use the Full Anno GLiNER2 Entity Stack

The local tabular extractor should not be a thin one-off call to `extract_with_label_descriptions`. The codebase already has a richer GLiNER2/legal extraction stack and the design should use it.

### Available GLiNER2/Fastino capabilities

`anno::backends::gliner2_fastino::GLiNER2Fastino` provides:

- `extract_with_label_descriptions` for per-label descriptions.
- `extract_with_label_thresholds` for different thresholds per legal label.
- `extract_structure` for structured extraction tasks where a table column maps to multiple related fields.
- `classify` for single-label enum/classification candidates.
- `batch_extract_with_schema_mode` and `batch_extract_streaming` for chunked document workloads.
- `ExecutionMode::IoBinding`, which keeps tensors device-resident and is documented as 1.5-3x faster on CPU and required for efficient GPU inference.

### Available anno-rag legal layer

`anno-rag` already defines:

- `default_legal_labels()` with labels such as `contract_party`, `court`, `jurisdiction`, `legal_reference`, `effective_date`, `deadline`, `amount`, `clause_type`, `obligation`, `risk_indicator`, `company_identifier`, `lawyer`, `judge`, and `regulator`.
- `default_thresholds()` with tuned per-label confidence thresholds.
- `LegalEnricher`, which combines Layer-1 GLiNER legal entities, raw-to-pseudonymized offset translation, and Layer-2 deterministic legal rules.
- deterministic rules for party roles, obligations, code references, court routing, and procedural events.
- normalized legal chunk enrichment fields that are directly useful for tabular extraction and routing.

### Required design implication

The local tabular extractor should become an adapter over this stack, not a parallel weaker extractor. It should:

1. Reuse `default_legal_labels()` and `default_thresholds()` where templates do not provide more specific labels.
2. Use `extract_with_label_thresholds` when the column maps to the legal label catalog.
3. Use `extract_with_label_descriptions` when the template provides richer column-specific descriptions.
4. Use `extract_structure` for grouped values such as party/name/form/SIREN, obligation/deadline/amount, and clause/type/effect candidates.
5. Use `classify` only as a candidate signal for enums and clause presence, never as final proof without citation.
6. Use existing raw-to-pseudonymized offset translation patterns when extracting on raw text but emitting pseudonymized citations.
7. Use `LegalEnricher` outputs and deterministic `TypedFact`s as high-precision signals before falling back to model-only candidates.
8. Allow `ExecutionMode::IoBinding` to be selected by configuration for production local extraction.

This is the difference between "GLiNER as span finder" and "Anno's GLiNER2 legal entity extraction as a local legal signal engine."

## Routing Model

The routing client should partition columns by `ExtractionMode`:

- `LocalSpan`, `LocalClause`, `LocalClassifier`: local extraction first.
- `LlmRequired`: optional LLM if configured, otherwise omitted/manual.
- `Manual`: never extracted.
- `Auto`: legacy behavior unless a local-first policy maps it to a mode.

When optional LLM fallback is enabled, the LLM should receive only the columns that local extraction cannot safely answer, not the full table. This reduces token usage and limits unnecessary exposure.

## PII and Confidentiality Boundary

LLM routing must be privacy-gated. The routing layer is allowed to call an external LLM only with pseudonymized chunk text and only for fallback columns. It must never rehydrate citations, vault values, raw source text, file paths containing client names, or clear PII before the fallback call.

The current tabular `ChunkRef` contract already states that `content` is pseudonymized chunk text and is what the LLM sees. That contract is necessary but not sufficient for safe routing. The routing implementation must also enforce:

1. **No raw source adapter:** production `ChunkSource` adapters used with fallback must source `SearchHit.text_pseudo`, not raw extracted text.
2. **No full-schema fallback:** fallback receives only `llm_required` or permitted `auto` columns, never local-only or manual columns.
3. **No unnecessary chunk exposure:** fallback should receive only pseudonymized chunks relevant to fallback columns. The first version may conservatively send the same pseudonymized document chunks as today's Anthropic extractor, but it must not send raw chunks and should be upgraded to top-k/predicate-selected chunks.
4. **Preflight PII guard:** before calling fallback, scan the constructed fallback prompt with the local detector/regex layer or a conservative pattern guard. If likely clear PII remains, abort fallback and return local-only/abstained results.
5. **No rehydration in routing:** rehydration remains an explicit citation-scoped user action after extraction, not part of LLM routing.

This means the correct guarantee is:

- **Raw PII:** must not be exposed to the fallback LLM.
- **Pseudonymized legal content:** may be exposed only when fallback is explicitly enabled and only for routed columns.
- **Residual PII risk:** handled by preflight scan and abort, because pseudonymization is strong but not mathematically perfect.

## Template Impact

Estimated local suitability from current built-in templates:

| Template | Local-friendly areas | Risk areas |
|---|---|---|
| `ip-v1` | asset name, type, owner, registration number, jurisdictions, dates, status | assignment chain, encumbrances, license rights, infringement claims |
| `employment-v1` | employee, role, date, contract type, salary, notice period | non-compete, IP assignment, change-of-control protection |
| `real-estate-v1` | landlord, tenant, premises, lease type, date, term, rent, deposit | break rights, assignment/sublet, repair obligations, registration |
| `nda-v1` | parties, effective date, governing law, liability cap | confidential info scope, permitted disclosures, residual clause, remedies |
| `customer-contract-v1` | parties, date, governing law, liability cap | renewal, termination, change of control, exclusivity, MFN, indemnity |

## Integration Boundaries

### `anno-rag-tabular`

Owns extraction metadata, local extraction, routing, tests, and template annotations.

### `anno`

Provides GLiNER2/Fastino public APIs. The tabular crate may depend on `anno` directly or route through an `anno-rag` adapter if model reuse becomes a priority.

### `anno-rag`

Owns stored chunks. It should expose a narrow chunk-source adapter or public methods for tabular extraction rather than exposing `Pipeline.store`.

It also owns the legal label catalog, thresholds, offset translation, `LegalEnricher`, and deterministic legal rule layer. `anno-rag-tabular` should reuse these through a narrow adapter rather than duplicating weaker label/threshold logic.

### `anno-rag-mcp`

Not part of this core quality change. MCP tabular tools can wrap the tabular API later.

## Acceptance Criteria

1. Existing Anthropic-based tabular extraction remains backward compatible.
2. Existing templates load without extraction metadata.
3. New metadata-rich templates load and preserve extraction metadata.
4. Local extraction can emit valid cell envelopes with exact byte-offset citations.
5. Local extraction abstains instead of emitting unsupported `false`, `none`, or `null`.
6. Tests cover UTF-8 offset conversion with accented French text and euro signs.
7. Tests cover local-safe fields and legal-reasoning fields separately.
8. Fallback routing never receives raw source text in tests; only pseudonymized `ChunkRef.content`.
9. Fallback routing filters schema/columns before calling the LLM.
10. Fallback routing has a preflight PII guard that aborts on obvious clear PII.
11. No live model download is required for default unit tests.

## Open Decisions

1. Whether extraction metadata should be persisted in `tabular_columns` immediately or remain template-only for the first local extractor pass.
2. Whether `LocalTabularClient` should implement `LlmClient` directly or whether `Extractor` should grow a more explicit `ExtractionBackend` abstraction.
3. Whether `false`, `unknown`, and `not_applicable` deserve first-class schema support before local boolean extraction is enabled broadly.

## Recommendation

Implement the smallest durable path:

1. Add optional extraction metadata to `Column` and templates.
2. Add JSON-schema vendor extensions so local/routing clients can inspect mode and prompt without reparsing private Rust structs.
3. Implement a local tabular client behind `LlmClient` for compatibility with the current `Extractor`.
4. Keep local output conservative and citation-backed.
5. Add routing and fallback only after local cell quality is measurable.
