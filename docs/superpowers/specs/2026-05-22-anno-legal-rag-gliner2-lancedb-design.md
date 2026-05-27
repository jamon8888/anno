# anno legal RAG — GLiNER2 + LanceDB for French Contracts and Litigation Files

**Date:** 2026-05-22
**Status:** Draft
**Author:** Design session with Codex

## Motivation

Build a legal intelligence layer for anno's Claude Desktop plugin so French contracts and
litigation files can be ingested, pseudonymized, searched, extracted, and reviewed with
verifiable citations. The system should combine anno's local privacy model, LanceDB hybrid
retrieval, and GLiNER2's multilingual entity/structure extraction.

The primary user flow is not generic chat over documents. It is semi-automatic legal work:
the plugin proposes parties, obligations, dates, amounts, clauses, risks, timelines, and
supporting citations, while surfacing uncertain fields for human validation.

## Product Scope

Initial gold paths:

1. French contracts: commercial agreements, service contracts, NDAs, leases, amendments,
   termination letters, and related exhibits.
2. French litigation files: pleadings, formal notices, correspondence, evidence bundles,
   procedural documents, decisions, and chronological case material.

Later compatible paths:

- French case law and judgments.
- Codes, statutes, decrees, and effective-date-aware normative text.
- Cross-document legal graph recall over parties, obligations, claims, and citations.

## Design Principles

1. **Provenance first** — every extracted field, risk, event, or answer must trace back to a
   `chunk_id`, offsets, quoted text, confidence, and extractor.
2. **Privacy by default** — Claude Desktop receives pseudonymized chunks by default. Original
   text is restored only through explicit citation-scoped rehydration.
3. **Semi-automatic, not autonomous** — anno fills tables and analyses automatically, but
   uncertain or high-impact fields are marked for user validation.
4. **GLiNER2 enriches; LanceDB retrieves** — GLiNER2 provides legal structure and query
   understanding. LanceDB remains the primary hybrid retrieval engine.
5. **Phased complexity** — start with LanceDB metadata enrichment, then add legal side
   tables, then graph recall after the core evidence model is stable.

## Target Architecture

```text
Claude Desktop legal plugin
        |
MCP tools:
  legal_ingest
  legal_search
  legal_extract_contract
  legal_extract_case_file
  legal_timeline
  legal_risk_review
  legal_rehydrate_citation
  legal_validate_field
        |
anno-rag legal pipeline
        |
+-- parsing / OCR / markdown
+-- pseudonymization / vault
+-- legal-aware chunking
+-- embeddings
+-- GLiNER2 legal enrichment
+-- LanceDB hybrid retrieval
+-- legal chunk enrichment
+-- tabular legal extraction / validation
        |
Local storage:
  LanceDB + vault + optional legal side tables
```

Claude Desktop orchestrates workflows through MCP tools, but the sensitive mechanics stay
inside anno: pseudonymization, storage, retrieval, provenance verification, and rehydration.

## Pipeline

### 1. Ingest

Input documents are parsed into text and structure using the existing ingestion stack. The
pipeline should preserve source metadata:

- `doc_id`
- file path or source id
- page number where available
- section heading
- piece number for litigation files
- sender / recipient where available
- document date where available

### 2. Pseudonymize

Before indexing, text is processed through the existing detector/vault pipeline. Claude
Desktop should receive pseudonymized text by default. The original is available only through
the vault and only for explicitly requested citation spans.

The existing French regex layer remains useful for deterministic PII such as SIRET, IBAN,
NIR-like identifiers, phone numbers, and emails. GLiNER2 complements it for names,
organizations, places, legal actors, and legal concepts.

PII detection and legal enrichment are separate responsibilities:

- `Detector` remains security/vault-facing. It detects entities that must be
  pseudonymized and should stay conservative.
- `LegalEnricher` is a new domain layer. It uses GLiNER2 labels, label descriptions,
  thresholds, and optional rules to extract legal metadata and candidate facts.

This avoids mixing anonymization behavior with business extraction behavior.

### 3. Legal-Aware Chunking

Chunking should respect legal units whenever possible:

- contract clause
- article
- section
- paragraph
- email or letter block
- exhibit/piece boundary
- judgment section later: facts, procedure, reasons, disposition

The chunker should avoid splitting a single contractual clause or obligation across unrelated
chunks. Each chunk keeps enough context to be legally interpretable, but citations remain
chunk-scoped.

### 4. GLiNER2 Legal Enrichment

GLiNER2 runs after chunking and emits legal entities and structured signals. The first version
should use the existing GLiNER2 model with label descriptions and per-label thresholds. LoRA
fine-tuning should wait until there is a local French legal evaluation set.

Candidate labels:

- `person`
- `organization`
- `company_identifier`
- `contract_party`
- `court`
- `jurisdiction`
- `legal_domain`
- `legal_reference`
- `code`
- `article`
- `case_number`
- `decision_date`
- `effective_date`
- `deadline`
- `amount`
- `clause_type`
- `obligation`
- `sanction`
- `risk_indicator`
- `lawyer`
- `judge`
- `regulator`

Example label descriptions:

```text
obligation: a duty imposed on a party by a contract, judgment, formal notice, law, or clause
deadline: a date by which a party must perform an action or lose a right
sanction: a penalty, damages, interest, termination, forfeiture, or legal consequence
legal_reference: a citation to a code, article, law, decree, case, or legal authority
```

### 5. Store

Phase 1 stores legal enrichment in a `legal_chunk_enrichment` table keyed by `chunk_id`.
This avoids a risky migration of the existing `chunks` LanceDB schema, whose current
columns are focused on source metadata, pseudonymized text, offsets, hashes, and vectors.

`legal_chunk_enrichment` should contain:

```text
doc_type
legal_domain
jurisdiction
document_date
parties
legal_entities
legal_refs
amounts
deadlines
risk_flags
confidence_min
confidence_avg
extractor_version
model_id
```

Recommended LanceDB indexes for Phase 1:

| Field | Type | Index |
| --- | --- | --- |
| `chunk_id` | UUID / FixedSizeBinary(16) | `BTREE` |
| `doc_id` | UUID / FixedSizeBinary(16) | `BTREE` |
| `doc_type` | low-cardinality string | `BITMAP` |
| `legal_domain` | low-cardinality string | `BITMAP` |
| `jurisdiction` | low/medium-cardinality string | `BITMAP` or `BTREE` |
| `document_date` | timestamp | `BTREE` |
| `parties` | list of normalized refs | `LABEL_LIST` |
| `legal_entities` | list of normalized refs | `LABEL_LIST` |
| `legal_refs` | list of normalized refs | `LABEL_LIST` |
| `risk_flags` | list of low-cardinality tags | `LABEL_LIST` |

After ingest or enrichment writes new rows, the pipeline should run an
`optimize_after_ingest` step or equivalent maintenance call so LanceDB scalar/FTS/vector
indexes do not accumulate a large unoptimized tail.

Phase 2 reuses `anno-rag-tabular` for semi-automatic extraction and validation wherever
possible. Its existing review/column/row/cell model already provides versioned cells,
citations, support scores, confidence, locks, and human edits. Dedicated legal tables should
only be introduced where the tabular model is not a good fit.

Additional legal relation tables, if needed:

```text
legal_documents
legal_entities
legal_obligations
legal_citations
legal_risks
legal_events
```

Every legal table row that claims a fact must include evidence:

```text
chunk_id
byte_start
byte_end
quoted_text
confidence
extractor
validated_status
```

### 6. Query

Search queries are pseudonymized, then parsed for legal intent and filters:

```text
"retrouve les clauses de penalite contre la societe X apres mars 2024"
```

Becomes:

```json
{
  "intent": "risk_review",
  "party": "ORG_1",
  "clause_type": "penalty",
  "date_after": "2024-03-01"
}
```

The retrieval flow:

```text
query
  -> pseudonymize
  -> GLiNER2 query parse
  -> LanceDB metadata prefilters
  -> LanceDB vector + FTS hybrid search
  -> RRF / rerank
  -> citation verifier
  -> Claude Desktop response
```

`legal_search` should not be a thin wrapper over the existing `Store::search`. It needs a
filter-aware store path that applies LanceDB metadata filters before retrieval when the
filter is part of the result contract. Post-filtering is acceptable only for exploratory
filters where returning fewer than `top_k` results is acceptable.

## MCP Tool Surface

### legal_ingest

Ingests one file or a directory. Returns summary counts:

```json
{
  "doc_id": "...",
  "doc_type": "contract",
  "chunks": 42,
  "entities": 118,
  "risks": 6,
  "needs_validation": 9
}
```

### legal_search

Runs legal-aware hybrid search over pseudonymized chunks with optional filters:

- dossier id
- document type
- date range
- party
- legal domain
- clause type
- risk flag
- confidence threshold

### legal_extract_contract

Produces a citation-backed contract review:

- parties
- effective date
- term and renewal
- payment obligations
- termination
- liability
- penalties
- confidentiality
- governing law
- jurisdiction
- assignment
- uncertain fields

### legal_extract_case_file

Produces a citation-backed litigation file summary:

- parties
- claims
- key facts
- evidence list
- procedural history
- deadlines
- legal issues
- missing documents
- uncertain fields

### legal_timeline

Builds a chronological event table:

```text
date
event
source document
chunk citation
confidence
validation status
```

### legal_risk_review

Finds contract or litigation risks:

```text
risk
severity
affected party
supporting clause/fact
citation
recommended human review
```

### legal_rehydrate_citation

Restores only the cited original span, not the whole source document. This must be a
dedicated API:

```text
legal_rehydrate_citation(chunk_id, byte_start, byte_end)
```

It should verify the chunk exists, validate UTF-8/byte boundaries, slice only the requested
span, and rehydrate only that span through the vault. It should not expose the existing
free-form `rehydrate(text)` behavior as the legal citation tool.

### legal_validate_field

Records human validation, correction, or rejection of an extracted field. These validations
become evaluation data for future threshold tuning and possible LoRA fine-tuning.

## Evaluation Plan

Create a small local gold corpus before any LoRA work:

- 10 French contracts
- 5 French litigation files or synthetic/anonymized case bundles
- gold labels for parties, dates, obligations, amounts, clauses, risks, events, and exact
  citations

Metrics:

- retrieval Recall@K for supporting clauses/facts
- MRR for exact clause retrieval
- citation validity rate
- unsupported answer rate
- entity precision/recall
- obligation extraction F1
- deadline extraction accuracy
- amount normalization accuracy
- human correction rate

LoRA French legal is justified only if this evaluation shows the base model and label
descriptions are not sufficient for target labels.

## Security and Audit

Default behavior:

- original text stays local
- Claude receives pseudonymized chunks
- rehydration is explicit and citation-scoped
- vault owns identity restoration
- no raw full-document rehydration by default

Audit events:

- ingest source
- pseudonymization run
- search query metadata
- citation rehydration request
- validation/correction event
- extractor version and model id
- LanceDB index maintenance / optimize runs

## Known Risks

1. **Offset mismatch** — byte and character offsets can diverge on French accents and
   ligatures. Citation verification must standardize offsets before production use.
2. **Chunk boundary errors** — splitting clauses can make obligations legally ambiguous.
3. **Overconfident GLiNER2 labels** — legal concepts such as sanctions and obligations need
   descriptions, thresholds, and validation.
4. **Pseudonymization side effects** — replacing names and entities can reduce retrieval or
   extraction quality if stable aliases are not preserved.
5. **Hybrid retrieval false positives** — semantically similar chunks can be legally wrong
   without metadata filters and reranking.
6. **LLM unsupported claims** — Claude must answer from retrieved evidence, with verifier
   checks for citations.
7. **Premature LoRA** — training without a gold legal eval set can make quality look better
   while reducing reliability.

## Phased Delivery

### Phase 1 — MVP Legal Metadata

- Add `LegalEnricher` separate from `Detector`.
- Add legal enrichment model output type.
- Run GLiNER2 with legal labels and descriptions on chunks.
- Store basic legal metadata in `legal_chunk_enrichment`, keyed by `chunk_id`.
- Add scalar indexes for legal metadata fields.
- Add `legal_search` with LanceDB metadata prefilters and hybrid retrieval.
- Add citation verification checks for legal search output.
- Add citation-scoped `legal_rehydrate_citation`.
- Add index maintenance after ingest/enrichment.

### Phase 2 — Semi-Automatic Extraction

- Reuse `anno-rag-tabular` for contract and case-file extraction grids.
- Add dedicated legal relation tables only where tabular rows/cells are insufficient.
- Add `legal_extract_contract`.
- Add `legal_extract_case_file`.
- Add `legal_timeline`.
- Add `legal_validate_field`.
- Record corrections as evaluation data.

### Phase 3 — Risk Review and Graph Recall

- Add `legal_risk_review`.
- Add relation expansion from hit chunks to related parties, obligations, events, and
  citations.
- Add eval dashboards for retrieval, extraction, and citation quality.
- Decide whether to train or load a French legal LoRA adapter.

## Open Implementation Questions

1. Should legal enrichment run on original text before pseudonymization, then map spans into
   pseudonymized text, or run only on pseudonymized text?
2. Should `legal_chunk_enrichment` live in LanceDB alongside chunks, or in a separate local
   SQLite/Arrow store with LanceDB only for retrieval?
3. Which exact offset convention should all legal citations use: byte offsets into
   pseudonymized chunk text, original text, or both?
4. Which fields should be scalar-indexed first for LanceDB filtering beyond the recommended
   MVP set?
5. What minimum gold corpus is acceptable before enabling LoRA training?
6. Should French FTS keep the current stemming/stop-word setup only, or add a second phrase
   index for exact legal references and clause names?

## Non-Goals

- No autonomous legal advice without citations and human validation.
- No full-document raw rehydration as a default workflow.
- No LoRA fine-tuning in the first implementation phase.
- No replacement of LanceDB hybrid retrieval with GLiNER2-only matching.
- No jurisprudence/versioned-law specialization until contracts and litigation files are
  stable.
