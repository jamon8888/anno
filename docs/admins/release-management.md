# Release Management

Status: Available in v0.11.0-rc.16
Audience: Admin, Developer
Language: EN

The current release candidate is `v0.11.0-rc.16`. Treat release upgrades as a
controlled change to binaries, MCP configuration, gateway configuration, model
cache, and local state directories.

## Upgrade Checklist

1. Read the release notes and confirm the intended tag.
2. Download the platform archive and `SHA256SUMS.txt`.
3. Verify the checksum before extracting.
4. Back up vault files, source documents, configs, audit logs, managed secrets,
   and LanceDB state when memory or tabular review is used.
5. Extract the new archive to a new versioned directory.
6. Update Claude Desktop, Cowork, service, or gateway configs to the new binary
   path.
7. Preserve `ANNO_RAG_DATA_DIR`, `ANNO_MODELS_DIR`, and vault secret handling
   unless the upgrade plan explicitly changes them.
8. Restart the MCP client or gateway process.
9. Run `anno-rag --version`, `anno_health`, and one representative search.
10. For gateway deployments, check `/health`, auth behavior, and a tokenized
    test request.

## Rollback Checklist

1. Stop the MCP client or gateway process.
2. Restore the previous binary path in client or service config.
3. Restore the previous config, vault backup, and LanceDB backup if the upgrade
   changed local state or if memory/tabular review state was migrated.
4. Restart the previous version.
5. Run `anno-rag --version`, `anno_health`, and a representative search.
6. Record the rollback reason and preserve logs for review.

Do not delete the previous versioned binary directory until the new version has
passed operational checks and the rollback window has closed.

## Related Links

- [Release README](../release/README-release.md)
- [Installation](../getting-started/installation.md)
- [Backups And Recovery](backups-and-recovery.md)
