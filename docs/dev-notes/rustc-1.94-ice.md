# rustc 1.94.0 ICE in lint rendering

Discovered 2026-05-05 during Phase 3 work on gliner2_fastino.

## Symptom

```
thread 'rustc' (268) panicked at /rustc-dev/.../library/alloc/src/vec/mod.rs:2873:36:
slice index starts at 9 but ends at 8
```

with backtrace pointing at:

```
12: <annotate_snippets::renderer::styled_buffer::StyledBuffer>::replace
13: annotate_snippets::renderer::render::render
14: <rustc_errors::annotate_snippet_emitter_writer::AnnotateSnippetEmitter>::emit_messages_default
```

The panic happens in `early_lint_checks` / `hir_crate` (during AST
lowering) **while rendering a lint warning**. Because the panic is in
the diagnostic emitter, real source-level errors are masked — there is
no useful "error in foo.rs:line N" output.

## Affected versions

- `rustc 1.94.0 (4a4ef493e 2026-03-02)` — confirmed broken.
- `rustc 1.95.0 (59807616e 2026-04-14)` — **also broken**, confirmed
  2026-05-06 during Phase 1.5 work. The lib-test build for the
  `gliner2_fastino` feature ICEs in `early_lint_checks` → `hir_crate`
  with the same `slice index starts at 9 but ends at 8` panic. Pin
  was supposed to fix this; the bug is in `annotate_snippets`
  (a transitive rustc dep) and isn't bound to one rustc minor.
- No clean rustc minor known as of 2026-05-06; latest stable is 1.95.0.

## Project-level workaround (current)

Each worktree where this bites should ship a gitignored
`.cargo/config.toml` with:

```toml
[build]
rustflags = ["--cap-lints", "allow"]
```

This applies the workaround automatically on every `cargo` invocation
in the worktree without requiring per-command `RUSTFLAGS`. The Phase 1.5
worktree at `feat/gliner2-fastino-phase1.5` ships such a file.

## Workaround for hosts on 1.94

If the toolchain pin can't be honored (e.g., locked CI image):

```bash
export RUSTFLAGS="--cap-lints allow"
cargo check -p anno --features gliner2-fastino
```

Capping lints below the rendering threshold sidesteps the buggy code
path. **Cost:** lints aren't emitted at all, so warnings about real
issues (dead code, unused imports, etc.) are silenced.

## Triggers

The panic fires on the `gliner2_fastino` backend specifically (lots of
new module surface). Other backends compile fine on 1.94. The minimal
repro hasn't been isolated; the obvious causal lint is unclear.
Upstream rustc bug, not anno's code.
