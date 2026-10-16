# Kete 0.54 to 0.57: reach, look, wake, critters

Jay, 2026-10-16, after 0.53 was handed back. In summary, he asked for the following:
- Can we embed VS Code and Visual Studio in the desktop app?
- "Open in editor" with a VS Code icon and a Visual Studio icon for the API project, plus a collapsed file tree.
- Manage the editor windows and instances, across editions and versions.
- Consolidate the UI without regressing anything.
- Personalisation: fonts, themes, the VS Code styling contract, and seeded defaults.
- A root-level wake-up feature for tomorrow's accounts, with usage time windows shown beside capacity.
- A te reo name and icon for it.
- "ULTRALOVE" right-click and quick actions, styled, in both apps.
- Remove the useless Print/Back browser menu.
- Port the stream critters into both apps as an overlay easter egg. Make them configurable, expand the cast, and give them personalities and mouse interactions.
- Learn from recent Claude Code releases and close real gaps without bloat.

**Process note.** Jay's standing Kete authorisation ([[feedback-kete-automatic-releases]]: plan, scope, deliver, release through 0.60 without check-ins) outranks the brainstorming approval gates. The 0.53 hand-back said "no 0.54 unless a defect or his feedback", and this is his feedback. The spec and plan are written, then delivered. Choices that would normally be asked are made with the recommended option and recorded here.

The audits behind this spec are in `.superpowers/sdd/2026-10-16-release-054/audit-*.md` (desktop, TUI, settings/usage). All were read-only against main 09698ed.

## Triage

| Idea | Verdict | Why |
|---|---|---|
| Embed the real VS Code / Visual Studio windows inside Kete | **Cut** (see Embedding) | Cross-process window reparenting into a WebView2 host breaks focus, input, DPI and popups; Visual Studio has no embedding surface at all. |
| "Dock beside Kete": snap the editor window next to Kete's window | **0.54** | Gives the side-by-side feel without reparenting; plain `SetWindowPos` on a window Kete found. |
| Open in VS Code / Visual Studio from every folder, file, worktree, project, session | **0.54** | The core ask. Detection, launch and instance focus live in core, shared by both apps. |
| Real app icons for each detected editor | **0.54** | The exe's own icon (Insiders green, Cursor, VS Community purple) beats a drawn logo, and no brand marks are shipped. |
| Editions and versions: VS 2017/2019/2022/2026, Community/Professional/Enterprise/Preview; VS Code Stable/Insiders, VSCodium, Cursor, Windsurf | **0.54** | vswhere and known install paths cover all of them; the edition doesn't change how Kete launches. Build Tools (no IDE) is filtered out. |
| Track open editor windows, focus instead of reopening | **0.54** | VS Code already focuses an open folder's window; Visual Studio windows are matched by title and solution, then focused. |
| VS Code inside a Kete tab via `code serve-web` | **Cut** | It doesn't use the user's install (own server build, own data, no desktop extensions), and its license bars combining it with another app. |
| Collapsed file tree per worktree | **0.56** | Git-listed files, collapsed by default, right-click menus; the TUI already has a tree browser. |
| Code outline ("structs") | **Cut** | Needs a language server per language. The editor already does it, one click away. |
| Context menus on every surface, one registry, both apps | **0.54** | Most of the plumbing exists (registry `MENUS`, `ui/Menu.svelte`, TUI `ContextMenu`); the gaps are targets, submenus, and surfaces with no menu. |
| Remove WebView2's Back/Refresh/Save as/Print/Inspect menu and browser keys | **0.54** | The one unambiguous fix. Copy/Paste for text fields, the terminal and Monaco move into Kete's own menus so nothing regresses. |
| Drag a file row onto a session to insert its path | **0.56** | Cheap, high-value QoL beside the file tree. |
| Fonts: UI, mono, terminal family and size | **0.56** | No font settings exist anywhere; the terminal is fixed at 13px JetBrains Mono. |
| Themes: VS Code theme import in the desktop (the TUI has it), presets, seeded Workshop default | **0.56** | Move the TUI's JSONC and VS Code theme loader into shared config; map the VS Code colour contract to Kete tokens. |
| Whakaoho (wake-up) at rail level: when switched on and the conditions hold, send the smallest prompt to the cheapest model at a chosen hour, so the 5-hour window starts and resets earlier | **0.54, first** | Jay wants it for tomorrow morning. It is opt-in per account, and Windows Task Scheduler runs it even when Kete is closed. |
| Hour slider with a preview of how the windows line up against the user's usual hours; usage windows timeline for every account beside the capacity bars | **0.54** | All the data exists (`resets_at`, nominal window length); it's a presentation job. |
| Critters overlay in both apps | **0.55 (lane starts with 0.54)** | Self-contained: a new crate plus two overlays, touching shared code only at the settings toggle. |
| Claude Code release gaps: `StopFailure`, auto-continue and blocking-dialog notifications, effort sync, exec-form hooks on Windows, `background_tasks` | **0.54** (`lane/limits`) | Rate-limited and auto-continuing sessions show as done today, and `/effort` is lost on restart. See "Claude Code releases". |

## Embedding

Research is in .superpowers/sdd/2026-10-16-release-054/ (the editor research report). The verdict is **no**, for these reasons:

- **Reparenting another app's window into Kete** (`SetParent` across processes) is "technically legal" but unsupported:
  - It joins the two apps' input queues, so a hung IDE freezes Kete.
  - It resets the child's DPI awareness.
  - DWM on Windows 11 can keep drawing the window as top-level.
  - UIPI blocks it outright for an elevated Visual Studio.
  - WebView2 is already a three-process stack, so focus would bounce between four.
  - VS Code's popups and Visual Studio's floating tool windows stay top-level anyway.
- **`code serve-web`** downloads its own server build, keeps its own data folder, and ignores the desktop install's settings and extensions. Its license also bars combining it with another app. openvscode-server and code-server are Linux/Docker-only and use Open VSX.
- **Visual Studio** has no web host and no embedding surface.

What Kete does instead:
- launches the user's own editors and focuses an already-open window
- docks the editor beside Kete with a snap
- keeps Monaco for read-only views inside the app

## Release map

**Update, same day:** Jay asked for everything to be planned now, built in parallel, then merged, tested and released together at the end. So the groups below are **waves inside one release, 0.54.0**, not separate releases. The plan doc docs/superpowers/plans/2026-10-16-kete-054-plan.md holds the waves, the lane dependencies and the release steps. Up to four lanes run at once, each in its own worktree.

1. **0.54 Whakaoho.** Foundation commit, then three lanes:
   - **wake-up:** the runner, the conditions, Task Scheduler, the hour slider and the windows timeline in both apps
   - **quiet browser:** WebView2's own menu and browser keys off, replaced by Kete's Copy/Paste menus for text fields, the terminal and Monaco
   - **limits:** the Claude Code release gaps B1–B4 (see the last section). B1 is what makes a limited session read "resumes 15:45" next to Whakaoho's windows.
   - Whakaoho comes first because Jay wants tomorrow's accounts warmed. The browser fix is small and unambiguous.
2. **0.55 Reach.** Four lanes:
   - desktop menus
   - TUI menus
   - editors
   - critters
   - The editors and critters lanes start alongside 0.54's lanes. They share no files with them, apart from the foundation.
3. **0.56 Look.** Three lanes:
   - personalisation (fonts, themes)
   - files (tree, drag a path into a session)
   - UI consolidation
4. **0.57.** Two lanes:
   - the Claude Code "Later" items that prove worth it
   - visual QA of everything above

## 0.54 foundation (one commit on release/0.54 before any lane branches)

The registry is append-only and position-indexed, and parallel lanes appending to it conflict on every merge. So these shared slots land first:

- **Registry.**
  - `AppliesTo` gains `File`, `Folder` and `Account` menus. `Account` already exists as a kind, but its `MENUS` entry stays empty today.
  - `render_ts` arrays are updated to match.
- **New verbs**, appended to `ACTIONS` in this order. They are unbound by default and listed in the unbound allowlist.
  - Open in an editor:
    - `open_in_vscode` (worktree, repo, session, folder, file)
    - `open_in_visual_studio` (worktree, repo, session, file)
    - `dock_editor` (worktree, repo, session)
  - Open elsewhere:
    - `reveal_in_explorer` (worktree, repo, session, folder, file)
    - `open_with_default` (file)
    - `open_file_in_kete` (file)
  - Copy:
    - `copy_path` (worktree, repo, session, folder, file)
    - `copy_relative_path` (folder, file)
    - `copy_branch` (worktree, session)
  - Accounts and critters:
    - `wake_account` (account)
    - `critters` (global; toggles the overlay)
- **`MENUS` rows.**
  - New entries for `File`, `Folder` and `Account`.
  - The test that pinned `menu_of(Account)` as empty is replaced with one that asserts its rows.
  - The editor verbs are added to the worktree, repo and session menus.
  - `MENU_LABELS` gets the short forms: "VS Code", "Visual Studio", "Show in Explorer", "Copy path".
- **Desktop and generation.**
  - `DESKTOP_ONLY` gets nothing new.
  - `actions.gen.ts` is regenerated, and the `len == 91` test is bumped.
- **Config.**
  - `critters` (Appearance, `off|on`, default `off`), `critters_size` (`small|medium|large`, default `medium`), `critters_often` (`rare|normal|busy`, default `normal`).
  - All are shared rows, so the TUI and the desktop agree.
  - The collection log is NOT config (see Critters).
- **Desktop prefs.**
  - `editor_folder` (`auto|vscode|visual-studio|command`) and `editor_file` (`auto|vscode|visual-studio|kete`) are Choice values.
  - They go under `desktop` and in `DesktopPrefs::choices`.
- **Protocol slots** (allocated now so the lanes don't collide):
  - 61 for `lane/wake` (`WakeAccount` request)
  - 62 for `lane/limits` (limit/stop-reason fields or event)
  - Each lane defines its own `*_PROTOCOL` constant with its number. The orchestrator sets `PROTOCOL_VERSION` when merging. New `ClientRequest`/`ServerEvent` variants are appended in lane order: wake first, then limits.
- **DB slots.**
  - None. Editor launch runs in the clients, and each app calls core.
  - Whakaoho keeps its state in machine-local files (see its lane), so `kete wake` runs with or without a daemon.
  - Protocol 61 is taken only if a lane needs a daemon request, and that lane must say so.

## 0.55 lane: desktop menus (`lane/desk-menus`)

The "kill the browser menu" part below ships first, as 0.54 `lane/desk-quiet`. This lane builds on it.

**Kill the browser menu without losing Copy/Paste.**
- Turn the browser menu and keys off at the source:
  - `main.rs` `with_webview` (Windows) sets `AreDefaultContextMenusEnabled(false)` and `AreBrowserAcceleratorKeysEnabled(false)` on `ICoreWebView2Settings`.
  - WebView2 keeps editing keys (Ctrl+C/V/X/A/Z) when browser accelerators are off.
  - Print, reload, find, zoom, devtools and back/forward keys stop.
  - Kete's own Alt+←/→ binding must still arrive as a DOM keydown. A test checks it.
- Add a JS `contextmenu` handler on `document` as a belt-and-braces backstop:
  - It calls `preventDefault` on every right-click.
  - It routes to the nearest `[data-target]`'s menu, else the background menu.
- Replacement menus for the surfaces that used the native one:
  - **Inputs and textareas:** Undo, Cut, Copy, Paste, Select all. Paste reads via `navigator.clipboard.readText()`. If WebView2 blocks that without a permission, the Tauri clipboard-manager plugin is added (check what's present first).
  - **xterm:** Copy (if a selection exists), Paste, Select all, Clear, Copy path of the session's folder, and Open link under the pointer if xterm's link addon has one. The terminal keeps `rightClickSelectsWord`.
  - **Monaco** (diff and file viewer): Copy, Select all, Copy path:line, Open at this line in VS Code.
- Shift+F10 and the Menu key open exactly one menu. A test checks there's no double open from the keydown plus the DOM `contextmenu` event.

**Menu primitive upgrade** (`ui/Menu.svelte`, `ui/ui.svelte.ts`):
- Submenus (`▸`, opened by hover with a short delay, → or Enter; ← closes), used by "Open in", "Copy", "Account", "PR", "View".
- An icon slot: a file icon, an editor's own icon, or a status dot.
- A checked state, which replaces the `✓ ` label prefixes in ScopeChip and AccountsView.
- Type-ahead on every menu, plus Home/End.
- Show all chords, not just the first.
- Right-click while a menu is open closes it and opens the new target's menu at the new spot, in one click.
- Styling stays Workshop:
  - square, 1px `rule-strong`, `bg`, active row `bg-ink text-bg`
  - danger `text-fail`
  - headings in `faint` small caps
  - separators as a 1px rule
  - motion only with `animations`, never on reduced-motion

**Menus everywhere.**
- Every surface without a handler gets one:
  - the rail repo (compact and full)
  - the header and breadcrumb (path parts)
  - PR chips
  - port chips
  - ADO branches, PRs and tasks
  - account rows in the rail and AccountsView
  - diff file rows
  - FileSheet tabs
  - the orchestrator panel background
  - empty space in Overview, Board and Worktrees
- Each target gets a `data-target` (`file:<worktree>:<rel>`, `folder:…`, `account:<key>`, `pr:…`, `branch:…`, `task:…`).
- **Unify the two worktree menus.**
  - `worktrees/verbs.ts` `worktreeMenu` is deleted.
  - Its "Open folder" and "Copy path" ride on the new registry verbs (`reveal_in_explorer`, `copy_path`), so they also reach `?`, the palette and the verb bar.
  - Every worktree surface uses `menuFor`.
- PR, branch and task rows get small local menus. They aren't registry kinds yet, so they're built in one place, `menus/rows.ts`:
  - PR: Open, Copy URL, Review, Vote ▸, Merge, Check out.
  - Branch: Check out in a new worktree, Copy name, Open in browser.
  - Task: Start, Open in browser, Copy id.
- **Background menu:**
  - New session, Quick prompt, Add project, Command palette
  - View ▸ (Overview, Board, Worktrees, Accounts)
  - Show archived, Settings
  - Critters (only once they've been discovered)

**Tests.**
- Vitest for menu building per target kind, the submenu model, type-ahead, and the text-field menu's verbs.
- E2E: a context-click helper in `e2e/webdriver.mjs` (W3C actions, button 2), then flows:
  - right-click a card, a worktree band and a file row, then pick a verb
  - right-click a text input, then Paste
  - a native menu never shows: no `contextmenu` reaches WebView2, checked via a page flag set by the backstop handler

## 0.55 lane: TUI menus (`lane/tui-menus`)

- Add a pure `hit_to_target(app, &HitTarget) -> Option<Target>`. Right-click opens `menu_for(target)` for the clicked thing, not for the current selection.
  - The selection still moves, as today, so the keyboard follows.
  - `KanbanCard`, `KanbanButton` and `CardButton` are added to the mapping.
- `verbs::Target` gains `File { worktree, rel }`, `Folder { … }` and `Account(key)`. `MenuAction` gains variants carrying a path and an editor.
- Right-click inside overlays opens that row's menu, with the overlay kept beneath via `verbs_under`:
  - **diff file rows** → File
  - **tree browser** → File / Folder
  - **files / grep results** → File
  - **accounts rows** → Account
  - **ADO PR / branch / task rows** → the same local menus as the desktop, in the TUI's `actions.rs` PR / issue / link builders
- Menu hover:
  - `Moved` updates `menu.hover`.
  - The wheel scrolls long menus.
  - A right-click while a menu is open reopens at the new spot.
  - Only the left button (or Enter) activates a row. Today any button does.
- Background right-click (`PanelBg` with no row) shows the global menu (the same rows as the desktop background menu).
- The TUI has no headings, separators or disabled rows. It omits them, as it does today. This is the documented parity rule.
- **Security parity.**
  - The TUI's "Open in editor" (`OpenWorktree`) gains the desktop's one-time approval for `.kete.json` open commands.
  - The approval is stored in the same `approved_open` list.
  - No new trust is added.
- Editor verbs dispatch to `nebula_core::editors` (from the editors lane). Remote and ssh sessions hide local-editor verbs, following the ghostty precedent.
- **Tests:** event-loop tests for right-click on a card, a band, a kanban card, a diff file row and an accounts row; for menu hover; and for reopen-at-spot. Use draw tests for the menu at 80×24.

## 0.55 lane: editors (`lane/editors`)

A `nebula_core::editors` module (no UI), with:

- **Detection.**
  - **VS Code family:** user and system installs found through:
    - `HKCU`/`HKLM` `…\Uninstall` keys
    - `App Paths`
    - known folders (`%LOCALAPPDATA%\Programs\Microsoft VS Code`, `…\Microsoft VS Code Insiders`, `VSCodium`, `cursor`, `Windsurf`)
    - `PATH` as a last resort
  - **Visual Studio:** `%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe -all -prerelease -products * -format json`. An entry counts as an IDE only when `productPath` ends in `devenv.exe`. On this machine that keeps Community 2022 and drops Build Tools 2022, whose productPath is `LaunchDevCmd.bat`.
  - **Fields kept:** display name, version, edition (from `productId`), prerelease, path.
  - **Cache:** results are cached and refreshed on demand.
  - **Test fixtures:** captured from this machine's real vswhere output and VS Code layout, with Insiders/Cursor/VSCodium fixtures written by hand.
- **Launch.**
  - Spawn the detected exe directly with argv, not through the shell: detached, no console, no `code.cmd` flash, and no `%`/`"` rejection from `shell.rs`.
  - Strip Kete's own environment before launch (`ELECTRON_RUN_AS_NODE`, `NEBULA_*`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GH_CONFIG_DIR`, the statusline/hook variables). A cold-started VS Code passes its environment to its terminals, which would leak a Kete profile into a user's own shell.
  - Read each VS Code-family install's `product.json` (`applicationName`, `nameShort`, `dataFolderName`). It lives under the versioned `<install>\<commit10>\resources\app\` folder, so don't hard-code the path. This covers Stable, Insiders, VSCodium, Cursor, Windsurf and other Code-OSS forks (Kiro, Trae) without special cases.
  - VS Code variants: `Code.exe <folder>` (focuses an existing window for that folder), `-g file:line:col` for files, and `-r` when the setting says reuse.
  - Visual Studio:
    - open the chosen solution or project (`.slnx` from 17.14 on)
    - `devenv.exe /Edit <file>` sends files to the most recent running instance
    - The Visual Studio verb and icon appear only where a `.sln`/`.slnx`/`.csproj`/`.vcxproj` exists: Jay's "icon for the API project". `devenv <folder>` (Open Folder) isn't a documented switch, so it isn't offered.
    - Going to a line in a running Visual Studio needs DTE over COM (Running Object Table). That is later, not this lane.
- **Solution finder.**
  - Searches the worktree root and one level down for `.sln`/`.slnx`, skipping `node_modules`/`bin`/`obj`.
  - The pick is remembered per worktree in desktop prefs, keyed by repo. With several solutions, the menu shows a submenu.
- **Instances.**
  - On Windows: `EnumWindows` (visible, unowned, not cloaked) on `Code.exe`/`devenv.exe` processes, matching titles to the folder or solution name. Titles are a heuristic: a custom `window.title` or two worktrees with the same folder name defeat them. So titles only drive the "open" dot, never which window gets focus.
  - Focus works by relaunching:
    - `Code.exe <folder>` makes VS Code focus the window that already has that folder.
    - `devenv /Edit` focuses the most recent Visual Studio.
  - Kete calls `AllowSetForegroundWindow(<child pid>)` before spawning, while it still has the foreground.
  - No `AttachThreadInput` (it can hang).
  - The editor icon shows a dot when a window for that worktree is open. The check is cheap and runs on hover or menu open, not polled.
- **Dock beside Kete.**
  - Snap the editor window to the half of Kete's monitor opposite Kete's window, and fit Kete to the other half.
  - Uses `rcWork` from `GetMonitorInfo`, restores a maximised window first, and compensates for the invisible resize borders with `DWMWA_EXTENDED_FRAME_BOUNDS`.
  - The OS restores it (Win+arrows) like any snap.
  - An elevated Visual Studio can't be moved (UIPI); the command says so instead of failing silently.
- **Icons.**
  - Each detected exe's own icon is extracted once (shell icon API) to a cached PNG and served to the desktop as a data URL.
  - The TUI uses the Nerd Font glyphs for VS Code / Visual Studio, falling back to the ASCII "VSC" / "VS".
- **Non-Windows:** detection via `PATH` (`code`, `code-insiders`, `codium`, `cursor`); Visual Studio is absent. All Windows-only code is `#[cfg(windows)]`.

**Wiring.**
- Desktop:
  - Tauri commands `editors_list`, `editor_open(target, editor)` and `editor_dock`.
  - Editor icons on RepoRow, WorktreeBand, WorktreesView and the Session header. The first two already have an "open in editor" hover icon. It becomes the default editor's real icon, plus a second icon for Visual Studio when a solution exists.
  - The "Open in ▸" submenu lists every detected editor, with a check mark on the default.
  - Settings, on the Desktop tab, gets an "Editors" section: detected installs, default for folders, default for files, "Reuse window", and a re-scan.
- TUI: the menu verbs plus a `kete editors` CLI (`list`, `open <path> [--with vscode|vs]`) for scripts.
- The existing per-project "Open command" stays. It is what `editor_folder = command` uses.

## 0.55 lane: critters (`lane/critters`)

Jay's stream critters (`Documents\wevds-stream\critters.html`) are rewritten as a cross-platform easter egg.

**One cast, two renderers.**
- A new crate `crates/kete-critters` (pure Rust, no deps beyond serde) is the source of truth:
  - the palette-indexed sprite frames
  - the species table (size, speed, gait, flags, lines, personality bias, origin)
  - the behaviour weights
  - a deterministic `World` with `tick(dt, input)` that the TUI runs directly
- A test generates `desktop/src/lib/critters/cast.gen.ts`, with a drift test, regenerated by `KETE_BLESS=1`. This is the `actions.gen.ts` pattern.
- The desktop runs a TypeScript port of the brain against that generated data. The data is one source; the behaviour is implemented twice, tested against the same scenarios.

**Art.**
- Rewritten small and chunky so both apps show the same pixels at "Twitch scale":
  - bodies roughly 10–16 px wide and 6–12 px tall, authored as ASCII pixel maps with a per-species palette
  - a 1px dark outline added at bake time
  - 2-frame walk, idle, sit, sleep (eyes closed, 1px lower), eat, and fly for fliers, plus a 1-frame "held" and a 1-frame "squash"
  - blink is derived by swapping the eye index
- **Desktop:** drawn at an integer scale for crisp pixels (`critters_size`: 2/3/4 CSS px per sprite pixel, times devicePixelRatio, rounded).
- **TUI:** drawn with half-blocks (`▀`/`▄`, foreground = top pixel, background = bottom). Each cell holds two square pixels, so a 14×10 critter is 14 columns × 5 rows. A transparent half takes the underlying cell's background, and fully transparent cells are left untouched.
- **References:** pixel-art principles (outline, ramps, readable silhouettes, walk/idle timing) from `.superpowers/sdd/2026-10-16-release-054/pixel-ref/`. A `cargo run -p kete-critters --example sheet` writes a PNG contact sheet, so every species can be eyeballed.

**Cast** (original pixel art; homages to archetypes, never copies of a game's characters, since the repo is public):
- **Aotearoa:**
  - kiwi, pūkeko, tūī, kea, kākāpō (nocturnal, waddles), takahē, ruru (nocturnal), pīwakawaka (fantail)
  - weka (pinches the cursor's "shiny")
  - kororā, hoiho, tuatara, wētā
  - moa (rare), kekeno (seal, flops)
  - sheep
- **Farm and classic:** cow, pig, chicken, duck, dog, cat, bunny, hedgehog, frog, capybara, fox, snail, slug, crab, mushroom, ghost (night only).
- **OSRS-flavoured:**
  - an adventurer (mines, "+25 xp")
  - a cow that drops "cowhide"
  - a chicken that drops "feather"
  - a goblin
  - an imp (blinks out and reappears)
  - a mole pet (digs into the floor and pops up elsewhere)
  - a pet rock (does nothing, beloved)
  - a genie random event (rare; click for an XP lamp)
- **Blocky-sandbox-flavoured:**
  - a green boom-bug (hisses, flashes, pops into harmless confetti, and is scared of cats)
  - a cube slime (splits when clicked, re-merges)
  - an axolotl, a bee, a blue helper sprite (fetches a dropped item to the cursor)
- **Maple-flavoured:** shell snails in three colours, an orange cap mushroom (bounces), a ribbon pig, a walking stump.
- **NZ-made-game nods:**
  - a balloon that floats up and gets popped by a dart monkey (rare)
  - a loot beam with a gleaming orb when a PR merges (event, not a critter)
  - a tiny dragon whelp (rare flier, puffs smoke)

**Behaviour.**
- A weighted state machine (walk, idle, look, eat, sit, sleep, emote, say, hop, special), as in the original, with personality per visitor:
  - **curious:** approaches the cursor
  - **shy:** scurries off
  - **sleepy**
  - **playful:** hops, chases
  - **social:** seeks other critters
  - **grumpy:** says less
  - **greedy:** eats more
- Species bias picks a personality, and the visitor's generated name shows on hover.
- **Pairs:** existing meet-and-emote, plus relationships:
  - cat chases mouse/boom-bug
  - kiwis greet
  - slimes merge
  - dog and cat bristle
- **Time of day:** nocturnal species and the ghost appear only at night (local clock); tūī are most likely in the morning.
- **App events** (desktop and TUI both see them):
  - an agent needs you: the nearest critter shows "!"
  - a turn finishes: the adventurer says "gz"
  - a PR merges: loot beam
  - nothing running and late: critters sleep
- Critters walk on the top edge of the status bar (desktop) or footer row (TUI). Fliers may perch on lane headers (desktop `data-perch`) or band tops (TUI hit rects).

**Interaction.**
- **Desktop:**
  - The overlay canvas is `pointer-events: none`. A small transparent hit box follows each critter, so only critters catch the mouse.
  - Hover shows the name.
  - Click pets (a heart and a species reaction).
  - Drag picks the critter up (held pose, legs wiggle). Release drops it with gravity, a bounce and a squash. Release velocity flings it, and fliers take off instead.
  - Double-click feeds (crumbs fall, and it eats).
  - Right-click opens the critter menu: Pet, Feed, Name…, Shoo, Hide critters until tomorrow, Critter log…, Critter settings…
- **TUI:**
  - Left-drag picks up and throws using the same physics in cell space.
  - Click pets.
  - Right-click opens the same critter menu.
  - Critter hit rects are checked before the app's own, only where a critter is drawn.

**Discovery and control.**
- `critters` defaults to off.
- **Turning it on:**
  - Settings → Appearance → Critters, in both apps.
  - The `critters` verb in Ctrl+K.
  - Clicking the kete mark five times (desktop). A kiwi climbs out of the basket.
  - ↑↑↓↓←→←→BA in either app.
- A toast says how to turn them off.
- **Collection log:** an OSRS-style grid of species seen, with silhouettes for unseen ones and pet counts. It is stored in `<data>/critters.json`, machine-local and never exported, and opened from the critter menu or Settings.

**Cost.**
- Animation runs only while a critter is on screen.
- **Between visits:** a single timer, with no animation-frame loop (desktop) and no ticks (TUI).
- **Frame rate:** the desktop uses requestAnimationFrame, paused when the window is hidden or unfocused. The TUI ticks at 8 fps while visible.
- **Gating:** off when `animations` is off. Under reduced motion, sprites hold their pose with no particles.
- **No external fonts:** speech uses a built-in 3×5 pixel font, since the CSP blocks Google Fonts and the original page used Silkscreen.

## 0.56 Look (outline; detailed plan written when 0.55 lands)

- **Personalisation lane.**
  - Move `jsonc.rs` and the VS Code theme loader (`vscode_theme.rs`) from nebula-tui into nebula-config, so the desktop can use them.
  - Desktop theme choices:
    - Workshop light/dark (the seeded default)
    - One Dark/One Light
    - "VS Code (live)", which follows the theme VS Code is set to
    - "Import a VS Code theme…": installed extension themes or a file. It maps the colour contract (`editor.background`, `foreground`, `focusBorder`, `list.activeSelectionBackground`, `terminal.ansi*`, …) onto Kete's tokens, then the existing contrast repair runs.
  - Fonts:
    - UI, mono and terminal family from installed system fonts (enumerated by the backend; the CSP allows only system fonts or bundled ones)
    - terminal and diff size
    - "Use VS Code's fonts" reads `editor.fontFamily` / `terminal.integrated.fontFamily`
  - Free-text font prefs need a new desktop pref path, since DesktopPrefs takes fixed choices only.
  - The TUI gets the same theme import, and `kete setup terminal` offers the chosen font.
- **Files lane:**
  - A collapsed file tree per worktree in the right pane (`git ls-files` plus untracked-not-ignored, scoped to the worktree root), with icons and File/Folder menus.
  - Drag a file or folder row onto a session or terminal to insert its path.
- **Consolidation lane** (no feature removed, each change listed in the CHANGELOG):
  - One accounts component shared by the rail and AccountsView.
  - One repo-row component for the compact and full rail.
  - Hover verbs on rows come from the registry's `PRIMARY` instead of hand lists.
  - PR chip logic is centralised.
  - The palette dedupes repeated verbs.
  - Native `<select>`s become the Menu primitive where they sit in Kete chrome.

## 0.54 lane: Whakaoho (`lane/wake`)

*Whakaoho* (Te Aka: "to startle, awaken, rouse, start up"), shown as "Whakaoho · Wake-up". It sits at rail level with a sunrise-over-kete mark.

Jay's framing: "if enabled and conditions warrant it, send the smallest message to the cheapest model to start the 5-hour window earlier, like a 5am heartbeat. Make the hour a slider, and show when the windows line up best for me."

- **One switch per account, plus an hour slider.**
  - The switch is per account and off by default. The hour slider (15-minute steps) sets the anchor, for example 05:00.
  - The slider sits on a 24-hour strip. As it moves, the strip shows:
    - the windows the anchor produces: 05:00–10:00, 10:00–15:00, 15:00–20:00 while in use
    - the user's usual activity, as a heat row built from Kete's own session history (turn timestamps per account, already in the DB)
    - a count of how many fresh windows land inside the active hours
  - A "Suggested" tick marks the anchor that fits the most fresh windows into the user's usual day. It puts the first reset shortly after the usual start time, and the user can snap the slider to it.
- **When it fires.** A heartbeat goes out only when all of these hold:
  - the switch is on, it's an enabled day, and the time has passed the anchor
  - the account has no live 5-hour window (`expired_at`, or the last `resets_at` has passed)
  - the account isn't disabled and is logged in
  - no heartbeat has already started today's window
  - **Catch-up:** if the PC was asleep at the anchor, the heartbeat still goes out on waking, but only while it's still earlier than the usual start time. Otherwise it is skipped and logged.
- **Nicer logic (each a separate opt-in under the switch).**
  - **Keep the chain rolling during active hours:** when a window ends while the account is idle, start the next one straight away. Resets then stay on the anchor's 5-hour grid instead of drifting to whenever the next prompt happens.
  - **Stagger accounts:** with two or more accounts switched on, offer offset anchors (for example 05:00 and 07:30). Their resets interleave, so account spread's failover always has a fresh window to move to.
  - **Skip idle accounts:** don't wake an account that saw no use in the last N days.
- **State lives in machine-local files**, so `kete wake` works with Kete closed and no daemon running, and nothing is forwarded to remotes:
  - Schedules (switch, anchor, days, options) go in `config.local.json` under a `wake` map keyed by account. That map is listed in `NOT_ROWS` because it is edited in its own view. It must be written to the local layer only, never config.json, which goes to remotes on `kete ssh`.
  - Runs are appended to `<data>/wake-log.jsonl`, one line per decision (fired, skipped and why, failed), with a lock. The apps read and watch it.
  - Usual-activity hours come from the account's turn timestamps. If reading them needs the daemon, the lane adds one additive request at protocol 61 and says so.
- **Runner.** Research found that `claude -p` billing moved in 2026. Max plans now get monthly API credits that cover `claude -p`, while the help page still says it can use subscription limits. A `-p` ping may therefore not start the 5-hour window at all. So the runner has two modes.
  - **Interactive (the default).** `kete wake` asks the daemon (starting it if needed, as the CLI always does) to run a hidden, unlisted Claude session:
    - under the account's profile, in an empty trusted `<data>/wake` folder, with Kete's statusline settings
    - it types a one-word prompt, waits for the turn to finish (Stop hook or idle), then ends the session
    - Interactive use is plainly subscription use, and Kete's statusline hook then records a real `rate_limits` reading with the true `resets_at`. No estimate needed.
    - This takes one additive daemon request at protocol 61 (`WakeAccount { account }` → done with the reading or an error). The lane may take it.
    - The folder-trust prompt is handled the way Kete already handles worktree trust (check `registry/spawn.rs`), or by pre-seeding trust for the wake folder in the profile.
  - **Print** (fallback, a per-account option). It runs a minimal headless `claude -p` under the profile's `CLAUDE_CONFIG_DIR` (profile env via `profile.prepare` + `launch_env`, base `claude_bg::run_captured`):
    - the cheapest model alias, one turn, a one-word prompt
    - no session persistence
    - the slimmest flags the installed CLI supports, checked with `claude --help` only. Never `--bare`, which likely needs an API key.
    - cwd is an empty `<data>/wake` folder, so no project hooks, CLAUDE.md or worktree are touched
    - a timeout, with the process tree killed on timeout
  - In print mode it records the decision and any reset time the CLI reports in the wake log, never in the usage cache that account spread reads. The timeline marks such a window as estimated until a real statusline reading replaces it.
  - **Verification:** the wake log records `resets_at` before and after every heartbeat, so a ping that didn't move the window shows up as "didn't start a window" in the view. Jay's first real morning run confirms the mode works on his plan.
  - `kete wake --all` evaluates every switched-on account; it's what the scheduled task runs.
- **Trigger.**
  - On Windows, one Task Scheduler task `Kete\Whakaoho` is created from an XML definition, so it can set `WakeToRun` (when the user allows it) and `StartWhenAvailable` (catch-up after sleep). It runs `kete wake --all`. Its triggers are each switched-on anchor, plus the anchor + 5h, + 10h… inside active hours when "keep the chain rolling" is on, since that chain sits on a fixed grid.
  - The task is rewritten whenever a schedule changes and removed when no account is switched on.
  - On macOS and Linux the lane prints the equivalent crontab line in the view instead of installing anything (YAGNI for now).
- **Terms.**
  - The view says plainly what it does: it sends one tiny prompt with the user's own subscription at the chosen time.
  - Research found no Anthropic statement on warm-ups. The Consumer Terms bar automated access except where Anthropic explicitly permits it. Claude Code, including `-p`, is a permitted client.
  - The feature stays off by default, per account, with that note in its help text.
- **Timeline.**
  - One row per account. Each row shows:
    - the live 5-hour window as a band from start (`resets_at` − 5h) to reset
    - the weekly reset as a tick
    - the Opus weekly bar
    - the scheduled wake-up as a marker
  - A "now" line and the next 24 hours.
  - Capacity bars sit beside each row, using the same `UsageBar` and its colours.
  - The rows scale to many accounts by staying one line tall.
- **Surfaces.**
  - **Desktop:**
    - a rail-level "Whakaoho" entry with the sunrise-over-kete mark, opening the view: timeline on top, then one row per account with its switch, hour slider, days and options, the last decision from the log, and "Wake now"
    - AccountsView rows link to it
    - the Account menu gets "Wake up now" and "Wake-up schedule…"
  - **TUI:**
    - the accounts overlay gets a Wake tab: the same rows, with ←/→ moving the anchor in 15-minute steps
    - the 24-hour strip drawn with block characters
    - the `wake_account` verb on account rows
- **Development.** Uses the fake-agent harness only. The first real wake-up on Jay's accounts is his morning check item, because it starts a real window.

## 0.57 (outline)

- Claude Code release gaps that the review marks BUILD.
- Visual QA of 0.54–0.56 at 640/900/1280 widths, light and dark, with TUI draw tests at 80×24 and 120×40.

## Context-menu matrix (desktop; TUI is the same minus headings, separators and disabled rows)

| Target | Rows (registry ids or local rows, in order) |
|---|---|
| Session | Open · Reply · Diff · Review… — Open in ▸ (VS Code, Visual Studio, Explorer) · Dock editor beside Kete — Copy ▸ (Path, Branch) — Account ▸ — Restart · Duplicate · Move to… · Rename · Pin as orchestrator — PR ▸ (Open, Create, Merge, Issue) — Focus scope · Move up/down — Archive/Unarchive · Delete |
| Terminal row | Open · Rename · Copy path · Delete |
| Terminal area | Copy · Paste · Select all · Clear · Copy folder path · Open link |
| Worktree | New session · Quick prompt · New terminal · Run/Stop · Diff · Files — Open in ▸ · Dock editor — Copy ▸ — Switch branch · PR ▸ — Focus scope · Move — Delete sessions · Delete worktree |
| Repo | Quick prompt · New worktree · Switch branch · Run/Stop — Open in ▸ · Dock editor — Copy ▸ — Refresh PRs · Default account ▸ · Rename · Move — Sweep merged worktrees · Remove project |
| Folder | Open in VS Code · Show in Explorer — Copy path · Copy relative path — Expand/Collapse all |
| File | Open in Kete · Open in VS Code · Open in Visual Studio · Open with default app · Show in Explorer — Copy path · Copy relative path — Diff this file · Mark reviewed (diff only) · Stage/Discard (diff only) |
| Account | Use as default · Use for this project — Wake up now · Wake-up schedule… — Enable/Disable · Log in again · Rename — Usage details — Remove… |
| PR | Open · Copy URL · Review… · Vote ▸ · Merge… · Check out |
| Branch | Check out in new worktree · Copy name · Open in browser |
| Task | Start · Open in browser · Copy id |
| Text field | Undo · Cut · Copy · Paste · Select all |
| Diff/viewer text | Copy · Select all · Copy path:line · Open this line in VS Code |
| Background | New session · Quick prompt · Add project · Command palette — View ▸ — Show archived · Settings — Critters |
| Critter | Pet · Feed · Name… · Shoo — Hide critters until tomorrow · Critter log… · Critter settings… |

## Non-goals

- Embedding editor windows.
- A code outline.
- Cloud routines for wake-up: Task Scheduler covers it locally.
- OS file drops into the window (`dragDropEnabled:false` keeps HTML5 drag working).
- Copies of game characters or logos.

## Claude Code releases

The release review read every weekly digest from W13 to W37 and the changelog up to the installed 2.1.301. The full report is in `.superpowers/sdd/2026-10-16-release-054/research-claude-releases.md`. Most new features are already covered or don't fit an interactive multi-account manager: agent view headlines, cross-session messaging, push notifications, model-picker settings, `worktree.baseRef` and `--restricted`/`--safe-mode` are cut. `--safe-mode` also disables hooks and the statusline, so Kete must never pass it, and the wake runner doesn't either.

Four real gaps go into 0.54 as `lane/limits`:

- **B1. Usage-limit and API-error state.**
  - **Why:**
    - A turn that ends on an API error fires `StopFailure` instead of `Stop`. Kete doesn't install that hook.
    - Auto-continue at a usage limit has been on by default since 2.1.234. It sends no `idle_prompt` while it waits.
    - Kete also ignores the `quota_auto_resume_*` notifications.
    - So a rate-limited or logged-out session lands in Review or Done as if it had finished, and a session waiting to resume at 15:45 looks done.
  - **The fix:**
    - Install `StopFailure` and map its `error`:
      - `rate_limit`: Running with "resumes 15:45"
      - auth, billing or account-hold errors: Needs you, with the reason
      - overloaded and similar: Done with an error badge
    - Handle `quota_auto_resume_fired/stale/disabled`.
    - Treat `agent_needs_input` and `elicitation_*` as waiting on the user. Auto mode has been the default since 2.1.284, and those dialogs used to be dropped.
    - The reset time comes from the footer text ("Continuing automatically at 3:45pm") via the existing `pty/usage.rs` scanner, falling back to the account's `resets_at`.
  - **Wire format:** no new `AgentStatus` variant, because older clients break on one. Instead, `limit_until` and `stop_reason` side fields go behind a protocol gate, in memory only.
- **B2. Effort sync.**
  - **Why:** Kete re-passes the stored `--effort` on every spawn and resume. A mid-session `/effort` change is lost on restart or relocation, and the card shows `max` when the `maxEffortLevel` cap runs it at `high`.
  - **The fix:** read `effort.level` from the statusline and from hook payloads, then store and broadcast it, as the model already is.
- **B3. Windows hook hygiene.**
  - On Windows, write Claude hooks in exec form (`command` + `args`). That removes a shell process and a quoting workaround per hook. `PostToolUse` currently fires on every tool call.
  - Add `PowerShell(kete …)` allow rules beside the `Bash(kete …)` ones, for machines without Git Bash.
  - 2.1.299 makes some hooks fail closed when Git for Windows is absent (reading unverified). Exec form removes that exposure.
- **B4. `Stop.background_tasks`.** Use the real list of in-flight subagents and workflows at turn end, instead of the 180-second drain and 30-minute quiet-grace guesses. Shells and monitors never hold a row Running.

**Later:**
- `--resume <transcript>` to continue a session on another account after a limit. Spike whether it writes into the second profile's folder.
- `claude purge` as an opt-in on worktree delete.
- `CwdChanged`.
- Claude's `dark-ansi` theme via Kete's settings file.
- Session recap: it needs focus forwarding and costs quota.

**Strains to watch:**
- Kete relies on OSC 9;4 progress, which the docs list only for ConEmu, Ghostty and iTerm2. A per-release canary against the real CLI belongs in Jay's manual checklist.
- A `/fork` copy that inherits `NEBULA_*` might take over a row's session id. Reproduce before changing anything.
