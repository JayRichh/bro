# Profiles

A PROFILE is a named account for one harness: a second Claude Max subscription, a work ChatGPT
login for Codex, another GitHub account for Copilot. A session launched under a profile gets that
account's config dir and nothing else, so two subscriptions of one harness run side by side.

## What a profile isolates

Each profile is an entry in `config.json`'s `profiles` list. Unless it adopts the CLI's existing
login (`use_default`), it owns a PROFILE HOME under `<data dir>/profiles/<name>/`
(`%LOCALAPPDATA%\bro\data\profiles\<name>\` on Windows) holding the harness's config:

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
kete profile remove max2 [--purge]                                # --purge deletes its home too
```

- **`add`:** `--use-default` points the profile at the CLI's own default config (`~/.claude`, `~/.codex`, `~/.copilot`). It moves no dir and copies nothing, so the login you already have becomes one of the profiles the picker cycles through. `--default` makes the profile what a launch takes when it names none; there is one default per harness. `--env KEY=VALUE` (repeatable) fills its `env`.
- **`login`:** runs the harness's own sign-in in the current terminal with the profile's environment applied: `claude auth login --claudeai`, `codex login` or `copilot login`. The CLI opens your browser for the OAuth flow and takes the callback, or prints the URL and asks for the code. The credentials it writes land in the profile's home. Anything after `--` is passed on: `kete profile login max2 -- --email you@example.com`.
- **`list`:** each profile's harness, whether it is the default, its home, and the last PLAN USAGE seen (below).
- **`remove`:** drops the entry and keeps the home, so adding the name back signs nothing in again. `--purge` also deletes the home, and only ever a directory directly inside `<data dir>/profiles`. An adopted login's config is never touched.

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

## Plan usage

Claude prints its plan usage in the session, e.g. `You've used 91% of your weekly limit · resets 2pm`.
The daemon reads that line off every Claude session that runs under a profile, with the same kind of
output scanner it uses for the Claude Cloud id. It keeps the latest reading per profile in
`<data dir>/profiles/.usage/<name>.json`. The QUICK PROMPT's profile field shows it beside the name
(`max2 91%`), and `kete profile list` shows it in full. A session with no profile has nowhere to file
a reading, so none is kept.

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
