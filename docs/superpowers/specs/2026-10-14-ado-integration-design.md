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

- **Sign-in:** reuse the user's existing sign-in. Git Credential Manager first (`git credential fill` for `https://dev.azure.com/<org>`), then `az account get-access-token --resource 499b84ac-1321-427f-aa17-267ca6975798`. No PAT entry, no app registration. The token lives only in memory.
- **Org/project/repo detection:** read from the git remote. Accept `https://dev.azure.com/{org}/{project}/_git/{repo}`, `https://{org}.visualstudio.com/{project}/_git/{repo}` and `git@ssh.dev.azure.com:v3/{org}/{project}/{repo}`.
- **API:** REST api-version 7.1, via `reqwest` with rustls in the Tauri backend (`desktop/src-tauri/src/ado/`).
- **Diffs:** always local `git` (source against merge-base with target); ADO is used for metadata only.
- **Never hard-code a sprint period.** Iterations come from the team's settings at request time.
- **No attribution:**
  - Kete-created PR titles and descriptions are stripped of `Co-Authored-By:` lines and "Generated with" footers.
  - Sessions Kete launches get Claude Code's attribution disabled in the per-worktree settings it already writes (`includeCoAuthoredBy: false`, plus `attribution` set to empty strings for commit and PR).
  - None of this is surfaced in the UI or docs.
- **Out of scope:** environments (dev/UAT/prod), databases, CI pipelines beyond reading PR build-policy status, and GitHub.

## Identity and branch names

- Initials come from `git config user.name`: the first two letters of the first word plus the first two of the last word, lowercased (Jay Richardson → `jari`). They can be overridden as `ado.initials` in config.
- The new-branch default is `<initials>/<slug>`, where the slug is the task title, kebab-cased, at most 40 characters, editable before create. A config template `ado.branch_template` supports `{initials}`, `{slug}` and `{id}`; the default is `{initials}/{slug}`.
- When Kete creates a branch from a task, it records `(repo, branch) → work item id` in its database. The task for a branch is resolved, in order, from that record, from a PR's linked work items, and from a numeric id in the branch name.

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
  - **Set state** (to the next board column only).

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
  - **Title and description:** the title is prefilled from the task title. The description template has Summary, Changes, Testing and Task, and Summary is prefilled from the agent session's last summary when one exists.
  - **Task link:** required by default. An explicit "No task (n/a)" checkbox allows submission with a warning line; it never blocks.
  - **Reviewers:** an identity search picker (ADO identities API), with recent reviewers stored per repo and suggested first. At least one is required, with the same n/a escape.
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
