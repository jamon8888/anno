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
- `rustc 1.95+` — pinned in `rust-toolchain.toml` at the workspace
  root. Lowest version known to compile cleanly.

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
