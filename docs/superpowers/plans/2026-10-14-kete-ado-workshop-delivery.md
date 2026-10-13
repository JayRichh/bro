# Kete: ADO integration, Workshop redesign and delivery: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kete's desktop app runs the TnT Azure DevOps loop:
1. Take a task from the sprint.
2. Branch and spawn an agent.
3. Draft a policy-compliant PR, review the diff and publish.

All of it is in a warm Workshop UI with a nested-agent overview, shipped as an installable release.

**Architecture:**
- **ADO:** lives in the Tauri backend (`desktop/src-tauri/src/ado/`). REST is called directly with `reqwest`. Tokens come from Git Credential Manager (or `az`). Diffs always come from local `git`.
- **Daemon:** gains parent links and subagent rows (an additive protocol change) so the overview can draw trees.
- **UI:** Svelte 5 + Tailwind v4 reading one token file, `desktop/src/theme.css`.

**Tech Stack:** Rust 2021, Tauri v2, reqwest (rustls), httpmock (tests), Svelte 5, Tailwind v4, Monaco, xterm.js, Geist / Geist Mono / JetBrains Mono.

**Spec:**
- `docs/superpowers/specs/2026-10-14-ado-integration-design.md`. Its "Audit resolutions" section overrides the rest.
- The visual brief is `docs/design/kete-desktop-brief.md`.

## Global Constraints

- The rebrand to Kete lands before Task 1. All paths, binary names and copy say `kete`, with `bro` as an alias binary. If the rebrand has not merged when you start, stop and report.
- ADO REST `api-version=7.1`, except policy evaluations (`7.1-preview.1`).
- The ADO resource for `az`: `499b84ac-1321-427f-aa17-267ca6975798`.
- GCM call: `git -c credential.azreposCredentialType=oauth credential fill`, 60 s timeout, off the UI thread.
- Tokens are held in memory only, never written to disk or logs.
- `ado.json` lives at `<kete data dir>/ado.json`. Writes are atomic (temp + rename); the cache keeps at most 500 items per repo.
- The branch template defaults to `{initials}/{slug}`. Slugs are kebab-case, ≤40 characters, and validated with `git check-ref-format --branch`.
- Initials: first 2 letters of the first word + first 2 of the last word of `git config user.name`, lowercase. Override: `ado.initials`.
- Diff side cap: 2 MB. A missing blob is an error state, never empty.
- **No attribution, anywhere:**
  - no `Co-Authored-By` naming an AI, and no "Generated with" footers
  - no attribution trailers in this project's own commits either
  - no user-facing copy that mentions the rule
- No em dashes in any user-facing string.
- Zero border radius, no shadows, no gradients. Colours and fonts only via `theme.css` tokens. Status is never colour alone: square + word.
- **Builds:** `CARGO_BUILD_JOBS=4`, one cargo/npm build at a time (the machine has a failing CPU core).
- **Root gates:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Linux clippy via Docker:

```
docker run --rm -v C:\Code\bro:/src -v bro-cargo-reg:/usr/local/cargo/registry -v kete-target-linux:/target -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=4 -w /src rust:latest bash /src/target/linux-check.sh
```

- **Desktop gates:** `cd desktop; npm run check; npm run build; cargo test --manifest-path src-tauri/Cargo.toml`.
- **Commits:** `git -c core.autocrlf=false commit`, plain messages, no trailers.
- **Quota:** no real Claude agent turns in tests.

## Review Focus

1. **A repo with a GitHub remote and no ADO remote:** ADO views stay hidden and nothing errors. Test in Task 1 (`parse_remote` returns `None`) and Task 13 (the rail omits the views).
2. **GCM returns nothing within 60 s, or hangs on a sign-in window:** the UI shows "Signing in…", then the fix-it message, and the app never freezes. Test in Task 2 (`TokenSource` times out and returns `AuthError::Timeout`).
3. **PR create times out after the server already created it:** the retry finds the existing PR and does not create a second. Test in Task 9 (`create_pr` reconciles by listing PRs for the source branch).
4. **Branch name typed with spaces or `..`, or an existing branch name:** it is refused with the reason before any git call. Test in Task 1 (`validate_branch_name`) and Task 10.
5. **Commit range contains `Co-Authored-By: Claude …` and a human co-author:** only the AI trailer is removed, and only on unpushed commits. Test in Task 5.

---

## File map

```
desktop/src-tauri/src/
  ado/mod.rs            AdoContext: remote, org/project/repo ids, client; Tauri command glue
  ado/remote.rs         parse_remote(url) -> Option<AdoRemote>
  ado/naming.rs         initials(), slug(), branch_name(), validate_branch_name()
  ado/auth.rs           TokenSource (GCM -> az), AuthError, tenant memory
  ado/client.rs         AdoClient: get/post/patch/put with retry, error classes
  ado/store.rs          AdoStore (ado.json): links, cache, reviewers, team, ui state
  ado/tasks.rs          iterations, iteration items, batch hydrate, WIQL, state categories
  ado/branches.rs       local+remote merge, sync state, grouping, task resolution
  ado/prs.rs            list, reviewers, policy evaluations, threads, create/publish/vote/reply, identities
  attribution.rs        trailer scan + rewrite of unpushed commits
  git.rs                (modify) SHA diffs, caps, errors, hunks
  main.rs               (modify) register commands
desktop/src/
  theme.css             Workshop tokens (light/dark/system)
  lib/shell/Rail.svelte, Breadcrumb.svelte, history.svelte.ts
  lib/overview/Overview.svelte, RepoRow.svelte, SessionTree.svelte, AttentionStrip.svelte
  lib/ado/Tasks.svelte, Branches.svelte, Prs.svelte, Composer.svelte, Review.svelte, Chip.svelte, api.ts
  lib/Diff.svelte       (modify) modes + hunks
crates/nebula-core/src/protocol.rs        (modify) parent, SubagentSummary, SubagentChanged
crates/nebula-daemon/src/hooks/*.rs       (modify) subagent tracking, attribution settings
crates/nebula-daemon/src/registry/spawn.rs (modify) record parent
crates/nebula-tui/src/theme.rs            (modify) kete-dark, kete-light
crates/nebula/src/setup.rs                (modify) Kete Workshop WT scheme
desktop/src-tauri/tauri.conf.json         (modify) NSIS bundle
.github/workflows/release.yml (+ overlay) (modify) desktop installer asset
```

---

### Task 1: Remote parsing and branch naming

**Files:**
- Create: `desktop/src-tauri/src/ado/remote.rs`, `desktop/src-tauri/src/ado/naming.rs`, `desktop/src-tauri/src/ado/mod.rs`
- Modify: `desktop/src-tauri/src/main.rs` (add `mod ado;`)

**Interfaces:**
- Produces:
  - `pub struct AdoRemote { pub host: String, pub org: String, pub project: String, pub repo: String, pub org_url: String }`. Here `org_url` is the HTTPS base: `https://dev.azure.com/{org}` or `https://{org}.visualstudio.com`.
  - `pub fn parse_remote(url: &str) -> Option<AdoRemote>`
  - `pub fn initials(user_name: &str) -> Option<String>`
  - `pub fn slug(title: &str) -> String`
  - `pub fn branch_name(template: &str, initials: &str, slug: &str, id: Option<u32>) -> String`
  - `pub fn validate_branch_name(name: &str, existing: &[String]) -> Result<(), String>`

- [ ] **Step 1: Write failing tests** in `remote.rs`/`naming.rs` `#[cfg(test)]`:

```rust
#[test] fn parses_three_remote_shapes() {
    let a = parse_remote("https://dev.azure.com/tnt/Platform/_git/api").unwrap();
    assert_eq!((a.org.as_str(), a.project.as_str(), a.repo.as_str()), ("tnt", "Platform", "api"));
    assert_eq!(a.org_url, "https://dev.azure.com/tnt");
    let b = parse_remote("https://tnt.visualstudio.com/Platform/_git/api").unwrap();
    assert_eq!(b.org_url, "https://tnt.visualstudio.com");
    let c = parse_remote("git@ssh.dev.azure.com:v3/tnt/Platform/api").unwrap();
    assert_eq!(c.org_url, "https://dev.azure.com/tnt");
    let d = parse_remote("https://jay@dev.azure.com/tnt/My%20Project/_git/web").unwrap();
    assert_eq!(d.project, "My Project");
}
#[test] fn non_ado_remotes_are_none() {
    assert!(parse_remote("https://github.com/JayRichh/kete.git").is_none());
    assert!(parse_remote("").is_none());
}
#[test] fn initials_from_names() {
    assert_eq!(initials("Jay Richardson").as_deref(), Some("jari"));
    assert_eq!(initials("Ana María López").as_deref(), Some("anlo"));
    assert_eq!(initials("Cher").as_deref(), Some("ch"));
    assert_eq!(initials("  ").as_deref(), None);
}
#[test] fn slug_and_template() {
    assert_eq!(slug("Fix GST rounding on cart totals!"), "fix-gst-rounding-on-cart-totals");
    assert!(slug(&"word ".repeat(30)).len() <= 40);
    assert_eq!(branch_name("{initials}/{slug}", "jari", "gst-rounding", Some(4211)), "jari/gst-rounding");
    assert_eq!(branch_name("{initials}/{id}-{slug}", "jari", "gst", Some(4211)), "jari/4211-gst");
}
#[test] fn branch_validation() {
    assert!(validate_branch_name("jari/ok-name", &[]).is_ok());
    assert!(validate_branch_name("jari/has space", &[]).is_err());
    assert!(validate_branch_name("jari/a..b", &[]).is_err());
    assert!(validate_branch_name("jari/x", &["jari/x".into()]).unwrap_err().contains("exists"));
}
```

- [ ] **Step 2:** Run `cargo test --manifest-path desktop/src-tauri/Cargo.toml ado::` and expect FAIL (unresolved).
- [ ] **Step 3: Implement.**
  - `parse_remote`: strip the userinfo, percent-decode path segments, and match the three shapes.
  - `initials`: split on whitespace, take the first and last words, use the first two chars of each (Unicode-aware, lowercased, ASCII-folded where trivial).
  - `slug`: lowercase, collapse non-alphanumeric runs to `-`, trim `-`, cut to 40 chars at a `-` boundary when possible.
  - `validate_branch_name`: reject when existing contains it ("branch exists"). Otherwise apply the `git check-ref-format` rules in pure Rust: no space, `..`, `~^:?*[\`, `@{`, leading `-` or `/`, trailing `/` or `.lock`, or empty component.
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: parse ADO remotes and name branches by initials`.

### Task 2: Token source (GCM, then az)

**Files:**
- Create: `desktop/src-tauri/src/ado/auth.rs`
- Modify: `desktop/src-tauri/Cargo.toml` (add `base64` (already present), `async-trait = "0.1"`)

**Interfaces:**
- Consumes: `AdoRemote.org_url`.
- Produces:
  - `pub enum Credential { Bearer(String), Basic(String) }` with `fn header(&self) -> String`
  - `pub enum AuthError { NoCredential, Timeout, InteractionRequired(String), Other(String) }`
  - `#[async_trait] pub trait Runner: Send + Sync { async fn run(&self, program: &str, args: &[&str], stdin: Option<&str>, timeout: Duration) -> Result<String, AuthError>; }`
  - `pub struct TokenSource { runner: Arc<dyn Runner>, tenant: Mutex<Option<String>> }`
  - `pub async fn token(&self, org_url: &str) -> Result<Credential, AuthError>`
  - `pub fn tenant_from_jwt(token: &str) -> Option<String>`

- [ ] **Step 1: Failing tests** with a fake `Runner` that returns canned stdout per program:

```rust
#[tokio::test] async fn gcm_oauth_token_becomes_bearer() {
    let r = Fake::new().on("git", "protocol=https\nhost=dev.azure.com\nusername=jay\npassword=eyJhbGciOi.eyJ0aWQiOiJ0LTEifQ.sig\n");
    let t = TokenSource::new(r).token("https://dev.azure.com/tnt").await.unwrap();
    assert!(matches!(t, Credential::Bearer(_)));
}
#[tokio::test] async fn non_jwt_password_is_basic() {
    let r = Fake::new().on("git", "password=abc123pat\n");
    assert!(matches!(TokenSource::new(r).token("https://dev.azure.com/tnt").await.unwrap(), Credential::Basic(_)));
}
#[tokio::test] async fn falls_back_to_az_with_remembered_tenant() {
    let r = Fake::new().fail("git", AuthError::NoCredential).on("az", "{\"accessToken\":\"eyJ.eyJ0aWQiOiJ0LTkifQ.s\"}");
    let src = TokenSource::new(r.clone());
    *src.tenant.lock().unwrap() = Some("t-9".into());
    src.token("https://dev.azure.com/tnt").await.unwrap();
    assert!(r.last_args("az").contains(&"--tenant".to_string()));
}
#[tokio::test] async fn gcm_timeout_surfaces() {
    let r = Fake::new().fail("git", AuthError::Timeout).fail("az", AuthError::NoCredential);
    assert!(matches!(TokenSource::new(r).token("https://dev.azure.com/tnt").await, Err(AuthError::Timeout)));
}
#[test] fn tenant_claim() { assert_eq!(tenant_from_jwt("x.eyJ0aWQiOiJ0LTEifQ.y").as_deref(), Some("t-1")); }
```

- [ ] **Step 2:** Run the tests and expect FAIL.
- [ ] **Step 3: Implement.**
  - **GCM:** `git -c credential.azreposCredentialType=oauth credential fill`. The stdin is `protocol=https\nhost=<host>\npath=<org path>\n\n`. Parse the `password=` line. Three dot-separated base64url parts mean Bearer, else Basic (`base64(":" + pat)`). Remember the `tid` claim as tenant.
  - **az:** `az account get-access-token --resource 499b84ac-1321-427f-aa17-267ca6975798 --output json [--tenant <tid>]`, read `accessToken`. If stderr contains `AADSTS50076`, `interaction_required` or `az login`, return `InteractionRequired(stderr line)`.
  - **Real `Runner`:** `tokio::process::Command` with `CREATE_NO_WINDOW` on Windows and a `tokio::time::timeout`.
  - Precedence when both fail: report the GCM error unless it was `NoCredential`, in which case report the az one.
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: ADO tokens from Git Credential Manager, then az`.

### Task 3: ADO HTTP client

**Files:**
- Create: `desktop/src-tauri/src/ado/client.rs`
- Modify: `desktop/src-tauri/Cargo.toml`:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
[dev-dependencies]
httpmock = "0.7"
```

**Interfaces:**
- Consumes: `TokenSource::token`, `Credential::header`.
- Produces:
  - `pub struct AdoClient { http: reqwest::Client, tokens: Arc<TokenSource>, org_url: String }`
  - `pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, AdoError>`, plus `post_json`, `patch_json` and `put_json` with the same shape and a `body: &serde_json::Value`
  - `pub enum AdoError { Auth(AuthError), Unauthorized, Forbidden { area: String }, NotFound, Conflict(String), Http(u16, String), Network(String) }` with `fn user_message(&self) -> String`
  - `fn area_of(path: &str) -> &'static str` maps `/wit/` to "work items", `/git/` to "code", `/identities` to "identities", `/policy/` to "policies", `/work/` to "boards"
- Path rules:
  - A `path` starting with `https://` is used as is (vssps).
  - Otherwise it is joined onto `org_url`.
  - `api-version=7.1` is appended unless the path already has `api-version`.

- [ ] **Step 1: Failing tests** with `httpmock::MockServer`:

```rust
#[tokio::test] async fn appends_api_version_and_bearer() {
    let s = MockServer::start();
    let m = s.mock(|w, t| { w.path("/p/_apis/x").query_param("api-version", "7.1").header("authorization", "Bearer tok"); t.status(200).json_body(json!({"ok":1})); });
    let c = test_client(&s, Credential::Bearer("tok".into()));
    let v: serde_json::Value = c.get("/p/_apis/x").await.unwrap();
    m.assert(); assert_eq!(v["ok"], 1);
}
#[tokio::test] async fn retry_after_is_honoured_once() { /* 429 with Retry-After: 0, then 200 -> Ok, two hits */ }
#[tokio::test] async fn forbidden_names_the_area() {
    // 403 on /p/_apis/wit/workitems -> Forbidden { area: "work items" }, user_message contains "work items"
}
#[tokio::test] async fn unauthorized_is_distinct() { /* 401 -> AdoError::Unauthorized */ }
```

- [ ] **Step 2:** Run and expect FAIL.
- [ ] **Step 3: Implement.** Use `reqwest` with a 30 s timeout. On 429 or 503 with `Retry-After`, sleep that long (capped at 30 s) and retry once. Map the status codes to the variants.
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: ADO REST client with error classes and Retry-After`.

### Task 4: `ado.json` store

**Files:**
- Create: `desktop/src-tauri/src/ado/store.rs`

**Interfaces:**
- Produces:
  - `pub struct RepoKey { pub org: String, pub project: String, pub repo: String }` (Display: `org/project/repo`)
  - `pub struct AdoStore { path: PathBuf, data: Mutex<StoreData> }`
  - `#[derive(Serialize, Deserialize, Default)] pub struct StoreData { pub repos: BTreeMap<String, RepoState> }`
  - `pub struct RepoState`, with fields:
    - `pub branch_tasks: BTreeMap<String, u32>`
    - `pub recent_reviewers: Vec<Reviewer>`
    - `pub team: Option<String>`
    - `pub tenant: Option<String>`
    - `pub cache: BTreeMap<String, CacheEntry>`
    - `pub ui: serde_json::Value`
  - `pub struct Reviewer { pub id: String, pub name: String, pub email: String }`
  - `pub struct CacheEntry { pub at: i64, pub value: serde_json::Value }`
  - Methods:
    - `open(path) -> Self`
    - `with_repo<R>(&self, key: &RepoKey, f: impl FnOnce(&mut RepoState) -> R) -> R` (persists atomically after `f`)
    - `remember_reviewer(&self, key, r)` (dedupe, newest first, max 12)
    - `cache_put(&self, key, name, value)` (max 500 entries per repo, oldest evicted)

- [ ] **Step 1: Failing tests:** round-trip through a temp file; atomic write leaves no `.tmp`; reviewer dedupe and cap; cache cap of 500 evicts the oldest; a corrupt file opens as empty and is backed up to `ado.json.bad`.
- [ ] **Step 2:** Expect FAIL.
- [ ] **Step 3:** Implement with `serde_json`. Write to `ado.json.tmp`, then `std::fs::rename` (on Windows, rename over an existing file via `std::fs::rename`, which replaces it).
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: ado.json store with atomic writes and caps`.

### Task 5: Attribution scan and rewrite

**Files:**
- Create: `desktop/src-tauri/src/attribution.rs`
- Modify: `crates/nebula-daemon/src/hooks/installer.rs` (Claude settings), plus the worktree setup code that writes per-worktree files (find it via `grep -rn "settings.local.json" crates/nebula-daemon/src`)

**Interfaces:**
- Produces:
  - `pub fn strip_ai_trailers(message: &str) -> (String, bool)`, which returns the cleaned message and whether anything changed
  - `pub fn find_unpushed_ai_commits(repo: &Path, range: &str) -> anyhow::Result<Vec<String>>`, the SHAs whose messages change under `strip_ai_trailers`
  - `pub fn rewrite_unpushed(repo: &Path, base: &str, branch: &str) -> anyhow::Result<usize>`
- Removal rule:
  - It removes lines matching, case-insensitively, `^co-authored-by:.*(claude|anthropic|copilot|codex|openai|chatgpt|gpt)`, and lines starting `🤖 Generated with` or `Generated with [Claude`.
  - Then it trims the trailing blank lines left behind.
  - Human `Co-Authored-By` lines are kept.

- [ ] **Step 1: Failing tests:**

```rust
#[test] fn strips_ai_keeps_humans() {
    let m = "Fix rounding\n\nCo-Authored-By: Claude Opus <noreply@anthropic.com>\nCo-authored-by: Sam Lee <sam@tnt.co.nz>\n";
    let (out, changed) = strip_ai_trailers(m);
    assert!(changed);
    assert!(!out.contains("Claude")); assert!(out.contains("Sam Lee"));
}
#[test] fn strips_generated_footer() {
    let (out, changed) = strip_ai_trailers("Add x\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n");
    assert!(changed); assert_eq!(out.trim_end(), "Add x");
}
#[test] fn untouched_message_unchanged() { assert!(!strip_ai_trailers("Plain\n").1); }
#[test] fn rewrite_only_touches_unpushed() {
    // temp repo: base commit pushed to a bare remote, then two local commits, one with an AI trailer;
    // rewrite_unpushed returns 1; the remote-tracking commit SHA is unchanged; file contents unchanged.
}
```

- [ ] **Step 2:** Expect FAIL.
- [ ] **Step 3: Implement.**
  - `rewrite_unpushed`:
    1. Compute `git rev-list --reverse <upstream-or-base>..<branch>`.
    2. If none needs changes, return 0.
    3. Otherwise check the worktree is clean and run `git -c core.hooksPath=/dev/null filter-branch --msg-filter` replaced by a pure-Rust loop: for each commit in order, run `git commit-tree <tree> -p <newparent> -F <msgfile>`, preserving author and committer env (`GIT_AUTHOR_NAME/EMAIL/DATE`, `GIT_COMMITTER_*`).
    4. Run `git update-ref refs/heads/<branch> <newtip> <oldtip>`.
    5. Refuse when the branch is checked out with uncommitted changes.
  - Daemon side: in the per-worktree Claude settings writer, add `"includeCoAuthoredBy": false` and `"attribution": {"commit": "", "pr": ""}`. Write `.github/copilot/settings.local.json` `{"includeCoAuthoredBy": false}` and add `.github/copilot/settings.local.json` to `<git-common-dir>/info/exclude` once. Add a daemon unit test asserting both files' JSON.
  - In the Codex session instruction text the daemon already injects (grep `instructions` in `crates/nebula-core/src/harness.rs`), append: `Write plain commit messages without co-author trailers.`
- [ ] **Step 4:** Desktop and daemon tests pass. Run the root gates.
- [ ] **Step 5:** Commit `Keep agent commits free of AI co-author trailers`.

### Task 6: Tasks data

**Files:**
- Create: `desktop/src-tauri/src/ado/tasks.rs`
- Modify: `desktop/src-tauri/src/ado/mod.rs`, `desktop/src-tauri/src/main.rs` (commands)

**Interfaces:**
- Consumes: `AdoClient`, `AdoStore`, `RepoKey`.
- Produces (serde camelCase for the frontend):
  - `pub enum TaskScope { CurrentSprint, Iteration(String), MyOpen, SprintAll }`
  - `pub struct Iteration { pub id: String, pub name: String, pub path: String, pub start: Option<String>, pub finish: Option<String>, pub timeframe: Option<String> }`
  - `pub struct WorkItem`, with fields:
    - `pub id: u32`
    - `pub kind: String`
    - `pub title: String`
    - `pub state: String`
    - `pub state_category: String`
    - `pub assigned_to: Option<String>`
    - `pub remaining: Option<f64>`
    - `pub parent: Option<u32>`
    - `pub board_column: Option<String>`
    - `pub description_md: String`
    - `pub acceptance_md: String`
    - `pub url: String`
  - `pub struct TaskList { pub iteration: Option<Iteration>, pub stale: bool, pub next: Option<Iteration>, pub groups: Vec<TaskGroup> }`
  - `pub struct TaskGroup { pub parent: Option<WorkItem>, pub items: Vec<WorkItem> }`
- Tauri commands:
  - `ado_teams(worktree) -> Vec<String>`
  - `ado_iterations(worktree) -> Vec<Iteration>`
  - `ado_tasks(worktree, scope: TaskScope) -> TaskList`
- Endpoints (from the spec): `teamsettings/iterations?$timeframe=current`, the full list for next/previous, `iterations/{id}/workitems`, `POST wit/workitemsbatch` (200 per batch, `$expand` relations for the parent), `POST wit/wiql`, `wit/workitemtypes/{type}/states`, `work/teamsettings/teamfieldvalues`.

- [ ] **Step 1: Failing tests** with httpmock fixtures (put the JSON under `desktop/src-tauri/tests/fixtures/ado/`, using the documented shapes):
  - the current sprint resolves, items are hydrated and grouped by parent
  - batch splits at 200 (fixture with 250 ids, so two batch calls)
  - `stale == true` when `finish` is in the past, and `next` is the following iteration by start date
  - `MyOpen` builds WIQL containing `[System.AssignedTo] = @Me`, and excludes Completed and Removed state categories via the states endpoint
  - HTML description converts to plain markdown (`<p>a</p><ul><li>b</li></ul>` becomes `a\n\n- b`)
  - acceptance falls back to the parent's
  - an empty iteration gives empty groups and no error
- [ ] **Step 2:** Expect FAIL.
- [ ] **Step 3:** Implement. The team is `store.team` or the project's `defaultTeam` name (from `GET {org}/_apis/projects/{project}`). The HTML-to-markdown converter is a tiny, tag-limited one (`p`, `br`, `ul`/`ol`/`li`, `b`/`strong`, `i`/`em`, `a`, `code`); strip everything else.
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: ADO sprint tasks with stale-sprint detection`.

### Task 7: Branches data

**Files:**
- Create: `desktop/src-tauri/src/ado/branches.rs`

**Interfaces:**
- Consumes: `naming::initials`, `AdoStore.branch_tasks`, `prs::PrSummary` (Task 8; use a `pr: Option<PrRef>` field filled later, where `pub struct PrRef { pub id: u32, pub status: String, pub is_draft: bool }`).
- Produces:
  - `pub struct BranchRow`, with fields:
    - `pub name: String`
    - `pub person: String` (the prefix before `/`, or `other`)
    - `pub local: bool`
    - `pub remote: bool`
    - `pub ahead: u32`
    - `pub behind: u32`
    - `pub gone: bool`
    - `pub last_commit_age: i64`
    - `pub last_author: String`
    - `pub task: Option<u32>`
    - `pub pr: Option<PrRef>`
    - `pub worktree: Option<String>`
    - `pub session: Option<String>`
  - `pub struct BranchTree { pub me: String, pub people: Vec<(String, Vec<BranchRow>)> }` (me first, then alphabetical)
  - `pub fn collect(repo: &Path, remote: &str) -> anyhow::Result<Vec<BranchRow>>`, using `git for-each-ref --format=...` over `refs/heads` and `refs/remotes/<remote>`, plus `%(upstream:track)`
  - `pub fn group(rows, me) -> BranchTree`
  - `pub fn resolve_task(name, store_map, pr_work_items: Option<&[u32]>) -> Option<u32>`, in the order store, then PR, then a `\b(\d{3,7})\b` match in the name
- Tauri commands: `ado_branches(worktree) -> BranchTree`, `ado_fetch(worktree)` (`git fetch --prune <remote>`), `ado_checkout_worktree(worktree, branch)` (through the daemon's existing worktree request; find it in `protocol.rs`, e.g. `CreateWorktree`), `ado_delete_local(worktree, branch)`.

- [ ] **Step 1: Failing tests** on a temp repo with a bare remote:
  - a local-only branch gives `local && !remote`
  - a pushed and advanced branch gives `ahead == 1`
  - a remote branch with no local gives `remote && !local`
  - a deleted upstream gives `gone`
  - grouping puts `jari/*` first when `me = "jari"`, and puts `main` under `other`
  - `resolve_task` follows the order
- [ ] **Step 2:** Expect FAIL.
- [ ] **Step 3:** Implement. Worktree and session for a branch come from the daemon model (`Client` state), matched on the worktree branch.
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: branches grouped by person with sync state`.

### Task 8: PRs read side

**Files:**
- Create: `desktop/src-tauri/src/ado/prs.rs`
- Modify: `desktop/src-tauri/src/main.rs`. Replace the `gh`-based `poll_prs` for ADO repos: keep `gh` for GitHub repos, and use `prs::list` when an ADO remote is detected.

**Interfaces:**
- Produces:
  - `pub struct PrSummary`, with fields:
    - `pub id: u32`
    - `pub title: String`
    - `pub source: String`
    - `pub target: String`
    - `pub is_draft: bool`
    - `pub status: String`
    - `pub created_by: String`
    - `pub reviewers: Vec<ReviewerVote>`
    - `pub work_items: Vec<u32>`
    - `pub threads: u32`
    - `pub url: String`
  - `pub struct ReviewerVote { pub id: String, pub name: String, pub vote: i8, pub required: bool }`
  - `pub enum PolicyState { Approved, Rejected, Queued, Running, NotApplicable, NotEvaluated }`
  - `pub struct PolicyRow { pub name: String, pub state: PolicyState, pub blocking: bool }`
  - `pub struct Thread { pub id: u32, pub status: String, pub file: Option<String>, pub line: Option<u32>, pub comments: Vec<Comment> }`
  - `pub struct Comment { pub author: String, pub text: String, pub at: String }`
  - Functions:
    - `list(client, repo_id, mine: &str /*my identity id*/) -> (Vec<PrSummary>, Vec<PrSummary>)`, which issues `searchCriteria.creatorId` and `searchCriteria.reviewerId` queries with `status=active`
    - `policies(client, project_id, pr_id, is_draft) -> Vec<PolicyRow>`, where a draft gives every row `NotEvaluated`
    - `threads(client, repo_id, pr_id) -> Vec<Thread>`, dropping system threads
  - My identity id comes from `GET {org}/_apis/connectionData` (`authenticatedUser.id`), cached per org.
- Tauri commands: `ado_prs(worktree) -> {mine, review}`, `ado_pr_detail(worktree, id) -> {pr, policies, threads}`.

- [ ] **Step 1: Failing tests** (httpmock):
  - the two queries carry the right search criteria
  - vote mapping covers 10/5/0/-5/-10
  - work items come from `GET .../pullrequests/{id}/workitems`
  - policy evaluation status maps to `PolicyState`
  - a draft gives `NotEvaluated`
  - system threads (`properties.CodeReviewThreadType`) are excluded
- [ ] **Step 2:** Expect FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: ADO pull requests, votes, policies and threads`.

### Task 9: PRs write side

**Files:**
- Modify: `desktop/src-tauri/src/ado/prs.rs`

**Interfaces:**
- Consumes: `attribution::{find_unpushed_ai_commits, rewrite_unpushed, strip_ai_trailers}`, `AdoStore::remember_reviewer`.
- Produces:
  - `pub struct NewPr`, with fields:
    - `pub source: String`
    - `pub target: String`
    - `pub title: String`
    - `pub description: String`
    - `pub draft: bool`
    - `pub reviewers: Vec<Reviewer>`
    - `pub work_item: Option<u32>`
  - `pub enum CreateOutcome { Created(u32), CreatedTaskNotLinked(u32, String), Existing(u32) }`
  - Functions:
    - `pub async fn create(ctx, repo_path, pr: NewPr) -> Result<CreateOutcome, AdoError>`
    - `publish(ctx, id)`
    - `vote(ctx, id, vote: i8)`
    - `reply(ctx, id, thread, text)`
    - `search_identities(ctx, q) -> Vec<Reviewer>`
- `create` order:
  1. Rewrite AI trailers in unpushed commits.
  2. Run `git push --set-upstream <remote> <source>`.
  3. List active PRs for `refs/heads/<source>`; if one exists, return `Existing`.
  4. POST with `sourceRefName`, `targetRefName`, `title`, `description` (passed through `strip_ai_trailers`), `isDraft`, `reviewers: [{id}]` and `workItemRefs: [{id: "<n>"}]`.
  5. On a network timeout, re-list and return `Existing` if found.
  6. If the response's work items are empty but a task was given, PATCH the work item with an `ArtifactLink` to the PR's `artifactId`. If that fails, return `CreatedTaskNotLinked`.
- Tauri commands: `ado_create_pr`, `ado_publish_pr`, `ado_vote`, `ado_reply`, `ado_search_identities`, `ado_recent_reviewers`.

- [ ] **Step 1: Failing tests** (httpmock + temp git repo with a bare remote standing in for origin):
  - a push happens before the POST
  - an existing PR short-circuits to `Existing` with no POST
  - a timeout then success-on-relist gives `Existing` with exactly one POST hit
  - workItemRefs is sent
  - a link-PATCH failure gives `CreatedTaskNotLinked`
  - the description has the AI footer removed
  - vote PUT body is `{"vote":10}`
  - identity search hits `https://vssps.dev.azure.com/{org}/_apis/identities?searchFilter=General&filterValue=sam`
- [ ] **Step 2:** Expect FAIL.
- [ ] **Step 3:** Implement. Make the vssps base derivable from `org_url` (dev.azure.com or visualstudio.com give the same `vssps.dev.azure.com/{org}`) and overridable in tests.
- [ ] **Step 4:** The tests pass.
- [ ] **Step 5:** Commit `desktop: draft, publish, vote and reply on ADO pull requests`.

### Task 10: Start a task

**Files:**
- Modify: `desktop/src-tauri/src/ado/tasks.rs`, `desktop/src-tauri/src/main.rs`

**Interfaces:**
- Produces:
  - `pub struct StartTask`, with fields:
    - `pub id: u32`
    - `pub branch: String`
    - `pub base: Option<String>` (default branch when None)
    - `pub assign_to_me: bool`
    - `pub activate: bool`
    - `pub spawn: Option<NewAgent>` (`NewAgent` is the existing struct in `main.rs`; the prompt is built by `task_prompt`)
  - `pub fn task_prompt(item: &WorkItem, parent: Option<&WorkItem>) -> String`, giving the title, description and acceptance criteria as markdown
  - Tauri command `ado_start_task(worktree, req: StartTask) -> {worktree_id, agent_id: Option<String>}`
- Steps:
  1. Validate the branch name (Task 1).
  2. Create the worktree on a new branch from base via the daemon.
  3. Record the branch-to-task link in the store.
  4. If `assign_to_me`, PATCH `System.AssignedTo` (my identity uniqueName).
  5. If `activate`, set `System.State` to the first state of the type's `InProgress` category.
  6. If `spawn`, create the agent with the prompt.
  7. Report every step's failure separately; a failed PATCH does not undo the branch.

- [ ] **Step 1: Failing tests:**
  - `task_prompt` content
  - an invalid branch is refused before any git or HTTP call
  - the PATCH body for assign and activate is the right JSON Patch (`op: add`, path `/fields/System.State`)
  - the store records the link
- [ ] **Step 2–4:** Implement and make the tests pass.
- [ ] **Step 5:** Commit `desktop: start an ADO task as a branch, worktree and agent`.

### Task 11: Diff engine upgrade

**Files:**
- Modify: `desktop/src-tauri/src/git.rs`, `desktop/src-tauri/src/main.rs`

**Interfaces:**
- Produces:
  - `pub enum DiffSide { Worktree, Index, Commit(String) }`
  - `pub fn diff_files_range(repo, base: &str, tip: &str) -> Result<Vec<DiffFile>>`, where `base` is resolved via `git merge-base`
  - `pub fn blob(repo, side: &DiffSide, path) -> Result<Blob>`
  - `pub enum Blob { Text(String), Missing, TooLarge(u64), Binary }`
  - `pub fn hunks(repo, path, staged: bool) -> Result<Vec<Hunk>>`
  - `pub struct Hunk { pub header: String, pub patch: String, pub old_start: u32, pub new_start: u32 }`
  - `pub fn apply_hunk(repo, hunk: &Hunk, action: HunkAction) -> Result<()>`
  - `pub enum HunkAction { Stage, Unstage, Discard }`, implemented with `git apply --cached`, `git apply --cached -R` and `git apply -R` on a temp patch file
  - `pub fn ensure_refs(repo, refs: &[&str]) -> Result<()>`, which fetches missing refs (including `refs/pull/{id}/merge`) and deepens shallow history (`git fetch --deepen=200`)
- Tauri commands: `diff_range(worktree, base, tip)`, `diff_blob(worktree, side, path)`, `diff_hunks(worktree, path, staged)`, `diff_apply_hunk(...)`.

- [ ] **Step 1: Failing tests** on a temp repo:
  - a missing path at a commit gives `Missing`, not empty
  - a 3 MB file gives `TooLarge`
  - a NUL byte gives `Binary`
  - two hunks in one file: staging hunk 1 leaves hunk 2 unstaged
  - discard reverts only that hunk
  - range diff uses the merge-base, not the target tip
- [ ] **Step 2–4:** Implement and make the tests pass.
- [ ] **Step 5:** Commit `desktop: SHA-pinned diffs with honest errors and hunk staging`.

### Task 12: Parent links and subagents in the protocol

**Files:**
- Modify: `crates/nebula-core/src/protocol.rs`, `crates/nebula-daemon/src/registry/spawn.rs`, `crates/nebula-daemon/src/hooks/mod.rs` (SubagentStart/Stop handling), `crates/nebula-daemon/src/store.rs` (persist parent), `desktop/src-tauri/src/daemon.rs` (model)

**Interfaces:**
- Produces:
  - `AgentSummary.parent: Option<AgentId>` (`#[serde(default)]`)
  - `pub struct SubagentSummary`, with fields:
    - `pub id: String`
    - `pub parent_agent: AgentId`
    - `pub kind: String`
    - `pub description: String`
    - `pub started_at: i64`
    - `pub ended_at: Option<i64>`
    - `pub status: SubagentStatus`
  - `pub enum SubagentStatus { Running, Done, Failed, EndedUnknown }`
  - `ServerEvent::SubagentChanged { sub: SubagentSummary }`
  - Snapshot gains `subagents: Vec<SubagentSummary>`
  - `PROTOCOL_VERSION += 1`, `MIN_COMPATIBLE_PROTOCOL` unchanged
- Daemon behaviour:
  - Subagent ids come from the hook payload (`agent_id`/`tool_use_id`, whichever the payload carries; read a captured SubagentStart payload from the hook tests' fixtures or Claude docs).
  - On parent exit or daemon restart, open subagents become `EndedUnknown`.
  - Kete-spawned children record `parent` from the spawning session's `KETE_SESSION`/`NEBULA_SESSION` env.

- [ ] **Step 1: Failing tests:**
  - codec round-trip of the new snapshot (old clients ignore the field: decode a snapshot without `subagents`)
  - a SubagentStart then SubagentStop hook sequence gives `Running` then `Done`
  - parent exit closes them
  - a spawn records the parent
- [ ] **Step 2–4:** Implement and pass. Run the root gates and the Linux Docker clippy (the protocol touches unix-only tests: update every `AgentSummary`/snapshot initializer found by `grep -rn "AgentSummary {" crates`).
- [ ] **Step 5:** Commit `Track session parents and Claude subagents in the protocol`.

### Task 13: Workshop shell, tokens and navigation state

**Files:**
- Modify: `desktop/src/theme.css`, `desktop/src/app.css`, `desktop/src/App.svelte`, `desktop/src/lib/theme.ts`, `desktop/package.json` (add `geist`; remove `@fontsource-variable/inter`)
- Create: `desktop/src/lib/shell/Rail.svelte`, `desktop/src/lib/shell/Breadcrumb.svelte`, `desktop/src/lib/shell/history.svelte.ts`, `desktop/src/lib/ado/Chip.svelte`
- Replace the app icon with `assets/brand/kete/icons/*` in `desktop/src-tauri/icons/`

**Interfaces:**
- Produces:
  - `theme.css`: the tokens from `docs/design/kete-desktop-brief.md` (Tokens table) as `@theme` variables, with `[data-theme=light]`, `[data-theme=dark]` and a `prefers-color-scheme` default for `system`. Tailwind utilities read them (`bg-bg`, `text-ink`, `border-rule`, `text-ember-ink`, …).
  - `history.svelte.ts`:
    - `export type Place = { view: 'overview'|'board'|'session'|'diff'|'tasks'|'branches'|'prs'; repo?: string; item?: string }`
    - `export const nav = { current: Place, back(): void, forward(): void, go(p: Place): void, ui(view: string, repo: string): Record<string, unknown> }`
    - per-view UI state is kept in memory and mirrored to `ado.json` `ui` via a debounced command `ado_save_ui`
  - `Chip.svelte` props: `{ kind: 'task'|'branch'|'pr'|'session', label: string, place: Place }`; clicking calls `nav.go(place)`
  - `Rail.svelte`:
    - the kete mark (svg inline from `assets/brand/kete/kete-mark.svg`)
    - the repo list with attention counts
    - Tasks / Branches / PRs entries, shown only when the repo has an ADO remote (from a new command `ado_context(worktree) -> Option<AdoRemote>`)
    - profiles, and the theme toggle cycling system, light, dark
  - `Alt+ArrowLeft` and `Alt+ArrowRight` bound globally to `nav.back()` and `nav.forward()`

- [ ] **Step 1:** Write `desktop/src/lib/shell/history.test.ts` (vitest; add `vitest` to devDependencies and an `npm test` script):

```ts
import { createNav } from './history.svelte';
test('back and forward restore places and per-view state', () => {
  const n = createNav();
  n.go({ view: 'tasks', repo: 'api' }); n.ui('tasks', 'api').filter = 'gst';
  n.go({ view: 'branches', repo: 'api' }); n.back();
  expect(n.current.view).toBe('tasks'); expect(n.ui('tasks', 'api').filter).toBe('gst');
  n.forward(); expect(n.current.view).toBe('branches');
});
test('go after back drops the forward stack', () => { /* ... */ });
```

- [ ] **Step 2:** `npm test` fails.
- [ ] **Step 3: Implement** `createNav` plus the shell components. Remove every literal colour and every `rounded*`/`shadow*` class (`grep -rnE "rounded|shadow|#[0-9a-fA-F]{3,6}" desktop/src/lib` must return nothing outside `theme.css`). Load Geist and Geist Mono from the `geist` package's woff2, and JetBrains Mono for terminals.
- [ ] **Step 4:** `npm test`, `npm run check` and `npm run build` pass.
- [ ] **Step 5:** Commit `desktop: Workshop shell, tokens and back/forward navigation`.

### Task 14: Overview home

**Files:**
- Create: `desktop/src/lib/overview/Overview.svelte`, `AttentionStrip.svelte`, `RepoRow.svelte`, `SessionTree.svelte`, `desktop/src/lib/overview/tree.ts`
- Modify: `desktop/src/App.svelte` (the overview is the default view), `desktop/src/lib/store.svelte.ts` (subagents in the model)

**Interfaces:**
- Consumes: the model's agents (with `parent`) and `subagents` (Task 12), live ports, and PR state.
- Produces:
  - `tree.ts`:
    - `export function buildForest(agents: Agent[], subs: Subagent[]): Node[]`
    - `export type Node = { id: string; kind: 'session'|'subagent'; status: Status; label: string; children: Node[]; depth: number }`
    - `export function attentionRank(n: Node): number`: needs-you 0, failed 1, unread 2, running 3, done 4. A row's rank is the minimum over its subtree.
    - `export function sortRepos(rows): rows`, by rank then name

- [ ] **Step 1: Vitest tests:**
  - forest nesting is three deep
  - a subagent nests under its session
  - an orphan whose parent is missing becomes a root
  - repo order: a repo with a needs-you child floats first
  - collapse default: depth > 2 is collapsed
- [ ] **Step 2–4:** Implement and pass. Layout per the brief's FIRST VIEWPORT:
  - The attention strip shows counts that jump.
  - Repo header in Geist SemiBold 15, with right-aligned Geist Mono 12 labels for branch, worktrees, ports and PR.
  - Session rows: status square + word, name, harness·profile, muted last prompt, age.
  - A needs-you row has a full-height ember left bar that retracts in 120 ms when resolved.
  - A running row has a 1 px line under its name.
- [ ] **Step 5:** Commit `desktop: overview of repos with nested agent trees by attention`.

### Task 15: Tasks view

**Files:**
- Create: `desktop/src/lib/ado/api.ts` (typed `invoke` wrappers for every `ado_*` command, matching the Rust structs field for field in camelCase), `desktop/src/lib/ado/Tasks.svelte`, `desktop/src/lib/ado/StartTask.svelte`

**Interfaces:**
- Consumes: `ado_tasks`, `ado_iterations`, `ado_teams`, `ado_start_task`, `nav`, `Chip`.

- [ ] **Step 1: Vitest** for the pure helpers in `api.ts`: `scopeLabel(scope)`, and `staleNotice(list)` returning "Sprint 42 ended 3 days ago" (no em dash).
- [ ] **Step 2–4: Implement the view:**
  - **Scope bar:** current sprint / iteration picker / my open / sprint all, plus a team picker.
  - **Stale notice:** a line with a "Show Sprint 43" action.
  - **Groups:** parent title as a ruled header, then items as rows: id (mono), title, state word, column, remaining.
  - **Fuzzy filter:** `/` focuses it.
  - **Detail split:** description and acceptance, plus linked branch, PR and session chips.
  - **Start dialog:** branch name prefilled from `initials/slug` and editable with live validation; base; assign and activate toggles, on; a spawn agent toggle with harness and profile; and a prompt preview.
  - Every async action shows its state and keeps the form on failure.
- [ ] **Step 5:** Commit `desktop: sprint tasks view with start-task flow`.

### Task 16: Branches view

**Files:**
- Create: `desktop/src/lib/ado/Branches.svelte`

- [ ] **Step 1: Vitest** for `filterBranches(tree, query)`: `ga/` narrows to person `ga`; a fuzzy `gst` matches `jari/gst-rounding`.
- [ ] **Step 2–4: Implement:**
  - person groups (mine expanded, others collapsed)
  - rows with the sync badge (`↑2 ↓1`, `local only`, `remote only`, `gone`), age and author, plus task, PR and session chips
  - breadcrumb `repo › branches › jari › gst-rounding`
  - actions: Fetch, Check out to worktree, Diff vs main (opens the Diff view with `diff_range`), Draft PR (opens the Composer), Delete local (confirm)
- [ ] **Step 5:** Commit `desktop: branches by person with sync and actions`.

### Task 17: PRs view, composer and review

**Files:**
- Create: `desktop/src/lib/ado/Prs.svelte`, `desktop/src/lib/ado/Composer.svelte`, `desktop/src/lib/ado/Review.svelte`, `desktop/src/lib/ado/policy.ts`

**Interfaces:**
- Produces in `policy.ts`:
  - `export function preflight(form): { ok: boolean; notes: string[] }`
  - Rules: task required unless `noTask`; ≥1 reviewer unless `noReviewer`. The escapes turn a block into a note, and `ok` is always true when the escapes are set.

- [ ] **Step 1: Vitest for `preflight`:**
  - no task and no escape gives `ok:false`, with note "Link a task (policy)"
  - with the escape it gives `ok:true` and note "No task (n/a)"
  - zero reviewers is the same pattern
- [ ] **Step 2–4: Implement.**
  - **PRs:** tabs Mine / To review. Rows show title, source → target, a DRAFT word, reviewer votes (square + word per reviewer) and the thread count.
  - **Detail:**
    - policy checklist from `ado_pr_detail` (a draft shows "not evaluated (draft)")
    - a Publish button when draft
    - the diff (pinned SHAs)
    - threads with file:line and reply
    - vote buttons (disabled on drafts, with the reason)
  - **Composer** (opened from a branch or a task):
    - title from the task
    - description template sections Summary, Changes (prefilled from the commit subjects in range), Testing, Task
    - an "Ask the session to summarise" button when a session exists (calls the existing daemon ask request and inserts the answer into Summary)
    - a task picker with a No task (n/a) checkbox
    - a reviewer search with recent reviewers first and a No reviewer checkbox
    - Draft on by default, with the diff preview beside the form
  - **Create:** shows the outcome states, including "Created, task not linked" with Retry, and "Already open" with a link.
- [ ] **Step 5:** Commit `desktop: ADO pull requests view, composer and review`.

### Task 18: Diff view modes and hunks (UI)

**Files:**
- Modify: `desktop/src/lib/Diff.svelte`, `desktop/src/lib/monaco.ts`

- [ ] **Step 1: Vitest** for `replacementModel(oldText, newText)`: it returns the new text plus decoration ranges for changed spans, and removed-line markers.
- [ ] **Step 2–4: Implement:**
  - a mode toggle (Side by side / Inline / Replacement), persisted via `nav.ui`
  - next and previous change (`F7`, `Shift+F7`)
  - per-hunk Stage, Unstage and Discard buttons on working-tree diffs only (Discard confirms)
  - `Missing`, `TooLarge` and `Binary` render as labelled states
  - Monaco workers are bundled locally (vite `?worker` imports); the CSP stays strict
- [ ] **Step 5:** Commit `desktop: diff modes, change navigation and hunk actions`.

### Task 19: TUI themes and terminal scheme

**Files:**
- Modify: `crates/nebula-tui/src/theme.rs` (add `kete-dark` and `kete-light` to `THEMES` and `by_name`), `crates/nebula/src/setup.rs` (the "Kete Workshop" scheme is the default for the Kete profile; One Dark stays available)

- [ ] **Step 1: Failing tests:**
  - `Theme::by_name("kete-dark")` gives `accent == Color::Rgb(0xE0,0x8A,0x35)` and `text == Color::Rgb(0xED,0xE6,0xDA)`
  - the existing role-invariant tests pass for both new presets (root vs worktree hue separation, done ≠ merged)
  - the setup dry-run output contains `"name": "Kete Workshop"` with `background "#161412"`
- [ ] **Step 2–4:** Implement and pass the root gates.
- [ ] **Step 5:** Commit `Kete Workshop themes for the TUI and Windows Terminal`.

### Task 20: Installer and release

**Files:**
- Modify:
  - `desktop/src-tauri/tauri.conf.json`: `bundle.active: true`, `targets: ["nsis"]`, productName `Kete`, and icons from `assets/brand/kete/icons`
  - `scripts/rebrand/overlay/.github/workflows/release.yml`, then regenerate
  - `scripts/rebrand/overlay/install.ps1`, which installs `kete.exe`, `bro.exe` and, when the release has it, runs the `kete-desktop` NSIS installer silently (`/S`)

- [ ] **Step 1:** Add a workflow job `desktop-windows` on `windows-latest`: `npm ci`, `npm run tauri build`, then upload `desktop/src-tauri/target/release/bundle/nsis/Kete_*_x64-setup.exe` as asset `kete-desktop-x86_64-setup.exe`. The release job attaches it beside `kete-x86_64-pc-windows-msvc.zip`.
- [ ] **Step 2:** Build locally (`npm run tauri build`). Install with `/S` into a temp dir via `/D=`, then launch `kete desktop` against an isolated daemon and confirm the window title "kete". Uninstall.
- [ ] **Step 3:** `bash scripts/rebrand.sh` gives "nothing to do". Run the root gates.
- [ ] **Step 4:** Commit `Ship the desktop app as an installer in releases`.

### Task 21: Scenario screenshots, README and GitHub finish

**Files:**
- Create: `desktop/scripts/seed-demo.mjs` (a demo seeder using an isolated daemon and fake ADO fixtures served by a tiny local server via `KETE_ADO_BASE` test override), `assets/screenshots/*.png`
- Modify: `scripts/rebrand/overlay/README.md`

- [ ] **Step 1: Seed and capture** (an isolated daemon, 4 demo repos with labelled synthetic content, nested orchestrator → 3 agents → 2 subagents, one needs-you, one PR draft) these four scenarios at 1440×900, light and dark:
  1. Overview with nested agents and attention strip.
  2. Tasks → Start task dialog.
  3. Branches grouped by person.
  4. PR composer with policy notes and diff.

  Screenshots come from the app window (PowerShell `CopyFromScreen` of the window rect).
- [ ] **Step 2:** Rewrite the README's feature section around those four scenarios, with the images and no em dashes. Regenerate via the rebrand script.
- [ ] **Step 3:** GitHub:
  - upload `assets/brand/kete/kete-social.png` as the social preview (Settings, via the browser; the user is signed in)
  - check the description and topics
  - rewrite our own commit messages since `a00b760` to the kete name and without em dashes: `git filter-branch --msg-filter` over `a00b760..windows` only, in a scratch clone first, then force-push `windows` and `main` after confirming with the user
  - tag `v0.1.0` and let the release workflow publish `kete-*.zip`/`.tar.gz` and `kete-desktop-x86_64-setup.exe`
- [ ] **Step 4:** Live ADO smoke with the user against TnT:
  1. read-only views first: tasks, branches, PRs
  2. then one draft PR from a scratch branch with a task linked
  3. confirm the policy rows, then abandon the PR and delete the branch
- [ ] **Step 5:** Commit `README: kete scenarios with screenshots`.

---

## Self-review notes

- **Spec coverage:**

  | Spec area | Task |
  |---|---|
  | auth | 2 |
  | repository identity | 1, 6, 7 |
  | tasks | 6, 10, 15 |
  | branches | 7, 16 |
  | PRs | 8, 9, 17 |
  | policy | 8, 17 |
  | review | 8, 9, 17 |
  | diff | 11, 18 |
  | store | 4 |
  | nested subagents | 12, 14 |
  | attribution | 5 (and 9) |
  | errors | 3, and every UI task's async states |
  | testing | per task, plus 21 live smoke |
  | Workshop UI | 13, 14 |
  | TUI themes | 19 |
  | delivery | 20 |
  | screenshots | 21 |

- **Order and parallelism:**
  - Tasks 1–4 are sequential.
  - Then 5, 6, 7, 11 and 12 can run in parallel.
  - 8 → 9 → 10 follow.
  - The UI tasks 13 → 14, then 15, 16, 17 and 18 in parallel.
  - 19 can run any time after the rebrand.
  - 20 and 21 last.
