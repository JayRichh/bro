# 0.49: one bench, both apps

Jay, 2026-10-15: "ensure more parity and focused delivery on the shared infrastructure design principles …
cleaner or more intuitive native design features/actions here to reduce learning curve? verbose currently with
qty of commands/keybinds. Did we finish ability to freely set/bind add crud for user accounts without needing
commands run? … Conceptualize 3 ideas or ways to refactor storyboards … scope anything needing refactor more deeply"

Storyboards and audit: https://claude.ai/artifact/1kbC1QZRCUDWbYk8d1YRRY. Recommended direction: **B (scope and the
review lane) with A's verb bar**. Jay may redirect to A or C before phase 3 starts.

Principle (docs/design/kete-desktop-brief.md: "keys and mouse both work everywhere"; session and diff open as a
split so the overview stays visible): one registry of verbs and one model of state, computed once in the shared
crates (nebula-core, nebula-config) and rendered by both apps. No logic is written twice in TS and Rust when
src-tauri can call the Rust.

## Phase 1: foundation (three lanes in parallel)

### `lane/core-model`: lane, attention, order rules in nebula-core
- `nebula_core::board::{Lane, lane_of, attention_tier}` and `nebula_core::order::{has_user_order, refill}`,
  pure functions over the snapshot types. TUI `launcher.rs lane_of`, `palette.rs` tier and `order.rs` call them.
- src-tauri computes lane and tier per agent with the same functions and passes them in the payload the frontend
  already receives (no wire change); desktop `status.ts laneOf`, `palette.ts` tier and `order.ts` rules read them.
- Fix: desktop counts terminated-unseen sessions as unread; TUI and daemon don't. Core decides (Finished and unseen).
- Tests: one table of agent states → lane/tier in core; desktop tests updated to the shared output.

### `lane/accounts-gaps`: finish accounts without commands
- Replace `profiles::login_hint` with `accounts::login_command` in `kete profile list`, desktop Settings→Profiles
  and the TUI add/duplicate follow-up.
- Fold desktop Settings→Profiles into the Accounts view (Settings links to it).
- TUI: env at add; clear env at edit.
- Rename an account (moves its home dir, updates project defaults and sessions' profile names) and purge on remove
  in both apps, confirmed, same core function as the CLI.
- CLI parity: `kete spawn --profile`, `kete profile spread <mode> [--threshold]`, `kete session move-account`
  (only if the existing SwitchAgentAccount request serves it; otherwise report).

### `lane/registry`: one verb registry
- nebula-config `ActionSpec` gains: footer word, destructive flag, applies-to (Global, Repo, Worktree, Session,
  Terminal, Account), works-in (Grid, Board, Pane, Diff, Overlay), desktop default chords. Add only; ids stable.
- src-tauri `actions_list` command; a committed generated `desktop/src/lib/actions.gen.ts` plus a Rust test that
  fails when it drifts. `shortcuts.ts` derives its table from it.
- Desktop key rebinding in Settings→Hotkeys, stored under the existing `keybindings` config key with a desktop
  section; the TUI's hotkeys tab stops being shown as the desktop's.

## Phase 2: keys and the verb bar (after phase 1 merges; one TUI-dispatch lane at a time)
- `lane/keys`: the key table in the storyboard page (d = diff, Del = delete, Space = reply, Enter = open, f = filter,
  ⇧R = refresh, digits = repos, z = full screen). Old chords kept as aliases where harmless; destructive ones not.
  CHANGELOG "Keys changed".
- `lane/verbs`: merge TUI `MenuAction` into `Action` with an optional target; `?` opens the selection's verbs
  (searchable, keys shown, full help one row down) in both apps; footer shows the selection's top four verbs from
  the registry; TUI menus show keys.

## Phase 3: storyboard B
- Scope: a core `Scope { repo, worktree, session, account }` and `Place`; a removable scope chip in both headers;
  board, attention strip, bands, quick prompt and diff follow it; digits and band clicks set it.
- Board lanes Running → Needs you → Review → Ship → Done; finished-unseen sessions land in Review.
- Review unit: board left, session/worktree diff right, against the merge-base with `worktree_base_branch` or the
  default branch; tree filter (repo → worktree → file); verbs reply, PR, merge, archive, delete worktree.
- Drops: Ship opens the PR composer (GitHub via gh, ADO via kete-ado); Done archives and offers the safe worktree
  delete. TUI by mouse and keys alike.

## Deeper, not in 0.49
- Record a worktree's base when it is cut (daemon), replacing the derived merge-base.
- Per-session attribution: snapshot HEAD and the dirty tree when each session starts.

## Release
- PR release/0.49 → main, CI on three OSes as the gate, CHANGELOG 0.49.0 (overlay copy too), version bump, tag,
  assets check.
