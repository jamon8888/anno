# Tabular Review — User Guide

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
#    --doc-ids accepts the UUIDs printed by `anno-rag ingest` or `anno-rag search`
anno-rag review add-rows \
  --review $REVIEW_ID \
  --folder-path "deals/Acme_NDA" \
  --doc-ids <uuid1>,<uuid2>,<uuid3>

# 4. Run extraction  (requires ANTHROPIC_API_KEY)
anno-rag review extract --review $REVIEW_ID

# 5. Export
anno-rag review export --review $REVIEW_ID --format csv > acme_nda.csv
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

To see the columns in a template before creating a review, create a temporary review and then list its columns via the MCP `review_list_columns` tool, or inspect the TOML files under `crates/anno-rag-tabular/src/templates/`.

---

## Via the MCP (Claude Desktop / Cowork)

All CLI subcommands are also exposed as MCP tools. Ask Claude:

> "Create a tabular review called 'Acme NDA batch' using the nda-v1 template."

> "Add the documents in the 'deals/Acme_NDA' folder to review <uuid>."

> "Run extraction on review <uuid>."

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

After creating an empty review (no `--template`), add columns via the MCP `review_add_column` tool:

```
"Add a column 'penalty_clause' (text) to review <uuid> asking:
 'Does the agreement include a penalty clause? If so, quote the clause verbatim.'"
```

Column types: `text`, `boolean`, `number`, `date`, `currency`, `verbatim`, `enum`.

---

## Exporting

| Format | Command | Notes |
|--------|---------|-------|
| CSV    | `--format csv` | stdout or `--output file.csv` |
| XLSX   | `--format xlsx --output file.xlsx` | Required: `--output`. Red fill on Low-confidence cells |
| Markdown | `--format md` | stdout or `--output file.md` |

---

## Privacy note

Tabular review is fully local and GDPR-compliant:

- Documents are pseudonymized by the anno-rag vault **before** any text is sent to the LLM.
- The LLM only sees pseudonymized text; real names/entities never leave your machine.
- Cell values and citations are stored in the local LanceDB index alongside the RAG index — nothing is uploaded to a cloud service.
- To check what PII was pseudonymized for a given document, run `anno-rag vault stats` or use the `vault_stats` MCP tool.
