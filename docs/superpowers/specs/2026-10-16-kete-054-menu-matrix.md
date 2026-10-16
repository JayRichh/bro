# Kete 0.54 menu matrix v2: every right-click, dropdown and quick action, both apps

Lane `lane/menu-matrix`, 2026-10-16. Jay asked for "ULTRALOVE" right-click and quick actions, context awareness, and connections between PRs, sessions, worktrees, branches, accounts, ports and tasks. This is the agreed matrix that `lane/desk-menus` and `lane/tui-menus` build, and that every other lane follows when it adds a row. It does not build UI.

Sources:
- Today's inventory of both apps, read from the code at release/0.54 (d26634a, after the foundation c97ac12). The desktop inventory lists every row with file:line. The TUI inventory does the same and counts each menu.
- The survey of comparable tools: Conductor, Vibe Kanban, the Claude Code desktop app, the Codex app, Superset, cmux, Warp, VS Code, JetBrains, GitHub Desktop, Fork, GitKraken, Windows Explorer, Windows Terminal, Linear, Raycast, GitHub web and Azure DevOps.
- The spec's own matrix, in `docs/superpowers/specs/2026-10-16-kete-054-reach-design.md`. This document replaces that matrix.

Contents:
1. [The grammar](#1-the-grammar-rules-every-menu-follows)
2. [Registry changes made by this lane](#2-registry-changes-made-by-this-lane)
3. [The submenu contract](#3-the-submenu-contract)
4. [State vocabulary](#4-state-vocabulary)
5. [Targets](#5-targets) (session, checkout submenu, terminal, worktree, repo, folder, file, account, PR, branch, task, port, orchestrator, background, lane header, scope chip, breadcrumb, attention strip, critter, Whakaoho, text, terminal area, viewer text)
6. [Connections](#6-connections)
7. [Multi-select](#7-multi-select)
8. [Defects the menu lanes fix](#8-defects-the-menu-lanes-fix-acceptance-criteria)
9. [Verdicts](#9-verdicts)
10. [Who builds what](#10-who-builds-what)
11. [Counts before and after](#11-counts-before-and-after)

Notation:
- `id` is a registry verb (crates/nebula-config/src/actions.rs).
- `local:x.y` is a row each app builds itself, with the same words in both apps.
- D is the desktop window, T is the terminal app.
- Sections are A act, G go, O open, C copy, Ch change, Or organise, W away (see §1).
- "Hide" means the row is left out. "Disable" means the row is shown greyed, with the tooltip text given. The TUI has no disabled rows (§1.8), so it leaves out both kinds.

---

## 1. The grammar (rules every menu follows)

**1.1 One menu per target, whatever opened it.** Every way in builds the same menu for the same thing:
- right-click
- the row's `⋯`
- Shift+F10 / the Menu key (`row_menu`)
- Enter on a Worktrees-view row
- `?`, which adds `More ▸` and `All keys…` (§3.4)

The palette lists the same verbs, flattened. Two builders for one target is a defect. The worktree's two desktop menus (§8.1) are the case today.

**1.2 Right-click acts on the clicked thing.** The menu is never built for the selection when that differs from what was clicked. The selection still moves to the clicked thing, so the keyboard follows. TUI: `hit_to_target(app, &HitTarget)`; the menu never depends on `on_selection()` (§8.6).

**1.3 Sections, in this order.** A rule (desktop only) is drawn where the section changes:

| Section | What it holds | Examples |
|---|---|---|
| A act | the thing's own verb | Open, Follow-up, New session, Run |
| G go | look at what it is connected to | Diff, Review, Pull request ▸, Focus here, remote, parent session |
| O open | another app | Open in ▸ |
| C copy | the clipboard | Copy ▸ |
| Ch change | change it in place | Mark seen, Restart, Duplicate, Rename, Make orchestrator, Switch branch, Wake |
| Or organise | move it among the rest, or onto another worktree or account | Move ▸ |
| W away | archive, delete, remove | Archive, Delete… (always last, behind a rule) |

The registry stores this as `MENU_SECTIONS` / `section_of(id, kind)`. A test enforces that every `MENUS` list runs through the sections in order and that every destructive verb sits in W.

**1.4 Submenus fold runs of rows.** Fold rule:
- Registry groups (`MENU_GROUPS`, `group_of(id, kind)`) and local groups fold the same way.
- On a kind the group lists, two or more of its rows that mean something now fold into one `Title ▸` row. It takes the place of the first row.
- A group with one row left does not fold. That row stays a plain row in its whole words (`menu_label`, e.g. "Open in VS Code").
- Inside the submenu each row uses its short word ("VS Code").
- Submenus never nest:
  - Inside a submenu, groups stay flat. Inside `Worktree ‹branch› ▸`, Move up and Move down are two rows, not `Move ▸`.
  - A row that opens a picker (Move ▸ To another account…, Move to…) opens it as the dialog D has today, or as a child menu in T. It is never a third level of submenu.
- Short menus never fold: file, folder, terminal, account and every local menu under 8 rows.

| Group | Title | Folds on | Rows (short words) |
|---|---|---|---|
| `pull_request` | Pull request | session, worktree | Open in browser, Create…, Merge…, Comment…, Open the issue |
| `remote` | Remote (shown as GitHub or Azure DevOps when the app knows the host) | repo | Open in browser, Pull requests…, Issues…, Azure DevOps…, Refresh pull requests |
| `open_in` | Open in | session, worktree, repo | Open command, VS Code, Visual Studio, Explorer, Browser, Dock editor beside Kete |
| `copy` | Copy | session, worktree | Path, Branch name, Pull request link |
| `move` | Move | session, worktree, repo | To another worktree…, To another account… (session), Up, Down |
| `view` | View | background | Overview, Board, Board or bands, Orchestrator, Accounts, Whakaoho, Archived sessions |
| local `checkout` | Worktree ‹branch› / Checkout ‹branch› | session, terminal | the checkout's menu less the card's rows (§5.2) |
| local `more` | More | `?` only | the verbs `?` lists beyond the menu (§3.4) |
| local `vote` | Vote | Azure DevOps PR | Approve, Approve with suggestions, Wait for author, Reject, Reset |
| local `go_worktrees` | Worktrees | repo | jump list of the linked worktrees |

Within the Open in group:
- Only detected editors are listed (Insiders, Cursor, VSCodium and the rest come from lane/editors' detection), each with its own icon.
- The default editor is checked.
- The browser row names its port: "Browser · localhost:5173".

**1.5 Stable order; state hides, disables or relabels, never reorders.**
- **Hide** a row that can never apply to this thing as it is: Delete worktree on a root checkout, Switch branch on a linked one, Visual Studio without a solution, a non-Claude account's Wake.
- **Hide** the other half of an exclusive pair: Archive and Unarchive; Create and Merge swap in the same slot.
- **Disable**, with a one-line reason (desktop tooltip), a row that applies to the thing but can't run now: Merge on a draft, Dock with no editor window open.
- **State-only rows** (Log in again on an auth failure) appear at their own section's place. They never push the other rows around in their own section.
- A label may turn: Run/Stop run, Make/Unpin orchestrator, Focus here/Widen to the repo, Merge…/Merge… (draft).

**1.6 Words.**
- Sentence case.
- `…` only when a dialog or picker follows.
- The kind's own words first (`MENU_LABELS`).
- A destructive row is red and keeps its confirm.
- Every row with a key shows it. Desktop shows all chords. T shows the first, as today.
- A disabled row's tooltip is the reason, never empty. This fixes the four empty ones in §8.4.
- Copy rows toast what they copied, e.g. "Copied C:\Code\bro".

**1.7 State banner.**
- Desktop: a menu whose thing is in a notable state opens with a faint small-caps heading naming the state, e.g. `kete-api · limited, resumes 15:45` or `needs you · permission`.
- T: the same words go in the menu's title, after the thing's name.
- The banner list for each target is in §4.

**1.8 TUI parity rule.** T has no headings, rules or disabled rows. It leaves them out, so a disabled desktop row is a hidden T row. The words, order, submenus and keys are the same in both apps. Each place T builds a menu says this in a comment (tui-menus).

**1.9 Size.** About 12 top-level rows in the common state. The orchestrator approved three trims to the session card on 2026-10-16, and all three are applied:
1. D's Review… is the first row of Pull request ▸ (§5.1 row 7).
2. Make orchestrator is left off a card while another session is pinned. Both apps do this already: T `menu_for`, D `agentVerbs`. To move the role, Unpin the pinned session first, from its own menu or the palette's Unpin orchestrator. In T, `?`'s More ▸, the palette and a bound key still reach Make directly.
3. Account ▸ folds into Move ▸ as "To another account…" (registry: `move_to_account` is in the Organise section and the `move` group).

The resulting counts:
- **Live session card, read, another session pinned:** 13 in D and 13 in T. That is 12 registry rows plus the card's Worktree ▸.
- **The same card with no orchestrator pinned:** 14 in both, because Make orchestrator shows.
- **The fullest live card in D:** 17, adding Log in, Open parent session and Mark seen.
- **The repo:** 15 in D in the common state. No trim applies to it. Its extra rows are the desktop's own: Default account ▸, Locate folder… and Worktrees ▸.

`menus_follow_their_sections_and_fold_into_groups` caps the registry rows, folded and at their fullest:
- session 14 (a live card, so Unarchive is left out);
- worktree 15 (a linked checkout, so Switch branch is left out);
- repo 14, terminal 4, file 7, folder 4, account 4, background 8.

Each cap equals the current maximum, so any new row has to make its case.

**1.10 Type-ahead.**
- Every menu filters as you type (desk-menus adds it to the desktop's non-search menus).
- A query searches folded rows too. A hit shows as `Open in › VS Code`, so the keyboard loses nothing to the fold.
- Home/End jump.
- T: in a filtered menu, letters filter. → and ← (not l/h) open and close submenus there.

---

## 2. Registry changes made by this lane

All are in `crates/nebula-config/src/actions.rs`, regenerated into `desktop/src/lib/actions.gen.ts`.

**New verbs** (appended after `critters` at ACTIONS[102..]; unbound; not destructive; T routes them to `pending_verb` in their own match arm, `key_modes.rs`, until a lane takes them):

| id | Label | Applies to | Section | Why it is a verb (both apps; `?`; Ctrl+K; rebinding) | Runs on |
|---|---|---|---|---|---|
| `mark_seen` | Mark seen (worktree, repo, background: Mark all seen) | session, worktree, repo, global (the board's scope) | Ch | Inbox triage across many projects without opening each card. Conductor and cmux have it. | D `markSeen`/`seeAgent` (store.svelte.ts:209); T `ClientRequest::MarkAgentSeen` |
| `copy_pr_url` | Copy pull request link | session, worktree | C | Sharing a PR in chat is the most common copy after the path | D `model.prs[worktree].url`; T `launcher::worktree_pr(..).url`; T copies through `copy_and_flash` (OSC 52-aware) |
| `archive_done` | Archive done sessions… | worktree, repo, global (the board's scope) | W | Clears the Done lane with many projects open. One confirm lists the sessions; U brings any back. | the existing archive per session (D `archiveAgent`, T `ClientRequest::Archive`), over finished, seen, unarchived sessions, not the orchestrator. A session still waiting on its subagents reads Running, not finished. |
| `wake_schedule` | Wake-up schedule… (background: Whakaoho…) | account, global | G (Ch on an account) | Reaches the Whakaoho view from Ctrl+K, the background View ▸ and an account row in both apps | lane/wake's view: D `whakaoho` view; T accounts overlay Wake tab |

**Applies-to widened** (no ids renamed or moved):

| Verb | Added | Reason |
|---|---|---|
| `copy_path` | Terminal | A terminal row's Copy path is its checkout's folder. This changes a foundation row. |
| `open_in_browser` (T `w`) | Session, Repo | The card's and repo's Open in ▸ Browser are their checkout's dev server. D now implements it from `model.ports`. |
| `accounts` | Global | Accounts sits in the background's View ▸ |
| `scope_account` | Account | "Show its sessions" on an account narrows the scope to it with no picker |

**MENUS** (full lists are in the code):
- **Session:**
  - gains `git_diff` (today it only arrived through the checkout rows), `comment_pull_request`, `open_in_browser`, `copy_pr_url` and `mark_seen`;
  - loses `focus_scope`, which moves into the card's checkout submenu and acts the same way (both apps scope a card to its checkout);
  - `move_to_account` moves from the Change rows into Move (section Organise, group `move`, "To another account…"), the third trim of §1.9.
- **Terminal:** activate, copy_path, rename, delete.
- **Worktree:**
  - gains `open_pull_request`. Today it was set in D's table but never listed, so no worktree menu showed it.
  - gains `comment_pull_request`, `open_in_browser`, `copy_pr_url`, `mark_seen` and `archive_done`.
- **Repo:**
  - gains `pull_requests`, `issues`, `azure_devops`, `open_in_browser`, `mark_seen` and `archive_done`;
  - loses `delete_worktree`. In T it deleted whichever checkout the grid was on, an unclear target for a repo menu. That row is still on the band's menu and in the card's checkout submenu.
- **Account:** scope_account, accounts, wake_account, wake_schedule.
- **Global** (new, the background): quick_prompt, add_project, command_palette, [view: home_overview, home_board, toggle_kanban, toggle_orchestrator_view, accounts, wake_schedule, toggle_archived], settings, mark_seen, critters, archive_done.

**MENU_LABELS.**
- Whole words for a row standing alone: "Open in VS Code", "Open in Visual Studio", "Copy branch name", "Dock editor beside Kete". The foundation's short forms moved into `MENU_GROUPS`, where they are a submenu row's words.
- New kind words:
  - Mark all seen (worktree, repo, global)
  - Archive done sessions…
  - Whakaoho… (global)
  - Wake-up schedule…
  - Show its sessions (scope_account on an account)
  - Account details… (accounts on an account)

**New tables and functions:**
- `MenuSection`, `MENU_SECTIONS`, `section_of`, `MenuGroup`, `MENU_GROUPS` and `group_of`, rendered to TS as `MenuSection`, `MENU_SECTION_ORDER`, `MENU_SECTIONS`, `MenuGroup` and `MENU_GROUPS`.
- `desktop/src/lib/verbs.ts` gains `sectionOf` and `groupOf`, the TS twins.

**State rules already in T** (event_loop/verbs.rs `menu_action`).

This lane added rows to T's flat menus before tui-menus folds them. So that T shows no dead row in the meantime, it applies the §5 rules for those rows now:
- comment only with an open pull request;
- the pull request link and the worktree's Open pull request only with a pull request;
- Browser only while a port listens. Never on a T repo menu yet: `w` opens the grid's checkout, not the repo's, so the repo row waits for a row-addressed form (tui-menus);
- Mark seen / Mark all seen only with an unread session;
- Archive done sessions only with a finished, read session;
- on a repo, Azure DevOps only on Azure DevOps remotes, and Pull requests and Issues only elsewhere.

`serves`, `unread`, `done`, `in_project` and `on_ado` there are the predicates tui-menus reuses. `on_ado` reads only the background probe's cache. A repo not probed yet counts as GitHub, so building a menu never runs `git` on the loop. The four new verbs still say "coming in this release" until their lanes route them.

**New kinds:** none. PR, branch, task, port, lane, critter, text and background-only rows stay local (§5). Each has no rebindable verb that both apps need, and every new kind costs about ten switch sites in D (audit-desktop.md "Adding a target kind").

---

## 3. The submenu contract

**3.1 Desktop** (`ui/Menu.svelte`, desk-menus):
- A row with `children` draws `▸`.
- Hover opens it after about 150 ms, with a safe triangle so a diagonal move keeps it open. →, Enter and Space open it too.
- ← or Esc closes one level.
- It flips left or up at the viewport edge.
- A click on a `▸` row opens the child; it never runs anything.
- Checked rows use `menuitemcheckbox` / `menuitemradio`; no "✓ " prefix.
- An icon slot holds: the editor's own icon, a file icon, a status dot, an account glyph.

**3.2 TUI** (tui-menus):
- **Child items.** Add a generic `MenuAction::Submenu { title, items }`, or `SubmenuKind::Rows`, carrying child `MenuItem`s. `build_submenu` (actions.rs:630) handles it next to the model/effort kinds.
- **Opening.** → (and `l` when there is no filter), Enter, or a left click on a `▸` row opens the child, with `parent` set as today (overlays.rs:260). Enter on a `▸` row never runs it.
- **Drawing.** The child replaces the parent in place, drill-in style, which is what the slot model supports. It keeps the parent's anchor `at`, not centred. Its title is `Parent › Child`.
- **Closing.** ← and Esc go back one level; a click outside closes the whole stack, as today.
- **Filter.** In a filtered menu (`?`), typing filters across the flattened children. A hit's label reads `Open in › VS Code`. Enter runs it directly.
- **Mouse.** `Moved` sets `hover`. The wheel scrolls a long menu. Only the left button or Enter runs a row. A right-click while a menu is open closes it and reopens the menu for the thing under the new spot.

**3.3 Picker rows.** `move_to_account` is the row Move ▸ To another account…:
- D opens today's pick dialog, then today's confirm (`moveToAccount`, accounts/actions.ts:52).
- T opens `account_switch::open_picker`'s menu as a child, with `parent` set, so ← comes back to Move ▸.
- The pick lists each target as "Restart on ‹account›…", with "(default)", "(off)", "(not signed in)" and, from lane/limits, "(limited · resets 15:45)".
- `move_session` (To another worktree…) stays a dialog, since its list of worktrees can run long.

The session's account itself (usage and settings, Accounts…) is reached from the session header's account chip in D and the pane's AccountChip in T. Both open today's account menu (`accountMenu`, accounts/actions.ts:40).

**3.4 `?` in T.** It shows the folded `menu_for` rows, then one `More ▸` row (the `more_rows`: Subagents, Full screen, Find file, Grep, Tree, Agent presets, Screenshot), then `All keys…`. `more_rows` counts folded children as shown, and leaves out rows that mean nothing now (§8.7). D's `?` is the same menu with its search field, plus `All keys…`.

**3.5 Counts for T's `?`** on a live session card with an open PR, linked checkout, Grid:
- Today: **43** rows (menu_for 32 + 10 extras + All keys).
- After, with another session pinned: **15** (Open, Follow-up prompt…, Diff, Pull request ▸, Open in ▸, Copy ▸, Restart agent, Duplicate, Rename…, Move ▸, Archive, Delete…, Worktree ‹branch› ▸, More ▸, All keys…). With no orchestrator pinned it is 16, because Make orchestrator shows.
- Right-click on the same card: 32 today, **13** after (14 with no orchestrator pinned): the same rows less More ▸ and All keys.

---

## 4. State vocabulary

Unknown is not "no". Where a fact isn't loaded, the row shows:
- D's `rows.git` fills only while Overview, Board or Worktrees is mounted (worktrees/rows.svelte.ts:134).
- The editor instance check runs on menu open.
- An absent field on an older daemon counts as unknown too.

| State | D data | T data | Banner | Notes |
|---|---|---|---|---|
| session running / needs you / finished / unseen / fresh / terminated / disconnected | `Agent.status`, `.unseen`, `.alive` (api.ts:34) | `Agent` entities.rs:221 | needs you · ‹reason›; running; done · unread; disconnected · restart to resume | |
| session archived | `.archived` | same | archived | only Unarchive, Duplicate, Delete… |
| session limited (resumes at) | `limit_until` | same | limited · resumes 15:45 | from lane/limits (protocol 62), conditional until it merges |
| session blocked on a dialog | `stop_reason`/needs-you reason | same | needs you · a question / permission | from lane/limits |
| session auth failed / billing / account hold | `stop_reason` | same | needs you · log in again (‹account›) | from lane/limits. Adds the state-only row "Log in to ‹account›…" (A). Without lane/limits: the row shows when the session's account has `login: needs_login` or `expired` |
| session error (overloaded…) | `stop_reason` | same | error · overloaded | from lane/limits |
| orchestrator | `orchestratorOf(model)` | `role.is_orchestrator()` | orchestrator | Make/Unpin label |
| cloud session | (not mirrored) | `cloud_session_id` | | T: Open = "Open in browser", Follow-up = "Send to cloud session". Editor/copy/restart rows hidden. D lacks the field (LATER) |
| spawned (has parent) | `.parent` | same | | adds "Open parent session ‹name›" (G) |
| issue session | (not mirrored) | `issue_url` | | T: Pull request ▸ Open the issue. D LATER (field not in api.ts) |
| worktree main vs linked | `Worktree.is_main` | same | | |
| worktree dirty | `rows.git[w].changed/untracked` | `worktree_changes` (app.rs:5126) | | label "Diff (3)" optional; delete confirms already list changes |
| worktree ahead/behind | `rows.git.ahead/behind/upstream` | none (only inside delete_check.rs) | | no BUILD row is gated on behind (T has no field) |
| on the base branch | `rows.git.on_default_branch` | `worktree_base.on_default_branch` | | Create PR disabled (D) / hidden (T) |
| PR open / draft / merged / closed / none | `model.prs[w].state`, `.draft` | `worktree_pr(..).standing` | | §5.1 Pull request ▸ |
| running a server | `app.runCommands[w]` + run terminal alive; `model.ports[w].live` | `worktree_running`, `tree.ports` | | Run/Stop; Browser · localhost:N; Show run terminal |
| placeholder worktree | | `is_placeholder_worktree` | | every checkout row hidden (as today) |
| repo has 0/1/many linked worktrees | `model.worktrees` | same | | sweep and Worktrees ▸ only with linked worktrees |
| repo remote GitHub / Azure DevOps / none | `app.adoCtx[p]`; GitHub when the PR poller knows the remote | ado.rs:31-65 check; gh remote | | Remote ▸ rows per host (§5.5) |
| account default / disabled / logged out / expired / problem | `AccountRow` api.ts:122 | `AccountRow` accounts_overlay.rs:23 | logged out; switched off | |
| account limited / near limit | `usage.windows[].percent`, `resets_at` | `usage` | limited · resets 15:45 / 92% of 5h | |
| account Whakaoho on | lane/wake schedule (config.local.json `wake`) | same | | Wake rows' words |
| file tracked / untracked / staged / deleted / binary / huge | `DiffFile.status`, hunks staged list, files.rs caps (2 MB / 10k lines) | `DiffView` files `xy`; FileTabs caps | | §5.7 |
| remote app (T over ssh) | n/a | `chrome.is_remote` | | hides every local-editor/Explorer/dock row (Copy rows stay; they go through OSC 52) |
| daemon too old | `canSwitch` (protocol ≥ 57) | account_switch.rs:137 | | Move ▸ To another account… disabled: "The running kete daemon is older than this window: restart it (kete kill, then reopen) to move sessions between accounts." |

---

## 5. Targets

Each table lists rows in order. Rows marked BUILD are 0.54 work for the lanes in §10. "Today" says what exists now. "Entry points" lists every way in; per §1.1 they all show the one menu.

### 5.1 Session (agent)

Entry points:
- **D:** SessionTree row, Board card, card `⋯`, Session pane header and `⋯`, OrchestratorPanel row, attention strip list item, `?`, Shift+F10, palette.
- **T:** grid card, kanban card (no menu today), card `⋯`, kanban `⋯`, pane-header `⋯`, `?`, Ctrl+K.

| # | Row (words) | id | Sec | Key | Show | Disable (D tooltip) | Verdict |
|---|---|---|---|---|---|---|---|
| 0 | banner (§1.7) | local:session.banner | — | | notable state | | BUILD |
| 1 | Log in to ‹account›… | local:session.login | A | | auth failed, or its account needs login | | BUILD (D `runLogin`, accounts/actions.ts:131; T accounts login `l` path) |
| 2 | Open (cloud: Open in browser) | activate | A | Enter | live | | today |
| 3 | Follow-up prompt… (cloud: Send to cloud session) | follow_up | A | Space | live | | today |
| 4 | Diff | git_diff | G | D | live | | BUILD (MENUS; was only in the checkout rows) |
| 6 | Open parent session ‹name› | local:session.parent | G | | `parent` set and that session live | | BUILD (D `openSession(parent)`; T Attach) |
| 7 | Pull request ▸ Review… (D) · Open in browser · Create… / Merge… · Comment… · Open the issue | local:session.review, open_pull_request, create_pull_request, merge_pull_request, comment_pull_request, open_issue | G | —, ⇧V, —, —, y, ⇧I | see the PR rules below. Review… (D: the orchestrator review sheet for a live session) is the submenu's first row (§1.9 trim 1); with no pull request rows it stands alone as "Review…" | Merge on a draft: "Draft pull request: publish it first". Create on the base branch: today's `why` | BUILD (fold + state fixes; Review moves in). T: Review LATER (the review dock opens from the Review lane only) |
| 8 | Open in ▸ VS Code · Visual Studio · Explorer · Browser · localhost:N · Dock editor beside Kete | open_in_vscode, open_in_visual_studio, reveal_in_explorer, open_in_browser, dock_editor | O | | local, live, not cloud; T not remote; each editor only when detected; Visual Studio only when the checkout has a solution or project; Browser only when a port is live | Dock: "No VS Code or Visual Studio window is open on this checkout" | BUILD (lane/editors runs them; menu lanes place them) |
| 9 | Copy ▸ Path · Branch name · Pull request link | copy_path, copy_branch, copy_pr_url | C | | live; link only with a PR | | BUILD |
| 10 | Mark seen | mark_seen | Ch | | `unseen` | | BUILD |
| 11 | Restart agent | restart | Ch | | local (not cloud), live | | today |
| 12 | Duplicate | duplicate_session | Ch | ⇧P | always | | today |
| 14 | Rename… | rename | Ch | R | live | | today |
| 15 | Make orchestrator / Unpin orchestrator | pin_orchestrator | Ch | | local. Make only while no other session is pinned; Unpin on the pinned one (§1.9 trim 2) | | BUILD (rule applied in both apps) |
| 16 | Move ▸ To another worktree… · To another account… · Up · Down | move_session, move_to_account, reorder_up, reorder_down | Or | M, —, Alt+↑/↓ | live; To another account… local (not cloud); Up/Down not from a subagent's row | To another account… on an old daemon (§4) | BUILD (fold; the account move is §1.9 trim 3, picker per §3.3) |
| 17 | Archive (from a subagent row: Archive parent session…) | archive | W | A | live | | today |
| 17′ | Unarchive | unarchive | W | U | archived | | today |
| 18 | Delete… | delete | W | Del | always | | today |
| 19 | Worktree ‹branch› ▸ (§5.2) | local:session.checkout | — | | live card | | BUILD (replaces the flat checkout rows) |

Pull request rows by state (session and worktree):

| PR state | Rows in Pull request ▸ |
|---|---|
| none, not on base | Create… |
| none, on base | Create… disabled with today's reason (D) / hidden (T) |
| open | Open in browser, Merge…, Comment… (T; D LATER: no GitHub comment box in D) |
| draft | Open in browser, Merge… disabled ("Draft pull request: publish it first"), Comment… |
| merged / closed | Open in browser only, labelled "Open merged pull request" / "Open closed pull request". Create… does not show. This fixes today's Open + Create pair (§8.5). |
| issue session | + Open the issue (T; D LATER) |

Fold: with one row left, that row stands alone in its whole words, e.g. "Create pull request…".

Archived card: Duplicate, Unarchive, Delete…, with the banner. No checkout submenu, no More ▸ extras in T (§8.7).

Removed from the session's own rows: Focus here. It is now in the checkout submenu (§5.2), with the same effect in both apps.

### 5.2 The card's checkout submenu: `Worktree ‹branch› ▸` (`Checkout ‹branch› ▸` for the root)

This is the checkout's own menu (§5.5) less every verb the card already shows, flat. T's existing dedupe (verbs.rs:402-416) does the filtering. Inside, Move rows read "Move worktree up / down", and no group folds again.

Typical contents: New agent… (T), New session…, New terminal, Run / Stop run, Show run terminal (when it runs), Focus here / Widen to the repo, Open remote in browser, Switch branch… (root), Mark all seen, Move worktree up / down (linked), Archive done sessions…, Delete all sessions…, Delete worktree… (linked).

Verdict: BUILD. It replaces 9 to 19 flat rows on every card in both apps.

### 5.3 Terminal row (D SessionTree terminal row; T terminal card)

| Row | id | Sec | Show | Verdict |
|---|---|---|---|---|
| Open | activate | A | | today |
| Copy path (its checkout's folder) | copy_path | C | | BUILD (widened to Terminal) |
| Rename… | rename | Ch | | today |
| Close terminal… | delete | W | | today |
| Worktree ‹branch› ▸ | local:session.checkout | — | | BUILD |

A run terminal shows the RunButton as today. Its own menu has no Run/Stop row; that row lives in the checkout submenu.

### 5.4 Terminal area (xterm; owned by lane/desk-quiet)

| Row | Show | Verdict |
|---|---|---|
| Copy | selection exists | BUILD (desk-quiet) |
| Paste | always; writes through the same paste path as Ctrl+V (bracketed paste rules unchanged) | BUILD |
| Select all | | BUILD |
| Clear | | BUILD |
| Copy folder path | | BUILD |
| Open link | xterm's link addon has a link under the pointer | BUILD |

Never paste on a bare right-click. The survey found that Windows Terminal and VS Code paste on right-click by default on Windows, and that would send clipboard text into a running agent. T: right-click on the pane PTY does nothing, as today. The pane header `⋯` is the menu.

### 5.5 Worktree

Entry points:
- **D:** WorktreeBand, WorktreesView row, breadcrumb worktree part, `?`.
- **T:** empty or folded band, columns Worktrees row, `?`.

T's non-empty band rule gives the card menu today; tui-menus makes it give this one (§8.6).

| # | Row | id | Sec | Key | Show | Disable (D) | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | New agent… | new | A | N | T only (D has no harness picker row) | | today T |
| 2 | New session… | quick_prompt | A | P | | | today |
| 3 | New terminal | new_terminal | A | T | | | today |
| 4 | Run / Stop run | toggle_run | A | | a run command, or a live run terminal (D's band rule wins over today's registry rule) | | BUILD (predicate fix §8.1) |
| 5 | Diff | git_diff | G | D | | | today |
| 6 | Pull request ▸ Open in browser · Create… / Merge… · Comment… | open_pull_request, create_pull_request, merge_pull_request, comment_pull_request | G | ⇧V | the PR rules in §5.1 | as §5.1 | BUILD (Open PR is new on this menu) |
| 7 | Focus here / Widen to the repo | focus_scope | G | | label turns when the scope is this checkout | | BUILD (the flip moves from D's hand-built menu into the shared one) |
| 8 | Open remote in browser | open_repo | G | ⇧G | a remote | | today |
| 9 | Show run terminal | local:worktree.run_terminal | G | | the run terminal is alive | | BUILD |
| 10 | Open session ▸ (live sessions and terminals, up to 9) | local:worktree.sessions | G | | D on surfaces where its sessions aren't on screen (Worktrees view, breadcrumb, a PR or branch jump) | | BUILD (connection) |
| 11 | Open in ▸ Open command · VS Code · Visual Studio · Explorer · Browser · localhost:N · Dock editor beside Kete | open_worktree, open_in_vscode, open_in_visual_studio, reveal_in_explorer, open_in_browser, dock_editor | O | ⇧O (Open command) | as §5.1. Open command only when the project or `.kete.json` has one. | Dock as §5.1 | BUILD. "Open command" replaces the ambiguous "Open in editor" next to "VS Code" (§8.8). |
| 12 | Copy ▸ Path · Branch name · Pull request link | copy_path, copy_branch, copy_pr_url | C | | link only with a PR | | BUILD |
| 13 | Mark all seen | mark_seen | Ch | | an unread session in it | | BUILD |
| 14 | Switch branch… | switch_branch | Ch | C | root, and no Azure DevOps remote | | today |
| 15 | Move ▸ Up · Down | reorder_up, reorder_down | Or | Alt+↑/↓ | linked | | BUILD (fold) |
| 16 | Archive done sessions… | archive_done | W | | a finished, seen session in it | | BUILD |
| 17 | Delete all sessions… | delete_all | W | ⇧D | something to delete | | today (now on D's band menu too) |
| 18 | Delete worktree… | delete_worktree | W | Del on the row | linked. Label "Delete worktree (pull request merged)…" when its PR merged | | today + label |

Banner: none, or `merged` / `on the base branch` / `running :5173`.

### 5.6 Repo / project

Entry points:
- **D:** RepoRow header, Rail repo (compact and full; no menu today), breadcrumb repo part, `?`.
- **T:** project tab, project row, `?`.

| # | Row | id | Sec | Key | Show | Verdict |
|---|---|---|---|---|---|---|
| 1 | New session… | quick_prompt | A | P | | today |
| 2 | New worktree… | new_worktree | A | ⇧N | | today |
| 3 | Run / Stop run | toggle_run | A | | the main checkout has a run command | today |
| 4 | Focus here / Widen to everything | focus_scope | G | | label turns when this repo is the scope | today + flip |
| 5 | Remote ▸ (GitHub ▸ / Azure DevOps ▸) Open in browser · Pull requests… · Issues… · Azure DevOps… · Refresh pull requests | open_repo, pull_requests, issues, azure_devops, refresh_pull_requests | G | ⇧G, v, i, ⇧B, ⇧R | Pull requests and Issues only on GitHub remotes (T today; D LATER: no GitHub lists in D). Azure DevOps only on Azure DevOps remotes (D: opens the repo's Tasks view). Refresh with a remote. | BUILD (fold; removes T's dead Azure DevOps row §8.9) |
| 6 | Worktrees ▸ (each linked worktree → reveal its band; last row "All worktrees…" opens the Worktrees view) | local:repo.worktrees | G | | linked worktrees exist | BUILD. Replaces D's unbounded "Open X in editor / Delete X…" rows. Those functions live on each band's menu and in the Worktrees view. |
| 7 | Open in ▸ Open command · VS Code · Visual Studio · Explorer · Browser · Dock | (as §5.5) | O | | as §5.5, for the main checkout | BUILD |
| 8 | Copy path | copy_path | C | | | BUILD |
| 9 | Mark all seen | mark_seen | Ch | | an unread session in the repo | BUILD |
| 10 | Switch branch… | switch_branch | Ch | C | a root checkout, no Azure DevOps remote | today |
| 11 | Rename… | rename | Ch | R | | today |
| 12 | Default account ▸ (per harness: each account, the current one checked; "Its default") | local:repo.default_account | Ch | | D today as a dialog. T LATER: T sets it from the account side, Use for… | BUILD D (picker as submenu) |
| 13 | Move ▸ Up · Down | reorder_up, reorder_down | Or | Alt+↑/↓ | | BUILD (fold) |
| 14 | Archive done sessions… | archive_done | W | | a finished, seen session in it | BUILD |
| 15 | Locate folder… | local:repo.locate | W | | D | today |
| 16 | Delete merged & clean worktrees… | sweep_worktrees | W | | linked worktrees exist | today |
| 17 | Remove from Kete… | remove_project | W | | | today |

T's columns ProjectRow keeps "Add project…" after the menu, as a background row of that panel.

### 5.7 Folder (D: Files tree in 0.56 and breadcrumb folder parts; T: tree browser rows)

| Row | id | Sec | Show | Verdict |
|---|---|---|---|---|
| Open in VS Code | open_in_vscode | O | detected; T not remote | BUILD (editors + files) |
| Show in Explorer | reveal_in_explorer | O | T not remote | BUILD |
| Copy path | copy_path | C | | BUILD |
| Copy relative path | copy_relative_path | C | | BUILD |
| Expand all / Collapse all | local:folder.expand | Ch | tree only | BUILD (lane/files; T tree browser now) |
| Find in this folder… | local:folder.grep | G | T (grep scoped) | LATER: grep has no scope argument yet |
| New session here | — | | | CUT: sessions run in a checkout, not a subfolder |
| Open in terminal | — | | | LATER (§9) |

### 5.8 File

Entry points:
- **D:** diff file rows, FileSheet tabs, the Files tree (0.56).
- **T:** diff overlay file rows, tree browser, find file, grep results.

| Row | id | Sec | Show | Disable (D) | Verdict |
|---|---|---|---|---|---|
| Open in Kete | open_file_in_kete | A | not deleted | binary: "Binary file: open it with its default app". Over 2 MB or 10k lines: "Too big for Kete's viewer: open it in VS Code" | BUILD |
| Open in VS Code (at the hunk's line from a diff) | open_in_vscode | O | detected, not deleted; T not remote | | BUILD |
| Open in Visual Studio | open_in_visual_studio | O | VS installed and the checkout has a solution | | BUILD |
| Open with default app | open_with_default | O | not deleted | executables and scripts (.exe .bat .cmd .ps1 .msi .lnk .vbs .js .sh): "Kete doesn't run programs: use Show in Explorer" | BUILD (GitHub Desktop's unsafe-extension rule) |
| Show in Explorer | reveal_in_explorer | O | not deleted; T not remote | | BUILD |
| Copy path | copy_path | C | | | BUILD |
| Copy relative path | copy_relative_path | C | | | BUILD |
| Diff this file | local:file.diff | G | from the tree or a finder, the file has changes | | BUILD (T tree/find; D Files tree 0.56) |
| Stage file / Unstage file | local:file.stage | Ch | diff against HEAD (working); Unstage only with staged hunks | | BUILD (D `canStage` hunks API; T `stage_change`) |
| Discard changes… | local:file.discard | W | diff against HEAD, a tracked file | | BUILD (red; confirm names the file) |
| Mark reviewed (checked) | local:file.reviewed | Ch | a review diff | | BUILD (T `toggle_reviewed`; D LATER if D has no file-level mark) |
| Add to session ▸ (insert `@path` into a live session's prompt, no Enter) | local:file.add_to_session | A | | | LATER (lane/files 0.56 owns the insert path with drag) |
| Discard an untracked file (delete it) | — | | | | CUT: deleting files is not Kete's job; Show in Explorer |
| Add to .gitignore | — | | | | CUT: rare; the agent or the editor does it |

### 5.9 Account

Entry points:
- **D:** rail AccountsSection row (no menu today), AccountsView row (buttons and `?` today), session account chip (today's `accountMenu`), Whakaoho view row.
- **T:** accounts overlay row (right-click does nothing today; `?` has 13 static rows).

| # | Row | id | Sec | Show | Disable (D) | Verdict |
|---|---|---|---|---|---|---|
| 0 | banner: logged out / switched off / limited · resets 15:45 / 92% of 5h | local:account.banner | | | | BUILD |
| 1 | Start a session on it… | local:account.start | A | profile, enabled, no problem | | BUILD (D AccountsSection `+` today; T quick prompt with profile) |
| 2 | Show its sessions | scope_account | G | sessions run on it | | BUILD |
| 3 | Account details… | accounts | G | not inside the Accounts view | | BUILD |
| 4 | Log in again… (ember when `needs_login`/expired) | local:account.login | Ch | `loginHint` | | today (buttons) → menu |
| 5 | Copy sign-in command | local:account.copy_login | C | `loginHint` | | BUILD (D has it in the chip panel) |
| 6 | Make default (checked when it is) | local:account.default | Ch | profile, no problem | | today. Fixes the disabled "The default" with no tooltip (§8.4) |
| 7 | Use for ▸ (repos, current checked; "Clear" on the checked one) | local:account.use_for | Ch | profile, no problem | "No repos yet" disabled: "Add a repo first" | today → submenu |
| 8 | Switch on (checked) | local:account.enabled | Ch | profile, no problem | | today |
| 9 | Wake up now | wake_account | Ch | a Claude account | switched off: "Switched off: switch it on to wake it". Logged out: "Not signed in: log in again first". Live window: "Its 5-hour window is running; it resets 15:45" | BUILD (lane/wake) |
| 10 | Wake-up schedule… | wake_schedule | Ch | a Claude account | | BUILD (lane/wake) |
| 11 | Move the session here | local:account.move_here | Ch | T accounts overlay with a session under the grid cursor, same harness, other account | | today T |
| 12 | Edit… / Rename… / Duplicate… | local:account.edit/rename/duplicate | Ch | profile (Rename: named; disabled while sessions block it, with `renameBlocker`'s text) | Edit on a problem row: "Fix this entry in config.json, or rename it" | today |
| 13 | Add as account | local:account.adopt | A | own login (no profile) | | today |
| 14 | Remove… | local:account.remove | W | profile | | today |

T's accounts overlay `?` keeps the overlay's keys for its own controls (Spread mode, Spread threshold, Add an account). Its rows come from this menu, so state filtering applies. Today ACCOUNT_VERBS is static (§8.10).

### 5.10 Pull request (local rows, one builder: D `menus/rows.ts`, T `actions.rs` PR builder)

Entry points:
- **D:** PR chips on RepoRow, WorktreeBand, WorktreesView and the Board card (inert today), Azure DevOps PR rows, palette PR rows.
- **T:** card `#42` line, band PR, columns open-PR group row, PR modal row, Azure DevOps PR row.

| Row | Sec | Show | Disable (D) | Data | Verdict |
|---|---|---|---|---|---|
| Open in browser (chip primary) / Review (Azure DevOps row primary: the in-app review) | A | | | `PrInfo.url`; Azure DevOps `Review.svelte` | today → menu |
| Go to session ‹name› | G | a live session in the worktree whose branch is the PR's head | | D `model.prs` keyed by worktree, reversed; Azure DevOps `BranchRow.session` | BUILD (connection: none of these exist today) |
| Go to worktree ‹branch› | G | a local worktree on its head branch | | `model.prs` key; `BranchRow.worktree` | BUILD |
| Check out in new worktree… | G | no local worktree on its branch, and Kete can cut one there (Azure DevOps today; GitHub via the PR-session path) | | D `branchesApi.checkout`; T ADO branch Enter / `NewAgentOfKind{pr}` | BUILD |
| New session on this pull request ▸ (harness → model → effort, T today) | A | GitHub PR in T | | T `pr_row_menu_items` | today T; D LATER (no GitHub PR session in D) |
| View diff | G | T PR rows | | `ViewPrDiff` | today T |
| Copy ▸ Link · Number (#42) · Branch name | C | | | | BUILD |
| Comment… | Ch | T GitHub | | `CommentPullRequest` | today T |
| Vote ▸ (Approve, Approve with suggestions, Wait for author, Reject, Reset) | Ch | Azure DevOps, not draft, not mine | draft: "Draft pull request: votes open once it is published" | D `Review.svelte` vote buttons; T `v` | BUILD (D menu; T `v` path) |
| Publish | Ch | Azure DevOps draft, mine | | D `Review.svelte:188` | BUILD |
| Merge… | Ch | open, a local worktree on its branch (the existing merge flow needs the checkout) | draft as §5.1 | `openMerge(worktree)`; T `MergePullRequest` | BUILD |
| Refresh pull requests | Ch | | | refresh_pull_requests | BUILD |

States:
- **Merged or closed:** Open, Go to session/worktree, Copy only.
- **To review vs mine:** Vote shows on others' PRs, Publish on yours.

### 5.11 Branch (Azure DevOps Branches rows; the root checkout's branch switcher rows in both apps)

| Row | Sec | Show | Disable (D) | Data | Verdict |
|---|---|---|---|---|---|
| Go to worktree ‹branch› / Check out in new worktree… | A | checked out somewhere / not | | `BranchRow.worktree`; D `branchesApi.checkout`; T ADO Enter | today (detail pane, Enter) → menu |
| Go to session ‹name› | G | `BranchRow.session` | | | BUILD |
| Go to pull request #n / Draft PR… | G | `pr` / no PR and local | Draft PR when one is open: "A pull request is already open" | | today → menu |
| Go to task #n | G | `task` | | `BranchRow.task` → Tasks view | BUILD |
| Diff vs main | G | a worktree or main exists | | Branches.svelte:172 | today → menu |
| Switch to this branch | Ch | branch switcher only (root checkout) | | `switch_branch` flow | today |
| Copy name | C | | | | BUILD |
| Open in Azure DevOps | O | Azure DevOps | | repo URL + `?version=GB‹name›` from `app.adoCtx` | BUILD |
| Delete local… | W | `local` | checked out: "Checked out in a worktree: remove the worktree first" | Branches.svelte:379 | today → menu |

### 5.12 Task (Azure DevOps work item rows)

| Row | Sec | Show | Data | Verdict |
|---|---|---|---|---|
| Start session… / Go to session ‹name› | A | not started / a linked session exists (then it becomes the primary row) | D `StartTask`; linked chips Tasks.svelte:434; T `s s` | today → menu |
| Go to branch ‹name› · Go to pull request #n | G | linked | Tasks.svelte:434-446 | BUILD |
| Draft PR… | G | a branch, no PR | | BUILD |
| Open in Azure DevOps | O | | `WorkItem.url` | today → menu |
| Copy ▸ Id (#1234) · Link · Title | C | | | BUILD |

State: a Completed or Removed task hides Start session.

### 5.13 Port / run chip (D RepoRow, WorktreeBand, WorktreesView `:5173` chips; T band port)

| Row | Sec | Show | Data | Verdict |
|---|---|---|---|---|
| Open localhost:5173 in browser | A | | `api.openUrl`; T `localhost::open_chooser` | today (click) → menu |
| Copy URL | C | | | BUILD |
| Show run terminal | G | the run terminal is alive | terminal with `run_command` and `alive` | BUILD (port → session connection) |
| Screenshot | G | T | `screenshot` (⇧W) | today T |
| Stop run / Run | Ch | a run command | toggle_run | BUILD |
| Restart run | — | | | LATER: needs stop-then-start sequencing in the backend |

### 5.14 Orchestrator

| Surface | Rows | Verdict |
|---|---|---|
| Rail entry (D) / orchestrator chip (T) | Open orchestrator · Review waiting (N)… · rule · the orchestrator session's menu (§5.1) as `Session ▸` · Start a new orchestrator… · Unpin orchestrator | BUILD (no menu today) |
| OrchestratorPanel background (D) | Open orchestrator · Review all waiting (N)… · Start a new orchestrator… | BUILD |
| OrchestratorPanel row (D) | the session menu with `viaSub` semantics when the row is a subagent's (§8.3) | BUILD (fix) |

### 5.15 Background (D: Overview, Board and Worktrees empty space, App shell; T: PanelBg with no row)

| Row | id | Sec | Show | Verdict |
|---|---|---|---|---|
| New session… | quick_prompt | A | | BUILD |
| Add project… | add_project | A | | BUILD (D `addRepo`) |
| Command palette | command_palette | A | | BUILD |
| View ▸ Overview · Board (D; checked: current home) / Board or bands (T) · Orchestrator · Accounts · Whakaoho · Worktrees (D local) · Archived sessions (checked) | home_overview, home_board, toggle_kanban, toggle_orchestrator_view, accounts, wake_schedule, toggle_archived | G | | BUILD |
| Settings | settings | G | | BUILD |
| Mark all seen | mark_seen | Ch | an unread session in scope | BUILD |
| Critters (checked) | critters | Ch | D: once discovered; T: always (as T's global verbs today) | BUILD (lane/critters) |
| Archive done sessions… | archive_done | W | a finished, seen session in scope | BUILD |

T's columns panels keep their own panel rows ahead of this menu:
- Projects: Add project.
- Worktrees: New worktree; Show or hide open PRs, issues and drafts.
- Sessions: New agent; Show archived.

Today's toggles stay.

### 5.16 Board lane header (D lane heading; T kanban lane header)

| Lane | Rows | Verdict |
|---|---|---|
| Needs you | Open next needing you (next_attention, `.`) | BUILD |
| Running | Open next running | BUILD |
| Review (unseen) | Mark all seen (mark_seen on the board's scope, this lane) | BUILD |
| Ship (PR) | Refresh pull requests | BUILD |
| Done | Archive done sessions… (archive_done, this lane) | BUILD |
| any | Focus scope ▸? Collapse lane? | CUT: no lane collapse exists; the scope chip already scopes |

### 5.17 Scope chip

Rows:
- Everything (checked when unscoped).
- `repo` heading, then repos (checked, with digit keys).
- `worktree` heading, then branches, when the scoped repo has more than one.
- `account` heading, then Any account and the enabled accounts (checked).
- Clear scope, when scoped.

Verdict: today's menu with checked state replacing the "✓ " prefixes (desk-menus); T's ScopeAccount picker the same. BUILD.

### 5.18 Breadcrumb (D)

- Each crumb's right-click is its target's menu: repo crumb → §5.6, worktree crumb → §5.5, session crumb → §5.1.
- A view crumb gives View ▸.
- Back/Forward right-click (history list) is CUT: no browser-style history menus.
- Verdict: BUILD.

### 5.19 Attention strip item (D strip counts; T attention hits)

- Right-click "3 need you" lists those sessions, newest change first. Each row opens the session. The list ends with Mark all seen when it is the unread count.
- Data: `model.lanes`.
- Verdict: BUILD.

### 5.20 Critter (lane/critters)

Pet · Feed · Name… · Shoo · rule · Hide critters until tomorrow · Critter log… · Critter settings…

Verdict: BUILD as specified. No change here.

### 5.21 Whakaoho view account row (lane/wake)

Wake up now · Wake-up on (checked) · Snap to the suggested hour (when it differs) · Decisions log… · rule · Account details… (its full menu is on the account row, §5.9; submenus never nest).

Verdict: BUILD (lane/wake).

### 5.22 Text field (lane/desk-quiet)

- Undo · Cut · Copy · Paste · Select all.
- Disabled rows: Cut and Copy with no selection; Paste with an empty clipboard; Undo with nothing to undo.
- Verdict: BUILD.

### 5.23 Diff / viewer text (Monaco; lane/desk-quiet, then lane/editors)

- Copy · Select all · Copy path:line · Open this line in VS Code.
- The last row needs lane/editors. It is not faked before then.
- Verdict: BUILD.

### 5.24 Link and issue rows (T columns)

- **Link:** Open in browser, View diff (a PR link), Comment… (a PR link), Edit URL, Delete (saved links). Today.
- **Issue:** Open in browser, then New session on this issue ▸ (T's `p`/`e` launches, as rows). BUILD.
- `?` on either row gives this menu, not the global or checkout one (§8.11).

---

## 6. Connections

The jumps Jay asked for, each with the data it runs on. "Today" means it exists somewhere, often only as a chip or key.

```
            ┌──────── task ────────┐
            │ (ADO linked chips)   │
   branch ──┼── PR ── worktree ── session ── account
     │      │   │       │   │        │         │
     └──────┘   │       │   └ port ──┘ (run    └ its sessions (scope)
                │       │     terminal)
                └ session (reverse of model.prs)
```

| From → to | Row | Where | Data | Today | Verdict |
|---|---|---|---|---|---|
| session → PR | Pull request ▸ Open in browser | §5.1 | `model.prs[worktree]` | yes | today |
| session → diff | Diff | §5.1 | | via checkout rows only | BUILD |
| session → account | the account chip's menu (D header, T pane AccountChip): ‹account›, Restart on…, Accounts…; Move ▸ To another account… on the card | §3.3, §5.1 | `Agent.profile` | chip (D), picker (T) | today + Move ▸ |
| session → log in | Log in to ‹account›… | §5.1 | lane/limits `stop_reason`, `AccountRow.login` | no | BUILD |
| session → parent | Open parent session ‹name› | §5.1 | `Agent.parent` | D `from` chip | BUILD |
| session → checkout | Worktree ‹branch› ▸ | §5.2 | | flat rows | BUILD |
| session → dev server | Open in ▸ Browser · localhost:N | §5.1 | `model.ports` | band chip | BUILD |
| worktree → sessions | Open session ▸ | §5.5 | `model.agents` | no (palette indirectly) | BUILD |
| worktree → run terminal | Show run terminal | §5.5, §5.13 | terminal `run_command && alive` | no | BUILD |
| port → run terminal | Show run terminal | §5.13 | same | no | BUILD |
| PR → session / worktree | Go to session, Go to worktree | §5.10 | `model.prs` reversed; `BranchRow.session/.worktree` | no | BUILD |
| PR → new worktree | Check out in new worktree… | §5.10 | `branchesApi.checkout` | ADO branch only | BUILD |
| branch → worktree / session / PR / task | Go to … | §5.11 | `BranchRow` | chips | BUILD (menu) |
| task → session / branch / PR | Go to … | §5.12 | linked chips | chips | BUILD (menu) |
| account → its sessions | Show its sessions | §5.9 | `scope_account` | count list | BUILD |
| account → start a session | Start a session on it… | §5.9 | quick prompt with profile | D `+` | BUILD |
| repo → worktrees | Worktrees ▸ | §5.6 | `model.worktrees` | D per-worktree rows | BUILD |
| repo → remote lists | Remote ▸ | §5.6 | | `?` extras | BUILD |
| attention count → sessions | the list | §5.19 | `model.lanes` | round-robin click | BUILD |
| session → ADO task/branch | — | | branch tree loads on demand per worktree | no | LATER (needs the branch list fetched on menu open; async) |

---

## 7. Multi-select

**Recommendation: no multi-select in 0.54.** Scoped bulk rows cover the jobs instead.

Why:
- **Selection model.** Both apps have one selection. D's is `selection.target` (selection.svelte.ts:10). T's is one cursor, and Space is Follow-up, so a Space-mark scheme collides with the most used key.
- **Mouse conflicts.** Ctrl/Shift-click on D's board and overview conflicts with drag-to-reorder and drag-to-lane. Vibe Kanban turns drag off while a selection exists, and that is the rule we would need.
- **Cost.** Each bulk verb needs its own confirm that lists the items. The work: a set-valued selection in `selection.svelte.ts` and the TUI's `nav`; marked-row styling on four D lists and two T views; a bulk menu built from the intersection of each item's verb table; and a confirm per verb.

What 0.54 ships instead (all BUILD, all with ids):
- `archive_done` on a worktree, a repo, the background (the board's scope) and the Done lane header.
- `mark_seen` on a worktree, a repo, the background and the Review lane header.
- Today's `delete_all` (a worktree's sessions) and `sweep_worktrees` (a repo's merged and clean worktrees).
- The lane headers come from desk-menus (D lane heading) and tui-menus (T kanban lane header hit).

LATER (0.57 or after), desktop only:
- Board and Worktrees view.
- Ctrl-click toggles, Shift-click selects a range, Esc clears, drag is off while two or more are selected.
- Bulk menu: Archive N, Mark N seen, Delete N…, Open N in VS Code.
- Move N to account ▸, only once `--resume` across profiles lands (spec "Claude Code releases", Later). A fresh restart for N sessions drops N conversations.

---

## 8. Defects the menu lanes fix (acceptance criteria)

1. **D worktree has two menus.**
   - Menu A is `worktrees/verbs.ts worktreeMenu`, used by the band and Worktrees view right-click, `⋯` and Enter. Menu B is `menuFor`, used by `?`, Shift+F10, the palette and cards.
   - Delete A. Every surface uses §5.5.
   - Where A's rows go:
     - Open folder → Open in ▸ Explorer (`reveal_in_explorer`).
     - Copy path → Copy ▸ Path (`copy_path`).
     - Focus/Widen flip → `focus_scope`'s turning label.
     - Run's live-run rule → `toggle_run`'s predicate.
   - Rows A lacked: Pull request ▸ and Delete all sessions… now show there.
   - The band's hover word buttons move to PRIMARY under lane/consolidate.
2. **D worktree menus never show Open pull request.** The id was set in the verb table but missing from MENUS. Fixed in the registry.
3. **D OrchestratorPanel builds a subagent's menu without `viaSub`.** It gives the parent's full menu, with no heading and no forced confirm. It must pass `viaSub` as SessionTree does.
4. **D disabled rows with no tooltip:** Merge on a draft, Move to account with an old daemon, "The default", "No repos yet". The words are in §5.1 and §5.9. "The default" becomes the checked Make default row.
5. **A merged or closed PR shows Open and Create together** (both apps). §5.1's PR table governs.
6. **T right-click builds from the selection, not the click.**
   - While the header holds the keys (`launcher_tab_cursor`), a card right-click drops every key-run row.
   - A right-click on a non-empty band's rule gives a card's menu.
   - `hit_to_target` fixes both. A band rule gives §5.5.
7. **T `?` lists what can't run:** Ghostty off macOS, Azure DevOps on a non-ADO repo, Open pull request with no PR (its key-run form only flashes), Comment with no PR, and an archived card's checkout extras (`more_rows` ignores `live`). Each is hidden by its §5 rule.
8. **"Open in editor" next to "VS Code"** (T today, D after editors) reads as two Opens. In Open in ▸ the project command is "Open command". Standing alone it keeps "Open in editor", unchanged for existing users.
9. **T Azure DevOps offered on every repo.** Shown only on Azure DevOps remotes.
10. **T `ACCOUNT_VERBS` and `DIFF_VERBS` are static** and ignore state. Account rows come from §5.9 and file rows from §5.8. The diff's own keys (layout, context, next change) stay the overlay's `?` list.
11. **T `?` on a PR or issue row gives the global menu, and on a link row the checkout's.** Each should give its own menu (§5.10, §5.24).
12. **Any mouse button runs a T menu row.** Only left or Enter runs one; a right-click reopens at the new spot (§3.2).
13. **D needs a second right-click to move a menu.** Right-click while a menu is open reopens it for the new target in one click (desk-menus primitive).
14. **Board card hover toolbar and card PR chip fall through to the browser menu.** They give the card's menu. The PR chip gives §5.10.

---

## 9. Verdicts

Every candidate considered, BUILD (0.54), LATER, or CUT, with the reason.

| Candidate | Target | Verdict | Reason |
|---|---|---|---|
| Sections and submenus from the registry | all | BUILD | One layout in both apps; the T `?` session menu drops from 43 to 17 rows |
| Diff on the session menu | session | BUILD | Spec leads with it; it was only reached through the checkout rows |
| Pull request ▸ with per-state verbs | session, worktree | BUILD | Fixes Open+Create on merged PRs; folds 3-5 rows |
| Open in ▸ (detected editors, Explorer, Browser, Dock) | session, worktree, repo | BUILD | Jay's core ask; lane/editors runs them |
| Copy ▸ Path / Branch name / PR link | session, worktree | BUILD | Most used quick actions in every comparable tool |
| `copy_pr_url` (new verb) | session, worktree | BUILD | Sharing PRs; cheap (clipboard) |
| `mark_seen` (new verb) | session, worktree, repo, lane, background | BUILD | Inbox triage with many projects; both apps already mark seen |
| `archive_done` (new verb) | worktree, repo, lane, background | BUILD | Scoped bulk instead of multi-select; reversible |
| `wake_schedule` (new verb) | account, background | BUILD | Ctrl+K and View ▸ reach Whakaoho in both apps |
| To another account… in Move ▸ (the account picker) | session | BUILD | §1.9 trim 3: one row fewer at the top; the account chip keeps the account's own menu |
| Review… inside Pull request ▸ | session | BUILD | §1.9 trim 1 (D) |
| Make orchestrator only while none is pinned | session | BUILD | §1.9 trim 2; Unpin first, or `?` More, the palette or a key |
| Log in to ‹account›… on auth failure | session | BUILD | Connects a failed session to its fix; needs lane/limits for the reason |
| State banner heading | session, worktree, account | BUILD | One line of context; D headings exist |
| Card checkout submenu | session, terminal | BUILD | Removes 9-19 flat rows per card |
| Open parent session | session | BUILD | Orchestrator-spawned sessions; `Agent.parent` exists |
| Show run terminal | worktree, port | BUILD | Port → process connection; data exists |
| Open session ▸ (jump list) | worktree (where sessions aren't on screen) | BUILD | Worktrees view and PR jumps lack it |
| Worktrees ▸ jump list | repo | BUILD | Replaces D's unbounded per-worktree rows |
| Remote ▸ (GitHub / Azure DevOps lists) | repo | BUILD | Moves T's `?` extras into the menu, gated by host |
| PR rows: Go to session / worktree, Check out in new worktree | PR | BUILD | The missing PR → work connection |
| Branch and task row menus with Go to rows | branch, task | BUILD | Chips become menus; data in `BranchRow` and linked chips |
| Vote ▸ and Publish on Azure DevOps PRs | PR | BUILD | Existing review actions reachable by right-click |
| Port chip menu (Open, Copy URL, Run/Stop, Show run terminal) | port | BUILD | Chips have no menu today |
| File menu with Stage / Unstage / Discard / Mark reviewed | file | BUILD | Both apps have hunk staging and review marks |
| Folder menu with Expand/Collapse all | folder | BUILD | With lane/files' tree; T tree now |
| Account menus everywhere | account | BUILD | No right-click on any account surface today |
| Background and lane header menus | background, lane | BUILD | No menu today in D; T panel rows only |
| Attention strip list | attention | BUILD | Jump to any needing session, not only round-robin |
| Open with default app refuses executables | file | BUILD | GitHub Desktop's rule; Kete shouldn't run programs on a click |
| Text field, terminal area, Monaco menus | text | BUILD | lane/desk-quiet; no native menu |
| Multi-select (Ctrl/Shift-click, Space-mark) | lists | LATER | §7 |
| Interrupt (Esc to a running agent) | session | LATER | `ClientRequest::Input` exists, but the Esc encoding depends on the PTY's kitty flags, which a client that isn't attached may not know; spike first |
| Mark unread | session | LATER | Needs a daemon request (only `MarkAgentSeen` exists) |
| Open in terminal (Windows Terminal tab / Ghostty) | worktree, folder | LATER | Fills T's dead ⇧T on Windows; goes with lane/files' 0.56 Open in ▸ growth so it ships with detection, not as a silent no-op |
| Restart run (stop then start) | port, worktree | LATER | Needs sequencing in the backend |
| Add to session ▸ (insert @path) | file, folder | LATER | lane/files 0.56 owns the insert path with drag |
| Find in this folder | folder | LATER | Grep has no scope argument yet |
| Copy session id / resume command | session | LATER | `claude --resume` across profiles needs the profile env; spike with the spec's `--resume` Later item |
| Auto-archive on PR merge (setting) | worktree | LATER | A setting, not a menu row; the Ship→Done drop already offers it |
| Go to ADO task/branch from a session | session | LATER | Needs the branch list fetched on menu open |
| VS Code Remote-SSH for a T running over ssh | session, worktree | LATER | `code --remote ssh-remote+host path`; needs host alias mapping from `hosts.rs` |
| D GitHub PR comment, issue link, cloud session rows | session | LATER | `issue_url`, `cloud_session_id` and a GitHub comment box are not in D's model yet |
| T Default account ▸ on the repo | repo | LATER | T sets it from the account (Use for…) today |
| Repo name copy, Copy repo path | repo | CUT | Copy path covers it; the name is on screen |
| Push / Pull / Fetch / Rebase rows | worktree, branch | CUT | Kete is not a git client; agents and the PR composer push |
| Add to .gitignore | file | CUT | Rare; the agent or the editor does it |
| Discard an untracked file | file | CUT | Deleting files is not Kete's job |
| New session in a subfolder | folder | CUT | Sessions run in a checkout |
| Lane collapse / sort | lane | CUT | No such board feature; the scope chip narrows |
| Back/Forward history menus | breadcrumb | CUT | Browser habit Jay asked to remove |
| Right-click pastes in the terminal | terminal area | CUT | Would type clipboard text into a running agent |
| New AppliesTo kinds for PR, branch, task, port, lane | registry | CUT | No rebindable verb needs one; about ten switch sites each in D |

---

## 10. Who builds what

| Lane | Takes |
|---|---|
| **desk-menus** (0.55) | Menu primitive (§3.1); every D surface in §5 (worktree unification §8.1; `menus/rows.ts` for PR, branch, task, port, lane header, attention, background, orchestrator); folding with `sectionOf`/`groupOf`; checked rows; D implementations of `mark_seen`, `copy_pr_url`, `archive_done`, `copy_path` (terminal), `open_in_browser`, `scope_account` (account), `accounts` (account and background), the state banner and the Log in row; defects 1-5, 13, 14 |
| **tui-menus** (0.55) | Generic submenu and drill-in (§3.2); `hit_to_target`; File/Folder/Account targets; overlay right-clicks; `More ▸`; folding; T implementations of `mark_seen`, `copy_pr_url`, `archive_done`; lane header and background menus; defects 6, 7, 9-12. It removes those ids from the matrix's `pending_verb` arm in key_modes.rs. |
| **editors** (0.55) | `open_in_vscode`, `open_in_visual_studio`, `dock_editor`, `reveal_in_explorer`, `open_with_default` (with the executable refusal), `open_file_in_kete` in both apps; the copy verbs (`copy_path`, `copy_relative_path`, `copy_branch`) in D. The TUI's copy verbs belong to tui-menus (its brief, item 6), through `copy_and_flash`. Also editor icons, the checked default, and the instance dot on menu open. |
| **wake** (0.54) | `wake_account` and `wake_schedule` in both apps. The account menu's "Wake-up schedule…" row is the `wake_schedule` verb, not a local row, so the palette and View ▸ reach the same view. Also the Whakaoho row menu (§5.21). |
| **limits** (0.54) | `limit_until`/`stop_reason` that the banner and the Log in row read |
| **desk-quiet** (0.54) | §5.22, §5.23, §5.4 (D) |
| **files** (0.56) | D Files tree with the §5.7 and §5.8 menus; Add to session (LATER row) |
| **critters** (0.55) | §5.20 |
| **consolidate** (0.56) | Hover words from PRIMARY; one accounts component (it carries §5.9 in both places) |

Each lane's report lists the rows it added, the rows it considered and rejected, and why (brief.md "Menus are critical").

**Release gate.** No verb may ship on `pending_verb`, which shows "coming in this release". Before 0.54 is tagged, each one below is either routed by its lane or taken out of `MENUS` and the palette:
- `mark_seen`, `copy_pr_url`, `archive_done`: desk-menus (D) and tui-menus (T)
- `wake_account`, `wake_schedule`: wake
- the editor verbs: editors
- the copy verbs: editors (D) and tui-menus (T)
- `critters`: critters

---

## 11. Counts before and after

Top-level rows. "After" is the common state, with the worst case in brackets. D is right-click; T is right-click, with `?` in brackets.

| Target | D today | D after | T today | T after |
|---|---|---|---|---|
| Session (live, PR open, linked; another session pinned) | 17 + rule + heading + 9 checkout rows | 13 (17; 14 with none pinned) | 32 (`?` 43) | 13 (`?` 15; 14 and 16 with none pinned) |
| Session (archived) | 3 | 3 + banner | 10 (`?` 33) | 3 (`?` 4) |
| Terminal | 3 + rule + 13 | 5 | 22 | 5 |
| Worktree (linked, PR open, sessions) | A 12 / B 13 | 12 (16) | 19 (`?` 29) | 13 (`?` 15) |
| Repo (one linked worktree) | 14 + 2 per linked worktree | 15 (17) | 19 (`?` 24) | 12 (`?` 14; 16 with every state row) |
| Folder | none | 4 (+2 in the tree) | none | 4-6 |
| File (diff row) | none | 9-11 | none | 8-10 |
| Account | none (`?` 9-10 in AccountsView) | 10-12 | none (`?` 13 static) | 9-11 |
| PR | none (chip → browser) | 6-8 | 4 (columns PR row only) | 6-8 |
| Branch | none | 6-8 | none | 5-7 |
| Task | none | 4-6 | none | 4-6 |
| Port | none | 4-5 | none | 4-5 |
| Background | none | 9 | 1-4 (panel rows) | 8-9 |
| Lane header | none | 1-2 | none | 1-2 |
