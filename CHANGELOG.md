# Changelog

## 0.46.0

The desktop app catches up with the terminal app, and both get an orchestrator, account usage and
`kete ado`.

- **Desktop settings.** A Settings view (`Ctrl+,`, or the gear at the bottom of the rail) over the
  same `config.json` the terminal app uses: General, Sessions, Appearance, Agents, Project,
  Experimental and a read-only Hotkeys tab, plus Desktop, Presets and Profiles. Rows that only the
  terminal app uses sit under a "Terminal app" group and stay editable. Reset puts back what the
  window's tabs show and leaves project commands, profiles, harness definitions, hotkeys and hand-added
  keys alone. The window's theme, diff layout, home view and **Open on** are saved under a `desktop`
  object, separate from the terminal app's `theme`.
- **Live settings in the terminal app.** A running `kete` picks up a change to `config.json` or
  `config.local.json` within about three seconds, whoever wrote it, and waits while its own settings
  overlay is open.
- **Desktop launches follow the Agents tab.** New sessions get the configured model and effort, custom
  harnesses launch as themselves, and the harness list follows each harness's Enabled switch,
  `harnesses` overrides and Hide missing CLIs.
- **Desktop session and repo management.** Right-click (or `Shift+F10`) on a session row, board card,
  session header or repo header for the terminal app's card menu: follow-up, restart, duplicate, move,
  rename, mark seen, archive, unarchive and delete, plus run and stop, new terminal, diff, open in
  editor, open remote, new and delete worktree, rename, locate and remove a repo. Row keys follow the
  terminal app's (`r`, `a`, `u`, `m`, `Shift+P`), and `Del` deletes. Archived sessions show with `Shift+A`. Unread
  clears wherever you see the session.
- **Command palette and help.** `Ctrl+K` (or `Ctrl+Shift+P`) jumps to any repo, worktree, session or
  pull request, what needs you first, and runs any window action or menu verb. `?` or `F1` lists every
  window key. `Ctrl+Shift+P` is the palette now; `Ctrl+Shift+N` stays the quick prompt.
- **Desktop quick prompt.** Model, effort, profile and agent preset pickers, a New worktree switch, and
  multi-line tasks (`Shift+Enter` for a new line). It honours **Follow new session**.
- **Presets and profiles in the window.** Settings, Presets edits the terminal app's agent presets;
  Settings, Profiles adds, edits and removes `kete profile` accounts. A profile that needs a login shows
  the `kete profile login <name>` command to copy: the window never signs anything in.
- **Orchestrator.** One pinned orchestrator agent per machine that watches every repo, reads and
  messages other sessions, starts workers in fresh worktrees and asks you to review them. `F10` in the
  terminal app starts it (pick a harness and profile) or opens it full-screen with every other session
  as a chip; a review opens a modal with Terminal, Diff, History and PR tabs. In the desktop app it has
  its own view (`0` or `F10`, or the rail item above Repos): its cross-repo tree beside its chat, review
  notices that open a review sheet, and the quick prompt sending it follow-ups. `kete orchestrator
  start|status|clear` from the CLI. **Open on the orchestrator** (`orchestrator_view`) starts the
  terminal app on it; the window's **Open on** does the same there. `F10` no longer crashes the
  terminal app.
- **`kete ado`.** Azure DevOps from the CLI, for you or the orchestrator: `tasks`, `start-task` (branch,
  worktree and agent in one step), `prs`, `pr`, `pr create` (a draft by default, with the desktop
  composer's rules), `vote` and `reply`. `--project` and `--worktree` name the repo from anywhere. It
  never pushes the default branch and never signs in interactively.
- **Accounts and usage.** Kete reads each account's 5h and weekly use (Copilot's monthly allowances)
  and each session's context, from Claude's status line, Codex's session logs, Copilot's logs and its
  quota. `Shift+U` in the terminal app and the rail's Accounts list in the window show bars, resets,
  login state and an on/off switch per profile; session cards and rows show `ctx 42%` and compactions.
  `kete profile list` draws the same table, and `kete profile enable|disable` switches a profile.
- **Account spread.** Settings, Sessions, Accounts: `failover` moves new sessions to the next enabled
  profile when the default reaches a threshold, `least_used` picks the least used one, `manual` is the
  old behaviour and the default. A named profile is always used as named.
- **Claude status line.** Claude sessions start with `--settings` pointing at a status line in Kete's
  data dir (`kete _statusline`). It feeds the usage readings, then runs your own statusLine, or prints a
  short `ctx · 5h · wk` line. A custom status line takes the place of Claude's footer hints; turn it off
  with Settings, Sessions, **Usage status line**. Nothing is written into your repos.
- **Safer settings files.** Every writer (the terminal app, the window, `kete profile`, settings
  imports) takes a lock on the file and writes through a temp file of its own, and the window writes
  only the keys it changed. A file saved with a byte order mark (Notepad) reads fine. A `config.json`,
  `config.local.json` or `agent_presets.json` that exists but can't be parsed is never overwritten:
  saves fail with a message naming it until you fix or remove it. Before, a broken local layer was
  ignored and a broken presets file was replaced.
- **Open command consent.** In the window, Open in editor shows an open command that comes from a
  repo's `.kete.json` and asks before it first runs, per repo and command.
- **Windows fix.** Hide missing CLIs finds `claude.exe`, `codex.cmd` and the like through `PATHEXT`; it
  used to hide every harness.

Upgrading: the daemon speaks protocol 55 and still accepts 53, so 0.45 and 0.46 clients and daemons
talk to each other. The orchestrator and the usage readings need a 0.46 daemon. On macOS and Linux,
`kete upgrade` moves the running daemon onto the new binary with every session kept (or run
`kete reload` after installing another way). On Windows in-place restart isn't there yet: the 0.45
daemon keeps running after the installer, and `kete kill` (which stops every session) is what moves
you onto 0.46. On Windows the in-app hints say `kete kill`.

Going back to 0.45 works, with two things to know. The database stays on 0.46's schema, which 0.45
opens as it is, so the index that allows one orchestrator stays. And 0.45 doesn't know a profile's
`disabled` flag: it launches disabled profiles, and drops the flag the next time it edits the profile
list.

## 0.45.0

The first Kete release: a fork of [nebula](https://github.com/AgentSystemLabs/nebula), native on
Windows, with a desktop app and Azure DevOps.

- **Windows port.** Runs natively on Windows 10 1809+ and 11, x64 and ARM64, with no WSL or tmux. The
  daemon talks over a per-user named pipe and runs sessions on ConPTY.
- **Rebrand.** Nebula is now Kete: the `kete` binary, with `bro` (the old name) beside it as the same
  program. Bro's data moves to Kete's folders on first run.
- **Desktop app.** `kete desktop` opens a native window over the same daemon: an overview of every
  repo's session tree, nested agents and subagents included, ordered by what needs you; the board;
  live terminals; and a diff view with side by side, inline and replacement modes.
- **Azure DevOps.** Sprint tasks with Start task (branch, worktree and agent in one step), branches
  grouped by person, and pull requests: a draft composer with policy notes, policy status, votes,
  threads and publish. Sign-in comes from Git Credential Manager or the Azure CLI, held in memory.
- **Workshop themes.** A warm, square light and dark palette for the desktop app, the `kete-dark`
  and `kete-light` TUI themes (`kete-dark` is the default), and a Kete Workshop Windows Terminal
  scheme.
- **Profiles.** Several accounts per harness, each with its own config dir.
- **Copilot.** GitHub Copilot CLI joins the supported harnesses.
- **Ports.** Each worktree gets its own block of localhost ports, shown on its band.
- **Installer.** `install.ps1` installs `kete` and `bro` and, on x64, the desktop app, per user with
  no admin prompt.
