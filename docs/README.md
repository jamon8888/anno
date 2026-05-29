# Hacienda Documentation

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin, Compliance
Language: Bilingual

This directory is the product documentation source of truth for Hacienda.

## Start By Role

| Role | Start here |
|---|---|
| User | [Installation](getting-started/installation.md), then [Claude Desktop/Cowork setup](getting-started/claude-desktop-cowork.md). |
| Developer | [Crates](developers/crates.md), [CLI](developers/cli.md), and [MCP tools](developers/mcp-tools.md). |
| Integrator | [Gateway API](developers/gateway-api.md), [Configuration](developers/configuration.md), and [File layout](reference/file-layout.md). |
| Admin | [Deployment](admins/deployment.md), [Operations](admins/operations.md), and [Release management](admins/release-management.md). |
| Compliance | [Privacy model](security-compliance/privacy-model.md), [RGPD controls](security-compliance/rgpd.md), and [Audit logging](security-compliance/audit-logging.md). |

## Core Journeys

1. [Install Hacienda](getting-started/installation.md)
2. [Connect Claude Desktop or Cowork](getting-started/claude-desktop-cowork.md)
3. [Run the first index and search](getting-started/first-index.md)
4. [Troubleshoot first-install issues](getting-started/troubleshooting.md)
5. [Use privacy-safe legal RAG](user-guide/legal-rag.md)
6. [Operate Hacienda](admins/operations.md)

## Product

- [Overview](product/overview.md)
- [Concepts](product/concepts.md)
- [Architecture](product/architecture.md)
- [Glossary](product/glossary.md)

## Getting Started

- [Installation](getting-started/installation.md)
- [Claude Desktop/Cowork setup](getting-started/claude-desktop-cowork.md)
- [First index and search](getting-started/first-index.md)
- [Troubleshooting](getting-started/troubleshooting.md)

## User Guides

- [Legal RAG](user-guide/legal-rag.md)
- [Memory](user-guide/memory.md)
- [Tabular review](user-guide/tabular-review.md)
- [Privacy gateway](user-guide/privacy-gateway.md)

## Developer And Integration

- [Crates](developers/crates.md)
- [CLI](developers/cli.md)
- [MCP tools](developers/mcp-tools.md)
- [Gateway API](developers/gateway-api.md)
- [Configuration](developers/configuration.md)

## Admin

- [Deployment](admins/deployment.md)
- [Operations](admins/operations.md)
- [Backups and recovery](admins/backups-and-recovery.md)
- [Release management](admins/release-management.md)

## Security And Compliance

- [Privacy model](security-compliance/privacy-model.md)
- [RGPD controls](security-compliance/rgpd.md)
- [AI Act position](security-compliance/ai-act.md)
- [Audit logging](security-compliance/audit-logging.md)
- [Threat model](security-compliance/threat-model.md)

## Reference

- [Commands](reference/commands.md)
- [Environment variables](reference/environment-variables.md)
- [File layout](reference/file-layout.md)
- [Model cache](reference/model-cache.md)
- [Changelog](reference/changelog.md)

## Existing Deep References

- [Quickstart](QUICKSTART.md)
- [Contract](CONTRACT.md)
- [Backends](BACKENDS.md)
- [Release archive guide](release/README-release.md)
- [Runbooks](runbooks/anno-engine-check.md)
- [ADRs](adrs/README.md)
- `superpowers/specs/` and `superpowers/plans/` for design and implementation history.

## Docs Hygiene

Run:

```powershell
python scripts\docs_audit.py
python scripts\check_docs_links.py
```
