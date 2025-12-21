## Commit checklist (current)

This file is intended to be short and stable. Prefer commands over long lists of filenames.

### Before pushing

- Run fast checks: `just check`
- If docs changed: `just docs-audit`

### Before merging

- Ensure CI is green (includes a docs audit job)
- If you moved docs/notes around, re-run: `just docs-links`

### Hygiene

- Don’t commit generated reports/artifacts (most are ignored by `.gitignore`).
- If you *do* need to commit a report, put it under `docs/` and explain why it’s stable.

