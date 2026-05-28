# Anno RAG Tabular - Local Subscription LLM Providers

Date: 2026-05-28
Status: Design approved, pending user spec review
Scope: `anno-rag-tabular` insight and fallback extraction through local Claude Code and Codex subscriptions, without API keys

## Context

`anno-rag-tabular` already has a narrow provider boundary:

- `crates/anno-rag-tabular/src/llm/mod.rs` defines `LlmClient`.
- `LlmClient::generate_structured(system, user, json_schema)` is provider-neutral.
- `crates/anno-rag-tabular/src/llm/anthropic.rs` is the current API-backed implementation.
- `crates/anno-rag-tabular/src/extract/batch.rs` already builds a prompt and JSON Schema, then parses a provider output into cited cells.

The user wants to use local subscriptions already authenticated in Claude Code CLI and Codex CLI for insight extraction, without creating or storing API keys.

This design adds local subscription-backed providers while preserving the existing tabular extraction contract and privacy boundary.

## Current External Evidence

Codex:

- Official Codex docs describe a Codex SDK that can programmatically control local Codex agents: https://developers.openai.com/codex/sdk
- The Codex SDK TypeScript package is `@openai/codex-sdk`.
- The experimental Python SDK controls the local Codex app-server over JSON-RPC.
- Codex authentication supports both ChatGPT subscription access and API key access; the CLI and IDE support both: https://developers.openai.com/codex/auth
- Codex app-server exposes local JSON-RPC transports, including stdio: https://developers.openai.com/codex/app-server
- Codex non-interactive mode supports structured outputs via `--output-schema`: https://developers.openai.com/codex/noninteractive
- Prior art exists for "CLI provider" patterns that reuse local subscriptions, notably Atmos CLI providers: https://atmos.tools/changelog/ai-cli-providers

Claude:

- Claude Code can run non-interactively with `claude -p`: https://code.claude.com/docs/en/headless
- Claude Code auth precedence prefers environment API keys over subscription OAuth when present: https://code.claude.com/docs/en/iam
- Claude Agent SDK is local and programmable in Python/TypeScript: https://code.claude.com/docs/en/agent-sdk/overview
- The Claude Agent SDK docs state that third-party products generally should not offer claude.ai login or rate limits without prior approval; use API key methods unless approved.
- Starting June 15, 2026, Claude Agent SDK and `claude -p` usage on subscription plans will draw from a separate monthly Agent SDK credit.

## Product Decision

Add local subscription providers, but treat them differently by provider maturity and terms.

Primary provider:

- `CodexLocalProvider`
- Uses local Codex authentication through ChatGPT sign-in or workspace Codex access token.
- Uses Codex app-server JSON-RPC from Rust, with `codex exec` as a fallback path.
- No OpenAI Platform API key required.

Secondary local/internal provider:

- `ClaudeLocalProvider`
- Uses `claude -p` with local subscription OAuth credentials, or Agent SDK only for internal/local workflows where the user controls the environment.
- Must strip `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` from the subprocess environment unless the user explicitly selects API billing.
- For distributed product use that exposes Claude subscription login to end users, provider approval is required before shipping.

Existing provider:

- `AnthropicApiProvider`
- Keep the current API-key-backed `AnthropicLlm` unchanged for users who explicitly want API billing.

## Goals

1. Let a local user run tabular insight extraction through an existing Codex or Claude subscription.
2. Avoid API keys in the default local-subscription path.
3. Keep the `LlmClient` boundary stable.
4. Preserve pseudonymized-only LLM routing.
5. Return strict structured JSON compatible with existing cell parsing and offset verification.
6. Audit which provider, binary, auth mode, schema, and prompt hash produced each result.
7. Keep provider failures isolated so local GLiNER2/LoRA extraction and manual review still work.

## Non-Goals

- Do not read or copy `~/.codex/auth.json`, Claude credentials, or OS keychain records.
- Do not proxy subscription credentials as an OpenAI-compatible or Anthropic-compatible API.
- Do not store provider access tokens in Hacienda.
- Do not bypass provider terms by emulating private APIs.
- Do not route raw source documents to local subscription providers; use pseudonymized chunks only.
- Do not make Claude Agent SDK a shipped end-user product path without provider approval.

## Provider Architecture

```text
anno-rag-tabular
  extract::Extractor
    llm::LlmClient
      AnthropicLlm
      CodexLocalLlm
      ClaudeLocalLlm
      MockLlm
```

Add a local provider support module:

```text
crates/anno-rag-tabular/src/llm/
  mod.rs
  anthropic.rs
  mock.rs
  local_cli.rs
  codex_local.rs
  claude_local.rs
```

`local_cli.rs` owns shared process and file handling:

- temporary working directory;
- schema file creation;
- prompt file creation when needed;
- stdout/stderr capture;
- timeout enforcement;
- environment allowlist and denylist;
- binary discovery;
- output JSON parsing;
- prompt and schema hashing for audit metadata.

## Codex Local Provider

### Preferred path: app-server JSON-RPC

Use `codex app-server` as a local subprocess with stdio transport. Rust can speak JSON-RPC directly, avoiding a Node sidecar.

High-level flow:

```text
CodexLocalLlm::generate_structured()
  -> start or reuse local codex app-server
  -> thread/start with selected model
  -> turn/run with prompt and structured-output instruction
  -> collect final_response
  -> parse JSON object
  -> return StructuredOutput
```

This is the cleanest path for a Rust crate because it avoids embedding TypeScript while still using the local Codex runtime and saved ChatGPT authentication.

### Fallback path: `codex exec`

Use the documented non-interactive CLI path when app-server control is not available:

```text
codex exec --ephemeral --output-schema schema.json -o result.json < prompt.txt
```

Feed the full prompt on stdin. Read `result.json` as the structured final answer.

Use `--ephemeral` to avoid persistent session files for extraction runs. If the installed Codex CLI requires a Git repository check, pass the documented skip flag for that version because legal document extraction may run outside a Git repository. Keep the sandbox read-only unless a future workflow explicitly needs tools, which v1 does not.

### Auth handling

Default to local ChatGPT/Codex auth. Do not set `OPENAI_API_KEY` or `CODEX_API_KEY`.

If the user has `OPENAI_API_KEY` or `CODEX_API_KEY` set globally, strip it from the provider subprocess environment in subscription mode. Add an explicit `api_key` mode later if users need API billing.

Credential modes:

```text
subscription_local
  use existing Codex CLI/app auth
  no API key variables forwarded

workspace_access_token
  optional Business/Enterprise trusted automation
  token supplied by user or secret manager
  not stored by Hacienda

api_key
  explicit opt-in only
  out of scope for this design because current goal is no API key
```

## Claude Local Provider

Use Claude only as a local/internal provider in the first implementation.

Preferred command:

```text
claude -p --output-format json --json-schema schema.json "..."
```

For large prompts, use stdin or a prompt file wrapper instead of putting the full document text in shell arguments.

Environment policy in `subscription_local` mode:

- remove `ANTHROPIC_API_KEY`;
- remove `ANTHROPIC_AUTH_TOKEN`;
- do not pass `ANTHROPIC_BASE_URL`;
- allow normal Claude Code config directories so local OAuth login can be used;
- fail with a clear diagnostic if the CLI reports that no subscription login is active.

Avoid `--bare` for the first implementation until local OAuth and subscription behavior are live-tested with the installed Claude Code version. For deterministic extraction, prefer normal print mode with restricted tools rather than relying on shell-level credential details.

For distributed product builds, hide or mark Claude local provider as experimental/internal until provider approval is resolved.

## Prompt Contract

The local subscription providers must receive the same effective extraction contract as `AnthropicLlm`:

- system playbook;
- user prompt containing chunk markers;
- JSON Schema for exact cell envelope shape;
- instruction to omit unsupported fields;
- citation rules using chunk IDs and byte offsets.

The provider output must be a JSON object shaped like current `json_schema::for_columns(columns)` output.

Every emitted cell still goes through:

```text
parse_cell_envelope
  -> verify_cell_offsets
  -> support scoring
  -> immutable cell storage
```

This means provider output is never trusted just because it came from Codex or Claude.

## Insight Extraction Mode

Add a separate higher-level operation for insights so the same providers can be used beyond strict cells:

```text
InsightRequest {
  review_id,
  doc_id,
  chunks,
  insight_schema,
  instructions,
  provider_policy,
}
```

Initial insight families:

- risks and unusual clauses;
- missing information;
- contradictions across cells;
- suggested follow-up questions;
- summary of validated table findings.

Insight outputs must still cite chunks. If an insight is not source-backed, it must be marked as hypothesis or omitted.

## Privacy Boundary

Only pseudonymized chunk text may cross into local subscription providers.

Before calling any provider:

1. Construct prompt from `ChunkRef.content`, not raw source text.
2. Run a local PII guard over the prompt.
3. Abort if obvious clear PII remains.
4. Strip source file paths that may contain client names.
5. Include stable document and chunk IDs, not raw paths.

After provider output:

1. Parse JSON.
2. Verify citations against pseudonymized chunk text.
3. Store provider metadata.
4. Do not rehydrate anything automatically.

## Provider Selection Policy

Recommended default routing:

```text
LocalSpan / LocalClause / LocalClassifier
  -> GLiNER2/LoRA local extraction first

LlmRequired / insight extraction
  -> CodexLocalProvider if available
  -> ClaudeLocalProvider only if local/internal mode enabled
  -> AnthropicApiProvider only if user opted into API key billing
  -> manual review otherwise
```

`CodexLocalProvider` should be the default subscription provider once implemented because the official Codex SDK/app-server path is aligned with local programmatic control.

## Configuration

Example TOML:

```toml
[tabular.llm]
provider = "codex-local"
auth_mode = "subscription_local"
timeout_seconds = 180
max_prompt_bytes = 800000
persist_provider_sessions = false

[tabular.llm.codex]
mode = "app_server"
model = "auto"
binary = "codex"

[tabular.llm.claude]
enabled = false
mode = "cli"
model = "sonnet"
binary = "claude"
local_internal_only = true
```

`default_from_env()` should not silently choose local subscription providers. It may keep the current Anthropic API behaviour for backward compatibility. A new resolver should be explicit:

```rust
pub fn from_config(cfg: &LlmProviderConfig) -> Result<Box<dyn LlmClient>>;
```

## Audit Metadata

Store at least:

- provider kind: `codex-local`, `claude-local`, `anthropic-api`;
- provider mode: `app_server`, `exec`, `cli`, `sdk`;
- auth mode: `subscription_local`, `workspace_access_token`, `api_key`;
- model requested;
- CLI binary path;
- CLI version output when available;
- prompt hash;
- schema hash;
- prompt byte length;
- timeout;
- exit status;
- usage if provider exposes it.

Do not store prompts by default. Debug prompt capture should be explicit, local-only, and scrubbed.

## Testing Strategy

Unit tests:

- command construction strips API key environment variables in subscription mode;
- schema and prompt files are written to temp dirs and cleaned up;
- output parser accepts final JSON and rejects markdown-wrapped JSON;
- timeout handling kills the subprocess;
- diagnostics distinguish missing binary, missing login, schema failure, and provider refusal.

Integration tests:

- fake `codex` executable returning a valid structured JSON file;
- fake `claude` executable returning Claude-style JSON output;
- fixture prompt with pseudonymized chunks only;
- PII guard aborts on obvious clear PII before provider invocation;
- existing `Extractor` parses and verifies cells from `CodexLocalLlm`.

Live ignored tests:

- `codex_local_live.rs`, requiring installed and logged-in Codex CLI.
- `claude_local_live.rs`, requiring installed and logged-in Claude Code CLI and explicit opt-in.

Security tests:

- no `OPENAI_API_KEY`, `CODEX_API_KEY`, `ANTHROPIC_API_KEY`, or `ANTHROPIC_AUTH_TOKEN` are forwarded in subscription mode;
- raw source path strings are not present in provider prompt fixtures;
- failed provider output does not create cells.

## Implementation Phases

### Phase 1 - Codex exec fallback

- Add `local_cli.rs`.
- Add `CodexLocalLlm` using `codex exec`.
- Add config structs.
- Add fake binary tests.
- Add one ignored live test.

### Phase 2 - Codex app-server

- Add a persistent app-server JSON-RPC client.
- Reuse sessions only when explicitly configured.
- Capture usage and richer events when available.
- Keep `codex exec` as fallback.

### Phase 3 - Insight extraction

- Add `InsightRequest`.
- Add strict insight schemas.
- Route only pseudonymized chunks.
- Store cited insights separately from table cells.

### Phase 4 - Claude local provider

- Add `ClaudeLocalLlm` via `claude -p`.
- Strip API-key env vars in subscription mode.
- Mark provider experimental/internal in config.
- Add live ignored tests.

### Phase 5 - UI and product gating

- Expose "Use local Codex subscription" as the recommended subscription provider.
- Expose Claude local mode only behind an advanced/internal setting.
- Show diagnostics for login state and provider availability.

## Risks

| Risk | Mitigation |
|---|---|
| Provider terms change | Keep provider modules isolated and documented; default to Codex local where docs align best |
| CLI output shape changes | Parse only structured files or JSON fields; add version checks and tests |
| Subscription usage limits are exhausted | Surface provider error and route to manual review or another configured provider |
| Prompt leaks PII | Use pseudonymized chunks, preflight PII guard, and path stripping |
| CLIs run tools unexpectedly | Use schema-only prompt, read-only sandbox where available, temp working dirs, and no write permissions beyond temp output |
| Long documents exceed CLI context | Keep existing batching and route only required columns/chunks |
| Rust integration with TypeScript SDK adds packaging weight | Prefer JSON-RPC app-server for Rust; keep TS SDK as reference/future sidecar |

## Success Criteria

- `anno-rag-tabular` can run a structured extraction through local Codex authentication without `OPENAI_API_KEY` or `CODEX_API_KEY`.
- API key environment variables are not forwarded in subscription mode.
- Provider prompts contain pseudonymized chunks only.
- Provider JSON output is parsed into existing `Cell` values and verified by existing citation checks.
- A missing login or exhausted subscription produces a clear recoverable error.
- Existing Anthropic API extraction remains backward compatible.
- Claude local mode is available only as an explicit local/internal provider until product approval questions are resolved.
