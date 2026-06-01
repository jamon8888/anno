# Legal RAG User Guide

Status: Available in v0.11.0-rc.11
Audience: User, Compliance
Language: Bilingual

Legal RAG helps you search and reason over legal documents while keeping
personal and confidential data inside the local Hacienda boundary.

Use it for contracts, case files, correspondence, exhibits, and internal legal
notes that may contain client names, counterparties, addresses, emails, IBANs,
SIRET numbers, NIR values, or other confidential identifiers.

Ce guide decrit le parcours utilisateur: preparer un corpus, indexer des
documents tokenises, interroger les resultats, puis rehydrater localement
uniquement lorsque la sortie finale doit contenir les valeurs originales.

## Workflow

```text
Prepare corpus
  -> initialize or unlock the local vault
  -> ingest documents
  -> detect and tokenize PII
  -> embed tokenized chunks
  -> store vectors and metadata in LanceDB
  -> query through CLI or MCP
  -> rehydrate locally only for final output
```

## Prepare The Corpus

Group documents by matter, client, or review batch. Keep source paths stable so
search results and citations remain easy to verify.

Recommended input hygiene:

- Remove duplicate drafts when they are not useful for review.
- Keep original filenames when they carry legal context.
- Separate privileged, confidential, and public material into clear folders.
- Do not paste secrets or vault passphrases into prompts or shared notes.

## Initialize Or Unlock The Vault

The vault stores the reversible mapping between cleartext values and generated
tokens. In normal local installs, use the OS keyring path and let Hacienda
manage the vault secret.

Through MCP, call `anno_init_vault` only when you intentionally provide a
managed passphrase. Through CLI, run your ingest/search workflow as the same OS
user that owns the vault.

## Ingest Documents

CLI:

```powershell
anno-rag ingest C:\Matters\Acme --recursive --output C:\Matters\Acme\outputs
```

MCP:

```text
Ingest the legal documents in C:\Matters\Acme and index them for legal search.
```

During ingest, Hacienda extracts text, detects PII, writes pseudonymized output,
embeds tokenized chunks, and stores vectors plus metadata in LanceDB.

## Detect And Tokenize PII

Cleartext is processed locally by extraction, OCR when enabled, and PII
detection. Before local indexing/search outputs and any optional remote LLM
call, personal data should be replaced with vault tokens such as `PERSON_1`,
`EMAIL_2`, or `IBAN_3`.

Original values stay in the encrypted local vault. The LanceDB index stores
tokenized chunks and metadata, not the cleartext mapping.

## Query Through CLI Or MCP

CLI:

```powershell
anno-rag search "clause de confidentialite Sophie Martin" --top-k 5
```

MCP:

```text
Search the legal corpus for the confidentiality clause and return cited,
tokenized evidence.
```

Legal MCP tools such as `legal_search`, `legal_graph_query`, and
`legal_rehydrate_citation` add legal filters and citation-oriented workflows on
top of the same local privacy model.

## Rehydrate Locally

Rehydration restores tokens to their original values from the local vault. Use
it only at the trusted local boundary, after retrieval or model reasoning has
finished and only for the final output that needs cleartext.

The LLM should see tokenized text. Originals remain local unless a human
operator explicitly copies or exports rehydrated content.

## Human Review

Hacienda is assistive. It can find evidence, structure results, and draft
answers, but a qualified reviewer must verify:

- the cited source passage;
- the surrounding legal context;
- whether the extracted answer is complete;
- whether the final wording is appropriate for the matter;
- whether rehydrated values are necessary in the final deliverable.

Ne considerez pas une reponse RAG comme un avis juridique final sans controle
humain.

## Related Links

- [Product Concepts](../product/concepts.md)
- [MCP Tools](../developers/mcp-tools.md)
- [Privacy Model](../security-compliance/privacy-model.md)
