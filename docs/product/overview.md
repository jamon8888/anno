# Product Overview

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin, Compliance
Language: Bilingual

Hacienda is a local-first Rust toolkit for privacy-safe legal AI workflows.
It helps teams extract structure from legal documents, tokenize personal data
before local indexing/search outputs and optional remote LLM calls, search local
corpora, and connect those capabilities to Claude Desktop, Cowork, or other MCP
clients. Local detection, OCR, and explicit rehydration are trusted cleartext
operations on the user's machine.

Hacienda est le nom du produit complet. The crates keep their historical
`anno` names for compatibility, while Hacienda describes the whole workspace
and user-facing system.

## Layers

| Layer | Role |
|---|---|
| `anno` | Core Rust library for NER, relation extraction, coreference, PII detection, and text preparation. |
| `anno-cli` | Command-line interface for extraction and local automation workflows. |
| `anno-rag` | Local anonymization, vault, LanceDB RAG index, long-term memory, and MCP server. |
| `anno-privacy-gateway` | Anthropic-compatible HTTP gateway that pseudonymizes outbound prompts before remote LLM calls. |
| `anno-rag-tabular` | Schema-driven legal review tables with cited cells, verification, and local storage. |

## Who It Is For

- Legal professionals who need AI assistance without exposing client names,
  identifiers, or matter details in cleartext.
- Developers and integrators building local-first legal AI tools.
- Admins and operators deploying a privacy boundary around LLM usage.
- Compliance teams reviewing RGPD, audit, and AI governance controls.

## What Is Available Now

In `v0.11.0-rc.11`, Hacienda provides:

- Local PII detection and pseudonymization with an encrypted vault.
- Local document ingestion, embeddings, and LanceDB search through `anno-rag`.
- Claude Desktop and Cowork integration through the MCP server.
- Long-term memory tools exposed through MCP.
- Privacy Gateway flows for Anthropic-compatible clients.
- Tabular legal review primitives for schema-driven extraction and review.
- Release binaries for supported platforms, plus source builds for developers.

Some enterprise documentation pages are part of the Phase 1 docs structure and
may be expanded after this release candidate.

## Start Here

- [Installation](../getting-started/installation.md)
- [Claude Desktop and Cowork](../getting-started/claude-desktop-cowork.md)
- [First Index](../getting-started/first-index.md)
- [Product Concepts](concepts.md)
