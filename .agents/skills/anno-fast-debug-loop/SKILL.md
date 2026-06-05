---
name: anno-fast-debug-loop
description: Use when checking, testing, or debugging Anno Rust changes with the local fast loop.
---

# Anno Fast Debug Loop

Prefer targeted Rust checks before broad builds.

1. Check for existing Rust builds before running local checks:
   `Get-Process cargo,rustc -ErrorAction SilentlyContinue`
2. Preview package detection:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1 -PrintOnly`
3. Run the targeted check:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1`
4. If package detection is ambiguous, pass the crate explicitly:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1 -Package anno-rag -Mode check`
5. For shared crate changes, include dependents:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1 -AllAffected`
6. For tests, use the local nextest wrapper:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-local.ps1 -Package anno-rag -NextestProfile local`
7. For focused integration tests, add `-TestTarget <name>`, `-LibOnly`, `-TestOnly`, `-BuildJobs 1`, or `-TestThreads 1` as needed.

Avoid `cargo build --workspace`, `cargo build --release`, and all-feature builds during debugging unless the user asked for release validation.
