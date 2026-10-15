use super::*;
use crate::auth::{Credential, TokenSource};
use std::collections::{HashMap, VecDeque};
use std::process::Command;
use std::sync::Mutex;

// ---- forge detection ----

#[test]
fn forges_are_told_apart_by_their_remote() {
    let gh = |host: &str, owner: &str, repo: &str| {
        Some(Forge::GitHub(GitHubRepo {
            host: host.into(),
            owner: owner.into(),
            repo: repo.into(),
        }))
    };
    let table = [
        (
            "git@github.com:JayRichh/kete.git",
            gh("github.com", "JayRichh", "kete"),
        ),
        (
            "https://github.com/JayRichh/kete",
            gh("github.com", "JayRichh", "kete"),
        ),
        (
            "https://jay@github.com/JayRichh/kete.git/",
            gh("github.com", "JayRichh", "kete"),
        ),
        (
            "ssh://git@github.com/JayRichh/kete.git",
            gh("github.com", "JayRichh", "kete"),
        ),
        ("ssh://git@github.com:22/o/r", gh("github.com", "o", "r")),
        (
            "https://github.acme.io/team/api.git",
            gh("github.acme.io", "team", "api"),
        ),
        ("https://gitlab.com/o/r.git", None),
        ("git@bitbucket.org:o/r.git", None),
        ("https://github.com/only-owner", None),
        ("https://github.com/o/r/tree/main", None),
        ("", None),
        ("/c/repos/bare.git", None),
    ];
    for (url, want) in table {
        assert_eq!(detect_forge(url), want, "{url}");
    }
    for url in [
        "https://dev.azure.com/tnt/Platform/_git/api",
        "git@ssh.dev.azure.com:v3/tnt/Platform/api",
        "tnt@vs-ssh.visualstudio.com:v3/tnt/Platform/api",
        "https://tnt.visualstudio.com/Platform/_git/api",
    ] {
        let Some(Forge::Ado(r)) = detect_forge(url) else {
            panic!("{url} is Azure DevOps");
        };
        assert_eq!((r.org.as_str(), r.repo.as_str()), ("tnt", "api"), "{url}");
    }
    let enterprise = parse_github("git@github.acme.io:team/api").unwrap();
    assert_eq!(enterprise.gh_repo(), "github.acme.io/team/api");
    assert_eq!(parse_github("git@github.com:o/r").unwrap().gh_repo(), "o/r");
}

// ---- title and body ----

fn commit(subject: &str) -> CommitLine {
    CommitLine {
        sha: "0".repeat(40),
        short: "0000000".into(),
        subject: subject.into(),
    }
}

#[test]
fn the_title_is_the_sessions_name_then_a_lone_commit_then_the_prompt_then_the_branch() {
    let one = [commit("Fix the login redirect")];
    let two = [commit("Add tests"), commit("Fix the redirect")];
    let s = |name, prompt, branch, commits: &[CommitLine]| {
        suggest_title_body(name, prompt, branch, commits).title
    };
    assert_eq!(
        s(Some("Login redirect"), Some("fix it"), None, &two),
        "Login redirect"
    );
    assert_eq!(
        s(Some("agent-3"), Some("fix it"), None, &one),
        "Fix the login redirect"
    );
    assert_eq!(
        s(Some("  "), Some("\n  Fix the login\nmore"), None, &two),
        "Fix the login"
    );
    assert_eq!(
        s(None, None, Some("jr/fix-login_redirect"), &two),
        "Fix login redirect"
    );
    assert_eq!(s(None, None, None, &[]), "Changes");
    // `agent-` alone, or with words, is a real name.
    assert_eq!(s(Some("agent-x"), None, None, &[]), "agent-x");
    let long = "word ".repeat(30);
    let t = s(None, Some(&long), None, &two);
    assert!(
        t.chars().count() <= TITLE_MAX + 1 && t.ends_with('…'),
        "{t}"
    );
}

#[test]
fn the_body_has_the_prompt_and_the_commits_oldest_first_without_co_authors() {
    let commits = [commit("Second"), commit("First")];
    let b = suggest_title_body(
        None,
        Some("Fix the redirect.\n\nCo-Authored-By: Claude <noreply@anthropic.com>"),
        Some("fix"),
        &commits,
    )
    .body;
    assert_eq!(
        b,
        "## Summary\nFix the redirect.\n\n## Changes\n- First\n- Second"
    );
    assert_eq!(suggest_title_body(None, None, None, &[]).body, "");
    assert_eq!(
        suggest_title_body(None, None, None, &commits[..1]).body,
        "## Changes\n- Second"
    );
}

// ---- gh ----

fn repo() -> GitHubRepo {
    GitHubRepo {
        host: "github.com".into(),
        owner: "o".into(),
        repo: "r".into(),
    }
}

#[test]
fn gh_arguments_name_the_repo_and_never_prompt() {
    assert_eq!(
        gh_list_args(&repo(), "fix").join(" "),
        format!("pr list --repo o/r --head fix --state all --limit 5 --json {GH_PR_FIELDS}")
    );
    assert_eq!(
        gh_create_args(&repo(), "fix", "main", "Fix it", true).join(" "),
        "pr create --repo o/r --head fix --base main --title Fix it --body-file - --draft"
    );
    assert!(!gh_create_args(&repo(), "fix", "main", "t", false).contains(&"--draft".to_string()));
    assert_eq!(
        gh_merge_args(&repo(), 12, MergeMethod::Squash, Some("abc")).join(" "),
        "pr merge 12 --repo o/r --squash --match-head-commit abc"
    );
    assert_eq!(
        gh_merge_args(&repo(), 12, MergeMethod::Rebase, None).join(" "),
        "pr merge 12 --repo o/r --rebase"
    );
    assert_eq!(
        gh_merge_args(&repo(), 1, MergeMethod::Merge, Some("")).join(" "),
        "pr merge 1 --repo o/r --merge"
    );
    for m in [MergeMethod::Squash, MergeMethod::Merge, MergeMethod::Rebase] {
        assert!(!gh_merge_args(&repo(), 1, m, None).contains(&"--delete-branch".to_string()));
    }
    assert_eq!(MergeMethod::parse(" Rebase "), Some(MergeMethod::Rebase));
    assert_eq!(MergeMethod::parse("ff"), None);
}

fn gh_pr(n: u64, branch: &str, state: &str, draft: bool, fork: bool) -> Value {
    serde_json::json!({
        "number": n, "url": format!("https://github.com/o/r/pull/{n}"), "title": format!("PR {n}"),
        "state": state, "isDraft": draft, "headRefName": branch, "headRefOid": format!("sha{n}"),
        "baseRefName": "main", "isCrossRepository": fork
    })
}

#[test]
fn the_branchs_pr_is_its_own_open_one_before_its_newest() {
    let list = serde_json::json!([
        gh_pr(9, "fix", "MERGED", false, false),
        gh_pr(7, "fix", "OPEN", true, false),
        gh_pr(11, "fix", "OPEN", false, true),
        gh_pr(12, "other", "OPEN", false, false),
    ])
    .to_string();
    let pr = pick_gh_pr(&list, "fix").unwrap();
    assert_eq!((pr.number, pr.state.as_str(), pr.draft), (7, "open", true));
    assert_eq!(pr.head_sha.as_deref(), Some("sha7"));
    assert_eq!(pr.base, "main");
    let closed = serde_json::json!([
        gh_pr(3, "fix", "CLOSED", false, false),
        gh_pr(5, "fix", "MERGED", false, false)
    ])
    .to_string();
    assert_eq!(pick_gh_pr(&closed, "fix").unwrap().state, "merged");
    assert_eq!(pick_gh_pr("[]", "fix"), None);
    assert_eq!(pick_gh_pr("not json", "fix"), None);
    assert_eq!(
        number_from_url("https://github.com/o/r/pull/42\n"),
        Some(42)
    );
}

#[test]
fn a_missing_gh_sign_in_is_a_sign_in_error() {
    let out = |stderr: &str| RunOut {
        ok: false,
        stdout: String::new(),
        stderr: stderr.into(),
    };
    let e = gh_error(
        &out("To get started with GitHub CLI, please run:  gh auth login\n"),
        "gh pr list",
    );
    assert_eq!(
        (e.kind, e.forge),
        (ShipErrorKind::SignIn, Some(ForgeKind::GitHub))
    );
    assert_eq!(e.message, GH_SIGN_IN);
    assert_eq!(
        gh_error(&out("HTTP 401: Bad credentials"), "x").kind,
        ShipErrorKind::SignIn
    );
    let e = gh_error(
        &out("\nGraphQL: Pull request is not mergeable\n"),
        "gh pr merge",
    );
    assert_eq!(e.kind, ShipErrorKind::Other);
    assert_eq!(
        e.message,
        "gh pr merge failed: GraphQL: Pull request is not mergeable"
    );
    let json = serde_json::to_value(e.with_url("https://x/1")).unwrap();
    assert_eq!(json["kind"], "other");
    assert_eq!(json["forge"], "github");
    assert_eq!(json["url"], "https://x/1");
}

// ---- flows over a scripted runner ----

/// One call: its key, its args, its stdin.
type Call = (String, Vec<String>, Option<String>);

/// Answers `program` + its first two args from a queue (the last answer
/// repeats), and records every call: no real `gh` ever runs.
#[derive(Default)]
struct Script {
    answers: Mutex<HashMap<String, VecDeque<std::io::Result<RunOut>>>>,
    calls: Mutex<Vec<Call>>,
}

impl Script {
    fn key(program: &str, args: &[String]) -> String {
        let mut k = program.to_string();
        for a in args.iter().take(2) {
            k.push(' ');
            k.push_str(a);
        }
        k
    }

    fn on(self, key: &str, ok: bool, stdout: &str, stderr: &str) -> Self {
        self.answers
            .lock()
            .unwrap()
            .entry(key.into())
            .or_default()
            .push_back(Ok(RunOut {
                ok,
                stdout: stdout.into(),
                stderr: stderr.into(),
            }));
        self
    }

    fn missing(self, key: &str) -> Self {
        self.answers
            .lock()
            .unwrap()
            .entry(key.into())
            .or_default()
            .push_back(Err(std::io::Error::new(std::io::ErrorKind::NotFound, "gh")));
        self
    }

    fn calls_of(&self, key: &str) -> Vec<(Vec<String>, Option<String>)> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _, _)| k == key)
            .map(|(_, a, s)| (a.clone(), s.clone()))
            .collect()
    }
}

#[async_trait]
impl Runner for Script {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        _cwd: &Path,
        stdin: Option<&str>,
    ) -> std::io::Result<RunOut> {
        let key = Self::key(program, args);
        self.calls
            .lock()
            .unwrap()
            .push((key.clone(), args.to_vec(), stdin.map(str::to_string)));
        let mut answers = self.answers.lock().unwrap();
        let queue = answers
            .get_mut(&key)
            .unwrap_or_else(|| panic!("unscripted call: {key}"));
        let next = if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else {
            match queue.front().unwrap() {
                Ok(out) => Ok(out.clone()),
                Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
            }
        };
        next
    }
}

fn state() -> AdoState {
    AdoState::new(
        crate::testing::temp_store(),
        Arc::new(TokenSource::fixed(Credential::Bearer("tok".into()))),
    )
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(cwd)
        .args([
            "-c",
            "user.name=Jay Rich",
            "-c",
            "user.email=jay@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A checkout on `fix`, one commit past `main`, whose `origin` is a local
/// bare repo holding `main`: pushes land there, never on a network.
struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("bare.git");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        git(
            dir.path(),
            &["init", "-q", "--bare", bare.to_str().unwrap()],
        );
        git(&work, &["init", "-q"]);
        std::fs::write(work.join("a.txt"), "a\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "Start"]);
        git(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&work, &["push", "-q", "origin", "main"]);
        git(&work, &["fetch", "-q", "origin"]);
        git(&work, &["checkout", "-q", "-b", "fix"]);
        std::fs::write(work.join("a.txt"), "b\n").unwrap();
        git(&work, &["commit", "-q", "-am", "Fix the thing"]);
        Self { dir }
    }

    fn place(&self) -> Place {
        Place::new(self.dir.path().join("work"), "main")
    }

    fn remote_has(&self, branch: &str) -> bool {
        Command::new("git")
            .current_dir(self.dir.path().join("bare.git"))
            .args([
                "rev-parse",
                "--verify",
                "-q",
                &format!("refs/heads/{branch}"),
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

fn github() -> Forge {
    Forge::GitHub(repo())
}

#[tokio::test]
async fn create_pushes_then_opens_the_pr_with_the_body_on_stdin() {
    let r = Repo::new();
    let list = serde_json::json!([gh_pr(14, "fix", "OPEN", true, false)]).to_string();
    let script = Arc::new(
        Script::default()
            .on("gh pr list", true, "[]", "")
            .on("gh pr list", true, &list, "")
            .on("gh pr create", true, "https://github.com/o/r/pull/14\n", ""),
    );
    let ado = state();
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let new = NewPr {
        title: " Fix the thing ".into(),
        body: "Why.\n\nCo-Authored-By: Claude <noreply@anthropic.com>".into(),
        draft: true,
        base: None,
    };
    let made = ship
        .create_on(&r.place(), "origin", &github(), new)
        .await
        .unwrap();
    assert!(r.remote_has("fix"), "the branch was pushed");
    assert!(!made.existing);
    assert_eq!((made.pr.number, made.pr.draft), (14, true));
    let create = script.calls_of("gh pr create");
    assert_eq!(create.len(), 1);
    let (args, stdin) = &create[0];
    assert_eq!(
        args.join(" "),
        "pr create --repo o/r --head fix --base main --title Fix the thing --body-file - --draft"
    );
    assert_eq!(
        stdin.as_deref(),
        Some("Why."),
        "co-author lines are cleaned"
    );
}

#[tokio::test]
async fn an_open_pr_is_updated_by_the_push_not_opened_twice() {
    let r = Repo::new();
    let list = serde_json::json!([gh_pr(3, "fix", "OPEN", false, false)]).to_string();
    let script = Arc::new(Script::default().on("gh pr list", true, &list, ""));
    let ado = state();
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let new = NewPr {
        title: "t".into(),
        ..NewPr::default()
    };
    let made = ship
        .create_on(&r.place(), "origin", &github(), new)
        .await
        .unwrap();
    assert!(made.existing && made.pr.number == 3);
    assert!(r.remote_has("fix"));
    assert!(script.calls_of("gh pr create").is_empty());
}

#[tokio::test]
async fn create_refuses_a_blank_title_and_the_base_branch_before_pushing() {
    let r = Repo::new();
    let script = Arc::new(Script::default());
    let ado = state();
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let e = ship
        .create_on(&r.place(), "origin", &github(), NewPr::default())
        .await
        .unwrap_err();
    assert_eq!(e.message, "Add a title.");
    let onto_itself = NewPr {
        title: "t".into(),
        base: Some("fix".into()),
        ..NewPr::default()
    };
    let e = ship
        .create_on(&r.place(), "origin", &github(), onto_itself)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ShipErrorKind::NoBranch);
    assert!(!r.remote_has("fix"));
    assert!(script.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn merge_pins_the_head_and_deletes_only_the_remote_branch() {
    let r = Repo::new();
    git(&r.dir.path().join("work"), &["push", "-q", "origin", "fix"]);
    let list = serde_json::json!([gh_pr(5, "fix", "OPEN", false, false)]).to_string();
    let done = serde_json::json!([gh_pr(5, "fix", "MERGED", false, false)]).to_string();
    let script = Arc::new(
        Script::default()
            .on("gh pr list", true, &list, "")
            .on("gh pr list", true, &done, "")
            .on("gh pr merge", true, "", "")
            .on("git push origin", true, "", ""),
    );
    let ado = state();
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let opts = MergeOpts {
        method: MergeMethod::Squash,
        delete_branch: true,
    };
    let merged = ship
        .merge_on(&r.place(), "origin", &github(), opts)
        .await
        .unwrap();
    assert_eq!(
        (merged.state.as_str(), merged.branch_deleted),
        ("merged", true)
    );
    assert_eq!(
        script.calls_of("gh pr merge")[0].0.join(" "),
        "pr merge 5 --repo o/r --squash --match-head-commit sha5"
    );
    assert_eq!(
        script.calls_of("git push origin")[0].0.join(" "),
        "push origin --delete fix"
    );
}

#[tokio::test]
async fn merge_refuses_a_draft_or_no_pr_and_passes_gh_refusals_on() {
    let r = Repo::new();
    let ado = state();
    let draft = serde_json::json!([gh_pr(5, "fix", "OPEN", true, false)]).to_string();
    let script = Arc::new(Script::default().on("gh pr list", true, &draft, ""));
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let e = ship
        .merge_on(&r.place(), "origin", &github(), MergeOpts::default())
        .await
        .unwrap_err();
    assert!(e.message.contains("draft"), "{e}");
    assert_eq!(e.url.as_deref(), Some("https://github.com/o/r/pull/5"));
    assert!(script.calls_of("gh pr merge").is_empty());

    let merged = serde_json::json!([gh_pr(5, "fix", "MERGED", false, false)]).to_string();
    let script = Arc::new(Script::default().on("gh pr list", true, &merged, ""));
    let ship = Ship::new(&ado, true).with_runner(script);
    let e = ship
        .merge_on(&r.place(), "origin", &github(), MergeOpts::default())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ShipErrorKind::NoPr);

    let open = serde_json::json!([gh_pr(5, "fix", "OPEN", false, false)]).to_string();
    let script = Arc::new(Script::default().on("gh pr list", true, &open, "").on(
        "gh pr merge",
        false,
        "",
        "X Pull request o/r#5 is not mergeable: the base branch policy prohibits the merge.\n",
    ));
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let opts = MergeOpts {
        method: MergeMethod::Merge,
        delete_branch: true,
    };
    let e = ship
        .merge_on(&r.place(), "origin", &github(), opts)
        .await
        .unwrap_err();
    assert!(e.message.contains("not mergeable"), "{e}");
    assert!(
        script.calls_of("git push origin").is_empty(),
        "nothing deleted"
    );
}

#[tokio::test]
async fn sign_in_and_a_missing_gh_are_typed() {
    let r = Repo::new();
    let ado = state();
    let script = Arc::new(Script::default().on(
        "gh pr list",
        false,
        "",
        "To get started with GitHub CLI, please run:  gh auth login\n",
    ));
    let ship = Ship::new(&ado, true).with_runner(script);
    let e = ship
        .pr_for(&r.place().path, &github(), "fix")
        .await
        .unwrap_err();
    assert_eq!(e.kind, ShipErrorKind::SignIn);
    let script = Arc::new(Script::default().missing("gh pr list"));
    let ship = Ship::new(&ado, true).with_runner(script);
    let e = ship
        .pr_for(&r.place().path, &github(), "fix")
        .await
        .unwrap_err();
    assert_eq!(e.kind, ShipErrorKind::NoTool);
}

/// A checkout whose only remote is a local path ships nowhere: the status
/// still says its branch, base and unpushed commits.
#[tokio::test]
async fn status_without_a_forge_still_reads_the_checkout() {
    let r = Repo::new();
    let ado = state();
    let ship = Ship::new(&ado, true).with_runner(Arc::new(Script::default()));
    let st = ship.status(&r.place()).await;
    assert_eq!(st.forge, None);
    assert_eq!(st.branch.as_deref(), Some("fix"));
    assert_eq!(st.base.as_deref(), Some("main"));
    assert_eq!(st.unpushed, Some(1));
    assert_eq!(st.error.map(|e| e.kind), Some(ShipErrorKind::NoForge));
    let s = r.place().suggest(Some("agent-1"), None).await;
    assert_eq!(s.title, "Fix the thing");
    assert_eq!(s.body, "## Changes\n- Fix the thing");
}

fn agent(id: &str, worktree: &str, at: i64, prompts: &[&str]) -> Agent {
    use nebula_core::{AgentKind, AgentRole, AgentStatus, PromptEntry};
    Agent {
        id: AgentId(id.into()),
        worktree_id: WorktreeId(worktree.into()),
        name: format!("name-{id}"),
        status: AgentStatus::Finished,
        archived: false,
        archived_at: 0,
        unseen: false,
        status_changed_at: at,
        kind: AgentKind::Claude,
        custom_harness: None,
        model: None,
        effort: None,
        session_id: None,
        cloud_session_id: None,
        issue_url: None,
        role: AgentRole::Worker,
        sort_order: 0,
        alive: true,
        recent_prompts: prompts
            .iter()
            .map(|t| PromptEntry {
                text: t.to_string(),
                submitted_at: 0,
            })
            .collect(),
        profile: None,
        parent: None,
    }
}

#[test]
fn the_suggestion_reads_the_named_session_else_the_worktrees_latest() {
    let mut gone = agent("c", "w1", 99, &["old"]);
    gone.archived = true;
    let agents = vec![
        agent("a", "w1", 5, &["first", "fix the login"]),
        agent("b", "w1", 9, &[]),
        agent("d", "w2", 50, &["elsewhere"]),
        gone,
    ];
    let w = |id: &str| WorktreeId(id.into());
    assert_eq!(
        session_words(&agents, &w("w1"), Some(&AgentId("a".into()))),
        (Some("name-a".into()), Some("fix the login".into()))
    );
    assert_eq!(
        session_words(&agents, &w("w1"), None),
        (Some("name-b".into()), None)
    );
    // A session id nobody has falls back to the worktree's latest.
    assert_eq!(
        session_words(&agents, &w("w2"), Some(&AgentId("zz".into()))).0,
        Some("name-d".into())
    );
    assert_eq!(session_words(&agents, &w("w3"), None), (None, None));
}

/// gh exits 0 having only queued the merge (a merge queue, auto-merge
/// behind checks): reported as queued, and the branch the PR needs stays.
#[tokio::test]
async fn a_merge_gh_only_queued_keeps_the_branch() {
    let r = Repo::new();
    let open = serde_json::json!([gh_pr(5, "fix", "OPEN", false, false)]).to_string();
    let script = Arc::new(Script::default().on("gh pr list", true, &open, "").on(
        "gh pr merge",
        true,
        "",
        "",
    ));
    let ado = state();
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let opts = MergeOpts {
        method: MergeMethod::Squash,
        delete_branch: true,
    };
    let m = ship
        .merge_on(&r.place(), "origin", &github(), opts)
        .await
        .unwrap();
    assert_eq!((m.state.as_str(), m.branch_deleted), ("queued", false));
    assert!(script.calls_of("git push origin").is_empty());
}

/// Nothing is rewritten or pushed until gh has answered: without it, or
/// signed out, the branch stays where it was.
#[tokio::test]
async fn create_asks_gh_before_it_pushes() {
    let r = Repo::new();
    let ado = state();
    let script = Arc::new(Script::default().missing("gh pr list"));
    let ship = Ship::new(&ado, true).with_runner(script);
    let new = NewPr {
        title: "t".into(),
        ..NewPr::default()
    };
    let e = ship
        .create_on(&r.place(), "origin", &github(), new)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ShipErrorKind::NoTool);
    assert!(!r.remote_has("fix"), "nothing was pushed");
}

/// A checkout left on the remote's default branch is not shipped onto
/// another base: that would push it and open main into develop.
#[tokio::test]
async fn the_default_branch_is_not_shipped() {
    let r = Repo::new();
    let work = r.dir.path().join("work");
    git(&work, &["remote", "set-head", "origin", "main"]);
    git(&work, &["checkout", "-q", "main"]);
    let ado = state();
    let script = Arc::new(Script::default());
    let ship = Ship::new(&ado, true).with_runner(script.clone());
    let new = NewPr {
        title: "t".into(),
        base: Some("develop".into()),
        ..NewPr::default()
    };
    let e = ship
        .create_on(&r.place(), "origin", &github(), new)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ShipErrorKind::NoBranch);
    assert!(e.message.contains("default branch"), "{e}");
    assert!(script.calls.lock().unwrap().is_empty());
}

/// A git push the remote refused for want of a sign-in (git never asks,
/// `git_env`) comes back as SignIn, in git's words rather than gh's; any
/// other push failure stays as it was.
#[test]
fn a_refused_push_sign_in_is_sign_in() {
    use crate::client::AdoError;
    for why in [
        "fatal: Authentication failed for 'https://dev.azure.com/o/p/_git/r/'",
        "git@github.com: Permission denied (publickey).",
        "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
    ] {
        let e = ShipError::from_ado(AdoError::Local(format!(
            "The git push of feat to origin failed: {why}"
        )));
        assert_eq!(e.kind, ShipErrorKind::SignIn, "{why}");
        assert!(e.message.contains(why), "{}", e.message);
        assert!(!e.message.contains("The git push of"), "{}", e.message);
        assert!(!e.message.contains("gh auth login"), "{}", e.message);
    }
    let e = ShipError::from_ado(AdoError::Local(
        "The git push of feat to origin failed: ! [rejected] feat -> feat (non-fast-forward)"
            .into(),
    ));
    assert_eq!(e.kind, ShipErrorKind::Other);
    let e = ShipError::from_ado(AdoError::Local(
        "The git push of feat to origin failed: Host key verification failed.".into(),
    ));
    assert_eq!(e.kind, ShipErrorKind::Other);
}
