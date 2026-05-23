# anno Legal RAG — French Lawyers, Graph-Ready (GLiNER2 + LanceDB + lance-graph)

**Date:** 2026-05-23
**Status:** Draft (supersedes `2026-05-22-anno-legal-rag-gliner2-lancedb-design.md`)
**Scope:** Phase 1 (legal metadata + filtered hybrid search) and Phase 2 (semi-automatic tabular extraction) with a graph foundation usable from day one.

## Motivation

Build a production-grade legal intelligence layer for anno's Claude Desktop plugin that is purpose-built for French lawyers and graph-ready from the first slice. The system combines:

- anno's local privacy model (vault-mediated pseudonymization),
- LanceDB hybrid retrieval for fast filtered search,
- `lance-graph` as the canonical entity/relationship store for cross-document reasoning,
- GLiNER2 multilingual entity extraction plus a curated French-legal rules layer,
- citation-scoped rehydration so original text never leaks beyond explicit user intent.

The primary user flow is not chat over documents. It is semi-automatic legal work:
the plugin proposes parties, obligations, dates, amounts, clauses, risks, timelines,
appeal chains, citation chains, mandatory-clause audits, and prescription
calculations — every fact backed by a verifiable citation, every uncertain field
surfaced for human validation.

## Product Scope

### Gold paths (Phase 1+2 delivery)

1. **French contracts** — commercial agreements, service contracts, NDAs, leases,
   amendments, termination letters, related exhibits. Extraction of parties,
   clauses, obligations, deadlines, governing law, mandatory mentions.
2. **French litigation files** — pleadings, formal notices, correspondence,
   evidence bundles, procedural documents, decisions, chronological case
   material. Extraction of parties (with plaintiff/defendant roles), courts,
   case numbers (RG, Portalis), procedural events, deadlines.
3. **French code/article references** — Code civil, Code de commerce, Code du
   travail, Code de la consommation, CGI, etc. Parsed as graph nodes so
   citation chains and "all documents citing article X" queries work
   out of the box.
4. **Mandatory clause compliance + prescription** — per-doctype checklists for
   French-law-required clauses (RGPD, B2B mentions obligatoires, consumer info,
   employment) and a prescription engine that applies French statute-of-
   limitations rules to detected events, with interruption logic for
   mise en demeure and other triggers.

### Later compatible paths

- French case law and judgments as referenceable corpora.
- Codes, statutes, decrees, and effective-date-aware normative text.
- Cross-document legal graph recall over parties, obligations, claims, and
  citations (already structurally enabled by the graph layer).

## Design Principles

1. **Provenance first** — every extracted field, risk, event, or answer must
   trace back to a `chunk_id`, byte offsets, quoted text, confidence,
   extractor version, and model id. No exceptions.
2. **Privacy by default** — Claude Desktop receives pseudonymized chunks by
   default. Original text is restored only through explicit citation-scoped
   rehydration. The graph stores canonical refs (e.g. `org:acme`) for
   traversal but never the original raw spans.
3. **Semi-automatic, not autonomous** — anno fills tables and analyses
   automatically, but uncertain or high-impact fields are marked for user
   validation. Confidence in the borderline band
   (`threshold ≤ conf < threshold+0.10`) sets `needs_validation = true`.
4. **GLiNER2 enriches; LanceDB retrieves; lance-graph reasons** — GLiNER2
   provides legal structure and query understanding; LanceDB remains the
   primary hybrid retrieval engine; lance-graph supplies the
   cross-document reasoning layer.
5. **Constrained graph surface** — Claude does not write Cypher. It picks a
   named intent and parameters; the server runs the parameterized
   template. Safe, auditable, performance-bounded.
6. **Atomic per-document writes with retry semantics** — three stores
   (`chunks`, `legal_chunk_enrichment`, `legal_kg`) end up consistent per
   document, with an `enrichment_status` sidecar tracking retries when a
   legal write transiently fails.
7. **Phased complexity, single foundation** — the graph layer lands in
   Phase 1 so we never have to migrate enrichment data into the graph
   later. Tabular extraction (Phase 2) and risk review (Phase 3) build
   on the same data model.

## Target Architecture

```text
              Claude Desktop
                   |
            anno-rag-mcp tools
                   |
            Pipeline.legal_*()
                   |
   +--------+------+------+----------+
   |        |             |          |
 Vault  Embedder  Detector(PII)  LegalEnricher
                                      |
                              (GLiNER2 + FrenchLegalRules
                              + FrenchNormalizer)
                                      |
                       +--------------+--------------+
                       |                             |
            LegalStore (LanceDB)            LegalKG (lance-graph)
            legal_chunk_enrichment          nodes + edges
                       |                             |
                       +-------+---------+-----------+
                               |         |
                          atomic dual-write inside
                            ingest_one_counted
```

All legal code lives in `crates/anno-rag/src/legal/` — no new crate. Two
swappability traits behind which everything plugs in:
`LegalEntityExtractor` (GLiNER2 today, fine-tuned LoRA later) and
`LegalKnowledgeGraph` (lance-graph today, escape hatch tomorrow).

Lazy initialization inherits from Phase C: `legal_store`, `legal_kg`,
`legal_enricher` are constructed inside `Pipeline::new`, which is already
wrapped in `OnceCell<Arc<Pipeline>>`. No new lazy-init plumbing at the
MCP layer.

## Data Model

### Graph node labels (canonical)

| Label | Key props | Used for |
|---|---|---|
| `Document` | `doc_id`, `doc_type`, `legal_domain`, `jurisdiction`, `document_date`, `dossier_id` | per-file root |
| `Chunk` | `chunk_id`, `doc_id`, `byte_start`, `byte_end`, `page` | bridges graph → LanceDB `chunks` row |
| `Party` | `party_id`, `kind` (person/org), `canonical_name`, `normalized_form` (e.g. `org:acme`), `siren`, `vault_aliases` | dedup across docs |
| `Court` | `court_id`, `name`, `jurisdiction`, `level` (tribunal / cour_appel / cour_cassation / conseil_etat) | litigation routing |
| `Article` | `article_id`, `code` (code_civil / code_commerce / code_travail / code_consommation / cgi / etc.), `article_num`, `normalized_ref` (`code_civil:1240`) | citation graph |
| `Clause` | `clause_id`, `clause_type` (termination / penalty / confidentiality / non_compete / governing_law / arbitration / force_majeure / data_protection / …), `chunk_id`, offsets | contract review |
| `Obligation` | `obligation_id`, `kind` (payment / delivery / performance / non_compete / disclosure / …), `text_pseudo` | who owes what |
| `Amount` | `amount_id`, `value_cents`, `currency`, `scope` (payment / penalty / damages / fee / rent) | money facts |
| `Event` | `event_id`, `kind` (signature / mise_en_demeure / assignation / audience / jugement / arrêt / breach / effective_date / …), `event_date`, `deadline_date?` | timelines & prescription |
| `Risk` | `risk_id`, `severity` (low/med/high), `category` (financial/procedural/contractual/regulatory), `text_pseudo` | risk reviews |
| `MandatoryClauseCheck` | `check_id`, `requirement` (rgpd / b2b_mentions / consumer_info / …), `status` (ok / missing / incomplete) | compliance audit |
| `Validation` | `validation_id`, `action` (confirm/correct/reject), `corrected_value?`, `actor?`, `recorded_at` | future eval data |

### Graph edge types

```text
(Document) -[:HAS_CHUNK]->        (Chunk)
(Chunk)    -[:MENTIONS]->         (Party | Article | Obligation | Clause | Amount | Event | Risk)

(Party)    -[:PARTY_TO {role}]->  (Document)        // role = plaintiff / defendant / buyer / seller / employer / lessor / …
(Party)    -[:BOUND_BY]->         (Obligation)
(Obligation) -[:OWED_TO]->        (Party)
(Obligation) -[:HAS_DEADLINE]->   (Event)
(Obligation) -[:HAS_AMOUNT]->     (Amount)

(Document) -[:REFERENCES]->       (Article)
(Document) -[:HEARS]->            (Court)
(Document) -[:APPEALS]->          (Document)
(Document) -[:AMENDS]->           (Document)
(Chunk)    -[:CITES]->            (Chunk)           // cross-doc citation by quoted-span match
(Event)    -[:TRIGGERS]->         (Obligation)      // mise en demeure → délai de mise en conformité
(Risk)     -[:INVOLVES]->         (Party)
(Risk)     -[:LOCATED_IN]->       (Chunk)
(MandatoryClauseCheck) -[:APPLIES_TO]-> (Document)
(Validation) -[:VALIDATES]->      (Chunk | Party | Obligation | …)
```

Every edge carries `extractor_version`, `model_id`, `confidence`, and
`source_chunk_id` so provenance survives traversal.

### LanceDB `legal_chunk_enrichment` projection (filter table)

A denormalized projection of the graph, optimized for sub-millisecond
filtered hybrid search. Same chunk grain as the `chunks` table, keyed by
`chunk_id`.

| Column | Type | Index | Notes |
|---|---|---|---|
| `chunk_id` | FixedSizeBinary(16) | BTREE | matches `chunks.chunk_id` |
| `doc_id` | FixedSizeBinary(16) | BTREE | matches `chunks.doc_id` |
| `doc_type` | Utf8 | BITMAP | contract / litigation / code / letter / mise_en_demeure / … |
| `legal_domain` | Utf8 | BITMAP | commercial / employment / real_estate / civil / criminal / consumer |
| `jurisdiction` | Utf8 | BITMAP | tribunal_paris / cour_appel_versailles / … |
| `document_date` | Timestamp | BTREE | source document date |
| `dossier_id` | Utf8 | BITMAP | optional grouping for litigation files |
| `parties` | List<Utf8> | LABEL_LIST | normalized refs `org:acme`, `person:dupont_martin` |
| `party_roles` | List<Utf8> | LABEL_LIST | parallel array with `parties` |
| `legal_refs` | List<Utf8> | LABEL_LIST | `code_civil:1240`, `code_commerce:L210-2` |
| `clause_types` | List<Utf8> | LABEL_LIST | termination / penalty / confidentiality / … |
| `obligation_kinds` | List<Utf8> | LABEL_LIST | payment / delivery / non_compete / … |
| `amounts_eur_cents` | List<Int64> | — | money values in cents |
| `deadlines` | List<Timestamp> | LABEL_LIST | deadline dates extracted |
| `event_kinds` | List<Utf8> | LABEL_LIST | mise_en_demeure / assignation / jugement / … |
| `risk_flags` | List<Utf8> | LABEL_LIST | overdue_obligation / missing_mandatory_clause / … |
| `mandatory_clause_status` | Utf8 | BITMAP | ok / missing / incomplete / not_applicable |
| `confidence_min` | Float32 | — | min over extracted fields |
| `confidence_avg` | Float32 | — | mean over extracted fields |
| `extractor_version` | Utf8 | — | enricher impl version |
| `model_id` | Utf8 | — | underlying NER model id |

### `enrichment_status` sidecar table

Tracks dual-write retry state.

| Column | Type | Index |
|---|---|---|
| `doc_id` | FixedSizeBinary(16) | BTREE |
| `status` | Utf8 | BITMAP — `pending` / `ok` / `failed_max_retries` |
| `attempts` | Int32 | — |
| `last_error` | Utf8 | — |
| `last_attempt_at` | Timestamp | BTREE |
| `chunk_count` | Int32 | — |

### Why the split

- **Filter queries** (`legal_search`) hit a flat LanceDB row — no traversal.
- **Traversal queries** (party dossier, citation chain, prescription, deadline
  calendar) hit lance-graph via Cypher — graph-native cost.
- **Deduplication** of parties/articles/courts across docs lives in the graph
  (one `Party` node, many `PARTY_TO` edges); the projection table stores the
  normalized refs.

## Extraction Stack

Three layers run sequentially per chunk batch:

### Layer 1 — GLiNER2 entity extraction (single shared pool)

The existing `gliner2-multi-v1-onnx` weights are shared between `Detector`
(PII) and `LegalEnricher` via `Arc<Pool<GlinerSession>>` (the `Pool<T>`
infrastructure from IS5). One model copy in RAM regardless of ingest
concurrency.

**One combined-label inference pass per chunk**, using the union of PII
labels and legal labels. Each consumer filters the output to its label set.
Cost: ~50% reduction vs. two passes.

Document-level batching: collect all chunk texts for a document, issue one
batched GLiNER2 call, fan results back to per-chunk vectors. Falls back to
per-chunk loop on batch failure.

**Per-label thresholds** (configurable, defaults below):

```rust
person: 0.70, organization: 0.70, contract_party: 0.65,
court: 0.80, jurisdiction: 0.75,
legal_reference: 0.85, article: 0.85, code: 0.85, case_number: 0.85,
effective_date: 0.75, deadline: 0.70, amount: 0.80,
clause_type: 0.60, obligation: 0.55, sanction: 0.65, risk_indicator: 0.55,
company_identifier: 0.90, lawyer: 0.70, judge: 0.75
```

Anything below threshold is dropped. Anything in `[threshold, threshold+0.10)`
is kept with `needs_validation = true`.

### Layer 2 — French legal rules (deterministic relation extractor)

`legal/rules.rs` — pure-function module turning GLiNER2 entities plus raw
text patterns into typed edges. Each rule is
`Fn(&Chunk, &[LegalEntity], &VaultForwardMap) -> Vec<TypedFact>`.

| Rule | French pattern | Edge produced |
|---|---|---|
| `party_role_litigation` | `"X c./ Y"`, `"X / Y"`, `"X contre Y"` | `Party(X)-[:PARTY_TO {role:'plaintiff'}]->Doc`, `Party(Y)-[:PARTY_TO {role:'defendant'}]->Doc` |
| `party_role_contract` | `"Entre les soussignés:"` + party + role keywords | `Party-[:PARTY_TO {role:…}]->Doc` |
| `obligation_engagement` | `"X s'engage à"`, `"X devra"`, `"Il appartient à X de"`, `"À la charge de X"` | `Party(X)-[:BOUND_BY]->Obligation` |
| `obligation_payment` | `"X versera la somme de N €"`, `"paiement de N € à Y"` | `Obligation{kind=payment}-[:HAS_AMOUNT]->Amount`, `-[:OWED_TO]->Party(Y)` |
| `code_reference` | `"art. N c. civ."`, `"Article N du Code civil"`, `"Cciv N"` | `Doc-[:REFERENCES]->Article` |
| `court_routing` | `"Tribunal de commerce de Paris"`, `"T.com. Paris"`, `"TGI Paris"` | `Doc-[:HEARS]->Court` |
| `procedural_event` | `"mise en demeure du DD/MM/YYYY"`, `"assignation délivrée le …"`, `"jugement rendu le …"` | `Event{kind, event_date}` + `Doc-[:MENTIONS]->Event` |
| `delay_relative` | `"dans un délai de N jours/mois à compter de …"`, `"sous N jours"` | `Obligation-[:HAS_DEADLINE]->Event{deadline_date = anchor + N}` |
| `mandatory_clause` | per-doctype checklist (RGPD/data-protection for B2B/B2C, mentions obligatoires for CGV, etc.) | `MandatoryClauseCheck{status}` |

Rules run on pseudonymized text using the vault forward map
(`ORG_1 → Acme SAS → org:acme`) so canonical refs land in nodes while
Claude still sees aliases.

Each emitted edge carries `confidence = min(rule_confidence, entity_confidence)`.

### Layer 3 — Co-occurrence fallback (weak edges)

Every entity GLiNER2 detected but Layer 2 didn't promote to a typed edge
still produces `Chunk-[:MENTIONS]->Entity` so it's reachable via graph
queries without claimed semantics.

### French normalization (`legal/normalize.rs`)

Pure-function module, no model dependency. Property-tested with a
corpus of French legal phrasings.

```rust
pub fn canonical_party_form(raw: &str) -> (PartyKind, String);
pub fn parse_siren(text: &str) -> Option<String>;
pub fn canonical_court(raw: &str) -> Option<CourtRef>;
pub fn parse_code_reference(text: &str) -> Vec<ArticleRef>;
pub fn parse_amount_eur(text: &str) -> Option<i64>;       // returns cents
pub fn parse_french_date(text: &str) -> Option<DateTime<Utc>>;
pub fn parse_delay(text: &str, anchor: DateTime<Utc>) -> Option<DateTime<Utc>>;
```

### Confidence + validation flags

Every fact written to graph or enrichment table:

```rust
pub struct ExtractedFact<T> {
    pub value: T,
    pub confidence: f32,
    pub needs_validation: bool,
    pub source_chunk_id: Uuid,
    pub byte_start: u32,
    pub byte_end: u32,
    pub extractor_version: String,
    pub model_id: String,
}
```

## Ingest Write Path

```rust
async fn ingest_one_counted(&self, path, output_dir, cfg) -> Result<IngestOutcome> {
    // ── existing (unchanged) ─────────────────────────────────────────
    let file_bytes = std::fs::read(path)?;
    let doc_id = doc_uuid(&file_bytes);
    if self.store.doc_exists(doc_id).await? { return Skipped; }
    let extracted = ingest::extract(path, cfg).await?;
    if !should_index_extracted_doc(&extracted) { return Skipped; }

    // ── NEW: single combined-label NER over all chunks ───────────────
    let raw_texts: Vec<&str> = extracted.chunks.iter().map(|c| c.text.as_str()).collect();
    let ner_out = self.gliner_pool
        .get().await
        .extract_batch(&raw_texts, &self.combined_label_set, &self.thresholds)?;

    // ── existing-style pseudonymization, with offset maps captured ───
    let mut pseudo_chunks = Vec::with_capacity(extracted.chunks.len());
    let mut offset_maps   = Vec::with_capacity(extracted.chunks.len());
    for (chunk, raw_ents) in extracted.chunks.iter().zip(&ner_out) {
        let pii_ents = raw_ents.iter().filter(|e| is_pii_label(e.label)).cloned().collect();
        let (pseudo, map) = self.vault.pseudonymize_with_map(&chunk.text, &pii_ents).await?;
        pseudo_chunks.push(pseudo);
        offset_maps.push(map);
    }

    // ── embeddings (existing) + chunks upsert (existing) ─────────────
    let vectors = self.embedder().await?.embed_batch(&pseudo_chunks)?;
    self.store.delete_doc_rows(&extracted.source_path).await?;
    self.store.upsert(records(extracted, &pseudo_chunks, vectors)).await?;

    // ── NEW: legal enrichment (graph + table) ────────────────────────
    let mut legal_rows = Vec::with_capacity(extracted.chunks.len());
    let mut node_batch = NodeBatch::new();
    let mut edge_batch = EdgeBatch::new();
    let doc_node = node_batch.add_document(doc_id, &extracted);

    for (i, chunk) in extracted.chunks.iter().enumerate() {
        let chunk_id = records[i].chunk_id;
        let chunk_node = node_batch.add_chunk(chunk_id, doc_id, chunk);
        edge_batch.has_chunk(doc_node, chunk_node);

        let legal_ents = self.legal_enricher.translate_to_pseudo(
            raw_ents_for_legal(&ner_out[i]),
            &offset_maps[i],
            &pseudo_chunks[i],
        );
        let typed_facts = self.legal_rules.apply(
            chunk_id, &pseudo_chunks[i], &legal_ents, &self.vault_forward_map,
        );
        let mentions = self.legal_enricher.mentions(chunk_id, &legal_ents, &typed_facts);

        legal_rows.push(LegalChunkEnrichment::from_facts(chunk_id, doc_id, &typed_facts));
        node_batch.absorb(typed_facts.nodes());
        edge_batch.absorb(typed_facts.edges());
        edge_batch.absorb(mentions);
    }

    let checks = self.mandatory_clauses.evaluate(doc_id, &legal_rows);
    node_batch.absorb(checks.nodes());
    edge_batch.absorb(checks.edges());

    // ── atomic dual-write with retry semantics ───────────────────────
    let written = async {
        self.legal_store.upsert(&legal_rows).await?;
        self.legal_kg.upsert_batch(&node_batch, &edge_batch).await?;
        Ok::<_, Error>(())
    }.await;

    if let Err(e) = written {
        tracing::warn!(doc_id=%doc_id, attempt=1, error=%e,
            "legal write failed; chunks committed, marking enrichment pending");
        self.enrichment_status
            .mark_pending(doc_id, ChunkCount(extracted.chunks.len()), &e)
            .await?;
        return Ok(IngestOutcome::Ingested(extracted.chunks.len()));
    }
    self.enrichment_status.mark_ok(doc_id).await?;

    write_anon_markdown(path, output_dir, &pseudo_chunks)?;
    Ok(IngestOutcome::Ingested(extracted.chunks.len()))
}
```

**Retry path:** `Pipeline::drain_enrichment_backlog(max_docs)` reads pending
rows from `enrichment_status`, re-fetches each doc's chunks out of LanceDB,
re-runs Layer 1+2+3 + dual-write. Exponential backoff (2ⁿ minutes up to
attempt 5), then `failed_max_retries` with audit log. Called automatically
at the end of `ingest_folder` (`drain_enrichment_backlog(64)`). Exposed as
`legal_drain_enrichment_backlog` MCP tool.

## Query Surface

### 1. `legal_search(query, top_k, filters)` — filtered hybrid retrieval

Cheapest path; filters go through the enrichment projection.

```rust
pub struct LegalSearchFilters {
    pub doc_type: Option<String>,
    pub legal_domain: Option<String>,
    pub jurisdiction: Option<String>,
    pub dossier_id: Option<String>,
    pub parties: Vec<String>,
    pub party_roles: Vec<String>,
    pub legal_refs: Vec<String>,
    pub clause_types: Vec<String>,
    pub obligation_kinds: Vec<String>,
    pub event_kinds: Vec<String>,
    pub risk_flags: Vec<String>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to:   Option<DateTime<Utc>>,
    pub min_confidence: Option<f32>,
    pub mandatory_clause_status: Option<String>,
}
```

Flow: pseudonymize query → enrichment `filter_chunk_ids(limit = top_k*20)`
→ `chunks.search_filtered_to_chunks(...)` → RRF → return
`LegalSearchHit{chunk_id, doc_id, text_pseudo, score, enrichment: Some(_)}`.

### 2. `legal_graph_query(intent, params)` — Cypher traversals (5 named intents)

Claude picks an intent + params; server runs the parameterized Cypher.

| Intent | Cypher (sketch) | Returns |
|---|---|---|
| `party_dossier` | `MATCH (p:Party {normalized_form:$party})-[r:PARTY_TO]->(d:Document)-[:HAS_CHUNK]->(c:Chunk) RETURN d, r.role, c ORDER BY d.document_date` | All docs + roles + chunks for one party |
| `obligations_owed_by` | `MATCH (p:Party {normalized_form:$party})-[:BOUND_BY]->(o:Obligation) OPTIONAL MATCH (o)-[:OWED_TO]->(t:Party), (o)-[:HAS_DEADLINE]->(e:Event), (o)-[:HAS_AMOUNT]->(a:Amount) RETURN o, t, e, a` | Obligations for one party |
| `citation_chain` | `MATCH (a:Article {normalized_ref:$article_ref})<-[:REFERENCES]-(d:Document)-[:HAS_CHUNK]->(c:Chunk) WHERE (c)-[:MENTIONS]->(a) RETURN d, c` | All docs + chunks citing one article |
| `procedural_timeline` | `MATCH (d:Document {dossier_id:$dossier})-[:HAS_CHUNK]->(c:Chunk)-[:MENTIONS]->(e:Event) RETURN e ORDER BY e.event_date` | Chronological events for a dossier |
| `appeal_chain` | `MATCH path = (d:Document {doc_id:$doc_id})-[:APPEALS*1..5]->(prior:Document) RETURN path` | Litigation appeal lineage |

Returned chunks carry `text_pseudo` (batched fetch from LanceDB by `chunk_id`)
so responses are immediately usable.

### 3. `legal_prescription_check(event_id | event_date, legal_basis)`

Pure code, reads the triggering event from the graph, applies French
prescription rules:

```rust
pub struct PrescriptionRule {
    pub legal_basis: &'static str,
    pub years: u32,
    pub article: &'static str,
    pub starts_from: PrescStart,
}
```

Returns `{ prescribes_on, time_remaining, legal_basis, article_ref,
source_event, interrupted_by[] }`. Detects interruption via
`(:Event{kind:'mise_en_demeure'})-[:TRIGGERS]->(:Obligation)`.

### 4. `legal_mandatory_clause_audit(doc_id)`

Reads `MandatoryClauseCheck` nodes for the document; returns the checklist
with status + chunk evidence for each missing/incomplete clause.

### 5. `legal_rehydrate_citation(chunk_id, byte_start, byte_end)`

Slice + rehydrate one span. Never the whole doc.

## MCP Tool Surface

Eleven tools. Each carries provenance and returns pseudonymized text by default.

| Tool | Inputs | Returns | Notes |
|---|---|---|---|
| `legal_ingest` | `folder` or `path`, `recursive`, `dossier_id?` | counts + `pending_enrichment_count` | wraps existing ingest + dual-write |
| `legal_search` | `query`, `top_k`, `LegalSearchFilters` | `Vec<LegalSearchHit>` | workhorse |
| `legal_graph_query` | `intent` (enum), `params` | `GraphQueryResult` with chunks pre-rehydrated to `text_pseudo` | 5 named traversals |
| `legal_extract_contract` | `doc_id` | citation-backed contract grid (parties, term, payment, termination, liability, penalty, confidentiality, governing law, jurisdiction, assignment, uncertain fields) | reuses `anno-rag-tabular` |
| `legal_extract_case_file` | `doc_id` or `dossier_id` | citation-backed litigation grid (parties, claims, facts, evidence, procedural history, deadlines, legal issues, missing docs) | reuses `anno-rag-tabular` |
| `legal_timeline` | `dossier_id` or `doc_id` | chronological event table with citations | `procedural_timeline` intent + formatting |
| `legal_risk_review` | `doc_id` or `dossier_id` | risks with severity + supporting clauses + recommended human review | reads `Risk` nodes |
| `legal_mandatory_clause_audit` | `doc_id` | checklist with status + chunk evidence | pre-computed |
| `legal_prescription_check` | `event_id` or `event_date + legal_basis` | `{prescribes_on, time_remaining, article_ref, interrupted_by[]}` | rule engine |
| `legal_rehydrate_citation` | `chunk_id`, `byte_start`, `byte_end` | rehydrated span | citation-scoped |
| `legal_validate_field` | `fact_ref`, `action`, `corrected_value?`, `note?` | acknowledgement | feeds future eval |
| `legal_drain_enrichment_backlog` | `max_docs?` (default 64) | drain report | manual retry trigger |

All tool schemas derive from `schemars::JsonSchema` and appear in the
MCP manifest.

## French-Specific Reference Data

### Court taxonomy (`legal/courts.rs`)

Alias table mapping French court names + abbreviations to canonical
`CourtRef`. Covers tribunal judiciaire, tribunal de commerce, tribunal de
proximité, conseil de prud'hommes, cour d'appel, cour de cassation,
conseil d'État, tribunal administratif, cour administrative d'appel.

### Code reference parser (`legal/codes.rs`)

Alias table mapping `c. civ.`, `Cciv`, `Code civil`, etc. → `code_civil`.
Same for `c. com.`, `c. trav.`, `c. cons.`, `cgi`, `csp`, `csss`, `cgct`,
`code de procédure civile`, `code pénal`, `code de procédure pénale`,
`code de l'environnement`, `code monétaire et financier`, etc.

### Mandatory clauses (`legal/mandatory.rs`)

Per-doctype × legal-domain checklist:

- **B2B contracts**: identification of parties, prix/montant TTC, modalités
  de paiement, pénalités de retard, indemnité de recouvrement (40 €), date
  de livraison, clause de réserve de propriété (optional but flagged),
  loi applicable, juridiction compétente, clause attributive de
  compétence (B2B specific).
- **B2C contracts**: identité et coordonnées du professionnel, mention
  garantie légale de conformité (2 ans), droit de rétractation (14
  jours), garantie des vices cachés, prix TTC, modalités de livraison,
  médiateur de la consommation.
- **Employment contracts**: identité parties, lieu de travail, fonction,
  date d'embauche, rémunération, durée du travail, convention
  collective applicable, période d'essai (if applicable), clause de
  non-concurrence (if applicable, with conditions).
- **Lease (commercial)**: désignation des locaux, destination, durée,
  loyer + indexation, charges, dépôt de garantie, état des lieux,
  clause de répartition des charges (décret du 3 novembre 2014).
- **Lease (residential)**: surface habitable, montant loyer + dernier
  loyer payé par précédent locataire (zones tendues), DPE, état des
  lieux, dépôt de garantie (1 mois max non meublé / 2 mois meublé).
- **Data processing (RGPD)**: when document mentions personal data
  processing — finalités, base légale, durée de conservation, droits
  des personnes, sous-traitants, transferts hors UE, mention CNIL.

Each requirement maps to one or more detector patterns (regex + GLiNER2
`clause_type` matches). Missing → `MandatoryClauseCheck{status: missing}`.

### Prescription rules (`legal/prescription.rs`)

```rust
const RULES: &[PrescriptionRule] = &[
    PrescriptionRule { legal_basis: "contractuel",        years: 5, article: "code_civil:2224",          starts_from: KnowledgeOfDamage },
    PrescriptionRule { legal_basis: "delictuel",          years: 5, article: "code_civil:2224",          starts_from: KnowledgeOfDamage },
    PrescriptionRule { legal_basis: "consommation_b2c",   years: 2, article: "code_consommation:L218-2", starts_from: EventDate },
    PrescriptionRule { legal_basis: "salarial",           years: 3, article: "code_travail:L1471-1",     starts_from: KnowledgeOfDamage },
    PrescriptionRule { legal_basis: "loyer_bail",         years: 3, article: "code_civil:7-1_loi_89",    starts_from: EventDate },
    PrescriptionRule { legal_basis: "bail_commercial",    years: 2, article: "code_commerce:L145-60",    starts_from: EventDate },
    PrescriptionRule { legal_basis: "facture_b2b",        years: 5, article: "code_civil:2224",          starts_from: EventDate },
    PrescriptionRule { legal_basis: "action_reelle_immo", years: 30, article: "code_civil:2227",         starts_from: EventDate },
    // … extensible
];
```

Interruption logic recognises `mise_en_demeure`, `assignation`,
`reconnaissance_dette`, and `acte_executoire` from the `Event.kind`
field with edges to the obligation/contract.

## Phased Delivery

### Stage A — Foundation (5 tasks)
- A1: Legal types & label catalog
- A2: French normalization module
- A3: Shared GLiNER2 pool refactor + combined-label API
- A4: `legal_chunk_enrichment` schema + indexes
- A5: `enrichment_status` sidecar table

### Stage B — Extraction & dual-write (5 tasks)
- B1: `LegalEntityExtractor` trait + GLiNER2 adapter + fake
- B2: French legal rules module
- B3: `LegalEnricher::enrich_chunks_batched`
- B4: `ingest_one_counted` dual-write (table only, no graph)
- B5: `drain_enrichment_backlog`

### Stage C — Graph layer (6 tasks)
- C1: `LegalKnowledgeGraph` trait + `LanceGraphStore`
- C2: Wire graph into `ingest_one_counted` (full dual-write)
- C3: Cross-document linking pass (dedup parties/articles/courts; APPEALS/AMENDS/CITES edges)
- C4: Five named graph intents
- C5: Index maintenance + compaction hooks
- C6: Batched chunk text round-trip helper

### Stage D — MCP tools, French specifics, eval (6 tasks)
- D1: MCP wave 1 (`legal_ingest`, `legal_search`, `legal_graph_query`, `legal_rehydrate_citation`)
- D2: MCP wave 2 (`legal_extract_contract`, `legal_extract_case_file`, `legal_timeline`, `legal_risk_review`)
- D3: Mandatory clauses + audit tool
- D4: Prescription engine + tool
- D5: `legal_validate_field` + Validation node wiring
- D6: Gold corpus + eval suite

Stages are independently shippable. Heavy tests (model-loading) marked
`#[ignore]` and run only against a warm GLiNER2 cache.

## Evaluation Plan

Local gold corpus before any LoRA work:

- 10 French contracts (mix of commercial, employment, leases, NDAs)
- 5 French litigation files (synthetic / anonymized)
- gold labels for parties, dates, obligations, amounts, clauses, risks,
  events, citations, mandatory-clause presence, prescription anchors

Metrics:

- retrieval Recall@K for supporting clauses/facts
- MRR for exact clause retrieval
- citation validity rate (byte-offset within chunk, exact text match)
- entity precision / recall (per label)
- obligation extraction F1
- deadline extraction accuracy
- amount normalization accuracy
- mandatory-clause detection F1
- prescription calculation accuracy (against hand-computed expected dates)
- graph-traversal cardinality + latency (party_dossier on 50-doc corpus)
- human correction rate

LoRA French legal is justified only if this evaluation shows the base
model + label descriptions + rules are insufficient for target labels.

## Security and Audit

- Original text stays local.
- Claude receives pseudonymized chunks.
- Rehydration is explicit and citation-scoped.
- Vault owns identity restoration.
- No raw full-document rehydration by default.
- Graph nodes carry canonical refs (`org:acme`), not raw names.

Audit events emitted (existing tracing target `anno_rag::*::audit`):

- ingest source + dual-write outcome
- pseudonymization run
- search query metadata + filters
- graph intent + parameters
- citation rehydration request (chunk_id + offsets)
- validation/correction event
- prescription check (event_id + computed result)
- mandatory-clause audit
- extractor version + model id
- LanceDB / lance-graph maintenance runs
- enrichment retry attempts (success + failure)

## Known Risks

1. **Offset mismatch** between raw and pseudonymized text — mitigated by the
   `PseudoOffsetMap` captured during pseudonymization and used for legal-span
   translation.
2. **Chunk boundary errors** splitting obligations — existing chunker is
   already legal-aware in v0 (paragraph + sentence boundaries); refinement
   to clause boundaries is a Stage D refinement, not a blocker.
3. **Overconfident GLiNER2 labels** — addressed by per-label thresholds and
   `needs_validation` band.
4. **Pseudonymization side effects** on retrieval — vault aliases (`ORG_1`)
   stay stable per entity within a doc; embeddings see consistent text.
5. **Hybrid retrieval false positives** — metadata filters and `min_confidence`
   filter most out; reranker stage further refines.
6. **LLM unsupported claims** — every tool result carries provenance so Claude
   is structurally pushed to cite.
7. **Premature LoRA** — explicitly deferred until eval corpus shows base
   model insufficient.
8. **lance-graph pre-1.0** — wrapped behind `LegalKnowledgeGraph` trait. If the
   library proves unstable, we swap to an alternative (in-process petgraph
   with periodic LanceDB persist, or a small SQLite graph) without touching
   the rest of the stack.
9. **Dual-write transient failure** — handled by `enrichment_status` retry
   queue; chunks remain searchable filter-less while pending.
10. **Cross-doc party dedup ambiguity** — `normalized_form` collisions
    (e.g., two different "Dupont") flagged for validation, not auto-merged.

## Non-Goals

- No autonomous legal advice without citations and human validation.
- No full-document raw rehydration as a default workflow.
- No LoRA fine-tuning in the first implementation phase.
- No replacement of LanceDB hybrid retrieval with GLiNER2-only matching.
- No free-form Cypher exposed to Claude.
- No jurisprudence/versioned-law specialization until contracts and litigation
  files are stable.
- No automated cross-party identity resolution beyond `normalized_form` matching.

## Open Implementation Questions

1. Confidence band width (`+0.10` for `needs_validation`) — tune against
   eval results.
2. Should `legal_graph_query` results stream or return atomically? Atomic
   for v1; streaming if 95p response times exceed 2s.
3. Cross-doc `CITES` edge construction — quoted-span match heuristic vs.
   embedding similarity. Start with quoted-span match; embedding fallback
   in Stage C3 if recall is low.
4. Should `legal_validate_field` create a new node version or mutate? New
   node version (`Validation`) to preserve audit trail; mutation only for
   reject status.
5. Pool size for shared GLiNER2 sessions — start at `min(num_cpus, 4)`,
   tune from `ingest_ner_pool` config field already present.
6. Should `legal_extract_contract` and `legal_extract_case_file` materialize
   to `anno-rag-tabular` rows synchronously or background-task them? Sync
   for v1; background queue if median latency exceeds 5s on real corpora.
