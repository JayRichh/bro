# Localhost ports

<sub>[← README](../README.md) · [Keys](keys.md) · [Commands](commands.md) · [Sessions](sessions.md) · [Configuration](configuration.md) · [How it works](how-it-works.md)</sub>

Two checkouts of one app both running `npm run dev` fight over `:3000`. Kete gives every WORKTREE a
PORT BLOCK of its own, tells each session in it which ports those are, watches what actually starts
listening, and opens or screenshots it from the grid.

## The port block

Each worktree gets `port_block` ports starting at `port_base + port_block × slot`. With the
defaults (`3000` and `10`) the first worktree that asks gets `3000`–`3009`, the next `3010`–`3019`,
and so on. A worktree's slot is the lowest free one when it first needs a block: its first session
or terminal, or its WORKTREE HOOK on create. The DAEMON keeps the slot in its database, so the block
never moves while the worktree exists. Deleting the worktree frees the slot for the next one.

Every agent session, shell terminal and RUN TERMINAL in the worktree starts with the block in its
environment:

| Variable | Value (slot 1) |
|---|---|
| `PORT` | `3010`, the block's first port |
| `KETE_PORT` | `3010`, the same, under a name nothing else sets |
| `KETE_PORT_1` … `KETE_PORT_9` | `3011` … `3019`, the rest of the block |
| `BRO_PORT`, `BRO_PORT_1` … | the same again under Kete's old name, so a config written for Bro still works |

Dev servers that read `PORT` (Next.js, Express, Remix, most of the Node world) bind there without
being told. Vite does not read it; point it there in `vite.config.ts` with
`server: { port: Number(process.env.PORT) }`, or run `vite --port $PORT`. Use `KETE_PORT_1` and up for
a second server, an API beside a frontend, a Storybook.

The WORKTREE HOOK gets the same variables, so a create hook can write the port into a `.env` file,
a proxy route or a database name before any session starts. The delete hook sees the block the
worktree held, so it can undo whatever it set up.

A session started before the worktree had a block, or a slot that would run past port 65535, simply
starts without the variables.

## Live ports

Every three seconds the daemon reads which TCP sockets are in LISTEN state on the machine and which
process owns each one. A port counts for a worktree when its owner is inside the process tree of one
of that worktree's sessions: the agent, the shell, the RUN TERMINAL, or anything they started.

- **Windows:** `GetExtendedTcpTable` for both IPv4 and IPv6, so a server bound only to `::1` (Vite and Next.js often are) is found too. The process tree comes from the toolhelp snapshot kete already uses for memory readouts.
- **macOS and Linux:** `lsof -nP -iTCP -sTCP:LISTEN`, best effort, with `ps` for the tree. Without `lsof` installed nothing is listed.

The worktree's band rule on the grid shows each live port with a green dot, `:3010 ●`, before the
session counts. Clients hear about a change when it happens, not on every sweep.

`kete sessions` adds `port=3010 listening=3010,3011` to each line, and `kete sessions --json` carries
`port` (the block's first port, or `null`) and `live_ports` (a list) on every row.

## Open in a browser

`w` opens a chooser for the selected worktree's dev server. The URL is
`http://localhost:<port>`, where the port is the first live one inside the block, else the first
live one at all, else the block's first port (a server you are about to start). With no block and
nothing listening, the footer says so.

The chooser lists the system default browser first, then each of these it finds installed:

| Browser | Windows | macOS | Linux |
|---|---|---|---|
| Google Chrome | `App Paths\chrome.exe` in the registry, then Program Files and `%LOCALAPPDATA%` | `/Applications`, `~/Applications` | `google-chrome`, `google-chrome-stable`, `chromium` on PATH |
| Microsoft Edge | `App Paths\msedge.exe`, then Program Files | same | `microsoft-edge`, `microsoft-edge-stable` |
| Firefox | `App Paths\firefox.exe`, then Program Files | same | `firefox` |

The system default opens through the OS's own handler (`url.dll` on Windows, `open` on macOS,
`xdg-open` on Linux).

A browser can open on one of its own profiles. Set it under `browsers` in `config.json`:

```json
{
  "browsers": {
    "chrome": { "profile": "Profile 1" },
    "edge": { "profile": "Default" },
    "firefox": { "profile": "dev" }
  }
}
```

Chrome and Edge get `--profile-directory=<profile>`, the folder name under their `User Data`
directory (`Default`, `Profile 1`, …; `chrome://version` shows it as *Profile Path*). Firefox gets
`-P <profile>`, the name `about:profiles` lists. The chooser shows the profile beside the browser's
name.

The pick is remembered per project, as `browser` in the project's `projects` entry, and the chooser
opens on it next time, marked `✓`.

## Screenshot

`Shift+W` takes a headless screenshot of the same URL and shows it. It runs Edge, or Chrome when
Edge is missing:

```text
msedge --headless=new --screenshot=<file> --window-size=1280,800 --hide-scrollbars <url>
```

with a throwaway `--user-data-dir`, so it never hands the job to a browser window you already have
open. It runs off the key handler and gives up after 30 seconds. The picture is saved under
`<data dir>/screenshots/<branch>-<time>.png` and opens in a file tab's image view, painted with
half-blocks, which every terminal shows. With `inline_graphics` set to `off` it opens in the OS image
viewer instead.

## Settings

| Key | Default | Owner | Meaning |
|---|---|---|---|
| `port_base` | `3000` | daemon | The first port of slot 0's block. |
| `port_block` | `10` | daemon | Ports per worktree. |
| `browsers` | `{}` | TUI | A profile per browser id (`chrome`, `edge`, `firefox`). |
| `projects.<repo>.browser` | `""` | TUI | The browser `w` last opened for the project (`default`, `chrome`, `edge`, `firefox`). Written by the chooser. |

Changing `port_base` or `port_block` moves every block at once, because a slot is stored, not a
port. Sessions already running keep the ports they started with until they restart.
