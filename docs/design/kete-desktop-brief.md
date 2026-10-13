# Kete desktop: surface brief

Mode: Operate. Users are developers running many coding agents across 4+ repos in a working day. The structure varies: sometimes one planning chat, sometimes an orchestrator spawning subagents that spawn subagents. Success: from the home screen, you know in two seconds which agent needs you and where everything else stands, without opening anything.

## Direction contract

**THESIS.** The home screen is a workshop bench, not a dashboard. It shows one row per repo, and each row holds its session tree (orchestrator > subagents > theirs). It refuses the category default: rounded cards in coloured kanban columns, glowing dots, and gradient chrome.

**OWN-WORLD.** Workshop palette, warm and square.
- **Materials:** bone and stone in light, charcoal and soot in dark.
- **Accent:** one ember carries attention.
- **Lines:** 1px rules only. Zero radius. No shadows, gradients or glass.
- **Type:** Geist for UI, with hierarchy set by weight and size and no colour noise. Geist Mono for paths, ports, branches and counts. JetBrains Mono inside terminals.
- **Status:** a 6px square plus a word (needs you / running / unread / done / failed / merged). Never colour alone.
- **Modes:** system, light and dark. System is the default and follows the OS.

**STORY.** The user sees attention first, then structure:
1. The top strip counts across all repos (`2 need you · 5 running · 3 unread`). Each count jumps to that session.
2. Repo rows follow, ordered by attention: a row with a needs-you session floats up.
3. Inside a row, the tree shows depth with indents and hairline guides. A deep tree collapses to `orchestrator + 7 agents` until expanded.

Keys and mouse both work everywhere.

**FIRST VIEWPORT.**
- **Left rail (220px):** the kete mark, the repo list with attention counts, profiles, and theme toggle.
- **Main column, top:** the attention strip (40px, ember text when anything needs you).
- **Main column, below:** repo rows. Each has a header (repo name in Geist SemiBold 15, then branch, worktrees, live ports and PR state as right-aligned Geist Mono 12 labels), then the session tree. Each session line shows status square, name, harness·profile, the last prompt (truncated, muted) and age (right).
- **Quick prompt:** pinned at the bottom, one line and full width, `p` to focus.
- **Session and diff:** open as a right split (resizable), so the overview stays visible.

**FORM.** Workshop (user-chosen world). Kanban stays as the alternate view (`Shift+K`), in the same grammar: columns are ruled lanes, not cards.

**SIGNATURE.** A needs-you session row carries a full-height ember bar on its left edge, plus ember text. When it resolves, the bar retracts (120ms, no bounce). Nothing else animates except a 1px running line under a running session's name.

## Tokens (theme.css)

| Role | Light | Dark |
|---|---|---|
| `--bg` | `#F4EFE6` paper | `#161412` soot |
| `--surface` | `#EDE6DA` bone | `#1E1B18` |
| `--raised` | `#E4DBCC` stone | `#26221E` |
| `--rule` | `#CFC5B4` | `#3A342D` |
| `--rule-strong` | `#1C1917` | `#EDE6DA` |
| `--ink` | `#1C1917` charcoal | `#EDE6DA` bone |
| `--muted` | `#57534E` | `#A89F92` |
| `--faint` | `#8A8178` | `#6E665C` |
| `--ember` (fill) | `#D9822B` | `#E08A35` |
| `--ember-ink` (text) | `#A9550F` | `#F0A35A` |
| `--unread` | `#2F6F6A` teal | `#5FA8A0` |
| `--done` | `#5B6B2E` olive | `#9BAA5E` |
| `--fail` | `#A23B22` oxide | `#D2694E` |
| `--merged` | `#6B4A7A` plum | `#B48EC2` |
| `--running` | `#1C1917` | `#EDE6DA` |

Type scale (px): 12 mono label, 13 body-small, 14 body, 15 row title, 18 section, 28 view title. Weights: 400 body, 500 labels, 600 titles, 700 counts in the attention strip. Spacing: 4/8/12/16/24/32.

## TUI and terminal

Matching `kete-dark` and `kete-light` TUI presets (truecolor), mapped to the same roles. A "Kete Workshop" Windows Terminal colour scheme ships via `kete setup terminal` in place of One Dark (One Dark stays available).
