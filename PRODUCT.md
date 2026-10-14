# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

<!-- The GUI is a Tauri desktop app (web view). The terminal TUI (ratatui) is a second surface over the same daemon. -->

## Stack

- Rust workspace forked from nebula (MIT, AgentSystemLabs): daemon + ratatui TUI.
- Added: a Tauri v2 desktop GUI over the daemon's IPC.
- Windows-native first; macOS and Linux stay supported from the same codebase.

## Users

Developers who run several AI coding agents in parallel across projects and git worktrees, and need to know at a glance which agent needs them. Public, open source from day one; the first user is a Windows 11 developer with two Claude Max subscriptions, a ChatGPT subscription (Codex) and GitHub Copilot.

## Product Purpose

Kete holds your coding agents in one place. One daemon owns every agent session (Claude Code, Codex, Copilot, Cursor, OpenCode, Pi, Grok, Muse), sessions survive closing the UI, and every session's state is readable without opening it. Success: you never tab through terminals to find which agent is waiting on you.

## Positioning

- Native Windows: no WSL, no tmux.
- A local daemon that owns the PTYs, with nebula's keyboard-first workflow.
- Multi-account profiles: several subscriptions per harness, isolated config dirs, switch and resume.
- Several worktrees running dev servers side by side, with ports allocated and visible per worktree.

## Operating Context

- Windows Terminal, the VS Code integrated terminal, or Kete's own desktop window.
- PowerShell 7 / 5.1, Git Bash optional.
- `git`, `gh`, `git worktree`.
- Dev servers on localhost (Next.js, Vite and similar), opened in Chrome, Edge or Firefox.
- Works next to VS Code. The user's VS Code uses Atom One Dark, JetBrains Mono, and Material Icon Theme; Kete should feel at home beside it.

## Capabilities and Constraints

- **Inherited from nebula:**
  - session cards with status dots (running / unseen / done / needs input)
  - project tabs, worktree bands, quick prompt, orchestrator chat
  - diff viewer, file finder, grep, tree browser
  - PR and issue views via `gh`
  - harness table configured from `config.json`
- **Added in Kete:**
  - **Windows port:** ConPTY sessions, a named-pipe daemon, `%LOCALAPPDATA%\kete`.
  - **Profiles:** accounts per harness (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GH_CONFIG_DIR`).
  - **Copilot:** as a harness.
  - **Kanban:** sessions in lanes by status (`Shift+K`).
  - **Diff:** three layouts (side by side, inline, replacement), word highlights, per-change stage and discard.
  - **File icons:** Material Icon Theme colours, Nerd Font glyphs with an ASCII fallback.
  - **Themes:** One Dark and One Light, following the OS on request, or imported from the VS Code theme.
  - **Localhost previews:**
    - a port block per worktree, allocated by the daemon
    - live ports on each worktree's band
    - open in a chosen browser (Chrome, Edge or Firefox)
    - headless screenshots
  - **`kete setup terminal`:** JetBrainsMono Nerd Font and a Kete profile in Windows Terminal.
  - **Desktop window:** board, live sessions, read-only diff.
- **CLI:** `kete`, with `bro` kept as an alias (Kete's old name). It must not collide with nebula's `nebula`; data lives in `%LOCALAPPDATA%\kete`, moved over from `%LOCALAPPDATA%\bro` on first start.
- **Optional:** agent-office runs alongside as a demo; it is not a dependency.

## Brand Commitments

- **Name:** Kete, from te reo Māori for a woven flax basket. The basket holds your agents, their work and its scope. Written `kete` as a command and wordmark, Kete in a sentence.
- **Voice:** plain, warm and direct, a little kiwi. Short sentences, concrete verbs. Never cutesy, never salesy. No em dashes in user-facing copy.
- **Tagline:** "Sweet as agent management."
- **Attribution:** keep nebula's MIT notice and credit.

## Evidence on Hand

- Upstream nebula README and docs describe the inherited workflow; the terminal showcase (`assets/showcase/`) and the desktop screenshots show Kete's own, on synthetic demo data.
- No users, testimonials, or benchmarks yet. Do not fabricate any.

## Product Principles

1. Status over screens: every session's state is readable without opening it.
2. Keyboard first, mouse welcome.
3. Native on Windows, honest everywhere: no WSL or tmux requirement, and macOS and Linux keep working.
4. Accounts are first-class: quota and identity are visible per session.
5. Never lose an agent: the daemon outlives every UI.

## Accessibility & Inclusion

- Status must not rely on colour alone: dots carry a glyph or label.
- Readable in both light and dark themes.
- Fully operable from the keyboard.
