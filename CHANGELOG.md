# Changelog

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
