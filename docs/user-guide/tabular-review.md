# Tabular Review — User Guide

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Compliance
Language: Bilingual

Tabular review lets you extract structured data from a set of documents (NDAs, contracts, employment agreements, …) into a spreadsheet-style grid — one row per document, one column per question. Every cell carries an offset-accurate citation back to the source text and a confidence score.

---

## Quick-start (CLI)

```bash
# 1. Ingest your documents first
anno-rag ingest deals/Acme_NDA/ --recursive

# 2. Create a review from the built-in NDA template
anno-rag review create \
  --name "Acme NDA batch" \
  --template nda-v1

# The command prints the review UUID, e.g.:
# Created review 01924c7f-…  (Acme NDA batch) — 14 columns from template 'nda-v1'
REVIEW_ID=01924c7f-...

# 3. Register the documents to extract
#    --doc-ids accepts ingested document UUIDs. Current CLI ingest/search builds
#    may not print doc_id; use MCP `search`/`legal_search` output or the
#    installed build's review/search output as the source of document IDs.
anno-rag review add-rows \
  --review $REVIEW_ID \
  --folder-path "deals/Acme_NDA" \
  --doc-ids <uuid1>,<uuid2>,<uuid3>

# 4. Run extraction  (requires ANTHROPIC_API_KEY)
anno-rag review extract --review $REVIEW_ID

# 5. Export
anno-rag review export --review $REVIEW_ID --format xlsx --output acme_nda.xlsx
anno-rag review export --review $REVIEW_ID --format md
```

---

## Built-in templates

| Template ID              | Columns | Use case |
|--------------------------|---------|----------|
| `nda-v1`                 | 14      | Non-disclosure agreements |
| `customer-contract-v1`   | 14      | B2B customer contracts |
| `real-estate-v1`         | 14      | Real-estate transactions |
| `employment-v1`          | 14      | Employment agreements |
| `ip-v1`                  | 14      | IP assignment & licensing |

To see the columns in a template before creating a review, inspect the TOML files under `crates/anno-rag-tabular/src/templates/`, or create a temporary review and read it back with the review tools available in your installed build.

---

## Via MCP (Claude Desktop / Cowork)

The installed MCP server exposes `review_*` tools for common tabular review
workflows. The MCP surface is not a strict one-to-one mirror of every CLI
subcommand, so use [MCP Tools](../developers/mcp-tools.md) and the installed
tool list as the source of truth.

Canonical MCP workflow:

1. Create a review with `review_create`.
2. Add ingested document UUIDs with `review_add_rows`.
3. Call `review_extract` when `review_add_rows.extraction_started` is `false`, or when a rerun is needed. Use `force_reextract=true` to rerun unlocked cells even when extraction already produced values.
4. Poll `review_get` and inspect `extraction_status.state` until it is `completed`, `completed_with_errors`, or `blocked`.
5. Correct cells with `review_refine_cell` for targeted re-extraction, or `review_set_cell` for a human override.
6. Lock verified cells with `review_lock_cell`; unlock them with `review_unlock_cell` before changing them again.
7. Export with `review_export`.

`extraction_status.state` reports `completed` when extraction finished cleanly,
`completed_with_errors` when some rows or cells failed, and `blocked` when the
review cannot proceed until the reported error is resolved.

Typical prompts:

> "Create a tabular review called 'Acme NDA batch' using the nda-v1 template."

> "Add these document UUIDs to review <uuid>: <doc_id_1>, <doc_id_2>."

> "Start extraction for review <uuid>; use force_reextract if unlocked cells should be rerun."

> "Export review <uuid> as a Markdown table."

The MCP tools return JSON-serialised results so Claude can summarise them inline.

---

## Confidence colours

Each cell has a confidence level set by the citation verifier:

| Colour / level | Meaning |
|----------------|---------|
| 🟢 **High** (`support_score ≥ 0.7`) | Citation text is a strong exact match to the answer |
| 🟡 **Medium** (`0.4 – 0.7`) | Plausible but weaker match |
| 🔴 **Low** (`< 0.4`, or citation offset failed) | Offset mismatch or the quoted text does not support the answer — review manually |

In the XLSX export, Low cells are highlighted in red. In CSV/Markdown, the `confidence` column carries `high`, `medium`, or `low`.

---

## Conditional columns

Some templates contain columns that are only extracted when a parent column meets a condition. For example, `non_solicitation_term` is extracted only when `non_solicitation = true`. When the condition is not met, no cell is written for that column and the export shows an empty cell.

---

## Locking a cell

Once you have verified a cell's value, you can lock it to prevent re-extraction from overwriting it:

```
# Via MCP
"Lock cell for column 'governing_law' in row <row_id> of review <review_id>."
```

Locked cells survive `anno-rag review extract --force`. The `locked` flag is preserved in CSV/XLSX exports (extra column: `locked`).

---

## Incremental extraction

By default `anno-rag review extract` skips columns that already have a cell (incremental mode). To re-run every column:

```bash
anno-rag review extract --review $REVIEW_ID --force
```

---

## Adding custom columns

Custom columns are schema-driven. For repeatable reviews, define or update a
TOML template under the tabular review templates and create the review from
that template. If your installed MCP build exposes column-management tools, use
the installed tool schema as the source of truth.

```
column_id = "penalty_clause"
label = "Penalty clause"
type = "text"
prompt = "Does the agreement include a penalty clause? If so, quote the clause verbatim."
```

Column types: `text`, `boolean`, `number`, `date`, `currency`, `verbatim`, `enum`.

---

## Exporting

| Format | Command | Notes |
|--------|---------|-------|
| XLSX   | `--format xlsx --output file.xlsx` | Required: `--output`. Red fill on Low-confidence cells |
| Markdown | `--format md` | stdout or `--output file.md` |

CSV export exists in the library/MCP review surface where exposed by the
installed build. The current CLI path documents `xlsx` and `md` as the reliable
formats.

---

## Privacy note

Tabular review keeps storage, vault mappings, citations, verification metadata,
and exports local. Extraction may call a configured LLM provider, so treat it as
privacy-safe only when the provider receives tokenized text and the operator has
approved that workflow.

- Documents should be pseudonymized by the anno-rag vault before text is sent to a remote LLM.
- Remote LLM providers should see pseudonymized text; local rehydration is a trusted local operation.
- Cell values and citations are stored in the local LanceDB index alongside the RAG index.
- To inspect vault pseudonymization totals, use the `vault_stats` MCP tool.

## Related Docs

- [Legal RAG](legal-rag.md)
- [MCP Tools](../developers/mcp-tools.md)
- [CLI](../developers/cli.md)
- [Privacy Model](../security-compliance/privacy-model.md)
