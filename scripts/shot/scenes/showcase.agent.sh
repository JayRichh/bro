#!/bin/bash
# The SHOWCASE's stand-in agent (showcase.setup.sh copies it in as NEBULA_AGENT_CMD). It never runs a
# model: it prints a transcript in the style of the harness it stands in for, posts the hooks that
# colour its card, reports a context reading through the status line endpoint, and for the
# orchestrator runs the real `kete orchestrator spawn / list / review`. Which story it tells follows
# from where it runs: the orchestrator answers `kete orchestrator list`, a worker is known by its
# checkout's directory name.
K="$NEBULA_SHOT_BIN"
stty -echo 2>/dev/null
here=$(basename "$PWD")

# --- colours (truecolor, as Claude Code and Codex draw over a dark theme) ---
E=$'\e'
R="$E[0m"; B="$E[1m"; DIM="$E[38;2;140;132;122m"; INK="$E[38;2;237;230;218m"
GRN="$E[38;2;155;190;110m"; RED="$E[38;2;220;110;90m"; AMB="$E[38;2;224;138;53m"; TEAL="$E[38;2;95;168;160m"
BLUE="$E[38;2;130;160;220m"; ADDBG="$E[48;2;34;52;30m"; DELBG="$E[48;2;62;30;26m"
# Everything printed is kept, and redrawn from the top when the pane resizes (SIGWINCH), the way a real
# harness repaints: a session spawned off screen keeps its tail in view once the pane shows it.
LOG="$NEBULA_SHOT_WORK/.transcript-$NEBULA_AGENT_ID"; : > "$LOG"
emit() { printf "$@" | tee -a "$LOG"; }
say() { emit '%s\r\n' "$@"; }
trap 'printf "\e[H\e[2J\e[3J"; cat "$LOG"' WINCH
pause() { sleep "${1:-0.5}"; }
cols=$(stty size 2>/dev/null | cut -d' ' -f2); cols=${cols:-120}
rule() { emit '%s%s%s\r\n' "$DIM" "$(printf '─%.0s' $(seq 1 "$cols"))" "$R"; }

post() { # event json [dialect]
  curl -sS -m 3 -X POST -H "Authorization: Bearer $NEBULA_API_TOKEN" -H 'Content-Type: application/json' \
    -d "$2" "$NEBULA_API_URL/api/hooks/${3:-claude}?agentId=$NEBULA_AGENT_ID&hookEvent=$1" >/dev/null 2>&1
}
ctx() { # percent
  curl -sS -m 3 -X POST -H "Authorization: Bearer $NEBULA_API_TOKEN" -H 'Content-Type: application/json' \
    -d "{\"session_id\":\"$NEBULA_AGENT_ID\",\"context_window\":{\"used_percentage\":$1,\"context_window_size\":200000}}" \
    "$NEBULA_API_URL/api/usage/claude?agentId=$NEBULA_AGENT_ID" >/dev/null 2>&1
}

# Claude Code's shapes: the prompt, a tool call with its result, a reply, the spinner, the input box.
prompt() { say "$DIM>$R $INK$1$R" ""; }
tool() { say "$GRN●$R $B$1$R($2)"; }
out() { local first=1; for l in "$@"; do if [ $first = 1 ]; then say "  $DIM⎿$R  $l"; first=0; else say "     $l"; fi; done; say ""; }
reply() { local first=1; for l in "$@"; do if [ $first = 1 ]; then say "$INK●$R $l"; first=0; else say "  $l"; fi; done; say ""; }
spin() { say "$AMB✻$R $AMB$1$R $DIM($2 · esc to interrupt)$R"; }
# The input box, with the cursor left after its `>`.
inbox() { say ""; rule; say "$DIM>$R "; rule; emit '  %s⏵⏵ accept edits on (shift+tab to cycle)%s\e[2A\r\e[2C' "$DIM" "$R"; }

orchestrator() {
  post UserPromptSubmit '{"session_id":"shot-orch","prompt":"Spread this sprint across workers"}'
  prompt "Pick up my sprint tasks in ledger-api and storefront-web and spread them across workers"
  pause 0.8
  tool Bash "kete ado tasks --project ledger-api && kete ado tasks --project storefront-web"
  out "${AMB}4211$R  ledger-api       Export invoices as CSV from the billing dashboard   ${TEAL}Active$R" \
      "${AMB}4207$R  ledger-api       Retry webhook deliveries with backoff               ${TEAL}Active$R" \
      "${AMB}4198$R  ledger-api       Document the nightly reconciliation job             ${DIM}New$R" \
      "${AMB}4219$R  storefront-web   Checkout page on the shared design tokens           ${TEAL}Active$R" \
      "${AMB}4224$R  storefront-web   Upgrade to Next 16                                  ${DIM}New$R"
  pause 0.6
  spawn() { # project worktree kind profile model name task
    local wt=() ; [ -n "$2" ] && wt=(--worktree "$2")
    tool Bash "kete orchestrator spawn --project $1 ${2:+--worktree $2 }--kind $3 --profile $4 --name \"$6\" \"${7%% *} …\""
    "$K" orchestrator spawn --project "$1" "${wt[@]}" --kind "$3" --profile "$4" --model "$5" --effort high --name "$6" "$7" >/dev/null 2>&1
    out "started $B$6$R  ${DIM}$3 $5 · $4 · $1${2:+ ↳ $2}$R"
    pause 1.2
  }
  spawn ledger-api feat/invoice-csv-export claude work opus "Invoice CSV export" "AB#4211 Export invoices as CSV"
  spawn ledger-api fix/webhook-retries codex codex-team gpt-5.6-sol "Webhook retry backoff" "AB#4207 Retry webhook deliveries with backoff"
  spawn ledger-api "" claude personal sonnet "Reconciliation notes" "AB#4198 Document the nightly reconciliation job"
  spawn storefront-web feat/checkout-tokens claude personal opus "Checkout design tokens" "AB#4219 Checkout page on the design tokens"
  spawn storefront-web chore/next-16 claude work opus "Next 16 upgrade" "AB#4224 Upgrade to Next 16"
  sleep 9
  tool Bash "kete orchestrator list"
  out "${RED}needs you$R  Checkout design tokens   storefront-web ↳ feat/checkout-tokens   claude · personal" \
      "${AMB}running  $R  Invoice CSV export       ledger-api ↳ feat/invoice-csv-export    claude · work" \
      "${AMB}running  $R  Next 16 upgrade          storefront-web ↳ chore/next-16          claude · work" \
      "${TEAL}unread   $R  Webhook retry backoff    ledger-api ↳ fix/webhook-retries        codex · codex-team" \
      "${TEAL}unread   $R  Reconciliation notes     ledger-api ↳ main                       claude · personal"
  reply "${B}Checkout design tokens$R wants to run ${B}pnpm add @storefront/tokens$R, which is yours to approve. ${B}Webhook retry backoff$R" \
        "finished with PR #208 green, and ${B}Invoice CSV export$R has its diff up while the tests run, so I've opened it for review."
  tool Bash "kete orchestrator review \"Invoice CSV export\""
  "$K" orchestrator review "Invoice CSV export" >/dev/null 2>&1
  say ""
  spin "Watching 5 workers…" "3m 08s"
  inbox
}

invoice_csv() {
  post UserPromptSubmit '{"session_id":"shot-a","prompt":"AB#4211 Export invoices as CSV"}'
  ctx 46
  prompt "AB#4211 Export invoices as CSV from the billing dashboard"
  tool Read "src/invoices/export.ts"; out "Read 21 lines"
  tool Read "src/invoices/types.ts"; out "Read 9 lines"
  reply "The export only speaks JSON. Adding a ${B}format$R option with a CSV writer beside it, so the endpoint" \
        "and the dashboard button share one code path."
  tool Update "src/invoices/export.ts"
  out "Updated ${B}src/invoices/export.ts$R with ${GRN}22 additions$R and ${RED}2 removals$R" \
      "${DIM}12$R ${DELBG}-  return JSON.stringify(rows, null, 2);$R" \
      "${DIM}21$R ${ADDBG}+  return opts.format === \"csv\" ? toCsv(rows) : JSON.stringify(rows, null, 2);$R"
  tool Update "test/export.test.ts"; out "Updated ${B}test/export.test.ts$R with ${GRN}16 additions$R"
  tool Bash "pnpm vitest run test/export.test.ts"
  out "${GRN}✓$R test/export.test.ts (3 tests) 41ms"
  tool Bash "gh pr create --draft --fill"; out "https://github.com/ledger/ledger-api/pull/212"
  touch "$NEBULA_SHOT_WORK/.csv-review"
  tool Bash "kete spawn --kind codex \"Review the CSV export diff for edge cases\""
  "$K" spawn --kind codex "Review the CSV export diff for edge cases" >/dev/null 2>&1
  out "started a codex session beside this one"
  spin "Running the full suite…" "1m 52s"
}

csv_review() {
  "$K" rename "CSV export review" >/dev/null 2>&1
  post UserPromptSubmit '{"session_id":"shot-r","prompt":"Review the CSV export diff for edge cases"}' codex
  say "$B›$R Review the CSV export diff for edge cases" ""
  say "$B•$R Explored" "  $DIM└$R Read src/invoices/export.ts, test/export.test.ts" ""
  say "$B•$R Ran git diff main --stat" "  $DIM└$R 2 files changed, 38 insertions(+), 3 deletions(-)" ""
  say "$DIM─ Worked for 1m 06s ─$R" ""
  say "$B•$R Two nits, no blockers: a number with a newline is quoted but never tested, and the" \
      "  header row could come from CSV_COLUMNS in the test instead of a literal."
  (sleep 5; post Stop '{"session_id":"shot-r"}' codex) &
}

webhooks() {
  post UserPromptSubmit '{"session_id":"shot-b","prompt":"AB#4207 Retry webhook deliveries with backoff"}' codex
  say "$B›$R AB#4207 Retry webhook deliveries with backoff" ""
  say "$B•$R Explored" "  $DIM└$R Read src/webhooks/deliver.ts" "    Search retry|backoff in src" ""
  say "$B•$R Edited src/webhooks/deliver.ts ${GRN}(+14$R ${RED}-3)$R" ""
  say "$B•$R Ran pnpm vitest run webhooks" "  $DIM└$R ${GRN}✓$R 6 passed (deliver.test.ts)" ""
  say "$B•$R Ran gh pr create --fill" "  $DIM└$R https://github.com/ledger/ledger-api/pull/208" ""
  say "$DIM─ Worked for 2m 41s ─$R" ""
  say "$B•$R A 5xx or a network error now retries up to five times with jittered exponential backoff;" \
      "  a 4xx stays final. PR #208 is up and its checks are green."
  (sleep 6; post Stop '{"session_id":"shot-b"}' codex) &
}

reconciliation() {
  post UserPromptSubmit '{"session_id":"shot-e","prompt":"AB#4198 Document the nightly reconciliation job"}'
  ctx 18
  prompt "AB#4198 Document the nightly reconciliation job"
  tool Read "src/invoices/export.ts"; out "Read 21 lines"
  tool Write "docs/reconciliation.md"; out "Wrote ${B}48 lines$R to docs/reconciliation.md"
  reply "Written up in docs/reconciliation.md: when the job runs, what it compares, and how to rerun one" \
        "day by hand."
  (sleep 8; post Stop '{"session_id":"shot-e"}') &
}

checkout_tokens() {
  post UserPromptSubmit '{"session_id":"shot-c","prompt":"AB#4219 Checkout page on the design tokens"}'
  ctx 31
  prompt "AB#4219 Checkout page on the shared design tokens"
  tool Read "src/app/checkout/page.tsx"; out "Read 13 lines"
  tool Update "src/app/checkout/page.tsx"; out "Updated ${B}src/app/checkout/page.tsx$R with ${GRN}5 additions$R and ${RED}4 removals$R"
  tool Bash "pnpm add @storefront/tokens"
  say "" "$B Bash command$R" "" "   pnpm add @storefront/tokens" "   ${DIM}Add the shared tokens package$R" "" \
      " Do you want to proceed?" " $AMB❯ 1. Yes$R" "   2. Yes, and don't ask again for pnpm add commands" "   3. No, and tell Claude what to do differently"
  sleep 0.4; post PermissionRequest '{"session_id":"shot-c","tool_name":"Bash"}'
}

next16() {
  post UserPromptSubmit '{"session_id":"shot-d","prompt":"AB#4224 Upgrade to Next 16"}'
  ctx 72
  prompt "AB#4224 Upgrade to Next 16"
  tool Bash "pnpm up next@16 react@19.2 react-dom@19.2"; out "${GRN}+$R next 16.0.1, react 19.2.0, react-dom 19.2.0"
  tool Bash "pnpm next build"; out "${GRN}✓$R Compiled successfully in 14.2s" "${GRN}✓$R Generating static pages (38/38)"
  spin "Checking the middleware for the async request APIs…" "4m 27s"
}

if "$K" orchestrator list >/dev/null 2>&1; then
  orchestrator
else
  case "$here" in
    feat-invoice-csv-export) if [ -e "$NEBULA_SHOT_WORK/.csv-review" ]; then csv_review; else invoice_csv; fi;;
    fix-webhook-retries) webhooks;;
    ledger-api) reconciliation;;
    feat-checkout-tokens) checkout_tokens;;
    chore-next-16) next16;;
    *) say "$DIM(idle)$R";;
  esac
fi
while :; do sleep 60 & wait $!; done  # waits in `wait`, so a resize redraws at once
