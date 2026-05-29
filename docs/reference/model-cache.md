# Model Cache

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin
Language: EN

Hacienda uses local model files for RAG embeddings and PII detection. The
release binaries do not make remote model calls for these local pipeline steps
once the model cache is populated.

## Populate The Cache

Run:

```bash
anno-rag download-models
```

The command downloads the embedder and NER model files, about 970 MiB total,
into the default models directory and prints the path to use as
`ANNO_MODELS_DIR`.

To choose a specific models directory, pass a path ending in `models`:

```bash
anno-rag download-models --dir /path/to/models
```

Then set:

```bash
export ANNO_MODELS_DIR="/path/to/models"
```

On Windows PowerShell:

```powershell
$env:ANNO_MODELS_DIR = "C:\path\to\models"
```

## Offline Use

After the cache is populated, set `ANNO_MODELS_DIR` in the shell, service
manager, or MCP client config. Set `ANNO_NO_DOWNLOADS=1` to disable downloads in
model-loading paths that honor it.

For hard offline control in `v0.11.0-rc.11`, combine a complete
`ANNO_MODELS_DIR` cache with network-level or host-level offline enforcement.
Do not rely on `ANNO_NO_DOWNLOADS` as the only download control for every
runtime path.

If `ANNO_MODELS_DIR` points at an incomplete cache, ingest/search/model-loading
workflows can still fail. Re-run `anno-rag download-models` with network access
or restore the cache from a known-good backup.

## Operational Notes

- Keep one shared read-only model cache per host when multiple users can trust
  the same binaries and model files.
- Keep the model cache outside disposable temp directories.
- Record `ANNO_MODELS_DIR` in deployment notes and MCP client config.
- The cache is large but rebuildable, so it is lower backup priority than the
  vault, source documents, and LanceDB memory/tabular review state.

## Related Links

- [Environment Variables](environment-variables.md)
- [File Layout](file-layout.md)
- [Installation](../getting-started/installation.md)
