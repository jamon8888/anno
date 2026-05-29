# First Index And Search

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator
Language: Bilingual

This walkthrough creates a small local corpus, downloads the local models,
ingests the corpus, searches it from the CLI, then verifies access through MCP.

Ce parcours permet de valider rapidement que Hacienda indexe et interroge des
documents locaux sans exposer le contenu en clair a un fournisseur distant.

## Prerequisites

- `anno-rag` is installed from the release archive or built from source.
- Claude Desktop or Cowork is configured if you want to verify through MCP.
- You have enough disk space for the models. `anno-rag download-models`
  downloads about 970 MiB.

Confirm the installed syntax on your machine:

```powershell
anno-rag --help
```

If you built from source on Windows, use the explicit binary path:

```powershell
.\target\release\anno-rag.exe --help
```

## Create A Sample Corpus

```powershell
$root = Join-Path $HOME "hacienda-sample"
$corpus = Join-Path $root "corpus"
$outputs = Join-Path $root "outputs"
New-Item -ItemType Directory -Force -Path $corpus, $outputs | Out-Null

@"
Contrat de prestation de services.

La societe Durand Conseil accompagne Sophie Martin sur un dossier de
responsabilite contractuelle. Les parties prevoient une clause de
confidentialite et une facturation mensuelle.
"@ | Set-Content -Encoding UTF8 -Path (Join-Path $corpus "contrat.md")
```

## Download Models

```powershell
anno-rag download-models
```

The command prints the models directory. For the default local install, this is
usually:

```powershell
$env:ANNO_MODELS_DIR = "$HOME\.anno-rag\models"
```

Add the same path to your MCP `env` block if Claude Desktop or Cowork launches
`anno-rag mcp`.

## Ingest

```powershell
anno-rag ingest $corpus --output $outputs
```

The command writes pseudonymized output files and indexes chunks in the local
Hacienda data directory.

## Search

```powershell
anno-rag search "clause de confidentialite Sophie Martin" --top-k 5
```

Expected behavior:

- Results show ranked pseudonymized chunks.
- Source paths point back to local documents.
- Personal data may appear as vault tokens in search output.

If your shell cannot find `anno-rag`, use the absolute path to the installed
binary or add the extracted release folder to `PATH`.

## Verify Through MCP

Restart Claude Desktop or Cowork after configuring MCP. Then ask:

```text
Call anno_health, then search the local Hacienda corpus for
"clause de confidentialite Sophie Martin" with top_k 5.
```

If CLI search works but MCP search returns no results, confirm that Claude or
Cowork launches the same `anno-rag` binary, uses the same OS user, and has the
same `ANNO_MODELS_DIR` and data directory expectations.

## Next Step

Learn the legal RAG workflow:
[Legal RAG](../user-guide/legal-rag.md).
