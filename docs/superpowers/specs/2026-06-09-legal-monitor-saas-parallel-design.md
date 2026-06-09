# Legal Monitor SaaS Parallel Design

**Status:** Draft for user review  
**Date:** 2026-06-09  
**Scope:** Separate SaaS product for EU legal monitoring using Parallel APIs, with MCP and webhook/API delivery  
**Out of scope:** Anno anonymization, client dashboard, Stripe billing, full enterprise SSO, and local document privacy workflows

## Official References Checked

Official Parallel pages checked on 2026-06-09:

- Account API: <https://docs.parallel.ai/integrations/account-api>
- Parallel CLI: <https://docs.parallel.ai/integrations/cli>
- Create App: <https://docs.parallel.ai/service-api/apps/create-app>
- Create Key: <https://docs.parallel.ai/service-api/keys/create-key>
- Get Balance: <https://docs.parallel.ai/service-api/balance/get-balance>
- Source Policy: <https://docs.parallel.ai/resources/source-policy>
- Create Monitor: <https://docs.parallel.ai/api-reference/monitor/create-monitor>

The design uses Account API for organization-level provisioning, Parallel API
keys for product calls, Monitor API for scheduled web monitoring, and CLI
semantics as an integration parity guide. The CLI is not a runtime dependency of
the SaaS backend.

## Goal

Build a separate legal monitoring product that lets customers create and manage
targeted legal/regulatory watches for Europe, receive alerts through Claude/MCP
and webhook/API, and enforce budget limits per customer.

The product is not an anonymization or privacy gateway in the first version.
Legal queries and alerts are still sensitive business data, so logs and audit
records must be content-light by default.

## Product Decision

Use a hybrid SaaS architecture:

```text
Claude clients / customer systems
  | MCP tools, REST API, webhook delivery
  v
Legal Monitor API (Rust / Axum)
  | tenants, monitors, source policies, budgets, usage ledger
  v
Postgres + durable run queue
  | scheduler
  +--> shared EU workers
  +--> optional dedicated customer workers
  v
Parallel APIs
```

The central control plane remains the source of truth for tenants, API tokens,
Parallel app/key metadata, source policies, monitors, usage, budgets, and alert
state. Workers execute Parallel calls. A tenant can start on shared EU workers
and later move to a dedicated worker pool without changing MCP tools or public
API contracts.

## Parallel Validation

The Account API documentation confirms a device-based OAuth flow that returns an
access token used as `Authorization: Bearer <access_token>` for Account API
requests. The product should implement or operationalize that flow for the
operator organization, persist refresh credentials securely, and use the bearer
token only for service provisioning.

The Create App and Create Key references confirm that the Account API can create
apps under the authenticated organization and create API keys under an app. The
recommended tenant isolation model is therefore one Parallel app per tenant and
one or more API keys per tenant app. Raw/display key material must be treated as
a secret: store it in a secret manager or envelope-encrypted key store, never in
logs, and support rotation/revocation.

The Get Balance reference is organization-level. It exposes organization balance
and pending debit state, not a tenant/app-level bill of materials. The SaaS must
therefore maintain its own append-only usage ledger per tenant, monitor, run,
tool, processor, and period. Parallel balance can be used for operator-level
credit/postpaid health, not for customer billing explanation.

The CLI confirms the developer-facing surfaces this product should cover:
Search, Extract, Research, Enrich, FindAll, and Monitor. For the MVP, Monitor is
primary. Search, Extract, Research, Enrich, and FindAll are support primitives
for alert explanation, manual follow-up, and later batch workflows.

The CLI also confirms useful operational semantics: JSON output for automation,
non-interactive commands, Monitor create/list/get/update/delete/events/simulate,
and asynchronous `run -> status -> poll` flows for long Research, Enrich, and
FindAll jobs. The backend should mirror this as durable jobs with run IDs rather
than blocking MCP tool calls.

The Source Policy docs confirm `include_domains`, `exclude_domains`, and an
`after_date` freshness control, with a combined 200-domain limit for
include/exclude domains. The product must validate source policies before
submitting Parallel requests and keep official packs and customer allowlists
small enough to fit this limit.

The Monitor API confirms `event_stream` monitors for search-query change
detection, `snapshot` monitors for task-output monitoring, frequency values from
1 hour to 30 days, processors `lite` and `base`, webhook configuration, metadata
echoing, and source policy under monitor advanced settings. The MVP should use
`event_stream` by default and store product IDs in Parallel monitor metadata
where possible.

## MVP Scope

MVP workflows:

1. Create a legal monitor from MCP or REST.
2. Receive and consult alerts through MCP and customer webhooks.
3. Track usage and budgets with progressive alerts and hard limits.
4. Administer tenants, Parallel keys, source policies, and worker mode.

Deferred workflows:

- Client dashboard and email digest.
- Full billing/subscription engine.
- Anonymization.
- Enterprise SSO beyond a simple OIDC/JWT-compatible boundary.
- Large batch analysis as a first-class customer workflow.

## Source Policy Model

Every monitor has a source mode:

- `official_strict`: default. Uses curated official EU/legal domains only.
- `official_plus_allowlist`: official pack plus tenant-approved domains.
- `controlled_web`: broader web research with explicit exclusions and higher
  review/cost policy. Available later, not the default.

Initial official pack:

- `eur-lex.europa.eu`
- `curia.europa.eu`
- `legifrance.gouv.fr`
- `cnil.fr`
- `edpb.europa.eu`
- `esma.europa.eu`
- `eba.europa.eu`
- `europarl.europa.eu`
- `ec.europa.eu`
- `euipo.europa.eu`

The source policy compiler validates apex-domain shape, removes duplicates,
enforces the 200-domain Parallel limit, and records the exact compiled policy on
each monitor run for auditability.

## Components

### API Gateway

Rust/Axum HTTP API for REST, admin, and MCP-facade calls. It authenticates the
caller, extracts tenant context, enforces RBAC, validates request bodies, and
emits structured audit and metrics events.

### Tenant Service

Owns tenants, workspaces, users, API tokens, RBAC roles, default worker mode,
quotas, budget policies, source policy defaults, and tenant lifecycle state.

### Parallel Key Service

Uses the Account API to create one Parallel app per tenant and one or more API
keys under that app. Stores key references securely, supports rotation, and
returns only metadata to other services. It distinguishes Account API bearer
credentials from tenant Parallel API keys.

### Monitor Service

Creates, updates, pauses, resumes, and cancels product monitors. It compiles
legal source policies, maps product monitors to Parallel monitor IDs, stores
metadata, and exposes alert/runs views.

### Scheduler And Job Queue

Maintains durable `monitor_runs` and background jobs. For the MVP, Postgres can
serve as the durable queue with row locking, `locked_by`, `locked_until`,
`attempt_count`, and backoff timestamps. Redis, NATS, or SQS can replace the
dispatcher later without changing public APIs.

### Worker Runtime

Executes Parallel calls with a shared `reqwest::Client`, bounded concurrency,
timeouts, retries, and per-tenant rate limits. It normalizes Parallel events into
product alerts and usage ledger entries.

Shared workers poll jobs eligible for `execution_mode = shared`. Dedicated
workers authenticate to the control plane with a `worker_id` and only receive
jobs for assigned tenants. Dedicated workers should not need direct access to
the central Postgres database.

### Budget Service

Tracks estimated and reconciled usage per tenant, monitor, tool, processor, and
period. It emits warnings at configured thresholds and enforces hard policies at
the limit.

Default thresholds:

- 50 percent: warning event.
- 80 percent: high warning event.
- 100 percent: hard policy enforcement.

Hard policy options:

- `pause_monitors`
- `block_expensive_processors`
- `manual_approval_required`

### Notification Service

Delivers customer webhooks with HMAC signatures, timestamp headers, retries,
backoff, and a dead-letter state. It should support idempotent redelivery and
record every attempt.

### MCP Facade

A thin MCP server that calls the REST API. It does not own business state. MVP
tools:

- `create_legal_monitor`
- `list_legal_monitors`
- `get_legal_alerts`
- `get_legal_alert_detail`
- `acknowledge_legal_alert`
- `trigger_legal_monitor_check`
- `get_usage_budget`

MCP tools are user-facing, not admin-facing. Admin provisioning remains REST or
internal CLI.

## Data Model

Core tables:

- `tenants`: customer identity, plan, status, region, default execution mode,
  budget policy.
- `users`: human users and roles.
- `api_tokens`: scoped REST/MCP/integration tokens.
- `parallel_apps`: tenant to Parallel app mapping, app status, key metadata, and
  secret reference.
- `source_policies`: official packs, tenant allowlists, exceptions, and compiled
  policy versions.
- `monitors`: product monitor definition, source mode, cadence, processor,
  budget policy, status, Parallel monitor ID.
- `monitor_runs`: scheduled/manual executions, lock state, attempt count, status,
  duration, processor, compiled source policy, Parallel IDs, and errors.
- `alerts`: normalized detected events, citations, source URLs, relevance score,
  lifecycle state, and related monitor/run IDs.
- `usage_ledger`: append-only usage/cost entries per tenant, monitor, run, tool,
  processor, and billing period.
- `webhook_endpoints`: customer endpoint config, secret reference, event filters,
  status.
- `webhook_deliveries`: payload reference, signature metadata, attempt status,
  HTTP result, retry schedule, and dead-letter state.
- `audit_events`: content-light admin/user actions and security-relevant events.

## Primary Flow

```text
1. Customer creates a monitor through MCP or REST.
2. API validates tenant, RBAC, cadence, budget policy, and source policy.
3. Monitor Service creates the product monitor and, when appropriate, a Parallel monitor.
4. Scheduler creates or imports monitor runs.
5. Worker checks budget and rate limits before each external call.
6. Worker calls Parallel using the tenant API key.
7. Worker normalizes detected events into alerts.
8. Usage Ledger records estimated cost and later reconciliation data if available.
9. Budget Service emits warnings or enforces limits.
10. Notification Service sends signed customer webhooks.
11. MCP reads alerts, monitor status, and remaining budget from the API.
```

The product keeps its own run and alert state even when using Parallel Monitor.
Parallel is the research/monitoring engine; the SaaS remains the customer-facing
source of truth for status, budgets, retries, and audit.

## REST API

Customer routes:

```text
POST   /v1/monitors
GET    /v1/monitors
GET    /v1/monitors/{id}
PATCH  /v1/monitors/{id}
POST   /v1/monitors/{id}/trigger
GET    /v1/monitors/{id}/runs
GET    /v1/alerts
GET    /v1/alerts/{id}
PATCH  /v1/alerts/{id}
GET    /v1/usage/summary
GET    /v1/budgets
PATCH  /v1/budgets/{id}
POST   /v1/webhook-endpoints
GET    /v1/webhook-endpoints
PATCH  /v1/webhook-endpoints/{id}
```

Admin routes:

```text
POST   /admin/tenants
PATCH  /admin/tenants/{id}
POST   /admin/tenants/{id}/parallel-app
POST   /admin/tenants/{id}/parallel-keys/rotate
POST   /admin/tenants/{id}/api-tokens
GET    /admin/usage
GET    /admin/job-health
GET    /admin/parallel-balance
```

All mutation routes use idempotency keys. API responses use a consistent
envelope with `data`, `error`, and pagination metadata where relevant.

## Performance And Reliability

Implementation defaults:

- Rust `axum` and `tokio`.
- Postgres with `sqlx`.
- Shared `reqwest::Client` per process.
- `tracing` for structured logs.
- Prometheus-compatible metrics.
- Postgres-backed durable queue for MVP.
- Per-tenant and global concurrency limits.
- Endpoint-specific timeouts.
- Retry with exponential backoff and jitter for network and 5xx failures.
- No retry for validation, auth, or hard budget failures.
- Circuit breaker by tenant and external provider.
- Short cache for alert detail reads, never as the source of truth.

The worker must check budget before submitting a Parallel run and again before
costly follow-up enrichment. If the tenant is over budget, the job is skipped or
blocked according to policy and the monitor state explains why.

## Security And Compliance

Security baseline:

- OIDC/JWT-compatible auth for human users.
- Scoped API tokens for MCP and integrations.
- RBAC roles: `admin`, `operator`, `viewer`, `integration`.
- Tenant isolation on every query and job.
- Parallel keys stored in a secret manager or envelope-encrypted store.
- HMAC-signed customer webhooks with timestamp and replay window.
- Content-light logs and audits.
- Explicit source allowlists.
- Key rotation and revocation workflows.

EU operational baseline:

- Control plane hosted in the EU.
- Clear separation between shared and dedicated workers.
- Configurable data retention for alerts, runs, and webhook payloads.
- Tenant export and tenant deletion workflows.
- Review Parallel contractual terms, DPA, subprocessors, and billing model before
  commercial production.

## Testing Strategy

Required tests:

- Unit tests for budget policy, source policy compilation, RBAC, webhook
  signatures, and idempotency.
- Integration tests for REST routes against a Postgres test database.
- Fake Parallel server covering Monitor, Search, Task, Account API provisioning,
  401, 422, 429, 5xx, and timeout behavior.
- Worker tests for locking, retries, backoff, circuit breaker, budget blocking,
  and deduplication.
- MCP contract tests confirming tools call the API and do not own state.
- Smoke test: create tenant, provision Parallel app/key metadata through fake
  Account API, create monitor, trigger run, ingest alert, deliver webhook, and
  verify usage ledger.

## Success Criteria

The MVP is successful when:

- A tenant can create an official EU legal monitor from Claude/MCP.
- The system produces a usable alert with source references.
- The same alert can be delivered through a signed webhook.
- Usage is attributed to tenant, monitor, run, tool, and period.
- Budget thresholds generate events and hard limits stop or downgrade work.
- An operator can explain which monitor consumed which budget and why.

## Known Assumptions

- Parallel does not expose tenant/app-level detailed billing in the checked
  Account API docs. The product ledger is required.
- The runtime backend should call Parallel HTTP APIs or SDK equivalents directly,
  not shell out to `parallel-cli`.
- `official_strict` is intentionally conservative. More domains can be added
  through versioned source packs after evaluating quality and noise.
- Dedicated workers are a deployment mode of the same product, not a separate
  codebase.
