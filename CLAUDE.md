<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **anno** (19833 symbols, 46131 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## When Debugging

1. `gitnexus_query({query: "<error or symptom>"})` — find execution flows related to the issue
2. `gitnexus_context({name: "<suspect function>"})` — see all callers, callees, and process participation
3. `READ gitnexus://repo/anno/process/{processName}` — trace the full execution flow step by step
4. For regressions: `gitnexus_detect_changes({scope: "compare", base_ref: "main"})` — see what your branch changed

## When Refactoring

- **Renaming**: MUST use `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` first. Review the preview — graph edits are safe, text_search edits need manual review. Then run with `dry_run: false`.
- **Extracting/Splitting**: MUST run `gitnexus_context({name: "target"})` to see all incoming/outgoing refs, then `gitnexus_impact({target: "target", direction: "upstream"})` to find all external callers before moving code.
- After any refactor: run `gitnexus_detect_changes({scope: "all"})` to verify only expected files changed.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Tools Quick Reference

| Tool | When to use | Command |
|------|-------------|---------|
| `query` | Find code by concept | `gitnexus_query({query: "auth validation"})` |
| `context` | 360-degree view of one symbol | `gitnexus_context({name: "validateUser"})` |
| `impact` | Blast radius before editing | `gitnexus_impact({target: "X", direction: "upstream"})` |
| `detect_changes` | Pre-commit scope check | `gitnexus_detect_changes({scope: "staged"})` |
| `rename` | Safe multi-file rename | `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` |
| `cypher` | Custom graph queries | `gitnexus_cypher({query: "MATCH ..."})` |

## Impact Risk Levels

| Depth | Meaning | Action |
|-------|---------|--------|
| d=1 | WILL BREAK — direct callers/importers | MUST update these |
| d=2 | LIKELY AFFECTED — indirect deps | Should test |
| d=3 | MAY NEED TESTING — transitive | Test if critical path |

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/anno/context` | Codebase overview, check index freshness |
| `gitnexus://repo/anno/clusters` | All functional areas |
| `gitnexus://repo/anno/processes` | All execution flows |
| `gitnexus://repo/anno/process/{name}` | Step-by-step execution trace |

## Self-Check Before Finishing

Before completing any code modification task, verify:
1. `gitnexus_impact` was run for all modified symbols
2. No HIGH/CRITICAL risk warnings were ignored
3. `gitnexus_detect_changes()` confirms changes match expected scope
4. All d=1 (WILL BREAK) dependents were updated

## Keeping the Index Fresh

After committing code changes, the GitNexus index becomes stale. Re-run analyze to update it:

```bash
npx gitnexus analyze
```

If the index previously included embeddings, preserve them by adding `--embeddings`:

```bash
npx gitnexus analyze --embeddings
```

To check whether embeddings exist, inspect `.gitnexus/meta.json` — the `stats.embeddings` field shows the count (0 means no embeddings). **Running analyze without `--embeddings` will delete any previously generated embeddings.**

> Claude Code users: A PostToolUse hook handles this automatically after `git commit` and `git merge`.

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->

# Build Isolation — Règles strictes

## Worktree actif (2026-05-28)

```
C:/Users/you/.config/superpowers/worktrees/anno/anno-tabular-local-extraction
```

**Une seule worktree de dev active à la fois.** La worktree `main` (`C:/Users/you/anno`) sert uniquement à des tâches de maintenance rapide.

## NEVER

- **NEVER** lancer `cargo build`, `cargo check`, ou `scripts/loop.ps1` si un processus `cargo`/`rustc` tourne déjà. Vérifier d'abord :
  ```powershell
  Get-Process cargo,rustc -ErrorAction SilentlyContinue
  ```
- **NEVER** lancer un build dans plusieurs worktrees simultanément.
- **NEVER** lancer `cargo build --release`, `cargo build --workspace` ou `cargo build --profile dist` en local sans `CARGO_TARGET_DIR` isolé (voir `scripts/release/build-windows-local.ps1`).
- **NEVER** créer une nouvelle worktree sans supprimer d'abord la précédente (commit ou discard).
- **NEVER** laisser des worktrees dormantes. Supprimer dès la tâche terminée :
  ```powershell
  git worktree remove <path> --force
  ```

## ALWAYS

- Utiliser `scripts/loop.ps1` comme point d'entrée unique pour le dev loop — il contient la garde de concurrence et l'isolation `CARGO_TARGET_DIR`.
- Travailler dans la worktree dédiée à la tâche en cours, jamais directement dans `main`.
- Avant tout build manuel : `Get-Process cargo,rustc | Select Id,Name,StartTime`
- En cas de build bloqué depuis >10 min : `Get-Process cargo,rustc | Stop-Process -Force`

## Règle des worktrees

| État | Action |
|------|--------|
| Tâche terminée, changements committés | `git worktree remove <path>` |
| Tâche abandonnée | `git worktree remove <path> --force` |
| Tâche en pause (>24h) | Commit WIP puis supprimer la worktree |
| Nouvelle tâche | Créer une worktree fraîche via le workflow superpowers |

Maximum **2 worktrees actives** à tout moment : `main` + une de dev.

# Local Rust Debug Loop

**Target dir:** `E:\cargo-target` (external SSD). Set as User env var and in `.cargo/config.toml`.

## Fast loop — one crate (check + unit tests)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package <crate>
```

## Check only (no link, no tests)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package <crate> -Mode check -Profile dev-fast
```

## With extra features

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -Features gliner2
```

For targeted check across affected crates (auto-detects from git diff):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1
```

Add `-AllAffected` when changing a shared crate so dependents are checked too. Add `-Package <crate>` to target manually.

## NEVER (local dev)

- **NEVER** run `cargo test --workspace` locally — links ALL test binaries (hours).
- **NEVER** run `cargo nextest run --workspace` locally — same problem.
- **NEVER** run `cargo build --release`, `cargo build --workspace` or `cargo build --profile dist` in local without `CARGO_TARGET_DIR` isolé (voir `scripts/release/build-windows-local.ps1`).
- **NEVER** lancer `cargo build`, `cargo check`, ou `scripts/loop.ps1` si un processus `cargo`/`rustc` tourne déjà. Vérifier d'abord :
  ```powershell
  Get-Process cargo,rustc -ErrorAction SilentlyContinue
  ```
- **NEVER** lancer un build dans plusieurs worktrees simultanément.
- **NEVER** créer une nouvelle worktree sans supprimer d'abord la précédente (commit ou discard).
- **NEVER** laisser des worktrees dormantes. Supprimer dès la tâche terminée :
  ```powershell
  git worktree remove <path> --force
  ```

## Expected times (warm sccache, E: SSD)

| Command | Time |
|---------|------|
| `test-local.ps1 -Package anno-rag-tabular` | < 1 min |
| `dev-fast.ps1 -Package anno-rag-tabular -Mode check` | < 30 s |
| Full test binary link (lance+datafusion) | 2–4 min |
| CI full build (warm sccache) | 8–12 min |

**Outils installés et configurés** :
- `sccache` — `RUSTC_WRAPPER=sccache` défini en User env. `CARGO_INCREMENTAL=0` imposé dans `.cargo/config.toml` (les deux sont mutuellement exclusifs).
- `lld-link` (LLD 22.1.2) — linker configuré dans `.cargo/config.toml [target.x86_64-pc-windows-msvc]`.
- `cargo-nextest` — profiles `dev-fast`, `ci`, `ml`, `quick`, `local` dans `.config/nextest.toml`.
- `cargo-hakari` — workspace-hack pour pré-unifier les features des dépendances partagées (voir crate `workspace-hack/`).
