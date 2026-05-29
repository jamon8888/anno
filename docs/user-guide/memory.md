# Memory User Guide

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Compliance
Language: Bilingual

Hacienda memory stores durable facts, preferences, references, and working
context for recall in later sessions. In v0.11.0-rc.11, privacy behavior depends
on the configured memory save path, so treat memory as durable local state and
avoid saving sensitive cleartext unless the deployment has validated the
synchronous tokenizing path.

La memoire sert a conserver des informations utiles entre sessions, tout en
gardant les donnees personnelles sous controle du vault local.

## Memory Types

| Type | Use it for | Example |
|---|---|---|
| Fact | Stable factual claims that should be recalled later. | "The matter concerns a commercial lease dispute." |
| Preference | User or team preferences. | "The user prefers concise tables before narrative analysis." |
| Reference | Pointers to canonical entities or recurring matters. | "Matter ACME-2026 is the reference dossier for this client." |
| Context | Temporary session or task context. | "Current task: compare termination clauses in batch 42." |

## Privacy Behavior

When `memory_save` uses the default async path, Hacienda stores the submitted
text immediately and enriches NER/token references later. That path may persist
raw text in the memory table. The synchronous tokenizing path detects PII
locally, stores a tokenized memory body, records token references, and keeps the
reversible mapping in the vault.

Operational guidance: use memory for durable facts, preferences, references,
and workflow context. Do not store sensitive cleartext in memory unless the
deployment is configured to use the synchronous/tokenizing path and the operator
has validated that behavior in that installed build.

Recall tools may return rehydrated text to the trusted local caller. Treat that
as a local privileged operation, similar to `rehydrate` for search results.

Do not use memory as the system of record for legal evidence, audit-critical
decisions, client instructions, or formal approvals. Keep those records in the
approved matter-management, DMS, or audit system.

## MCP Tools

| Tool | Purpose |
|---|---|
| `memory_save` | Save a fact, preference, reference, or context item. |
| `memory_recall` | Hybrid recall over stored memories. |
| `memory_graph_recall` | Expand recall through entity relationships. |
| `memory_invalidate` | Mark a memory invalid from a given time. |
| `memory_forget` | Delete memory rows and cascade vault erasure where safe. |
| `memory_list` | Page through stored memories by session or kind. |

## Good Use

Use memory for information that is helpful across sessions:

- stable user preferences;
- recurring matter context;
- durable facts that are not the legal evidence itself;
- internal workflow choices;
- references to where source material lives.

Avoid storing:

- secrets, passwords, API keys, or vault passphrases;
- final legal advice as the only retained source;
- unverified allegations without context;
- evidence that must be preserved with chain-of-custody requirements.

## Review And Forget

Use `memory_list` to inspect saved memories and `memory_forget` when a memory is
wrong, stale, or no longer allowed to be retained. Use `memory_invalidate` when
the old record should remain historically visible but no longer apply to current
recall.

## Related Links

- [MCP Tools](../developers/mcp-tools.md)
- [Privacy Model](../security-compliance/privacy-model.md)
- [File Layout](../reference/file-layout.md)
