# Privacy Vault Word Comments Design

**Status:** Draft for user review  
**Date:** 2026-06-05  
**Scope:** MVP A - folder workflow, normalized Word working documents, Word comments, no Word add-in

## Goal

Create a Cowork-friendly local privacy workflow that lets a user prepare a
folder, review editable Word documents, mark missed PII or false positives with
plain-language comments, then finalize a clean anonymized corpus for sharing and
indexing.

The workflow must keep source documents intact, keep cleartext PII inside the
local machine, and never return document contents or detected values through
Claude/Cowork tool responses.

## User Model

The user works with a local folder that may contain subfolders:

```text
C:\Clients\Matter X\
  contracts\
  emails\
  exhibits\
  scans\
```

When the user asks Cowork to prepare the folder, Anno creates a generated
workspace named `vault` inside that folder:

```text
C:\Clients\Matter X\
  contracts\
  emails\
  exhibits\
  scans\
  vault\
    manifest.json
    01-working-documents\
    02-anonymized-documents\
    03-reports\
    04-cache\
```

Source files stay in place and are not modified.

The user edits only documents in `vault\01-working-documents`. The user shares
only files from `vault\02-anonymized-documents` or safe reports from
`vault\03-reports`.

## User Instructions

The plugin should avoid technical entity labels. The user only needs two
instructions:

- Add a Word comment `à masquer` on text that should be hidden.
- Add a Word comment `à garder` on text that should remain visible.

The parser also accepts ASCII fallbacks:

- `a masquer`
- `a garder`

Comments are instructions, not document content. They are never indexed.

## Proposed Cowork Actions

Expose dedicated MCP tools rather than overloading the existing `index` tool:

```text
privacy_prepare_folder
privacy_finalize_folder
privacy_status
```

Cowork may present those as simple intents:

```text
Prepare this folder for anonymization.
Finalize the folder after my Word edits.
Show privacy workflow status.
```

Tool responses must contain only paths, counts, status, and warnings. They must
not include cleartext values, excerpts, comments, or PII.

## Prepare Flow

`privacy_prepare_folder(source_root)`:

1. Create `source_root\vault`.
2. Discover supported documents while excluding generated folders.
3. Extract each source file with the existing Kreuzberg ingest boundary.
4. Build a normalized cleartext `.docx` in `01-working-documents`, preserving
   the source relative folder structure.
5. Detect PII on extracted chunks using the existing detector.
6. Pseudonymize with the existing local vault.
7. Build a `.docx` without PII in `02-anonymized-documents`.
8. Build local reports in `03-reports`.
9. Write `manifest.json`.
10. Index only anonymized outputs or the existing pseudonymized chunk stream.

The MVP generates normalized Word documents from extracted text. It does not
try to preserve the exact formatting of the original PDF/DOCX/email/table.

## Finalize Flow

`privacy_finalize_folder(vault_path)`:

1. Read `manifest.json`.
2. Hash each `01-working-documents\*.docx`.
3. Skip unchanged working documents.
4. For changed documents, read Word comments and their anchored text spans.
5. Convert comments into local decisions:
   - `à masquer`: force anonymization for the anchored text.
   - `à garder`: suppress anonymization for the anchored text.
6. Extract clean text from the working document, excluding comments.
7. Detect PII.
8. Apply user decisions before pseudonymization.
9. Pseudonymize remaining sensitive values through the vault.
10. Regenerate `02-anonymized-documents`.
11. Re-index only changed anonymized documents.
12. Refresh reports and `manifest.json`.

The working document may contain cleartext PII. It is never indexed directly.
The anonymized document may contain kept-visible exceptions only when the user
explicitly marked them with `à garder`. Finalize reports this count without
showing the values.

## Data Artifacts

### Working Documents

Location:

```text
vault\01-working-documents\<relative-source-path>.docx
```

Purpose:

- user-readable cleartext review copy;
- editable in Word;
- may contain user comments;
- may contain PII;
- never indexed or shared by default.

### Anonymized Documents

Location:

```text
vault\02-anonymized-documents\<relative-source-path>.anon.docx
```

Purpose:

- PII-safe output for sharing and indexing after user review;
- contains tokenized values such as `PERSON_1` or `EMAIL_1`;
- no comments used as instructions;
- no cleartext PII unless the user explicitly marked a value as `à garder`.

`à garder` is treated as a reviewed false-positive decision. If any kept-visible
values exist, finalize must surface a safe warning with counts so the user knows
the anonymized folder contains user-approved visible exceptions.

### Reports

Location:

```text
vault\03-reports\
  sensitive_report.docx
  shareable_report.docx
```

`sensitive_report.docx` is local-only and may show original values alongside
tokens for audit.

`shareable_report.docx` contains only status, counts, file labels, and warnings.

### Manifest

Location:

```text
vault\manifest.json
```

Shape:

```json
{
  "version": 1,
  "source_root": "C:\\Clients\\Matter X",
  "created_at": "2026-06-05T00:00:00Z",
  "updated_at": "2026-06-05T00:00:00Z",
  "documents": [
    {
      "document_id": "uuid",
      "relative_path": "contracts\\contract.pdf",
      "source_path": "C:\\Clients\\Matter X\\contracts\\contract.pdf",
      "working_path": "C:\\Clients\\Matter X\\vault\\01-working-documents\\contracts\\contract.docx",
      "anonymized_path": "C:\\Clients\\Matter X\\vault\\02-anonymized-documents\\contracts\\contract.anon.docx",
      "source_hash": "sha256",
      "working_hash": "sha256",
      "last_indexed_hash": "sha256",
      "status": "prepared",
      "counts": {
        "detected": 12,
        "forced_mask": 0,
        "kept_visible": 0
      }
    }
  ]
}
```

The manifest may store source paths because it is local operational state. Tool
responses should prefer the workspace path and aggregate counts, not full per
document cleartext metadata.

## User Decisions

User decisions are derived from Word comments during finalize. They should be
stored as local decision records so future runs can explain what happened.

Decision records should avoid cleartext values where possible:

```json
{
  "action": "keep_visible",
  "scope": "document",
  "document_id": "uuid",
  "value_hmac": "hmac-sha256",
  "category_hint": null,
  "source": "word_comment",
  "created_at": "2026-06-05T00:00:00Z"
}
```

For MVP, decisions are document-scoped by default. Applying a decision to the
whole folder is out of scope.

## DOCX Comment Parsing

Kreuzberg remains the extraction engine for source documents. A small dedicated
DOCX reader is needed for Word comments because comments are stored in DOCX XML
parts, typically:

```text
word/document.xml
word/comments.xml
```

The reader should:

1. read comment text;
2. map comments to anchored text spans;
3. normalize comment commands case-insensitively;
4. return only decisions and span locations to the pipeline;
5. never log anchored text.

If a comment cannot be mapped to text, finalize should report a safe warning
with document id and comment id, not the comment body or selected text.

## Code Integration

Keep the existing ingest/index code stable. Additive modules in `anno-rag`:

```text
crates/anno-rag/src/privacy_artifacts.rs
crates/anno-rag/src/privacy_decisions.rs
crates/anno-rag/src/docx_instructions.rs
crates/anno-rag/src/privacy_docx.rs
crates/anno-rag/src/privacy_workspace.rs
```

Responsibilities:

- `privacy_artifacts`: manifest structs, document report structs, status enums.
- `privacy_decisions`: apply `mask` / `keep` decisions to detected entities.
- `docx_instructions`: parse DOCX comments from working documents.
- `privacy_docx`: write normalized working docs, anonymized docs, and reports.
- `privacy_workspace`: orchestrate prepare/finalize.

Existing modules reused:

- `ingest::extract` for Kreuzberg extraction.
- `Detector::detect_for_ingest` for PII detection.
- `Vault::pseudonymize_with_map` for local tokenization.
- existing legal and knowledge indexing paths where practical.

One likely additive change:

```text
Vault::pseudonymize_with_report_map(...)
```

This should return the pseudonymized text plus a structured list of replacement
records:

```text
original span, token, category, confidence, source
```

The records are for local artifact generation only and must not be returned via
MCP.

## MCP Surface

### privacy_prepare_folder

Input:

```json
{
  "source_root": "C:\\Clients\\Matter X",
  "recursive": true
}
```

Output:

```json
{
  "ok": true,
  "workspace": "C:\\Clients\\Matter X\\vault",
  "working_folder": "C:\\Clients\\Matter X\\vault\\01-working-documents",
  "anonymized_folder": "C:\\Clients\\Matter X\\vault\\02-anonymized-documents",
  "reports_folder": "C:\\Clients\\Matter X\\vault\\03-reports",
  "documents_seen": 20,
  "documents_prepared": 18,
  "documents_failed": 2
}
```

### privacy_finalize_folder

Input:

```json
{
  "workspace": "C:\\Clients\\Matter X\\vault"
}
```

Output:

```json
{
  "ok": true,
  "workspace": "C:\\Clients\\Matter X\\vault",
  "documents_changed": 3,
  "documents_reindexed": 3,
  "manual_decisions": {
    "to_mask": 5,
    "to_keep": 2
  },
  "anonymized_folder": "C:\\Clients\\Matter X\\vault\\02-anonymized-documents",
  "shareable_report": "C:\\Clients\\Matter X\\vault\\03-reports\\shareable_report.docx"
}
```

### privacy_status

Returns workspace status and counts without loading models when possible.

## Safety Rules

The pipeline must:

- never modify source files;
- never index `vault\01-working-documents`;
- never index `vault\03-reports\sensitive_report.docx`;
- exclude any folder named `vault` from ordinary folder discovery;
- never include cleartext values in MCP responses;
- never log cleartext values, anchored comment text, or selected spans;
- keep report generation local;
- make the shareable report PII-safe by default.

If the user points an indexing command at `vault`, the tool should refuse and
explain which folder is safe to use.

## Performance Design

Performance requirements:

- extract each source document once during prepare;
- detect once per chunk and reuse results for anonymized doc, reports, and
  indexing;
- hash source and working documents to skip unchanged files;
- process documents one at a time to avoid a folder-sized memory buffer;
- write large reports incrementally or cap detailed per-document text sections;
- keep OCR opt-in and preserve existing OCR budget behavior;
- defer model loading for `privacy_status`.

`finalize` should usually be proportional to changed working documents, not the
whole folder.

## Error Handling

Per-document failures should not abort the whole folder unless workspace setup
itself fails.

Examples:

- extraction failure: mark document failed in manifest and report;
- comment parse failure: ignore unsafe instruction, warn by document id;
- anonymized doc write failure: mark finalize failed for that document;
- indexing failure: keep anonymized output and report indexing warning.

Warnings returned to Cowork must be safe and should not include source text or
cleartext PII.

## Tests

Unit tests:

- comment text normalization accepts `à masquer`, `a masquer`, `à garder`,
  and `a garder`;
- `privacy_decisions` removes keep-visible detections before pseudonymization;
- force-mask decisions create a replacement even when the detector missed the
  selected text;
- generated folder detection excludes `vault`.

Integration tests:

- prepare a folder with nested documents and verify mirrored working/anonymized
  paths;
- finalize skips unchanged working documents;
- finalize applies Word comments and regenerates anonymized output;
- MCP responses contain paths and counts only, not cleartext fixture values;
- sensitive reports are not included in discovery or indexing.

Performance tests:

- unchanged finalize path avoids extraction/model work;
- large folder prepare streams per document and does not hold all extracted text
  in one global buffer.

## Out Of Scope For MVP

- Word add-in or ribbon buttons.
- Clipboard companion.
- Exact source formatting preservation.
- Editing original files.
- Folder-wide permanent keep/mask rules.
- Remote provider or Claude for Word integration.
- `.doc` binary Word output. MVP writes `.docx`.

## Open Implementation Questions

1. Whether anonymized `.docx` should be indexed by re-extracting with Kreuzberg
   or whether the existing pseudonymized chunks should be committed directly.
   Direct commit is faster; re-extraction validates the generated artifact.
2. Whether local decision records should be encrypted separately or rely on the
   existing local workspace and HMAC-only value references.
