# 0.48: drag-and-drop ordering, kanban everywhere, diff anywhere

Jay, 2026-10-15: "drag/drop for reordering things inside terminal/desktop app — branches/trees etc,
ordering for things. Kanban view/toggle needs to be immediately available/integrated and able to
toggle states/diff views anywhere if in context required."

Rides on release/0.48 with the six lanes already running (paste, accounts, desktop UX, TUI UX,
desktop worktrees view, TUI port). Lane rules: .superpowers/sdd/2026-10-15-release-048/brief.md.

## Goals

1. **One ordering, both apps.** Projects, worktrees within a project, and sessions within a worktree
   have a user order that persists in the daemon and is the same in the TUI and the desktop.
2. **Drag to reorder** in both apps: project tabs / rail repos, worktree bands / worktree groups,
   session cards / rows. Keyboard equivalents (move up / move down) for every drag.
3. **Drag to act on the board.** Dropping a card on a lane does what that lane means, where the move
   is something the user can cause: Unread → Done = mark seen; any → Done from Needs you or Running is
   refused with a reason (status is the agent's); a new **Archived** drop zone archives; dropping on a
   repo / worktree target moves the session (existing `MoveAgent`). Order within a lane follows the
   session order from goal 1 when the user has set one, else status age.
4. **Kanban one keystroke away, everywhere.** The board toggle is reachable from every view (session,
   orchestrator, diff, worktrees, ADO views, settings closes to it) in both apps. In the desktop the
   board can sit in the split's left side while a session or diff is open (the overview already does);
   in the TUI the board keeps the pane on the right like the grid does.
5. **Diff in context, anywhere.** Wherever a session, card, worktree row/band or PR is focused, one key
   (desktop `Ctrl+Shift+D` / `d` on cards; TUI `g`) and a visible button open the diff for that
   checkout, and closing it returns to where it was opened (the 0.47 desktop fix, extended to the TUI's
   git diff overlay and to the board).

## Deliverables

### Phase A: daemon and protocol (lane `lane/order-core`, starts now)
- `ClientRequest::Reorder { req_id, scope, ids }`: scope = projects | worktrees(project) |
  agents(worktree); `ids` is the full new order of that scope; the daemon writes `sort_order`
  0..n in one transaction, ignores unknown ids, appends missing ones in their old order, and
  broadcasts the upserts. Backward compatible: new request, older daemons answer with an error the
  clients turn into "update the daemon to reorder".
- Protocol version bump with `MIN_COMPATIBLE` unchanged so 0.47 clients still connect.
- Snapshot/order rules documented: every client sorts by `sort_order`, then its existing tiebreak.
- Main checkout stays first among a project's worktrees (refused otherwise).
- `kete reorder` CLI is not needed.
- Tests: store transaction, partial lists, unknown ids, main-first rule, broadcast.

### Phase B: UI (after Phase A and the UX lanes merge, to avoid colliding with them)
- **Desktop (`lane/desk-order`)**: HTML5 drag with a visible drop indicator (1px ember rule) on the
  rail repos, worktree groups (from the worktrees-view lane), session rows, and board cards; board lane
  drops per goal 3, plus the Archived zone; `Alt+↑/↓` move-up/down on the focused row; the board
  toggle and diff button in every view's header; diff opened from the board returns to the board.
- **TUI (`lane/tui-order`)**: extend the existing card drag (mouse_modes.rs `handle_left_drag`, which
  today moves cards between bands / tabs) to reorder within a band, reorder bands, and reorder project
  tabs; board drags per goal 3 with an Archived lane shown while dragging; `Alt+↑/↓` move keys added to
  ACTIONS; `⇧K` works from every overlay that isn't a text field (fullscreen pane, git diff, settings
  closes into it); `g` diff from the board returns to the board.

### Phase C: release
- Merge order: lanes from the first wave as they finish → order-core → accounts TUI port → desk-order
  and tui-order.
- Gates: per-crate tests on the machine (memory), the full matrix on CI (PR into main), Linux clippy
  via CI.
- CHANGELOG 0.48.0, version bump, tag, release assets check.

## Out of scope
- Dragging to change an agent's status (needs you / running): the agent owns it.
- Reordering branches that have no worktree (the ADO Branches list sorts by the server's order).
