<div align="center">

# bro

**Sweet as agent management.**

Mission control for your coding agents, native on Windows. Run **Claude Code**, **Codex**, **Cursor**,
**Pi**, **Muse**, **Grok Build**, **OpenCode** and **Copilot** across every project and git worktree you own, from one
terminal, one keyboard, one grid. They keep working when you close it.

[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square)](#install)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square)](https://www.rust-lang.org)

[**Keys**](docs/keys.md) · [**Commands**](docs/commands.md) · [**Sessions**](docs/sessions.md) · [**Configuration**](docs/configuration.md) · [**How it works**](docs/how-it-works.md) · [**Windows port**](docs/PORT.md)

```powershell
irm https://raw.githubusercontent.com/JayRichh/bro/main/install.ps1 | iex
```

<img src="assets/screenshot.png" alt="Project tabs with status dots across the top, a band of session cards per checkout, and the running session's terminal under them" width="100%">

</div>

---

Bro is a fork of nebula by AgentSystemLabs.
It keeps nebula's keyboard-first workflow and daemon, and runs natively on Windows 10 1809+ and
Windows 11 (x64 and ARM64): no WSL, no tmux. macOS and Linux build from the same tree.

## Three agents, three tabs, and no idea which one needs you

You start three agents in three terminal tabs. Five minutes later you don't know which one is waiting
on a permission prompt, which one finished, and which one is still thinking. So you tab through all
three and read the screens, every time.

Bro swaps the reading for a grid and a colour. Every session is a card with a status dot that says
what it's doing. The cards sit in a band per checkout under a row of project tabs, and every tab
carries its sessions' dots with a count. A red `●1` on a tab tells you where to look without opening
anything.

## What you get

| | |
|---|---|
| **One grid, every session** | Project tabs across the top, a band per worktree, a card per session, and the live terminal of the card under the cursor along the bottom or down the right. `j`/`k` walk the bands, `Tab` opens one, `h`/`l` walk its cards, `Enter` steps into the pane, `` Ctrl+` `` hands the keys back. |
| **A daemon that owns the PTYs** | Close the TUI, come back tomorrow. The agents never stopped, and their scrollback is replayed when you attach. On Windows the daemon talks over a per-user named pipe and runs sessions on ConPTY. |
| **Status dots you read instead of screens** | Yellow mid-turn, blue finished and unseen, green finished and read, red waiting on you. On every card, and counted on every project tab. |
| **A task box, not a picker** | `p` opens the quick prompt: type the task, `Enter`, and an agent is on it. Project, worktree, harness and model are each a picker you can open without losing the text. |
| **Real git worktrees, one keystroke** | `Ctrl+N` in the box sends the launch to a fresh `git worktree`. Two agents in two directories never collide. |
| **One orchestrator chat** | `F10` toggles a pinned agent chat that can list, read, message and spawn sessions across projects. |
| **Agents that drive Bro back** | Ask a session to "do this in a worktree" and it runs `bro worktree`. "Show me the file" runs `bro open`. "Start a new session that..." runs `bro spawn`. `bro sessions`, `bro read` and `bro ask` pull context from other sessions. |
| **Pull requests and issues in place** | Bro asks `gh` what's open. `v` lists pull requests, `i` lists issues, and either one can start an agent on it. |
| **Diff, find, grep, browse** | `g` opens the diff viewer, `f` the file finder, `F` a `git grep`, `b` the tree browser, all scoped to the card's worktree. |
| **`/` finds anything** | The palette spans every project on the machine, sorted by what needs you first. |

## Supported harnesses

| Harness | CLI |
|---|---|
| [Claude Code](https://code.claude.com/docs/en/setup) | `claude` |
| [Codex](https://github.com/openai/codex) | `codex` |
| [Cursor](https://cursor.com/install) | `cursor-agent` |
| [Pi](https://pi.dev) | `pi` |
| [Muse](https://developer.meta.com/ai/lp/muse-code) | `muse` |
| [OpenCode](https://opencode.ai/docs/) | `opencode` |
| [Grok Build](https://github.com/xai-org/grok-build) | `grok` |
| [GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli) | `copilot` |

Install the CLI, pick it in the quick prompt's `Tab` picker, done. More can be added from
`config.json`; see [Configuration](docs/configuration.md).

## Install

**Windows** (PowerShell 5.1 or 7):

```powershell
irm https://raw.githubusercontent.com/JayRichh/bro/main/install.ps1 | iex
```

It downloads `bro.exe` for your CPU from the latest GitHub release into `%LOCALAPPDATA%\bro\bin` and
adds that folder to your user `PATH`. With no release asset to download, it falls back to
`cargo install` from the `windows` branch and says so. Run it again to update.

**macOS and Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/JayRichh/bro/main/install.sh | sh
```

It installs into `~/.local/bin` (override with `NEBULA_INSTALL_DIR`), falling back to
`cargo install --git` when no release matches. `bro upgrade` re-runs it for you.

You need at least one agent CLI on your `PATH`, and `git`. Bro spawns the agents; it doesn't ship them.
`gh` is optional and lights up the pull request and issue views.

On Windows, `bro ssh`, `bro tunnel`, `bro browser` and `bro upgrade` are not available yet and say
so. To update on Windows, run the installer again.

## Quickstart

```powershell
bro add C:\code\my-app    # or, from inside the repo: bro add .
bro                       # open the TUI; the daemon starts on its own
```

Press `p`, type a task, `Enter`. Press `Ctrl+N` in the box first to give it its own worktree. Press
`q` to leave: the sessions keep running, and `bro` brings you back to them.

## Where things live

| | Windows | macOS | Linux |
|---|---|---|---|
| Settings and database | `%LOCALAPPDATA%\bro\data` | `~/Library/Application Support/dev.bro.bro` | `~/.local/share/bro` |
| Runtime (pidfile, socket) | `%LOCALAPPDATA%\bro\run` | `$XDG_RUNTIME_DIR/bro` or `/tmp/bro-<uid>` | same as macOS |

A project's run and open commands live in `.bro.json` at the root of the repo. Bro still reads
`.nebula.json` when a checkout has no `.bro.json`. Environment overrides keep their `NEBULA_*` names.

## Coming

These are planned and not in Bro yet:

- **Profiles:** several accounts per harness (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GH_CONFIG_DIR`), picked per session.
- **Kanban:** sessions laid out by status.
- **A VS Code-grade diff:** side-by-side, inline and replacement modes, with per-hunk accept and reject.
- **Theme and file icons:** follows your VS Code theme, light or dark, with Material-style file icons.
- **Ports per worktree:** dev server ports allocated and shown per worktree, opened in Chrome, Edge or Firefox.
- **A desktop GUI:** a Tauri window over the same daemon.
- **GitHub Copilot** as a harness.

## Building

```sh
cargo build --release     # target/release/bro (bro.exe on Windows)
cargo test
```

The crates keep their upstream names (`nebula-core`, `nebula-daemon`, `nebula-tui`, `nebula`) so
rebasing on nebula stays cheap. The rebrand is applied by `scripts/rebrand.sh`, which is safe to run
again after a rebase.

## Credits and license

Bro is a fork of nebula by AgentSystemLabs.
nebula is © Cody Seibert and released under the MIT license; Bro keeps that license and notice. See
[LICENSE](LICENSE).
