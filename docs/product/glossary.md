# Glossary

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin, Compliance
Language: Bilingual

## AI Act

European Union regulation for AI systems. Hacienda documentation uses this term
for governance, transparency, and risk-management discussions.

## anno

The core Rust library for extraction tasks such as NER, PII detection,
coreference, relation extraction, and text preparation.

## anno-privacy-gateway

The Anthropic-compatible HTTP gateway that pseudonymizes outbound content before
it reaches a remote LLM provider.

## anno-rag

The local RAG and MCP engine. It owns ingestion, vault pseudonymization,
LanceDB indexing, memory, search, rehydration, and Claude Desktop/Cowork tools.

## anno-rag-tabular

The schema-driven legal review layer. It extracts structured, cited cells from
documents into local review tables.

## Hacienda

The product name for the full workspace made of `anno`, `anno-cli`,
`anno-rag`, `anno-privacy-gateway`, and `anno-rag-tabular`.

## LanceDB

The local vector database used by Hacienda for document chunks, memory records,
and tabular review storage.

## MCP

Model Context Protocol. A protocol that lets clients such as Claude Desktop or
Cowork call local tools exposed by `anno-rag mcp`.

## PII

Personally identifiable information. Examples include names, emails, phone
numbers, NIR, SIRET, IBAN, and other identifiers tied to a person or matter.

## Pseudonymization

Replacing cleartext PII with stable tokens while keeping the reversible mapping
inside a protected local vault.

## RAG

Retrieval-augmented generation. A workflow where a system retrieves relevant
documents or chunks and supplies them as evidence for model reasoning.

## Rehydration

Replacing pseudonym tokens with their original cleartext values using the local
vault.

## RGPD

Reglement general sur la protection des donnees, the French name for GDPR. In
Hacienda docs, RGPD refers to privacy rights, data minimization, erasure, and
auditability obligations.

## Vault

The encrypted local store that maps cleartext PII to pseudonym tokens and makes
controlled rehydration possible.
