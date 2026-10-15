# 0.53: hardening

Everything Jay asked for (parity, one verb registry and state model, a lower learning curve, accounts in the apps,
storyboards, drag ordering, kanban everywhere, diff with tree filters) shipped in 0.49–0.52. 0.53 adds no features.
It looks for the defects the new surface hides, guided by what the Windows e2e suite turned up on its first run
(two real bugs).

## Goals
1. **Upgrading from 0.47/0.48 to 0.53 is safe.** A data dir made by an old release migrates cleanly. Rebound keys
   still work. What's new shows once. Snapshot refs are adopted, not swept. Projects, worktrees, sessions and
   review marks survive.
2. **No visible defect in the main grid.** Find and fix the pane text truncation the e2e lane saw.
3. **CI is a trustworthy gate.** The three tests that failed under load are fixed or quarantined with a reason.
4. **The desktop has end-to-end coverage.** At least the board, drag, the review split, the ship sheet and Hotkeys
   run against a real daemon.

## Deliverables
- `lane/upgrade-path`:
  - A test that downloads nothing. It builds an old-format data dir from the 0.47.0 / 0.48.0 tags with
    `git worktree` + `cargo build` of the old `kete` (or, if too heavy, from fixture files captured from one),
    then runs the current daemon and TUI on it.
  - Checks: migrations, keybindings with TUI and `desktop.*` overrides, What's new once, ref adoption, review
    marks, ui_state, accounts.
  - Fix what breaks.
- `lane/grid-and-flakes`: reproduce the pane truncation (Windows e2e harness plus TUI draw tests of the pane under
  streamed output), fix it, and fix or quarantine `board::a_review_offers_its_session_filter_when_the_daemon_answers`,
  `branch_switch::a_fetch_with_no_remotes_finishes_and_a_raised_quit_stops_one`, and
  `statusline::tests::a_line_whose_background_child_holds_stdout_still_returns`.
- `lane/desktop-e2e`: `tauri-driver` + msedgedriver on Windows (or, if that can't run headless in CI, the demo
  bridge pointed at a real daemon through a small HTTP/WebSocket shim) with tests for the board, drag, review
  split, ship sheet errors and Hotkeys rebinding; a CI step if it runs on windows-latest.

## Release
- 0.53.0, then hand back to Jay with install steps, a smoke-test list and the decisions only he can make. No 0.54
  unless a concrete defect or his feedback calls for one.
