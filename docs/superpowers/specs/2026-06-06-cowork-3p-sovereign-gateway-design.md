# Anno Cowork 3P Sovereign Gateway Design

**Status:** Draft for user review  
**Date:** 2026-06-06  
**Scope:** Cowork 3P deployment, Anno RAG local privacy tools, sovereign provider gateway, optional anonymization, and file ingress handling  
**Depends on:** `anno-rag mcp`, `anno-privacy-gateway v0.4 streaming-first`, `anno document ingress v0.5`, `privacy vault word comments`

## Official References Checked

Official pages checked on 2026-06-06:

- Cowork on 3P overview: <https://claude.com/docs/cowork/3p/overview>
- Cowork on 3P gateway provider: <https://claude.com/docs/cowork/3p/gateway>
- Cowork on 3P configuration reference: <https://claude.com/docs/cowork/3p/configuration>
- Cowork on 3P MCP, plugins, skills, and hooks: <https://claude.com/docs/cowork/3p/extensions>
- Cowork on 3P desktop and filesystem access: <https://claude.com/docs/cowork/3p/local-access>
- MCPB desktop extension docs: <https://claude.com/docs/connectors/building/mcpb>
- Mistral OpenAI-compatible migration docs: <https://docs.mistral.ai/resources/migration-guides>
- Scaleway Generative APIs docs: <https://www.scaleway.com/en/docs/generative-apis/how-to/query-language-models/>
- OVHcloud AI Endpoints docs: <https://docs.ovhcloud.com/en/guides/public-cloud/ai-machine-learning/ai-endpoints-function-calling>
- Mistral legal center and Data Processing Addendum entry: <https://mistral.ai/en/terms>
- Scaleway contracts and Data Processing Agreement entry: <https://www.scaleway.com/en/contracts/>
- OVHcloud Data Processing Agreement: <https://us.ovhcloud.com/legal/data-processing-agreement/>

Cowork on 3P is still documented as a research preview. Implementation should
recheck the official pages before final rollout, especially gateway auth, model
discovery, and managed MCP configuration keys.

## Goal

Deploy Anno as the privacy boundary for Cowork on 3P so a regulated team can:

- run Cowork with inference routed to an Anno-controlled gateway;
- use local Anno RAG and privacy tools without exposing raw documents to model
  providers by default;
- route inference to Mistral, Scaleway, OVHcloud, or a local provider;
- make anonymization explicitly optional when a provider has a verified DPA and
  the administrator enables cleartext processing;
- intercept API file/document inputs so raw files are never forwarded unchanged
  to upstream providers in pseudonymized mode.

## Product Decision

The solution is built in three ordered phases:

1. **Cowork 3P local-first:** deploy Cowork with `inferenceProvider="gateway"`
   and a managed local `anno-rag mcp` stdio server. This is the safest first
   path because document extraction, PII detection, pseudonymization, vault
   storage, and RAG indexing already happen locally.
2. **Sovereign provider gateway:** extend `anno-privacy-gateway` from a
   single upstream proxy into a provider router with OpenAI-compatible adapters
   for Mistral, Scaleway, OVHcloud, and local providers.
3. **File ingress API:** implement `/v1/files` and Anthropic `document` block
   interception so API uploads from Claude Desktop/Cowork are extracted and
   transformed before any provider sees them.

This order avoids waiting for file-upload emulation before users get a usable
privacy-preserving Cowork workflow.

## Privacy Modes

Anno exposes privacy as an explicit mode, not as an implicit provider property.

```text
pseudonymized    default; detect and pseudonymize outbound text/files
cleartext_dpa    anonymization off; cleartext allowed to DPA-verified providers
cleartext_local  anonymization off; cleartext allowed only to a local provider
```

`pseudonymized` is the default for every provider and every model. It sends
tokens such as `PERSON_1`, `EMAIL_1`, or `IBAN_1` upstream and keeps the mapping
inside the local encrypted vault.

`cleartext_dpa` is allowed only when all of these conditions are true:

- the provider profile has `dpa_verified = true`;
- the provider profile allows `cleartext_dpa`;
- the gateway deployment allows DPA cleartext globally;
- the model ID or request policy explicitly selects `cleartext_dpa`;
- the gateway writes a content-free audit event with user, provider, model,
  privacy mode, region/profile, request id, and byte/token counts.

`cleartext_local` is allowed only for local inference endpoints. It does not
apply to Mistral, Scaleway, OVHcloud, Anthropic, TensorZero, or any other remote
service.

The user-facing model picker should make the mode visible. Example gateway model
IDs:

```text
anno/mistral/mistral-large-latest:pseudonymized
anno/mistral/mistral-large-latest:cleartext-dpa
anno/scaleway/configured-chat-model:pseudonymized
anno/scaleway/configured-chat-model:cleartext-dpa
anno/ovh/configured-chat-model:pseudonymized
anno/ovh/configured-chat-model:cleartext-dpa
anno/local/configured-chat-model:cleartext-local
```

The exact upstream model IDs must be loaded from provider configuration and
verified against current provider model availability during implementation.

## Target Architecture

```text
Cowork on 3P
  managed config:
    inferenceProvider = gateway
    inferenceGatewayBaseUrl = https://anno-gateway.example
    allowedWorkspaceFolders = approved client roots
    coworkEgressAllowedHosts = gateway and approved telemetry only
    managedMcpServers = local anno-rag stdio server
  |
  | Anthropic Messages API, streaming, tool use
  v
anno-privacy-gateway
  authenticate Cowork request
  resolve model id -> provider profile + privacy mode
  parse Anthropic-compatible request
  transform text and document inputs according to privacy mode
  normalize chat/tool request
  route to provider adapter
  scan/rehydrate response according to privacy mode
  emit Anthropic-compatible response/SSE
  |
  +--> Mistral OpenAI-compatible API
  +--> Scaleway OpenAI-compatible Generative APIs
  +--> OVHcloud OpenAI-compatible AI Endpoints
  +--> local OpenAI-compatible endpoint

Cowork on 3P
  |
  | MCP stdio
  v
anno-rag mcp
  enforce ANNO_RAG_ALLOWED_ROOTS
  index/sync/search local folders
  prepare/finalize privacy vault folders
  return metadata, paths, counts, warnings, and pseudonymized snippets only
```

The gateway is the only remote inference endpoint configured in Cowork. Anno RAG
is the local document and privacy surface. This keeps provider routing, DPA
mode selection, response rehydration, and audit policy under Anno control.

## Cowork 3P Configuration

Cowork should be rolled out through the official 3P managed configuration path.
For an enterprise deployment, use managed configuration instead of user-installed
extensions.

Conceptual managed configuration:

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "https://anno-gateway.example",
  "inferenceGatewayApiKey": "managed-by-mdm",
  "inferenceGatewayAuthScheme": "bearer",
  "modelDiscoveryEnabled": "true",
  "allowedWorkspaceFolders": "[\"C:\\\\Clients\", \"D:\\\\Dossiers\"]",
  "coworkEgressAllowedHosts": "[\"anno-gateway.example\", \"otel.example\"]",
  "isLocalDevMcpEnabled": "false",
  "isDesktopExtensionEnabled": "false",
  "managedMcpServers": "[{\"name\":\"anno-rag\",\"transport\":\"stdio\",\"command\":\"C:\\\\Program Files\\\\Hacienda\\\\anno-rag.exe\",\"args\":[\"mcp\"],\"env\":{\"ANNO_MODELS_DIR\":\"C:\\\\ProgramData\\\\Hacienda\\\\models\",\"ANNO_RAG_ALLOWED_ROOTS\":\"C:\\\\Clients;D:\\\\Dossiers\"},\"toolPolicy\":{\"anno_health\":\"allow\",\"vault_stats\":\"allow\",\"search\":\"allow\",\"index\":\"ask\",\"sync_corpus\":\"ask\",\"rehydrate\":\"ask\",\"privacy_prepare_folder\":\"ask\",\"privacy_finalize_folder\":\"ask\",\"privacy_status\":\"allow\"}}]"
}
```

Cowork stores object and array typed settings as JSON strings in managed
configuration. The deployment tooling must generate `.mobileconfig` or `.reg`
values accordingly.

If the gateway authenticates through network identity or OIDC rather than a
static shared key, the Cowork gateway API key field still needs a non-empty
managed value unless the selected official auth flow removes that requirement.

`allowedWorkspaceFolders` constrains what Cowork can attach, but it does not
replace Anno-side path validation. Anno must enforce `ANNO_RAG_ALLOWED_ROOTS`
inside every path-taking MCP tool because MCP arguments are still user/model
controlled inputs.

For evaluation and self-service pilots, an MCPB desktop extension can package
the local Anno MCP server. For locked-down enterprise rollout, prefer
`managedMcpServers` with `isLocalDevMcpEnabled=false` and
`isDesktopExtensionEnabled=false`.

## Gateway Provider Configuration

Provider configuration should be data-driven, with no hardcoded provider keys.

Example TOML shape:

```toml
[gateway]
default_privacy_mode = "pseudonymized"
allow_cleartext_dpa = true
require_authenticated_user = true

[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "https://api.mistral.ai/v1"
api_key_env = "MISTRAL_API_KEY"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]

[[providers]]
id = "scaleway"
kind = "openai_compatible"
base_url = "https://api.scaleway.ai/v1"
api_key_env = "SCALEWAY_API_KEY"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]

[[providers]]
id = "ovh"
kind = "openai_compatible"
base_url = "https://oai.endpoints.kepler.ai.cloud.ovh.net/v1"
api_key_env = "OVH_AI_ENDPOINTS_ACCESS_TOKEN"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]

[[providers]]
id = "local"
kind = "openai_compatible"
base_url = "http://127.0.0.1:11434/v1"
api_key_env = ""
dpa_verified = false
allowed_privacy_modes = ["pseudonymized", "cleartext_local"]
```

`dpa_verified=true` means an administrator has verified the active contract for
that tenant and provider. It is not inferred automatically from a public legal
page alone.

Sovereignty is also not inferred from the provider name alone. Region, service
variant, data retention, subprocessors, and contractual terms remain deployment
properties that must be reviewed per customer.

## Gateway Components

The provider gateway should use clear boundaries:

- `ModelCatalog`: returns `/v1/models` with Cowork-visible IDs and labels.
- `PrivacyModeResolver`: maps model id, request headers, and defaults to a
  provider plus privacy mode.
- `AnthropicRequestParser`: parses `/v1/messages`, content blocks, tools, and
  streaming flags.
- `PrivacyTransformer`: pseudonymizes text and file-derived content, or permits
  cleartext only when the resolved mode allows it.
- `ChatRequestNormalizer`: converts Anthropic-shaped chat/tool requests into a
  provider-neutral request.
- `ProviderAdapter`: sends normalized requests to provider-specific APIs.
- `OpenAiCompatibleProvider`: common adapter for Mistral, Scaleway, OVHcloud,
  and local OpenAI-compatible endpoints.
- `AnthropicResponseRenderer`: converts provider responses and streaming deltas
  back to Anthropic-compatible responses for Cowork.
- `GatewayAuditLog`: writes content-free audit records.

Cowork on 3P requires `POST /v1/messages` with streaming and tool use for a
gateway. The v0.4 streaming design already notes fail-closed handling for some
streaming tool-use deltas. Production Cowork support must close that gap before
the sovereign provider phase is considered complete.

## File And Document Ingress

File handling has two complementary paths.

### Local MCP Folder Path

The default regulated workflow remains local:

```text
Cowork asks Anno to prepare a folder
  -> privacy_prepare_folder(source_root)
  -> local extraction with Kreuzberg
  -> local PII detection
  -> local encrypted vault pseudonymization
  -> anonymized working/shareable outputs
  -> index pseudonymized chunks
  -> Cowork search returns pseudonymized snippets and metadata
```

This path does not upload raw file bytes through the inference gateway. It is
the preferred first version for legal, finance, healthcare, and client-confidential
workflows.

### Gateway `/v1/files` Path

When Claude Desktop/Cowork sends files through the API, the gateway should own
the file lifecycle:

1. Accept `POST /v1/files` multipart uploads.
2. Enforce authentication, size limits, MIME allowlists, and tenant/user quotas.
3. Hash the raw bytes and create a local Anno file id such as `anno_file_*`.
4. Extract text locally with Kreuzberg.
5. Apply the resolved privacy mode:
   - `pseudonymized`: pseudonymize extracted text and store only sanitized
     provider-facing artifacts by default.
   - `cleartext_dpa`: allow cleartext extracted content only for a DPA-verified
     provider/profile and write an audit event.
   - `cleartext_local`: allow cleartext only to a local provider.
6. Store raw bytes only inside the Anno trust boundary when retention policy
   requires it, encrypted at rest, with an expiry.
7. Return metadata and the Anno file id to Cowork.
8. On later `/v1/messages` requests, resolve only Anno file ids. Unknown
   upstream provider file ids are rejected.
9. Implement `DELETE /v1/files/{id}` to remove raw cache, sanitized artifacts,
   and provider-side sanitized references.

If the gateway is central rather than local on the endpoint, raw uploads still
leave the user's machine for the organization's Anno gateway. That is acceptable
only if the gateway is inside the customer's trust boundary and covered by the
same operational controls. For the strongest privacy posture, use local MCP
folder ingress first.

### Inline `document` Blocks

For `document` blocks inside `/v1/messages`:

- `source.type = "base64"`: decode locally, extract, apply privacy mode, then
  replace the block with sanitized text or a sanitized file reference.
- `source.type = "file"`: resolve only `anno_file_*` ids created by the gateway.
- `source.type = "url"`: reject by default. Optional policy may fetch the URL
  locally with an allowlist, size limits, MIME checks, and the same transform.

No opaque file, URL, or document block should be forwarded unchanged in
`pseudonymized` mode.

Image-only scans, handwriting, and visual PDF page understanding are deferred
unless local OCR and image redaction are added first.

## Data Flow Examples

### Pseudonymized RAG Query

```text
User: "Prepare C:\Clients\Matter X"
Cowork -> anno-rag privacy_prepare_folder
Anno -> extracts, detects, pseudonymizes, writes vault outputs

User: "Find the clause about termination"
Cowork -> anno-rag search
Anno -> returns pseudonymized snippets
Cowork -> anno-privacy-gateway /v1/messages
Gateway -> provider sees pseudonymized prompt/snippets only
Provider -> response with pseudonym tokens
Gateway -> rehydrates allowed tokens locally
Cowork -> displays final answer on the user's device
```

### Cleartext DPA Query

```text
Cowork selects anno/mistral/mistral-large-latest:cleartext-dpa
Gateway authenticates user and resolves provider profile
Gateway verifies provider.dpa_verified and allow_cleartext_dpa
Gateway sends cleartext request to Mistral
Gateway records content-free audit event
Gateway returns response without pseudonymization
```

This mode is useful when contractual, operational, and latency requirements make
provider-side cleartext acceptable. It must not become the default.

### API File Upload In Pseudonymized Mode

```text
Cowork -> POST /v1/files raw PDF
Gateway -> extracts locally and pseudonymizes text
Gateway -> returns anno_file_123
Cowork -> /v1/messages references anno_file_123
Gateway -> expands to sanitized derivative
Provider -> sees sanitized derivative only
```

## Security Requirements

- Provider API keys are read from environment variables or secret managers, not
  from source-controlled files.
- Gateway requests are authenticated with API key, credential helper, or OIDC
  SSO. Per-user identity is required for `cleartext_dpa`.
- Logs and telemetry must not include raw prompts, file contents, vault values,
  provider API keys, or rehydrated answers.
- Every path-taking MCP tool validates against `ANNO_RAG_ALLOWED_ROOTS` after
  canonicalization.
- Gateway model IDs are allowlisted. Unknown models fail closed.
- `cleartext_dpa` fails closed unless provider and deployment policy both allow
  it.
- `/v1/files` and `document` blocks fail closed until the transform is complete.
- Provider-side native file upload is disabled in pseudonymized mode unless the
  uploaded artifact is sanitized.
- Prompt-injection risk from local files is handled with Cowork tool policies:
  search/status can be `allow`; indexing, finalization, and rehydration should
  remain `ask` by default.
- Audit events contain ids, counts, provider, model, mode, status, and timing,
  but never content.

## Testing Requirements

Unit tests:

- `PrivacyModeResolver` defaults to `pseudonymized`.
- `cleartext_dpa` is rejected when `dpa_verified=false`.
- `cleartext_dpa` is rejected when deployment-level
  `allow_cleartext_dpa=false`.
- Unknown model IDs are rejected.
- Provider configs load Mistral, Scaleway, OVHcloud, and local profiles without
  hardcoded secrets.
- Anthropic content blocks and tool-use payloads are normalized without dropping
  required Cowork fields.
- Pseudonymized mode sends no raw PII to a mock provider.
- Cleartext DPA mode sends cleartext only to the selected verified mock
  provider.
- `/v1/files` creates local Anno file ids and never returns raw excerpts.
- Inline `document` URL blocks are rejected by default.

Integration tests:

- Cowork-shaped `/v1/messages` with `stream=true` and tool use succeeds through
  a mock OpenAI-compatible provider.
- `/v1/models` returns exact IDs that can be used in `inferenceModels`.
- Mock Mistral, Scaleway, and OVHcloud endpoints receive the expected
  OpenAI-compatible request shape.
- In pseudonymized mode, mock upstream request capture proves raw names, emails,
  phone numbers, IBANs, SIRETs, and file bytes do not leave Anno.
- In cleartext DPA mode, mock upstream receives cleartext and audit receives
  content-free metadata.
- MCP `privacy_prepare_folder`, `search`, and `rehydrate` work under a
  Cowork-shaped managed stdio configuration.

Manual smoke:

1. Configure Cowork on 3P with `inferenceProvider="gateway"`.
2. Confirm Cowork discovers Anno gateway models or uses explicit
   `inferenceModels`.
3. Confirm managed `anno-rag` appears and lists expected tools.
4. Prepare a local client folder and verify tool responses contain paths,
   counts, and warnings only.
5. Ask a RAG question in `pseudonymized` mode and inspect mock upstream logs for
   sanitized content only.
6. Switch to a `cleartext-dpa` model and verify audit plus upstream cleartext
   behavior with a test document.
7. Upload a file through `/v1/files` and verify only an Anno file id and
   sanitized derivative are used downstream.

## Acceptance Criteria

- Cowork on 3P can run against `anno-privacy-gateway` using the official gateway
  configuration path.
- Cowork has a managed local `anno-rag mcp` server with locked tool policies.
- Anno enforces its own allowed local roots.
- Gateway `/v1/models` exposes provider plus privacy-mode model IDs.
- Mistral, Scaleway, and OVHcloud are represented as OpenAI-compatible provider
  profiles.
- `pseudonymized` is the default and sends no raw PII or raw file bytes
  upstream.
- `cleartext_dpa` is available only for verified provider profiles and always
  writes content-free audit events.
- File uploads and inline documents are transformed or rejected; they are never
  forwarded opaquely in pseudonymized mode.
- Streaming and tool use work for Cowork-shaped requests before provider
  routing is marked production-ready.

## Implementation Phases

### Phase 1: Cowork 3P Local-First

- Document and validate the managed Cowork configuration.
- Add Anno-side `ANNO_RAG_ALLOWED_ROOTS` enforcement if missing.
- Confirm `anno-rag mcp` exposes `index`, `sync_corpus`, `search`,
  `rehydrate`, `privacy_prepare_folder`, `privacy_finalize_folder`, and
  `privacy_status` in a Cowork-friendly way.
- Add a Cowork 3P setup guide referencing the official gateway and managed MCP
  docs.

### Phase 2: Sovereign Provider Gateway

- Add model catalog and privacy-mode model IDs.
- Add provider profile config loading.
- Add OpenAI-compatible provider adapter.
- Add Mistral, Scaleway, OVHcloud, and local provider presets.
- Implement DPA-gated `cleartext_dpa`.
- Complete streaming tool-use support required by Cowork.
- Add audit logging and provider request tests.

### Phase 3: File Ingress API

- Replace `/v1/files` fail-closed behavior with local Anno file registry.
- Add multipart upload, delete, metadata, extraction, privacy transform, and
  retention policy.
- Add inline `document` block handling for base64 and Anno file ids.
- Keep URL documents rejected by default.
- Add sanitized provider-upload support only when a provider requires file
  handles and the artifact has already been transformed.

## Deferred

- Local OCR and image redaction for scanned PDFs.
- Provider-side visual document understanding in pseudonymized mode.
- Cross-device synchronized vaults.
- Full multi-tenant gateway vault isolation beyond authenticated audit and
  provider routing.
- Automatic legal-contract validation from provider public pages.
