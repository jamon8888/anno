# MCP Stabilization Fixes Design

## Context

The installed `anno-rag.exe mcp` was tested through JSON-RPC stdio with a temporary data directory and a non-destructive Piighost verification. The MCP surface exposes 47 tools and all tools respond, but the run found three correctness problems that can mislead Claude Desktop during client-folder work:

- `forget(target=path)` returns `ok: true` while skipping legal deletion if the pipeline has not already been initialized.
- `search(scope="legal")` defaults to fast mode and skips legal search, while `legal_search` returns relevant hits for the same corpus and query.
- `anno_init_vault` can return `ok: true` even though `anno-rag.exe vault status` in a later process still reports `keyring_entry_present=false`.

These are MCP reliability issues, not model-quality issues. They should be fixed before treating the MCP as production-ready for client-folder ingestion and targeted search in Claude Desktop.

## Goals

- Make MCP tools report truthful outcomes.
- Keep folder/corpus maintenance tools usable without loading GLiNER or embeddings unless strictly required.
- Make the unified `search` tool consistent with legacy `legal_search`.
- Make vault initialization/status use one shared persistence contract across CLI and MCP.
- Add a repeatable full MCP smoke harness that exercises all exposed tools safely.

## Non-Goals

- Do not redesign legal extraction quality or graph enrichment rules.
- Do not require a GPU build.
- Do not make tabular LLM extraction mandatory for tabular review CRUD/export.
- Do not delete generated `anon/` files from user folders; indexing already ignores them.

## Approach A: Immediate Tool Truthfulness

### Forget

Change `forget_impl_routing` so legal deletion is never silently skipped:

- For `target` matching `legal_folder_*`, call `self.pipeline().await` before resolving/deleting in the immediate patch.
- For path targets, always attempt both knowledge deletion and legal deletion.
- If legal deletion cannot run because the vault/pipeline cannot initialize, return `ok: false` with `errors=["legal forget: ..."]`.
- Preserve the existing corpus-id path, which already uses document ids and can initialize the pipeline when needed.

Acceptance:

- `forget(path)` on a folder with only legal chunks removes those chunks in a fresh MCP process.
- If legal deletion fails, `ok` is false and the removed counts remain accurate.

### Search

Change unified search mode resolution:

- If `scope="legal"` and `mode` is omitted, use `semantic`.
- If `scope="all"` and `mode` is omitted, use `fast` for knowledge and `semantic` for legal, then merge hits. This is the new `auto` behavior without requiring callers to specify it.
- If `mode="fast"` and `scope="legal"` is explicit, return `ok:false` with `error="legal scope requires semantic mode"` rather than silently skipping legal.
- If `mode="fast"` and `scope="all"` is explicit, keep knowledge fast search and include a warning that legal was skipped.

Acceptance:

- `search(scope="legal", query=Q, corpus_id=C)` returns legal hits when `legal_search(Q, corpus_id=C)` does.
- `search(scope="knowledge")` remains model-free fast FTS.
- `search(scope="all")` returns knowledge hits and legal hits when both exist.

### Corpus And Status

Fix simple false positives:

- `corpus_get` and `corpus_health` must check `corpus_exists`, not only UUID parseability.
- `status` should report legal chunk count without requiring the pipeline to already be initialized.
- `download_models` should validate expected model files or a manifest, not only folder existence.

Acceptance:

- Unknown corpus id returns an error.
- Fresh MCP `status` reports legal chunk count if chunks exist.
- Empty model folders do not return `already_present`.

## Approach B: MCP Stabilization Layer

### Vault Key Service

Introduce a shared `VaultKeyService` used by CLI vault commands, MCP `anno_init_vault`, MCP `anno_health`, and pipeline startup:

- `derive_key_for_runtime()`: current behavior, but with verified persistence.
- `status()`: returns key source, persistence health, and user-facing remediation.
- `initialize_from_passphrase(passphrase)`: writes key material and verifies it through the same status path.

Windows fallback:

- First try OS keyring.
- If write verification fails, use a DPAPI-protected current-user file under the Anno data/config directory.
- Never print the passphrase or raw key.
- `vault status` must see the same persisted key source in a fresh process.

Acceptance:

- `anno_init_vault` returns `ok:false` when persistence cannot be verified.
- After a successful init, `anno-rag.exe vault status` reports the key as present in a new process.
- Pipeline startup refuses to replace `vault.enc` on mismatch and gives an actionable error.

### Model Inventory Service

Introduce `ModelInventoryService` for `download_models`, `status`, and diagnostics:

- Validate E5 files and GLiNER ONNX/tokenizer files.
- Return `missing`, `partial`, `ready`, or `downloading`.
- Avoid any model load during status checks.

Acceptance:

- `download_models` with partial folders reports partial/missing, not ready.
- `status.models` can distinguish “downloaded” from “loaded”.

## Approach C: Controlled Service Refactor

Extract small non-model services to keep MCP maintenance operations independent from ML startup:

- `LegalMaintenanceService`
  - Opens legal LanceDB stores and graph/status stores.
  - Counts chunks.
  - Lists legal folder ids.
  - Deletes by path or document ids.
- `VaultKeyService`
  - Owns all key persistence/status/init.
- `ModelInventoryService`
  - Owns model file readiness.
- Extend existing `CorpusService`
  - `get`, `health`, binding/document counts, orphan diagnostics.

The MCP server keeps lazy `Pipeline` for operations that need detector/embedder/vault pseudonymization, but maintenance tools use lightweight services.

Acceptance:

- `status`, `sources`, `corpus_health`, and `forget` do not need to load GLiNER or embeddings.
- MCP startup remains cheap.
- All maintenance services have focused unit tests.

## Test Plan

Add a reusable MCP smoke harness:

- Launch installed or target `anno-rag.exe mcp` over stdio.
- Use a temporary `ANNO_RAG_DATA_DIR`.
- Use real downloaded models via `ANNO_MODELS_DIR`.
- Create a small corpus with knowledge files, legal text, generated `anon/` outputs, memory data, and tabular review data.
- Call all 47 exposed tools.
- Mark LLM-dependent extraction as `expected_error` when no LLM config exists, while requiring CRUD/export tools to pass.
- Verify final cleanup leaves `knowledge.objects=0` and `legal.chunks=0`.

Add targeted tests:

- `forget(path)` removes legal chunks in a fresh MCP server.
- `search(scope="legal")` matches `legal_search` hit availability.
- `corpus_get` unknown id fails.
- `status` legal count works before pipeline init.
- `download_models` rejects empty model folders.
- `anno_init_vault` fails if persistence cannot be verified.

## Rollout

1. Implement Approach A first.
2. Add the full MCP smoke harness and run it locally.
3. Implement `VaultKeyService` and `ModelInventoryService`.
4. Refactor `LegalMaintenanceService` only after the targeted fixes are green.

This order fixes user-visible false outcomes quickly while keeping the larger architecture changes bounded and testable.
