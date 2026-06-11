# Anno Control — Tauri Companion Control Plane

Date: 2026-06-11
Status: Design approved, pending user spec review

## Context

Anno is a local, privacy-by-design legal RAG engine exposed to Claude Desktop as an
MCP server over stdio. Claude Desktop owns the conversational UI and spawns the
`anno-rag` MCP process. There is today no surface for everything that happens *around*
the chat: turning the engine on and off, controlling the encrypted vault, proving the
data never left the machine, managing indexed folders and models, editing
configuration, and producing the regulatory evidence a European legal practice needs.

An earlier design (`2026-05-28-hacienda-tauri-client-workbench-design.md`) proposed a
full standalone Tauri workbench that *replaced* the chat with its own document editor.
This design takes a different, narrower position for v1: **Claude Desktop stays the UI;
the Tauri app is a companion control plane.** The earlier workbench is not discarded —
the boundaries here are drawn so document-editing/workflow screens can be added later
without re-architecting.

The existing `apps/hacienda-workbench/` is a stale `dist/` only (no source, no
`src-tauri`), so this is effectively a greenfield Tauri v2 app.

## Product Goal

Build a Windows-first (macOS second) desktop companion app for lawyers, DPOs, and legal
operations teams that, alongside Claude Desktop, can:

- wire Anno into Claude Desktop and verify the connection (onboarding);
- turn the engine on and off with an instant, audited, provable kill-switch;
- control the encrypted vault (lock/unlock, keyring binding, backup);
- prove a local-only posture ("nothing left this machine");
- manage indexed folders/corpora and models;
- edit configuration through a UI instead of TOML;
- show a tamper-evident audit trail and export an inspection pack;
- generate RGPD and EU AI Act evidence from that audit trail.

The app leads with **two co-equal pillars**: a **trust & compliance cockpit** and an
**operational control panel**, sharing the same engine/vault/audit plumbing.

## Non-Goals (v1)

- No replacement of Claude Desktop's chat; the Tauri app is not the conversational UI.
- No always-on daemon or local HTTP server.
- No document editor or workflow execution screens (deferred to the workbench phase).
- No tax/accounting features (only the seam that will later receive them).
- No modification of original source documents.

## Architecture

### Control model — state-based, no daemon

Claude Desktop spawns and owns the `anno-rag` MCP process. The Tauri app does **not**
attach to that process. Instead, both sides communicate through **shared on-disk
state**, and the Tauri app issues short-lived CLI calls for live readings.

```text
Claude Desktop (chat UI) ──stdio MCP──> anno-rag (engine, spawned by Claude Desktop)
                                                │
                        shared on-disk state ───┤  config.toml, vault.enc,
                                                │  audit log (hash-chained),
                                                │  corpus manifest, engine-state.json
                                                │
Tauri app (control plane) ──reads state + short-lived CLI calls (anno_health,
        │                    vault_stats)───────┘
        └── crates/anno-control-core (Rust): the only code that touches engine state;
            Tauri commands are thin wrappers over it, unit-testable without a UI.
```

Rationale: you cannot cleanly attach to another app's child process; the vault and
LanceDB index dislike concurrent writers; and avoiding a listener removes a class of
port/firewall/antivirus support problems and shrinks the attack surface (itself a
compliance win). Monitoring is therefore **poll-based**, which is sufficient for a
control plane. If the later workbench phase needs live progress streaming, a daemon can
be introduced behind the same Tauri command boundary without changing the UI.

### Co-design with the engine — not a frontend skin

The compliance features cannot be faked by a UI reading files. The engine must *produce
provable guarantees*; the Tauri app *views and controls* them. This work splits in two
and must be designed together:

| Feature | Engine-side work required |
|---|---|
| Panic kill-switch | Check `engine-state.json` per tool-call; drop vault key when OFF |
| Sovereignty proof | Expose an egress ledger (or compile-time no-network guarantee) |
| Tamper-evident audit | Emit hash-chained audit events for detect/rehydrate/erase/export |
| RGPD Art.17 erasure cert | `memory_forget` + vault purge emit a verifiable receipt |
| AI Act human-oversight log | Record who validated which AI-proposed field |
| Model provenance | Stamp outputs with model/adapter/version/thresholds |

### Components

- `apps/anno-control/` — Tauri v2 app (React frontend + Rust `src-tauri`).
- `crates/anno-control-core/` — Rust library: engine-state file, audit reader,
  vault control, corpus/model/config readers, CLI invokers, `RegulatoryProfile`.
  Tauri commands are thin wrappers; all logic and tests live here.
- Engine instrumentation in existing `anno-rag` crates (kill-switch enforcement,
  egress ledger, hash-chained audit, erasure receipts, oversight log, provenance).

## The On/Off Kill-Switch

"On/off" is the safety primitive: an instant, audited, provable cut of AI access to
client data — the secret-professionnel panic button.

### Mechanism — one owned, signed state file

```jsonc
// ~/.anno-rag/engine-state.json   (written only by anno-control-core)
{
  "enabled": false,
  "mode": "panic",            // "normal" | "maintenance" | "panic"
  "actor": "m.dupont",        // local OS user who flipped it
  "reason": "client meeting", // optional
  "ts": "2026-06-11T14:02:11Z",
  "sig": "…"                  // HMAC over the record, keyed to the vault/keyring
}
```

### Three enforcement points in the engine

1. **At MCP startup** — if `enabled:false`, tools register but every call returns a
   uniform `EngineDisabledByOperator` error; no client data is touched.
2. **Per tool-call** — re-check the file (stat + mtime cache) so flipping OFF takes
   effect without restarting Claude Desktop (the "instant" property).
3. **Vault coupling** — `panic` mode also drops the in-memory vault key, so already
   tokenized data cannot be rehydrated until a deliberate unlock.

A signed state file (not "kill the process") gives a deliberate, logged, attributable
action; the `sig` prevents a stray editor or file-sync tool from silently re-enabling
the engine. Every flip writes an audit event.

### Three states

- **Normal** — tools live, vault unlockable.
- **Maintenance** — tools refuse; vault unchanged (reindex/model updates without a
  privilege event).
- **Panic** — tools refuse and vault key dropped (the red button).

UI: one prominent green/amber/red toggle with current actor/timestamp and a
"last 5 state changes" strip from the audit log.

## Feature Areas

### Pillar A — Trust & compliance cockpit

1. **Local-only sovereignty proof** — a "nothing left this machine" badge backed by an
   egress monitor; an inventory of configured LLM providers flagging any cloud provider
   (e.g. tabular extraction subscription LLMs).
2. **Vault + panic kill-switch** — lock/unlock bound to OS keyring; the three-state
   kill-switch above; vault backup/restore.
3. **Tamper-evident audit + inspection export** — hash-chained, append-only log of every
   detect/rehydrate/erase/export, with a one-click export pack for CNIL, bâtonnier, or
   the Ordre.
4. **RGPD + AI Act evidence generator** — Art.15 access pack; Art.17 erasure with a
   verifiable certificate (via `memory_forget` + vault purge); Art.30 records of
   processing; DPIA status; Art.14 human-oversight log; model/version provenance views.

### Pillar B — Operational control panel

5. **Engine lifecycle/health** (baseline) — start/stop/restart, health, version.
6. **Claude Desktop wiring + onboarding** — register the MCP server in Claude Desktop's
   config, run the engine-compat check (`required_tools`, min version per
   `claude-for-legal/engine-compat.json`), verify the connection.
7. **Folder & corpus management** — indexed folders, add/remove sources, sync/ingestion
   progress, per-matter corpus scoping (ties to existing `mcp-folder-autosync` and
   corpus-scoping work).
8. **Model & resource manager** — downloaded models (embedder, NER, reranker, OCR),
   downloads/updates, disk usage, accelerator (CPU/GPU).
9. **Settings-as-UI** — a form rendered from `config-schema.json` (the config-management
   work already shipped) instead of hand-editing TOML.

## Frontend Stack

A control-plane dashboard with heavy tables, schema-driven forms, and poll-based data.

- **React 18 + Vite + TypeScript**.
- **Tailwind CSS + shadcn/ui** (Radix primitives) for the component kit.
- **TanStack Query** for poll-based reads from Tauri commands (health, vault stats,
  audit tail).
- **TanStack Table** for audit / PII-token / folder / model tables (virtualized).
- **react-hook-form + zod** for settings-as-UI, with the form schema derived from
  `config-schema.json`.
- **Recharts** for compliance charts (PII categories, confidence, ingestion progress).
- **lucide-react** icons; **sonner** (or shadcn toast) for notifications.
- Dark/light theme; the app opens on an operational dashboard, not a marketing screen.

The Tauri bridge: the React app calls Tauri commands only; all engine-state logic lives
in `anno-control-core` so the frontend stays a thin presentation layer.

## RegulatoryProfile Seam (tax/accounting later, no rewrite)

Everything industry-specific routes through one abstraction. v1 ships exactly one
profile; industry #2 becomes a signed content pack, not a code fork.

```text
RegulatoryProfile {
  id,                  // "legal-commercial"  (later: "tax-accounting")
  evidence_templates,  // which RGPD/AI-Act/sector packs are offered
  retention_rules,     // legal: matter-based │ tax: 10-year mandatory
  export_formats,      // legal: DOCX/audit pack │ tax: FEC, Factur-X
  extraction_adapters, // LoRA/labels per domain
  terminology,         // "matter/dossier" vs "exercice/écriture"
  sector_obligations   // legal: bâtonnier │ tax: LCB-FT/TRACFIN, Ordre
}
```

The kill-switch, vault, audit, and sovereignty proof are industry-agnostic plumbing:
secret professionnel comptable uses the same machinery as secret professionnel avocat.
Only evidence content and export formats differ.

**Deferred to a future sub-project + its own spec:** FEC (Fichier des Écritures
Comptables) exporter for DGFiP audits; Factur-X / e-invoicing (2026–2027 reform);
piste d'audit fiable reports; 10-year retention policy; LCB-FT/TRACFIN AML checklists.
These ship as signed resource packs, reusing the prior workbench design's pack
mechanism — no app rebuild.

**v1 does now (cheap):** introduce the seam with one `legal-commercial` profile; every
industry-specific path goes through it. **v1 defers (not retrofit):** all actual tax
features — just the empty socket they plug into.

## Phased Delivery

Ordered by dependency, not by pillar: vault and audit are load-bearing for the
compliance cockpit. Each phase is independently demoable to a client.

### Phase 1 — Safety spine (walking skeleton)

Tauri shell + `anno-control-core` + command boundary; Claude Desktop wiring/onboarding
(register MCP server, engine-compat check, verify connection); engine lifecycle/health/
version; the three-state kill-switch.
Ship value: "Install it, it wires Anno into Claude Desktop, and you get a panic button."

### Phase 2 — Audit core + vault control

Hash-chained append-only audit log (engine emits, UI tails); vault lock/unlock bound to
OS keyring; vault backup/restore.
Ship value: every action is logged and tamper-evident; vault is under operator control.
This is the foundation the evidence generator stands on.

### Phase 3 — Operational surfaces

Folder & corpus management; model & resource manager; settings-as-UI (rendered from
`config-schema.json`). Parallelizable; depend only on Phase 1 + the config work shipped.
Ship value: the app becomes the daily driver — no more TOML, no CLI.

### Phase 4 — Sovereignty proof

Egress monitor / "nothing left this machine" badge; LLM-provider inventory flagging
cloud providers; one-click inspection export of the audit pack (CNIL / bâtonnier /
Ordre).
Ship value: the headline trust artifact a European buyer wants to see.

### Phase 5 — RGPD + AI Act evidence generator

Art.15 access pack; Art.17 erasure with verifiable certificate; Art.30 records of
processing; DPIA status; Art.14 human-oversight log; model/version provenance views.
Composes audit + vault + provenance + egress, so it is deliberately last.
Ship value: turns logged truth into regulator-ready documents.

Trade-off: this is trust-spine-first, so operational ergonomics (Phase 3) land third,
not first. Phases 2 and 3 may swap if the app should feel useful-as-a-control-panel
sooner, at the cost of building the evidence generator on a less-mature audit layer.
Recommendation: keep audit early.

## Security & Privacy Requirements

- No silent network calls; egress is monitored and surfaced.
- Clear PII never stored outside the encrypted vault; rehydration is explicit and
  audited.
- The kill-switch state file is signed; OFF in panic mode drops the vault key.
- The audit log is append-only and hash-chained (tamper-evident).
- OS keyring holds the vault secret; no plaintext secrets on disk.
- Error messages and logs must not leak document text or PII (per repo privacy rules);
  inspection exports are scrubbed and explicit.
- Originals are never modified (carried from the workbench non-goals).

## Testing Strategy

Unit (`anno-control-core`):
- engine-state file read/write, signing, and per-call semantics;
- audit reader + hash-chain verification (detect tampering);
- vault lock/unlock and keyring binding;
- config-schema → form-schema derivation;
- `RegulatoryProfile` resolution with one profile.

Integration:
- onboarding registers the MCP server and the engine-compat check passes/fails
  correctly;
- kill-switch flip is honored by the engine without a Claude Desktop restart;
- folder add/scan reflects in the corpus manifest;
- erasure produces a verifiable certificate.

Security/privacy:
- OFF (panic) makes every tool refuse and rehydration impossible;
- audit log is append-only and tamper-detectable;
- sovereignty proof reflects real egress (fails closed if the ledger is unavailable);
- no PII or document text appears in logs or unscrubbed exports.

Packaging:
- Windows signed installer installs/launches/updates/uninstalls cleanly;
- first run works offline;
- macOS DMG launches after signing/notarization (second platform).

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Treated as a frontend skin, guarantees become theater | Co-design engine instrumentation per the table; test the guarantees, not the UI |
| Poll-based monitoring feels stale | Short intervals for health; event-driven file-watch on audit log + engine-state |
| Concurrent vault/index writers corrupt state | Tauri is reader + config writer; mutating actions go through the engine or run while idle |
| Kill-switch re-enabled by a sync tool | Signed state file; engine rejects unsigned/mismatched records |
| v1 scope (9 areas) overruns | Strict phase boundaries; each phase ships independently |
| Tax/accounting creeps into v1 | RegulatoryProfile seam with exactly one profile; tax features deferred to their own spec |
| Cloud LLM (tabular) undermines local-only claim | Provider inventory flags cloud providers explicitly; sovereignty badge reflects actual configuration |

## Success Criteria

- A user can install the app, wire Anno into Claude Desktop, and pass the engine-compat
  check.
- Flipping the kill-switch to panic instantly makes every Anno tool refuse and drops the
  vault key, with an audit event recorded.
- The audit log is hash-chained and an inspection pack can be exported.
- Folders, models, and configuration are manageable without touching TOML or the CLI.
- The sovereignty badge truthfully reflects egress and provider configuration.
- An Art.17 erasure produces a verifiable certificate.
- The `RegulatoryProfile` seam exists with one `legal-commercial` profile; no tax code
  is present, only the socket.
- The frontend is React + Tailwind + shadcn/ui with schema-driven settings and
  virtualized data tables.
