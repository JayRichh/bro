# The README's TERMINAL SHOWCASE (assets/showcase/): one run, six captures, one live state.
#
#   SHOT_SCALE=2 SHOT_FONT=/path/to/GeistMono-Regular.ttf scripts/shot/shot.sh showcase
#   cp design-screenshots/0[1-6]-*.png assets/showcase/
#
# On Linux (or in a `rust` container with tmux, python3-pil, python3-fonttools, fonts-dejavu-core and
# fonts-noto-core; render.py draws missing glyphs from those and names any it still lacks). Geist Mono
# is the brand's mono, from github.com/vercel/geist-font. The keys run ~90 s; rerun if a capture
# catches a card mid-transition.
#
# Two made-up repos, `ledger-api` (a TypeScript billing service) and `storefront-web` (a Next.js shop),
# each with linked worktrees holding uncommitted work, so their bands count lines and the review's Diff
# tab has real hunks. Three accounts (two Claude, one Codex) with seeded 5h and weekly readings, so the
# quick prompt's profile field and the accounts overlay draw bars. The stand-in agent tells one story
# per checkout: the orchestrator (F10) reads the sprint, starts a worker per task across both repos with
# the real `kete orchestrator spawn`, lists them and opens a review on one with the real `kete
# orchestrator review`; each worker prints a Claude- or Codex-like transcript, reports its context
# through the status line endpoint and posts the hooks that colour its card (running, needs you,
# finished unread). Every name, task, number and pull request is invented.
mkdir -p "$WORK/data"
NOW=$(date +%s)
cat > "$WORK/data/config.json" <<JSON
{"prewarm_agents": false, "prewarm_sessions": false, "session_pane": "${SHOWCASE_PANE:-bottom}", "worktree_layout": "${SHOWCASE_LAYOUT:-cards}", "theme": "kete-dark",
 "claude_model": "opus", "claude_effort": "high", "codex_model": "gpt-5.6-sol", "codex_effort": "high",
 "profiles": [
   {"name": "work", "harness": "claude", "default": true},
   {"name": "personal", "harness": "claude"},
   {"name": "codex-team", "harness": "codex", "default": true}
 ]}
JSON

# --- accounts: signed in, with 5h and weekly readings taken just now ---
P="$WORK/data/profiles"
mkdir -p "$P/.usage" "$P/work/claude" "$P/personal/claude" "$P/codex-team/codex"
echo '{}' > "$P/work/claude/.credentials.json"
echo '{}' > "$P/personal/claude/.credentials.json"
echo '{}' > "$P/codex-team/codex/auth.json"
usage() { # name harness 5h% 5h-reset-secs wk% wk-reset-secs plan source
  cat > "$P/.usage/$1.json" <<JSON
{"percent": $5, "window": "weekly", "at": ${NOW}000, "harness": "$2", "source": "$8", "plan": $7,
 "windows": [
   {"kind": "session", "label": null, "percent": $3, "resets_at": $((NOW + $4)), "read_at": $NOW},
   {"kind": "weekly", "label": null, "percent": $5, "resets_at": $((NOW + $6)), "read_at": $NOW}
 ]}
JSON
}
usage work claude 62 7920 41 302400 null statusline
usage personal claude 18 15600 9 446400 null statusline
usage codex-team codex 84 3300 57 172800 '"pro"' rollout

g() { git -C "$1" -c user.name="Mere Tane" -c user.email=mere@example.invalid "${@:2}"; }

# --- ledger-api ---
L="$WORK/ledger-api"; mkdir -p "$L/src/invoices" "$L/src/webhooks" "$L/test"
git -C "$L" init -q -b main
cat > "$L/package.json" <<'EOF'
{
  "name": "ledger-api",
  "version": "2.14.0",
  "type": "module",
  "scripts": { "dev": "tsx watch src/server.ts", "test": "vitest run", "lint": "eslint src" }
}
EOF
cat > "$L/src/invoices/export.ts" <<'EOF'
import { db } from "../db";
import type { Invoice } from "./types";

export interface ExportOptions {
  customerId: string;
  from?: Date;
  to?: Date;
}

// Exports a customer's invoices for the billing dashboard.
export async function exportInvoices(opts: ExportOptions): Promise<string> {
  const rows: Invoice[] = await db.invoices.findMany({
    where: { customerId: opts.customerId, issuedAt: { gte: opts.from, lte: opts.to } },
    orderBy: { issuedAt: "asc" },
  });
  return JSON.stringify(rows, null, 2);
}

export function totalCents(rows: Invoice[]): number {
  return rows.reduce((sum, r) => sum + r.amountCents, 0);
}
EOF
cat > "$L/src/invoices/types.ts" <<'EOF'
export interface Invoice {
  id: string;
  number: string;
  customerId: string;
  issuedAt: Date;
  amountCents: number;
  currency: "NZD" | "AUD" | "USD";
  status: "draft" | "open" | "paid" | "void";
}
EOF
cat > "$L/src/webhooks/deliver.ts" <<'EOF'
import { fetch } from "undici";

export async function deliver(url: string, body: unknown): Promise<void> {
  const res = await fetch(url, { method: "POST", body: JSON.stringify(body) });
  if (!res.ok) throw new Error(`webhook ${url} answered ${res.status}`);
}
EOF
cat > "$L/test/export.test.ts" <<'EOF'
import { describe, expect, it } from "vitest";
import { totalCents } from "../src/invoices/export";

describe("totalCents", () => {
  it("adds every row", () => {
    expect(totalCents([{ amountCents: 1200 }, { amountCents: 800 }] as never)).toBe(2000);
  });
});
EOF
g "$L" add -A; g "$L" commit -q -m "Invoices, webhooks and the export endpoint"
g "$L" worktree add -q -b feat/invoice-csv-export "$WORK/ledger-api-worktrees/feat-invoice-csv-export" main
g "$L" worktree add -q -b fix/webhook-retries "$WORK/ledger-api-worktrees/fix-webhook-retries" main

A="$WORK/ledger-api-worktrees/feat-invoice-csv-export"
cat > "$A/src/invoices/export.ts" <<'EOF'
import { db } from "../db";
import type { Invoice } from "./types";

export type ExportFormat = "json" | "csv";

export interface ExportOptions {
  customerId: string;
  format?: ExportFormat;
  from?: Date;
  to?: Date;
}

const CSV_COLUMNS = ["number", "issuedAt", "status", "currency", "amount"] as const;

// Exports a customer's invoices for the billing dashboard, as JSON or CSV.
export async function exportInvoices(opts: ExportOptions): Promise<string> {
  const rows: Invoice[] = await db.invoices.findMany({
    where: { customerId: opts.customerId, issuedAt: { gte: opts.from, lte: opts.to } },
    orderBy: { issuedAt: "asc" },
  });
  return opts.format === "csv" ? toCsv(rows) : JSON.stringify(rows, null, 2);
}

export function toCsv(rows: Invoice[]): string {
  const lines = rows.map((r) =>
    [r.number, r.issuedAt.toISOString().slice(0, 10), r.status, r.currency, (r.amountCents / 100).toFixed(2)]
      .map(escapeCsv)
      .join(","),
  );
  return [CSV_COLUMNS.join(","), ...lines].join("\r\n");
}

function escapeCsv(value: string): string {
  return /[",\r\n]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value;
}

export function totalCents(rows: Invoice[]): number {
  return rows.reduce((sum, r) => sum + r.amountCents, 0);
}
EOF
cat > "$A/test/export.test.ts" <<'EOF'
import { describe, expect, it } from "vitest";
import { toCsv, totalCents } from "../src/invoices/export";

const row = (number: string, amountCents: number) =>
  ({ number, issuedAt: new Date("2026-09-30"), status: "paid", currency: "NZD", amountCents }) as never;

describe("totalCents", () => {
  it("adds every row", () => {
    expect(totalCents([{ amountCents: 1200 }, { amountCents: 800 }] as never)).toBe(2000);
  });
});

describe("toCsv", () => {
  it("writes a header and one line per invoice", () => {
    expect(toCsv([row("INV-1042", 129900)])).toBe(
      "number,issuedAt,status,currency,amount\r\nINV-1042,2026-09-30,paid,NZD,1299.00",
    );
  });

  it("quotes a number with a comma in it", () => {
    expect(toCsv([row("INV-7,A", 500)])).toContain('"INV-7,A"');
  });
});
EOF

B="$WORK/ledger-api-worktrees/fix-webhook-retries"
cat > "$B/src/webhooks/deliver.ts" <<'EOF'
import { fetch } from "undici";
import { setTimeout as sleep } from "node:timers/promises";

const MAX_ATTEMPTS = 5;

// Retries a 5xx or a network error with exponential backoff and jitter; a 4xx is final.
export async function deliver(url: string, body: unknown, attempt = 1): Promise<void> {
  try {
    const res = await fetch(url, { method: "POST", body: JSON.stringify(body) });
    if (res.ok || (res.status >= 400 && res.status < 500)) return;
    throw new Error(`webhook ${url} answered ${res.status}`);
  } catch (err) {
    if (attempt >= MAX_ATTEMPTS) throw err;
    await sleep(2 ** attempt * 250 + Math.random() * 250);
    return deliver(url, body, attempt + 1);
  }
}
EOF

# --- storefront-web ---
S="$WORK/storefront-web"; mkdir -p "$S/src/app/checkout" "$S/src/styles"
git -C "$S" init -q -b main
cat > "$S/package.json" <<'EOF'
{
  "name": "storefront-web",
  "private": true,
  "scripts": { "dev": "next dev", "build": "next build", "test": "playwright test" },
  "dependencies": {
    "next": "15.4.2",
    "react": "19.1.0",
    "react-dom": "19.1.0"
  }
}
EOF
cat > "$S/src/app/checkout/page.tsx" <<'EOF'
import { CartSummary } from "@/components/cart-summary";
import { PayButton } from "@/components/pay-button";

export default function CheckoutPage() {
  return (
    <main style={{ background: "#fafaf9", color: "#1c1917", padding: 32 }}>
      <h1 style={{ fontSize: 28, fontWeight: 600 }}>Checkout</h1>
      <CartSummary borderColor="#e7e5e4" />
      <PayButton background="#d9822b" color="#ffffff" radius={6} />
    </main>
  );
}
EOF
printf ':root {\n  --surface: #fafaf9;\n  --ink: #1c1917;\n  --accent: #d9822b;\n}\n' > "$S/src/styles/tokens.css"
g "$S" add -A; g "$S" commit -q -m "Checkout page"
g "$S" worktree add -q -b feat/checkout-tokens "$WORK/storefront-web-worktrees/feat-checkout-tokens" main
g "$S" worktree add -q -b chore/next-16 "$WORK/storefront-web-worktrees/chore-next-16" main
C="$WORK/storefront-web-worktrees/feat-checkout-tokens"
cat > "$C/src/app/checkout/page.tsx" <<'EOF'
import { CartSummary } from "@/components/cart-summary";
import { PayButton } from "@/components/pay-button";
import { tokens } from "@storefront/tokens";

export default function CheckoutPage() {
  return (
    <main style={{ background: tokens.surface, color: tokens.ink, padding: tokens.space[8] }}>
      <h1 style={{ fontSize: tokens.text.xl, fontWeight: 600 }}>Checkout</h1>
      <CartSummary borderColor={tokens.line} />
      <PayButton background={tokens.accent} color={tokens.onAccent} radius={tokens.radius.md} />
    </main>
  );
}
EOF
D="$WORK/storefront-web-worktrees/chore-next-16"
sed -i 's/"next": "15.4.2"/"next": "16.0.1"/; s/"react": "19.1.0"/"react": "19.2.0"/; s/"react-dom": "19.1.0"/"react-dom": "19.2.0"/' "$D/package.json"

# shot.sh registers `$DEMO` with `$BIN add`; this wrapper registers the two repos instead.
REAL_BIN="$BIN"
cat > "$RUNTIME/nebula" <<WRAP
#!/bin/sh
if [ "\$1" = add ]; then
  "$REAL_BIN" add "$WORK/ledger-api" >/dev/null
  exec "$REAL_BIN" add "$WORK/storefront-web"
fi
exec "$REAL_BIN" "\$@"
WRAP
chmod +x "$RUNTIME/nebula"
BIN="$RUNTIME/nebula"

# --- pull requests: a draft on the CSV export, a ready one on the webhook fix ---
cp -R "$HERE/fixtures" "$WORK/fx"
export NEBULA_GH_FIXTURES="$WORK/fx"
echo mere-tane > "$WORK/fx/user"
cat > "$WORK/fx/pr-list.json" <<'JSON'
[
  {"number": 212, "title": "Export invoices as CSV", "url": "https://github.com/ledger/ledger-api/pull/212", "isDraft": true, "headRefName": "feat/invoice-csv-export",
   "statusCheckRollup": [{"status": "IN_PROGRESS", "conclusion": null}]},
  {"number": 208, "title": "Retry webhook deliveries with backoff", "url": "https://github.com/ledger/ledger-api/pull/208", "isDraft": false, "headRefName": "fix/webhook-retries",
   "statusCheckRollup": [{"status": "COMPLETED", "conclusion": "SUCCESS"}]}
]
JSON
cat > "$WORK/fx/pr-view-feat-invoice-csv-export.json" <<'JSON'
{"number": 212, "url": "https://github.com/ledger/ledger-api/pull/212", "title": "Export invoices as CSV", "state": "OPEN", "isDraft": true, "comments": [], "reviews": []}
JSON
cp "$WORK/fx/pr-view-feat-invoice-csv-export.json" "$WORK/fx/pr-212.json"
cat > "$WORK/fx/pr-view-fix-webhook-retries.json" <<'JSON'
{"number": 208, "url": "https://github.com/ledger/ledger-api/pull/208", "title": "Retry webhook deliveries with backoff", "state": "OPEN", "isDraft": false, "comments": [], "reviews": []}
JSON
cp "$WORK/fx/pr-view-fix-webhook-retries.json" "$WORK/fx/pr-208.json"

cat > "$WORK/fx/issue-list.json" <<'JSON'
[
 {"number": 231, "url": "https://github.com/ledger/ledger-api/issues/231", "title": "Invoice export times out for customers with 10k+ invoices",
  "author": {"login": "aroha-p"}, "createdAt": "2026-10-09T21:40:00Z", "updatedAt": "2026-10-12T02:15:00Z", "labels": [{"name": "bug"}, {"name": "invoices"}], "body": "Exporting a year of invoices for a large customer hits the 30s gateway timeout."},
 {"number": 229, "url": "https://github.com/ledger/ledger-api/issues/229", "title": "Webhook deliveries are not retried after a 503",
  "author": {"login": "sam-w"}, "createdAt": "2026-10-07T03:12:00Z", "updatedAt": "2026-10-11T19:30:00Z", "labels": [{"name": "bug"}, {"name": "webhooks"}], "body": "A partner's endpoint answered 503 during their deploy and the event was dropped."},
 {"number": 224, "url": "https://github.com/ledger/ledger-api/issues/224", "title": "Document the nightly reconciliation job",
  "author": {"login": "mere-tane"}, "createdAt": "2026-10-02T22:05:00Z", "updatedAt": "2026-10-08T01:44:00Z", "labels": [{"name": "docs"}], "body": "Nobody but Sam knows how to rerun a failed day."}
]
JSON

# The terminal card's shell: `$SHELL -l` prints a watch-mode test run and waits; a `-c` probe is /bin/sh's.
cat > "$RUNTIME/shell" <<'SHELL'
#!/bin/sh
case "$*" in
  -l) stty -echo 2>/dev/null
      printf '%s\r\n' '$ pnpm vitest --watch' '' ' DEV  v3.2.4 /home/mere/code/ledger-api' '' \
        ' ✓ test/export.test.ts (1 test) 12ms' ' ✓ test/webhooks.test.ts (6 tests) 48ms' ' ✓ test/reconcile.test.ts (9 tests) 131ms' '' \
        ' Test Files  3 passed (3)' '      Tests  16 passed (16)' '' ' Waiting for file changes...'
      exec sleep 100000 ;;
  *) exec /bin/sh "$@" ;;
esac
SHELL
chmod +x "$RUNTIME/shell"
export SHELL="$RUNTIME/shell"

# --- the stand-in agent ---
export NEBULA_SHOT_BIN="$REAL_BIN" NEBULA_SHOT_WORK="$WORK"
cp "$HERE/scenes/showcase.agent.sh" "$RUNTIME/agent"
chmod +x "$RUNTIME/agent"
export NEBULA_AGENT_CMD="$RUNTIME/agent"
