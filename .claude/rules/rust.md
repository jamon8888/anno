# Rust Rules
- Prefer scripts/dev-fast.ps1 over broad workspace builds.
- Use targeted crate checks after Rust edits.
- Avoid unwrap() and expect() in production paths.
- Use thiserror for public error types and anyhow for application-level CLI errors.
- Use tracing for structured runtime logging.
- Add // SAFETY: comments for every unsafe block.
