# Kete on Windows: the port of nebula

Kete is a cross-platform fork of [nebula](https://github.com/AgentSystemLabs/nebula) (MIT) that runs natively on Windows 10 1809+ and Windows 11 (x64, ARM64). macOS and Linux keep building from the same tree. Binary: `kete`.

## Ground rules

- Every OS difference lives behind `cfg` in a platform seam (`nebula-core::platform`, plus per-crate `platform` modules where a crate's own types are involved). Call sites stay OS-agnostic.
- Crate and module names stay `nebula-*`, to keep upstream rebases cheap. User-facing names change: binary, data dirs, pipe name, installer, and every string where the app tells an agent or a user to run itself.
- `cfg(unix)` code stays byte-identical where possible. Windows gets a sibling implementation, not a rewrite of the unix one.
- `cargo check --workspace --all-targets` must stay green on Windows, Linux and macOS.

## ConPTY findings (spike, verified on Windows 11 26200)

- **Startup DSR:** ConPTY sends `ESC[6n` at startup and blocks until it gets a reply. The daemon already answers DSR (`pty/kitty.rs` `Reply::CursorPosition`).
- **Passthrough:** OSC 0 titles, OSC 8 hyperlinks, OSC 9;4 progress, kitty keyboard queries (`ESC[>5u`, `ESC[?u`) and DECSET 2026 all reach the reader unchanged.
- **Claude on Windows:** emits no OSC 9;4 even with `TERM=xterm-256color`, `WT_SESSION` or `TERM_PROGRAM` set. Busy/idle uses hooks plus the OSC 0 spinner glyph (`pty/title.rs`). Cancel detection uses the title flipping to the idle glyph while no permission notification is pending.
- **Codex:** spinner glyph titles (`⠋ kete`) arrive. On first run in a folder, Codex shows trust and update dialogs.

## Platform map

| Concern | Unix (upstream) | Windows |
|---|---|---|
| Runtime dir | `$XDG_RUNTIME_DIR/nebula`, `/tmp/nebula-<uid>` (0700) | `%LOCALAPPDATA%\kete\run` |
| Daemon endpoint | unix socket `daemon.sock` | named pipe `\\.\pipe\kete-<user>-<hash(runtime_dir)>`, current-user DACL, `reject_remote_clients` |
| Daemon spawn | `setsid` via `pre_exec` | `CREATE_NEW_PROCESS_GROUP \| DETACHED_PROCESS \| CREATE_NO_WINDOW` |
| Pidfile lock | `flock` | `std::fs::File::try_lock` (exclusive; it is the `LockFileEx` analogue) |
| File identity | `ino()` | `file_index` via `GetFileInformationByHandle`, or skip the check |
| Permissions | `mode(0o700/0o600)` | no-op (the per-user profile dir is the boundary) |
| Signals | SIGTERM/SIGHUP/SIGINT/SIGWINCH | `tokio::signal::windows::{ctrl_c, ctrl_close, ctrl_shutdown}`; the other signals are absent |
| Kill a session tree | `killpg` | job object per session (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), or `taskkill /T /F /PID` |
| In-place restart (`handoff.rs`) | exec over self, fds kept | unsupported: the daemon stops and sessions resume by id |
| Agent launch | `/bin/sh -c` login-shell wrapper | direct spawn: resolve via PATH + PATHEXT; `.cmd`/`.bat` through `cmd /d /s /c` with strict quoting; `.ps1` through `pwsh -NoProfile -File` (or `powershell`) |
| User shell | `$SHELL` | `KETE_SHELL` (or `BRO_SHELL`), else pwsh 7, then `powershell.exe`, then `%COMSPEC%` |
| Hooks | `curl … ${API_TOKEN}` shell strings | `kete _hook <event>` subcommand: token from env, payload from stdin |
| `ssh` / `tunnel` / `browser` | supported | disabled on Windows at first (clear error) |

## Features beyond the port

- **Profiles:** several accounts per harness. Each profile maps to env: `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GH_CONFIG_DIR`, `COPILOT_*`. A picker in the quick prompt chooses one, quota is shown per profile, and switch-and-resume is supported.
- **Copilot:** as a harness.
- **Kanban:** sessions in lanes by status (Needs input, Running, Unseen, Done, PR open), on `Shift+K`.
- **Diff:** side by side, inline and replacement layouts, word highlights, and per-change stage and discard.
- **Themes:** One Dark and One Light, `auto` to follow the OS, or `vscode` to derive the palette from the VS Code theme.
- **File icons:** Material Icon Theme mapping. Nerd Font glyphs in the TUI with an ASCII fallback; SVGs in the GUI.
- **Localhost:**
  - ports per worktree, allocated by the daemon and handed to sessions and the WORKTREE HOOK (see [ports.md](ports.md))
  - open in Chrome, Edge or Firefox
  - headless screenshots
- **`kete setup terminal`:** JetBrainsMono Nerd Font and a Kete profile in Windows Terminal.
- **Desktop GUI:** Tauri v2, a client of the daemon protocol over the named pipe: board, live sessions, read-only diff.

## The name

Kete was called Bro. `bro` is still installed beside `kete` as the same program, and the first
`kete` run moves `%LOCALAPPDATA%\bro\data` and `\run` to `%LOCALAPPDATA%\kete` (unix: the old data,
state and runtime dirs). It leaves them where they are while Bro's daemon still holds its lock, and
uses them, and that daemon's pipe, until it stops. `.bro.json`, `BRO_PORT` and `BRO_SHELL` keep
working.

## Upstream sync

```sh
git fetch upstream && git rebase upstream/main
```

The rebrand lives in its own commits, re-applied by `scripts/rebrand.sh`, which is idempotent.
