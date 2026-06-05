---
name: anno-security-review
description: Use when reviewing Anno changes that touch secrets, auth, storage, paths, crypto, MCP, gateway, network IO, or unsafe Rust.
---

# Anno Security Review

Lead with findings ordered by severity.

1. Inspect the base diff:
   `git diff --name-status <base>...HEAD`
   `git diff <base>...HEAD -- crates docs scripts`
2. Search for secret risks:
   `rg -n "(API_KEY|TOKEN|SECRET|PASSWORD|Bearer|PRIVATE KEY|passphrase)" crates docs scripts`
3. Check auth and trust boundaries: gateway endpoints, MCP tool input, CLI arguments, config loading, vault access, and permission checks.
4. Check path traversal and filesystem writes: reject untrusted `..`, symlink surprises, absolute-path escalation, and writes outside intended stores.
5. Check crypto and vault behavior: key derivation, nonces, authenticated encryption, passphrase handling, OS keyring fallback, and secret redaction in logs.
6. Check network IO: downloads, provider routing, localhost gateway calls, SSRF-like URLs, TLS assumptions, and offline mode.
7. Check unsafe Rust:
   `rg -n "unsafe|unwrap\\(|expect\\(" crates`
8. Run relevant gates when scope warrants:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1 -AllAffected`
   `cargo audit` or `cargo deny check` if configured.
