# ADR-002 — Encrypted file vault, key from passphrase OR OS keyring

**Status:** Accepted (v0.1) · **Date:** 2026-05-12 · **Deciders:** anno team

## Context

The pseudonym mapping (`token → original`) is the most sensitive artefact in the deployment — possession of it is sufficient to re-identify everything in the index. Storage options considered:

1. **In-memory only** — no persistence; tokens reset on restart. Unusable for a multi-session product.
2. **Plain file** — simple but every backup, every accidental copy, every developer laptop carries an unencrypted dump of every name the cabinet has ever touched.
3. **OS-managed secrets store** (Windows Credential Manager / macOS Keychain / Linux Secret Service) — strong on origin host but per-secret, not per-vault; doesn't scale to thousands of mappings.
4. **External KMS** (Azure Key Vault, AWS KMS, HashiCorp Vault) — strong but introduces a cloud dependency for an on-premise product.
5. **Encrypted file**, key sourced from somewhere safe — combines the simplicity of option 2 with the protection of options 3–4 by separating *what is encrypted* (the file) from *the key custody* (separate problem).

The cabinet IT maturity varies: some have KMS, some have well-managed keyrings, some have neither. The product must work for all three.

## Decision

**Encrypted file vault using AES-256-GCM (cloakpipe `Vault::open`/`save`).** The 32-byte key is derived by one of two methods, in priority order:

1. If `ANNO_RAG_VAULT_PASSPHRASE` is set → Argon2id(passphrase, salt=fixed-per-vault) → 32 bytes.
2. Otherwise → read a 32-byte random secret from the OS keyring (auto-generated and stored on first run).

Future option (v0.5+): a third method that reads the key from a configured KMS adapter. Tracked as U6 in the readiness spec.

## Consequences

- A stolen file is useless without the key — DPIA v1 R1 gross severity drops from Maximum to Significant.
- Backups inherit the protection: the same `vault.bin` is backed up; the key stays on the host (or in the cabinet's secret manager).
- Argon2id (m=64 MiB, t=3) makes brute-forcing a strong passphrase impractical with current hardware; weak passphrases remain weak (operator responsibility).
- OS keyring path is invisible to the deployer guide reader: works without configuration but binds the vault to the host. Migrating hosts requires either exporting the key beforehand or re-keying the vault.
- AES-GCM AEAD tag turns any tampering into a decryption error — supports the DPIA R2 mitigation.
- No cloud dependency in the default deployment.

## Reference

`vendor/cloakpipe/crates/cloakpipe-core/src/vault.rs::Vault::open` / `::save`, `crates/anno-rag/src/vault.rs::derive_key`, deployer guide §3.1.
