#!/bin/sh
# Re-apply the Kete rebrand. Idempotent; see scripts/rebrand.mjs for what it
# changes. Needs node (v18+). On Windows, `node scripts/rebrand.mjs` does the
# same from PowerShell.
set -eu
exec node "$(dirname "$0")/rebrand.mjs" "$@"
