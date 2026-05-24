## Pre-anno-tool check (mandatory)

Every practice-area skill that calls anno tools (`/search`, `/rehydrate`,
`/detect`, etc.) MUST invoke `/anno-engine-check` as its first step. The
check reads `claude-for-legal/engine-compat.json` and verifies the
installed engine version + tool surface. If the check emits a blocker,
the practice-area skill aborts before any anno call.

This is not enforced by hooks (the plugin currently has `hooks.json` set
to `{}`) — it is enforced by skill authors. New skills must add the check;
reviewers must confirm its presence.
