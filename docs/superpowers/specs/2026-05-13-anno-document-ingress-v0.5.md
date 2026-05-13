# anno document ingress v0.5 — Claude/Cowork files without cleartext provider leakage

**Status:** Draft spec
**Date:** 2026-05-13
**Depends on:** `anno-privacy-gateway v0.4`, `anno-rag v0.2`

## Goal

Let users work with documents from Claude Desktop/Cowork while preserving the same privacy invariant as chat prompts:

> No original document text or file bytes may reach TensorZero, a sovereign provider, OpenAI, Anthropic, or any other non-local model provider unless policy explicitly permits a local-cleartext path.

v0.5 closes the gap left by v0.3/v0.4: native Claude file upload flows and `document` content blocks.

## Research findings

Anthropic exposes document inputs in two families:

- Files API: upload once, receive a `file_id`, then reference that ID from a Messages `document` or `image` block.
- Inline Messages API: send PDF/document content as URL or base64 `document` blocks.

PDF support can process text and visual page content, but that processing happens provider-side unless the gateway extracts locally first.

Claude/Cowork 3P also supports local extension surfaces:

- MCPB desktop extensions run locally over stdio and are appropriate for privacy-sensitive local operations.
- Cowork 3P can receive organization plugins that bundle `.mcp.json` MCP server configuration.

Local code findings:

- `anthropic-proxy-rs` does not support Files API and its `ContentBlock` model has no `document` variant.
- TensorZero supports file inputs in its native API and can store files for observability, but it is behind Anno's trust boundary and must not receive raw sensitive files by default.
- `anno-rag` already extracts local documents with `kreuzberg`, pseudonymizes chunks through the local vault, writes anonymized markdown copies, and indexes pseudonymized chunks.
- Current `anno-rag mcp` exposes `search`, `rehydrate`, `detect`, and `vault_stats`; it does not yet expose `ingest_file` or `ingest_folder`.

## Product decision

Default path for regulated professions:

```text
Claude Desktop/Cowork
  -> local Anno MCP tool or desktop extension
  -> anno-rag local ingest
  -> kreuzberg extraction
  -> local PII detection + vault pseudonymization
  -> pseudonymized index and pseudonymized copy
  -> Cowork queries pseudonymized chunks
```

This is cleaner than trying to transparently forward raw uploads. It keeps original bytes local and aligns with the existing RAG architecture.

Native `/v1/files` interception is still useful, but it is a second implementation track because it must emulate enough of Anthropic's file lifecycle for Cowork clients.

## Modes

### Mode A — Local MCP document ingress (recommended first)

Add tools to `anno-rag mcp`:

- `ingest_file(path, output_dir?, labels?)`
- `ingest_folder(path, recursive?, output_dir?, labels?)`
- `list_documents(filter?)`
- `get_document(doc_id, include_chunks?)`

Security properties:

- Claude/Cowork sees only tool result metadata and pseudonymized snippets.
- Original files remain on disk under user control.
- No file bytes transit through the LLM gateway.
- Works with MCPB/local desktop extension packaging.

Tradeoff:

- The user selects or references local paths through a tool workflow rather than relying on native Claude upload UI.

### Mode B — Anthropic `/v1/files` interception

Implement in `anno-privacy-gateway`:

- `POST /v1/files`: accept multipart upload locally.
- Extract with `kreuzberg` before any upstream call.
- Pseudonymize extracted text and persist mappings in the local vault.
- Store original bytes only in local encrypted/cache storage according to retention policy.
- Store sanitized derivative as text/markdown, and optionally as a sanitized provider-uploadable file.
- Return a local `file_id` whose metadata maps to the sanitized artifact, not the original file.
- `DELETE /v1/files/{id}` deletes local original cache, sanitized artifact, and any upstream sanitized file reference.

When a later `/v1/messages` request references that `file_id`, the gateway expands it to a pseudonymized text block or uploads only the sanitized derivative upstream if the selected provider requires file handles.

Security properties:

- Raw upload never goes behind Anno.
- Provider-side file storage, TensorZero observability, and model prompts only contain pseudonymized derivative content.

Tradeoff:

- Requires file lifecycle state, multipart handling, retention policy, and provider-specific sanitized upload adapters.

### Mode C — Inline `document` block interception

Handle `/v1/messages` blocks:

- `source.type = "base64"`: decode locally, extract, pseudonymize, replace with pseudonymized text or sanitized file representation.
- `source.type = "url"`: default reject for regulated mode because the provider could fetch the remote URL directly; optional mode fetches locally, validates size/MIME, extracts, pseudonymizes, and sends only sanitized derivative.
- `source.type = "file"`: resolve only local Anno file IDs created by Mode B; reject unknown provider file IDs.

Security properties:

- No opaque `document` block is forwarded unchanged.

Tradeoff:

- Visual PDF understanding is degraded unless we add local page rasterization + image/table redaction, or a policy-approved provider gets sanitized page images.

## TensorZero role

TensorZero stays behind the privacy boundary.

For v0.5, TensorZero may receive:

- pseudonymized extracted text,
- sanitized files derived from local extraction,
- sanitized file metadata for observability.

TensorZero must not receive:

- original uploaded PDF/DOCX/TXT bytes,
- original file names if they contain PII,
- original document URLs unless policy explicitly permits it.

If TensorZero file observability is enabled, configure object storage as local or controlled S3-compatible storage and store only sanitized artifacts. For reproducibility, prefer TensorZero's fetch-and-encode behavior only after Anno has already sanitized any remote content.

## Provider routing

Provider policy extends v0.4:

```text
allowed_file_data = "none" | "sanitized_only" | "local_cleartext"
remote_url_documents = "reject" | "fetch_local_then_sanitize" | "allow"
native_provider_files = "disabled" | "sanitized_upload_only"
```

Defaults for regulated mode:

- `allowed_file_data = "sanitized_only"`
- `remote_url_documents = "reject"`
- `native_provider_files = "sanitized_upload_only"`

## Implementation plan

1. Add `Pipeline::ingest_path` wrapper that accepts one file or a folder and returns structured ingest receipts instead of only printing CLI output.
2. Add MCP tools `ingest_file` and `ingest_folder`; keep tool outputs pseudonymized and metadata-only.
3. Package the MCP server as an MCPB/local desktop extension for Claude Desktop/Cowork installs.
4. Add v0.3/v0.4 gateway fail-closed tests for `document` blocks and `/v1/files`.
5. Implement local file registry for Mode B with local `file_id` generation, metadata, retention, delete, and sanitized artifact references.
6. Implement `document` block transform for base64 documents and local Anno file IDs.
7. Add optional provider sanitized-upload adapter only after Mode A and fail-closed behavior are stable.

## Test plan

Unit tests:

- MCP `ingest_file` returns receipt with document id, chunk count, output path, and no cleartext snippets.
- Base64 PDF/text `document` block is decoded, extracted, pseudonymized, and never forwarded raw.
- Unknown provider `file_id` is rejected.
- URL document blocks are rejected by default.
- File names with PII are pseudonymized or replaced in provider-facing metadata.

Integration tests:

- Mock upstream records every request body; assert no raw name, email, phone, NIR, SIRET, IBAN, or original file bytes leave Anno.
- Upload flow: `POST /v1/files`, then `/v1/messages` with local `file_id`; assert upstream receives only sanitized derivative.
- MCP flow: ingest a local PDF, search from Cowork-shaped tool request, rehydrate final answer locally.
- TensorZero mock/object storage receives only sanitized files or pseudonymized text.

Manual smoke:

```text
1. Install Anno local MCPB/organization plugin.
2. In Cowork, ingest a local legal document through the Anno tool.
3. Ask a question about the document.
4. Verify retrieved snippets are pseudonymized before model routing.
5. Verify final answer can be rehydrated locally when policy allows.
```

## Acceptance criteria

- v0.3/v0.4 never forward native documents unchanged.
- Local MCP ingest is usable from Claude Desktop/Cowork without raw provider upload.
- `/v1/files` interception returns local file IDs and maps only sanitized derivatives to upstream providers.
- `document` blocks are transformed or rejected; never passed through opaquely.
- TensorZero and external providers see sanitized/pseudonymized content only.
- Deleting a local file ID removes cached original bytes, sanitized artifacts, and upstream sanitized references.

## Deferred beyond v0.5

- Local visual PDF understanding with page images, charts, handwriting, and table layout redaction.
- OCR/image redaction for scanned PDFs before provider visual analysis.
- Cross-device document registry sync.
- Enterprise retention UI.
- Per-client legal privilege policy engine.
