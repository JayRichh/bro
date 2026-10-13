# Kete desktop: Azure DevOps integration

Date: 2026-10-14. Status: awaiting review.

## Intent

Jay and colleagues at TnT author most of their work from the ADO sprint board. A task is picked, a branch `jari/<descriptor>` is made, an agent works it, and a draft PR goes up. PR policy requires a linked task and at least one reviewer. Kete's desktop app should run that loop without leaving the app, and make reviewing possible but secondary. ADO only; no GitHub parity work in this scope.

**Success:**
- From Kete you can do all of this without opening the ADO website:
  - see the current sprint's tasks and start one (branch + optional agent)
  - see every branch grouped by person, with sync state
  - draft a policy-compliant PR with task and reviewers
  - preview its diff
  - see reviewer votes and threads

## Constraints

- **Sign-in:** reuse the user's existing sign-in. Git Credential Manager first (`git credential fill` for the remote's own host and org URL, so `*.visualstudio.com` remotes resolve too; run off the UI thread with a 60 s timeout and a "Signing in…" state, since an expired GCM login may open its own sign-in window), then `az account get-access-token --resource 499b84ac-1321-427f-aa17-267ca6975798`. No PAT entry, no app registration. The token lives only in memory.
- **Org/project/repo detection:** read from the git remote. Accept `https://dev.azure.com/{org}/{project}/_git/{repo}`, `https://{org}.visualstudio.com/{project}/_git/{repo}` and `git@ssh.dev.azure.com:v3/{org}/{project}/{repo}`.
- **API:** REST api-version 7.1, via `reqwest` with rustls in the Tauri backend (`desktop/src-tauri/src/ado/`).
- **Diffs:** always local `git` (source against merge-base with target); ADO is used for metadata only.
- **Never hard-code a sprint period.** Iterations come from the team's settings at request time. The team defaults to the project's default team, switchable from a team picker, remembered per repo.
- **No attribution:**
  - Kete-created PR titles and descriptions are stripped of `Co-Authored-By:` lines and "Generated with" footers.
  - Sessions Kete launches get Claude Code's attribution disabled in the per-worktree settings it already writes (`includeCoAuthoredBy: false`, plus `attribution` set to empty strings for commit and PR).
  - Codex and Copilot get their equivalent setting where the harness has one.
  - Backstop: the PR composer scans the branch's commits for AI co-author trailers. On an unpushed or own-only branch it offers a one-click clean (message rewrite of those commits); once pushed it only notes it.
  - None of this is surfaced in docs; the composer note is the only UI.
- **Out of scope:** environments (dev/UAT/prod), databases, CI pipelines beyond reading PR build-policy status, and GitHub.

## Identity and branch names

- Initials come from `git config user.name`: the first two letters of the first word plus the first two of the last word, lowercased (Jay Richardson → `jari`). They can be overridden as `ado.initials` in config.
- The new-branch default is `<initials>/<slug>`, where the slug is the task title, kebab-cased, at most 40 characters, editable before create. A config template `ado.branch_template` supports `{initials}`, `{slug}` and `{id}`; the default is `{initials}/{slug}`.
- When Kete creates a branch from a task, it records `(repo, branch) → work item id` in a desktop-local store, `<data_dir>/ado.json` (the daemon's database is not the desktop's to write). The same file holds the offline cache, recent reviewers per repo, the chosen team and per-view UI state. The task for a branch is resolved, in order, from that record, from a PR's linked work items, and from a numeric id in the branch name.

## Views (desktop app, Workshop design)

A left rail gains three entries under the repo list: **Tasks**, **Branches** and **PRs**. Each view keeps its own selection, filters and scroll per repo across navigation. `Alt+←` and `Alt+→` move through a shared back/forward history. A breadcrumb (`repo › view › item`) sits atop each view. Task, branch, PR and session render as cross-linking chips everywhere they appear.

### Tasks
- **Scope:**
  - The default scope is the team's current iteration (`/work/teamsettings/iterations?$timeframe=current`), filtered to items assigned to me.
  - The scope switcher offers: current sprint, a chosen iteration, all my open work, or all open work in the sprint. The last one serves "taking tasks off the board".
  - When the current iteration's end date is past, a notice line says "Sprint 42 ended 3 days ago" and offers the next iteration.
- **List:** grouped by parent (User Story / PBI / Bug), with id, title, state, board column, assignee and remaining work. Fuzzy search across title and id.
- **Detail pane:** description (rendered HTML sanitised to text and markdown), acceptance criteria, linked branches and PRs, and sessions working it.
- **Actions:**
  - **Start:** branch name (prefilled), base (the default branch), and "spawn agent" with harness/profile and a prompt prefilled from title + description + acceptance criteria. It creates a worktree via the daemon.
  - **Open in ADO.**

### Branches
- **Tree:** person prefix (the part before `/`; branches without one go under "other"). The user's own initials are pinned first, then people alphabetically, collapsible.
- **Data:** one row per branch name, local and remote merged. Sync badge: `↑2 ↓1`, `local only`, `remote only`, `gone` (upstream deleted). Last commit (age, author) is shown, along with the linked task chip, PR chip with state, and a worktree/session chip if one is checked out.
- **Search:** fuzzy across all branches (typing `ga/` filters to a person).
- **Actions:**
  - Fetch (all remotes, with prune), check out into a worktree, diff against target (default branch), draft PR, and delete a local branch (with confirmation).
  - No remote deletes.

### PRs
- **Tabs:** *Mine* (created by me, active) and *To review* (I'm a reviewer, active). Rows show title, the source → target branches, draft/active, reviewer votes (approved / waiting / rejected) and comment threads. The policy checklist covers a linked task, at least one reviewer, and build status if a build policy exists.
- **Composer** (from a branch or task):
  - **Title and description:** the title is prefilled from the task title. The description template has Summary, Changes, Testing and Task, and Summary is prefilled from the commit subjects in the range, with an "Ask the session to summarise" button that uses `kete ask` against the branch's session when one exists.
  - **Task link:** required by default. An explicit "No task (n/a)" checkbox allows submission with a warning line; it never blocks.
  - **Reviewers:** an identity search picker (ADO IdentityPicker API, which needs no Graph scopes), with recent reviewers stored per repo and suggested first. At least one is required, with the same n/a escape.
  - **Draft and diff:** Draft is on by default. A diff preview of source against merge-base sits beside the form.
  - **Create:** creates the PR, then shows it in PRs › Mine with its checklist.
- **Review:** open any PR to see the diff (local fetch of both refs), threads (read, reply) and vote (approve / approve with suggestions / wait / reject).

### Diff (shared)
- **Viewer:** the Monaco diff with side-by-side, inline and replacement (new text only, with changes highlighted) modes, a file list with icons, and next/previous change.
- **Per-hunk actions:** for local working-tree diffs only, stage hunk / discard hunk (discard asks first).

## Overview tree: nested agents

On the home overview, a session's children nest beneath it:
- sessions spawned by it (via `kete spawn`, recorded as parent in the daemon)
- in-session subagents, reported by Claude's SubagentStart and SubagentStop hooks, shown as lighter rows with live status, and collapsed after they finish

Depth is unlimited, with collapse at depth > 2 by default.

## Errors

- **No token:** "Sign in to Azure DevOps" plus the exact fix (`git fetch` once, or `az login`).
- **403:** show which scope failed, e.g. "Can't read work items in TnT/Project".
- **Offline:** cached last results with an age stamp; local branches and diffs keep working.
- **Rate limiting:** respect `Retry-After`.
- **Failed writes:** keep the composer's state so nothing typed is lost.

## Testing

- **Rust unit tests** for:
  - remote URL parsing
  - initials and branch template
  - the task resolution order
  - attribution stripping
  - WIQL and iteration query building
  - an `ado` client test suite against a local mock HTTP server with recorded-shape JSON fixtures
- **Frontend:** component tests for the composer's policy logic (task / no-task / reviewers).
- **Live smoke with Jay against TnT:** read-only views first, then one real draft PR on a scratch branch, which is then abandoned.

## Audit resolutions (Codex review, 2026-10-14)

These decisions override anything above that conflicts.

**Auth**
- Call GCM as `git -c credential.azreposCredentialType=oauth credential fill`, and send `Authorization: Bearer`. If only a PAT comes back, send Basic `:token` and probe capability once per org. Read-only probes are `GET _apis/connectionData` and `GET _apis/wit/workitemtypes?$top=1`. The probe reports exactly which area (work items, identities, code) is not permitted.
- SSH remotes are normalised to the HTTPS org URL for the credential lookup.
- The `az` fallback passes `--tenant` from the remembered tenant (learned from the first successful token's `tid` claim).
- Conditional-access and interaction-required errors show "Sign in again" and run `az login --tenant <tid>` on click.
- Error copy distinguishes authentication (401, interaction required) from permission (403 on one area) from policy.

**Repository identity**
- **Remote:** `origin`, or the single ADO remote. A picker appears only when several ADO remotes exist.
- **Metadata:** `GET {org}/{project}/_apis/git/repositories/{repo}?api-version=7.1` gives the repo GUID and `defaultBranch`. `GET {org}/_apis/projects/{project}?api-version=7.1` gives `defaultTeam`.
- **Unsupported cases:**
  - non-ADO repos show no ADO views
  - forks and detached HEAD are shown as such, read-only
  - new branch names are validated with `git check-ref-format --branch`
  - name collisions and branches already checked out in a worktree are refused with the reason

**Tasks**
- **Current sprint:** `GET {team}/_apis/work/teamsettings/iterations?$timeframe=current`, then `GET {team}/_apis/work/teamsettings/iterations/{id}/workitems?api-version=7.1`, hydrated by `POST _apis/wit/workitemsbatch?api-version=7.1` in batches of 200.
- **Other iterations:** the previous and next ones come from the full iteration list (no timeframe).
- **All my open:** WIQL (`POST _apis/wit/wiql`) on `[System.AssignedTo] = @Me` and state category not Completed or Removed, using the team's area paths (`teamfieldvalues`, honouring `includeChildren`).
- **States:** open/closed is decided by state *category* (`_apis/wit/workitemtypes/{type}/states`), never by state name.
- **Missing fields:** empty or undated iterations, and missing fields, render as "none" rather than erroring. Acceptance criteria fall back to the parent's when the task has none.
- **Start:** assigns the task to me if it's unassigned, and moves it to the first InProgress-category state. Both are on by default, as toggles in the Start dialog.

**PR creation**
1. Push the source branch to the chosen remote, with `--set-upstream` if needed.
2. `POST _apis/git/repositories/{repoId}/pullrequests?api-version=7.1` with full `refs/heads/...` names, `isDraft`, `reviewers[].id` (ADO identity IDs) and `workItemRefs[].id` for the task link.
   - If linking fails after creation, keep the PR ID, show "Created, task not linked" with Retry, and never create twice.
   - An ambiguous create (timeout) is reconciled by listing PRs for that source branch before any retry.
3. **Publish:** `PATCH .../pullrequests/{id}` with `isDraft: false`. While a PR is a draft, voting is unavailable, and policy rows show "not evaluated (draft)".

**Policy and reviewers**
- **Policy is the server's:** `GET _apis/policy/evaluations?artifactId=vstfs:///CodeReview/CodeReviewId/{projectId}/{prId}&api-version=7.1-preview.1`, showing pending, approved, rejected and not-applicable separately.
- **The "No task (n/a)" and "No reviewer" escapes** only silence Kete's own pre-submit nudge. The server's verdict is shown as returned.
- **Reviewer search:** `GET https://vssps.dev.azure.com/{org}/_apis/identities?searchFilter=General&filterValue={q}&api-version=7.1`. Ambiguous matches show email to disambiguate.

**Review**
- **Vote:** `PUT .../pullrequests/{id}/reviewers/{myId}` with vote `10 / 5 / 0 / -5 / -10`.
- **Reply:** `POST .../pullrequests/{id}/threads/{threadId}/comments`.
- **Threads:** shown with their file and line context from the PR iteration.

**Diff**
- Comparisons are by commit SHA (merge-base..tip), with PR tips pinned when opened. Missing refs are fetched, including `refs/pull/{id}/merge`; shallow history is deepened.
- Missing blob content is an error state, never an empty side. Each side is capped at 2 MB, with a "too large" state beyond that.
- **Modes:** side-by-side, inline and replacement are all rendering. Per-hunk stage/discard applies only to working-tree diffs, uses `git apply --cached` / `git apply -R` on the hunk's patch, and is computed from `git diff` (index vs worktree) and `git diff --cached`.
- Monaco and its workers are bundled locally for the app's CSP.

**Store (`ado.json`)**
- Keyed by org/project/repo. Writes are atomic (temp file + rename), and the cache is capped (last 500 items per repo).

**Nested subagents (protocol)**
- `AgentSummary` gains `parent: Option<AgentId>`, filled for `kete spawn` children.
- A new `subagents: Vec<SubagentSummary { id, parent_agent, kind, description, started_at, ended_at, status }>` is carried in snapshots and updated by `SubagentChanged` deltas fed from the SubagentStart/SubagentStop hooks.
- When the parent session ends or the daemon restarts, open subagents are closed as "ended (unknown)".
- Protocol version bump (additive: the minimum compatible version stays).

**Attribution (hard rule, quiet)**
- **Claude:** per-worktree settings get `attribution: { commit: "", pr: "" }`, plus `includeCoAuthoredBy: false` for older versions. Managed settings can override this; if they do, Kete can't, and the gate below catches it.
- **Copilot CLI:** `includeCoAuthoredBy: false` in `.github/copilot/settings.local.json` in each Kete worktree, kept out of git via the worktree's `info/exclude`.
- **Codex:** no switch exists. Its session instructions ask for plain commit messages without co-author trailers.
- **Gate:** before Kete pushes or creates a PR, it scans the commits being published for AI footers and trailers (`Co-Authored-By:` naming Claude, Copilot, Codex or OpenAI, and "Generated with"). Human co-authors are kept.
  - If any are found in commits not yet on the remote, Kete rewrites only those messages (no content change), then continues.
  - If they're already on the remote, it shows a one-line note and does not rewrite.
