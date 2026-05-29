# Product Concepts

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin, Compliance
Language: Bilingual

This page defines the core product ideas used across Hacienda.

## Local-First

Hacienda runs on the user's machine by default. Models, vault data, indexes,
audit files, and configuration are local unless the operator explicitly routes
tokenized content to a remote LLM.

## PII

PII means personally identifiable information. In Hacienda this includes names,
emails, phone numbers, NIR, SIRET, IBAN, addresses, organizations, and other
legal or administrative identifiers that should not leave the local privacy
boundary in cleartext.

## Pseudonymization

Pseudonymization replaces cleartext PII with stable tokens such as
`PERSON_1`, `EMAIL_2`, or `NIR_3`. The tokenized document remains useful for
search and reasoning, while the sensitive mapping stays in the local vault.

## Vault

The vault is the local encrypted mapping between cleartext PII and generated
tokens. It is the only component that can restore the original values. Losing
the vault means losing reversible rehydration for those tokens.

## Rehydration

Rehydration replaces tokens back with their original cleartext values from the
vault. It should happen only at a trusted local boundary, after search or model
reasoning has finished.

## Local RAG

Retrieval-augmented generation (RAG) in Hacienda indexes pseudonymized document
chunks locally with LanceDB. Queries are pseudonymized with the same vault,
searched locally, and returned as tokenized evidence for a model or user.

## MCP

MCP is the Model Context Protocol. Hacienda exposes `anno-rag mcp` so Claude
Desktop, Cowork, and other MCP clients can call local tools such as search,
rehydration, health checks, memory, and legal review operations.

## Privacy Gateway

The Privacy Gateway is an Anthropic-compatible HTTP boundary. It accepts client
requests, pseudonymizes outbound content, forwards tokenized prompts to a
configured upstream model, and keeps cleartext PII local.

## Long-Term Memory

Long-term memory stores facts, preferences, references, and context as local
pseudonymized records. Recall can combine vector search, full-text search, and
entity relationships while keeping PII under vault control.

## Tabular Review

Tabular review turns legal documents into structured review tables. Templates
define expected columns, extraction fills cited cells, and verification checks
that values are supported by source passages.
