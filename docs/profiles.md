# Profiles

A PROFILE is a named account for one harness: a second Claude Max subscription, a work ChatGPT
login for Codex, another GitHub account for Copilot. A session launched under a profile gets that
account's config dir and nothing else, so two subscriptions of one harness run side by side.

## What a profile isolates

Each profile is an entry in `config.json`'s `profiles` list. Unless it adopts the CLI's existing
login (`use_default`), it owns a PROFILE HOME under `<data dir>/profiles/<name>/`
(`%LOCALAPPDATA%\kete\data\profiles\<name>\` on Windows) holding the harness's config:

| Harness | Dir in the home | Variable set on its sessions |
|---|---|---|
| Claude | `claude/` | `CLAUDE_CONFIG_DIR` |
| Codex | `codex/` | `CODEX_HOME` |
| Copilot | `copilot/`, `gh/` | `COPILOT_HOME`, `GH_CONFIG_DIR` |
| any other harness id | none | only the profile's `env` |

Every session under a profile also loses the inherited variables that would override its login,
whatever the daemon's environment says:

- **Claude:** `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN` (and its refresh and file-descriptor variants), and `CLAUDE_SECURESTORAGE_CONFIG_DIR`.
- **Codex:** `OPENAI_API_KEY` and `CODEX_API_KEY`.
- **Copilot:** `COPILOT_GITHUB_TOKEN`, `GH_TOKEN` and `GITHUB_TOKEN`.

The profile's own `env` map is applied last, so an API-billed account can set its key back on
purpose. On macOS and Linux the edit is repeated on the launch line after the login shell's rc files
run, so an `export ANTHROPIC_API_KEY=…` in `~/.zshrc` cannot win the login back.

A fresh Claude home is seeded the way agent-office seeds its accounts: `.claude.json` gets
`hasCompletedOnboarding`. Each spawn also copies over folder trust (`hasTrustDialogAccepted`) for the
worktree and the folders above it that your default config already trusts. The first session under
a new profile therefore goes straight to Claude's login.

Copilot keeps its OAuth token in the OS credential store, so a separate `COPILOT_HOME` gives a
Copilot profile its own settings, sessions and current-account choice. That two Copilot logins stay
fully apart through it has not been verified; `copilot login` under each profile is the way to find
out.

## Commands

```sh
kete profile add main --harness claude --use-default --default   # adopt today's login
kete profile add max2 --harness claude                            # a second subscription
kete profile login max2                                           # sign it in, here
kete profile list
kete profile default max2                                         # launches with no pick take it
kete profile disable max2                                         # never picked unless named
kete profile enable max2
kete profile duplicate max2 max3                                  # same harness and variables, its own login
kete profile rename max3 work                                     # its home and project defaults follow
kete profile remove max2 [--purge]                                # --purge deletes its home too
kete profile spread failover --threshold 80                       # which account an unpicked launch takes
kete session move-account fix-auth work                           # restart a session fresh on another account
```

- **`add`:** `--use-default` points the profile at the CLI's own default config (`~/.claude`, `~/.codex`, `~/.copilot`). It moves no dir and copies nothing, so the login you already have becomes one of the profiles the picker cycles through. `--default` makes the profile what a launch takes when it names none; there is one default per harness. `--env KEY=VALUE` (repeatable) fills its `env`.
- **`login`:** runs the harness's own sign-in in the current terminal with the profile's environment applied: `claude auth login --claudeai`, `codex login` or `copilot login`. The CLI opens your browser for the OAuth flow and takes the callback, or prints the URL and asks for the code. The credentials it writes land in the profile's home. Anything after `--` is passed on: `kete profile login max2 -- --email you@example.com`.
- **`list`:** every account (each profile, then each harness's own login that has a reading, e.g. `claude (own login)`): harness, `*` for the default and `(off)` for a disabled one, a bar and percent for its session (5h) and weekly use (Copilot: its monthly allowances), and whether it is signed in. One that is not shows the `kete profile login <name>` to run. The account spread in force is printed last.
- **`enable` / `disable`:** a disabled profile is never picked for a launch that names no profile (not as the default, not by a spread) and pickers leave it out. Sessions already under it keep it, and a launch that names it still runs.
- **`remove`:** drops the entry and keeps the home, so adding the name back signs nothing in again. `--purge` also deletes the home, and only ever a directory directly inside `<data dir>/profiles`. An adopted login's config is never touched.
- **`duplicate`:** a new profile with the source's harness, variables and kind of login, but not its default star or its off switch, in an empty home of its own: sign it in separately.
- **`rename`:** moves the profile's home to the new name, renames the entry in place (its other fields kept), points every project default naming it at the new name, and carries its kept usage reading over. Refused while a session names the profile, running or stopped (a stopped one would fail to resume: only the daemon can change the name a session stores): move those to another account or archive them first. Archived sessions that still name it are listed after the rename. Refused too when a directory of the new name already sits in `<data dir>/profiles` (a home a remove left behind).
- **`spread`:** sets the [account spread](#account-spread): `manual`, `failover` or `least-used`; `--threshold` takes one of 50, 70, 80, 90, 95.
- **`kete session move-account <session> <profile>`** (or `--own-login`): restarts a session fresh on another account of its harness, as the apps' **Move to account** does. The session is its id, its name, or part of one name; two matches are refused. Its conversation stays with the old account.

## In the apps

The desktop window's **Accounts** view (`Shift+U`, or Settings → Accounts → **Manage accounts**) and the TUI's Accounts overlay (`⇧U`) do everything the commands above do: add (with variables), edit, duplicate, rename, switch on or off, make default, use for a repo, sign in, move a session onto it, set the spread, and remove, with a choice to delete the home too. Both run the same `nebula_core::profiles` functions as the CLI. A rename is offered greyed, its tooltip naming the sessions in the way, while a session names the account. Editing keeps the variables unless they are replaced; **Clear** (desktop) or `Del` (TUI) drops them all.

## In the TUI

The QUICK PROMPT's details row gains a field while the box's harness has profiles:
`profile max2 ^A`. `^A`, or a click on the field, steps the launch to the next of that harness's
profiles in place, leaving the typed task as it is. The box opens on the harness's default profile.
When the harness has no default, the steps start at `none`, which is the daemon's own environment as
before profiles. Picking another harness with `Tab` starts over on that harness's default. Picking a
model with `^O` keeps the profile.

A card names the profile its session runs on when it is not the harness's default: `claude@max2 opus`.

## Sessions keep their profile

The daemon resolves a create's profile when the session is created: the one picked, else the
harness's default. It stores the profile with the row, so a resume, a restart or a move to another
worktree boots under the same account. Changing the default later never moves a running session to
another login. A session whose profile has since been removed, or now names another harness, refuses
to start and says why. `kete spawn` from inside a session runs its sibling on the caller's profile when
it runs the caller's harness.

Things that follow the profile:

- **WARM SPARES:** a spare boots on its harness's default profile, and only a create on that same account adopts it.
- **Hooks:** Codex and Copilot keep their hooks in their home, so a session under a second Codex or Copilot profile gets kete's hooks installed into that profile's `CODEX_HOME` / `COPILOT_HOME`. Claude's hooks are per worktree (`.claude/settings.local.json`) and need nothing per profile.
- **Transcripts:** Claude transcripts are looked for in every Claude profile's `projects/` dir, so a resume after a daemon restart finds them. A backgrounded session is listed through `claude agents` under the session's own profile.
- **`kete sessions --json`:** carries each agent's `profile`.

## Usage and context

Kete keeps each ACCOUNT's plan usage and each session's context window, and shows them in the
desktop window's Accounts list and session rows, the TUI's quick prompt, Accounts overlay and
cards, and `kete profile list`. An account is a profile, or a harness's own login when a session
runs under none (kept as `@claude`, `@codex`, …). Unknown numbers read `—`; nothing here ever
holds up a launch.

| Harness | Plan usage | Context |
|---|---|---|
| Claude | its statusLine JSON: `rate_limits.five_hour` / `seven_day` (Pro and Max plans, after the first reply) | the statusLine's `context_window`; compactions from the transcript's `compact_boundary` lines; the transcript's last reply as a fallback |
| Codex | the rollout's `token_count` events: primary (5h) and secondary (weekly) limits | the last request's input over `model_context_window`; `compacted` lines |
| Copilot | `gh api copilot_internal/user` under the profile's `GH_CONFIG_DIR`, every 15 minutes: premium requests and chat, monthly. A plan without an allowance reads `—`, not 100% | the session's `events.jsonl` `session.usage_record` |

Claude's numbers come through a STATUS LINE kete passes to each Claude session it starts, as
`--settings <data dir>/claude-statusline.json` (a file kete owns and rewrites at every daemon start,
so it always names the current `kete` binary). Nothing is written into your repositories, and a
plain `claude` run outside kete keeps Claude's own footer. The status line (`kete _statusline`)
hands Claude's JSON to the daemon, then runs the statusLine Claude would have drawn without it (the
project's `.claude/settings.local.json`, then `.claude/settings.json`, then your user settings) and
prints its output. When you have none, or yours fails or runs past 5 seconds, it prints a compact
`ctx 34% · 5h 21% · wk 3%`. Settings → Sessions → **Usage status line** turns it off for sessions
started after the change. Claude's on-screen notice (`You've used 91% of your weekly limit`) is
still read as a last source.

A window with a reset time counts as empty once that time passes. One without (the on-screen notice,
a reading an older kete kept) counts as empty once its nominal length (5 hours, 7 days, 30 days) has
passed since it was read, and the apps draw a reading dim once it is older than a few hours (session)
or its window (weekly, monthly).

The latest reading per account is kept in `<data dir>/profiles/.usage/<account>.json`; older kete
builds still read its `percent` and `window`.

## Account spread

Settings → Sessions → Accounts → **Account spread** (`account_spread`) decides the profile of a
launch that names none, from the desktop window, the TUI, `kete spawn` and the orchestrator alike:

- **manual** (the default): the harness's default profile, else none. One Claude subscription marked default is all most setups need.
- **failover**: the default until its 5h or weekly use (only those two windows; a model weekly or monthly allowance never counts) reaches **Failover at** (`account_spread_threshold`, 90% unless set), then the next enabled profile of that harness under it; when every one is over, the least used.
- **least_used**: the enabled profile with the least use. An account with no reading yet counts as unused.

A window past its reset time counts as empty. The apps say which account a launch took and why
(`claude: max2 (failover: main at 92%)`).

## Configuration

```json
{
  "profiles": [
    { "name": "main", "harness": "claude", "use_default": true, "default": true },
    { "name": "max2", "harness": "claude" },
    { "name": "api", "harness": "claude", "env": { "ANTHROPIC_API_KEY": "sk-ant-api03-…" } }
  ]
}
```

| Field | Meaning |
|---|---|
| `name` | Letters, digits, `-` and `_`; unique across harnesses. Names the home dir and what a session stores. |
| `harness` | The harness id it is an account for. |
| `env` | Extra variables for its sessions, applied after the strip. |
| `default` | The profile a launch on this harness takes when it names none. |
| `use_default` | Adopt the CLI's default login instead of a home of its own. |
| `disabled` | Never picked for a launch that names no profile. |
