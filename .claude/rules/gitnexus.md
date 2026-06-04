# GitNexus Rules
- Run GitNexus impact analysis before editing functions, classes, methods, or public symbols.
- If the index is stale, run npx gitnexus analyze.
- Use npx gitnexus query --repo anno "<concept>" before grepping unfamiliar flows.
- Use npx gitnexus context --repo anno <symbol> for callers and callees.
- Before commits, use GitNexus detect-change tooling when available; otherwise use npx gitnexus status and git diff --name-status.
