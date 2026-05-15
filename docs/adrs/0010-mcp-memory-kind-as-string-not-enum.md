# ADR-010 — MCP `memory_*` tools take `kind` as a String, not the typed enum

**Status:** Accepted (v0.1 memory) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

The four new MCP tools — `memory_save` / `memory_recall` / `memory_forget` / `memory_list` — have parameter types that derive `schemars::JsonSchema` for rmcp's `#[tool]` macro. The natural typing for `kind` is `MemoryKind` (the enum defined in `memory.rs`, lowercase JSON via `#[serde(rename_all = "lowercase")]`).

When compiled, this fails: `Option<MemoryKind>: JsonSchema` is not satisfied. Reason: `memory.rs` derives the workspace's `schemars::JsonSchema` (one version), but rmcp 1.6 internally uses its own `rmcp::schemars::JsonSchema` (a different version path in the type system). The two are distinct traits even when the code is identical.

Three ways to handle:

1. **Double-derive on `MemoryKind`** — derive both schemars paths. Requires importing `rmcp::schemars` into `memory.rs`, polluting the type's dependencies and tying `memory.rs` to the rmcp version forever.
2. **Define a wrapper enum in `mcp.rs`** that mirrors `MemoryKind` and derives the rmcp schemars. Doubles the surface; every change to `MemoryKind` requires updating two places.
3. **Use `String` in the MCP param types**, parse to `MemoryKind` via a small `parse_kind` helper at handler entry. Looser typing at the API boundary; trivial conversion.

The Cowork / Claude Desktop client renders JSON Schema enums as a dropdown, which is mildly nicer UX than a free-text field. But option 3 lets the client send any of `"fact"` / `"preference"` / `"reference"` / `"context"` (documented in the tool description) without forcing the schema gymnastics.

## Decision

**Option 3 — String at the MCP boundary.** `MemorySaveParams::kind: Option<String>`, `MemoryRecallParams::kinds: Option<Vec<String>>`, `MemoryListParams::kind: Option<String>`. Handlers call `parse_kind(&str) -> Option<MemoryKind>` to convert; unknown strings yield `None` (treated as "no filter") rather than an error, because that's the more forgiving UX for an MCP tool the user is invoking by hand.

## Consequences

- `memory.rs` stays decoupled from rmcp; its `MemoryKind` derives only the workspace schemars.
- The MCP tool description documents the legal string values; client-side validation is the client's responsibility. Cowork can map the strings to its own UI.
- A typo in `kind` from the client falls back to "no filter" rather than failing the call. Trade-off: less strict input validation in exchange for a more usable MCP surface. Documented in the v0.1 plan T10.
- If rmcp's schemars dependency ever aligns with the workspace's, this ADR can be revisited — the change would be additive (add the enum back, keep parse_kind for compatibility).
- The same pattern (string at the MCP boundary, typed inside the Pipeline) is the recommended approach for any future enum-shaped parameter in this crate.

## Reference

`crates/anno-rag/src/mcp.rs::MemorySaveParams`, `::MemoryRecallParams`, `::MemoryListParams`, `::parse_kind`, commit `3cdc4105`.
