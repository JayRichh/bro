use super::daemon::{next_agent_name, project_for_checkout, resolve_kind, spawn_request, Tree};
use super::*;
use kete_ado::auth::fake::Fake;
use kete_ado::auth::{AuthError, TokenSource};
use kete_ado::client::AdoClient;
use kete_ado::tasks::{Iteration, TaskGroup};
use nebula_core::{
    Agent, AgentId, AgentKind, AgentRole, AgentStatus, ClientRequest, ProjectId, Worktree,
    WorktreeId,
};
use std::sync::Arc;

#[test]
fn votes_read_as_words_or_numbers() {
    use crate::cli::parse_vote;
    assert_eq!(parse_vote("approve"), Ok(10));
    assert_eq!(parse_vote("approve-with-suggestions"), Ok(5));
    assert_eq!(parse_vote("WAIT"), Ok(-5));
    assert_eq!(parse_vote("reject"), Ok(-10));
    assert_eq!(parse_vote("reset"), Ok(0));
    assert!(parse_vote("lgtm").unwrap_err().contains("approve"));
}

#[test]
fn the_description_is_the_composers_four_sections() {
    // The desktop's `describe` (prs-state.svelte.ts), byte for byte.
    assert_eq!(
        describe("Why", "- a\n- b", "", "#12 Fix it"),
        "## Summary\nWhy\n\n## Changes\n- a\n- b\n\n## Testing\n\n## Task\n#12 Fix it\n"
    );
    assert_eq!(
        describe("", "", "", "No task (n/a)"),
        "## Summary\n\n## Changes\n\n## Testing\n\n## Task\nNo task (n/a)\n"
    );
}

#[test]
fn a_pr_needs_a_task_and_a_reviewer_unless_waived() {
    assert!(preflight(Some(1), false, 1, false).is_ok());
    assert!(preflight(None, true, 0, true).is_ok());
    let e = preflight(None, false, 0, false).unwrap_err().to_string();
    assert!(e.contains("--task") && e.contains("--reviewer"), "{e}");
    let e = preflight(Some(1), false, 0, false).unwrap_err().to_string();
    assert!(!e.contains("--task") && e.contains("--no-reviewer"), "{e}");
}

fn person(name: &str, email: &str) -> Reviewer {
    Reviewer {
        id: email.into(),
        name: name.into(),
        email: email.into(),
    }
}

#[test]
fn a_reviewer_is_one_identity_or_an_error_naming_the_choices() {
    let sam = person("Sam Lee", "sam@x.nz");
    let samantha = person("Samantha Ng", "samantha@x.nz");
    assert_eq!(
        pick_reviewer("sam@x.nz", &[sam.clone(), samantha.clone()]).unwrap(),
        sam
    );
    assert_eq!(
        pick_reviewer("samantha", std::slice::from_ref(&samantha)).unwrap(),
        samantha
    );
    let e = pick_reviewer("sam", &[sam, samantha])
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("Sam Lee <sam@x.nz>") && e.contains("Samantha Ng"),
        "{e}"
    );
    assert!(pick_reviewer("nobody", &[]).is_err());
}

#[test]
fn the_branch_suggestion_is_the_desktops() {
    assert_eq!(
        suggest_branch("jr", "Fix GST rounding"),
        "jr/fix-gst-rounding"
    );
    assert_eq!(suggest_branch("", "Fix GST rounding"), "fix-gst-rounding");
}

/// No GCM credential and no az: the error says how to sign in, GCM was
/// asked never to prompt, and nothing ran `az login`.
#[tokio::test]
async fn a_missing_credential_says_how_to_sign_in_and_never_prompts() {
    let runner = Fake::new()
        .fail("git", AuthError::NoCredential)
        .fail("az", AuthError::NoCredential);
    let tokens = Arc::new(TokenSource::new(runner.clone()));
    let client = AdoClient::new("https://dev.azure.com/tnt", tokens).silent();
    let e = client
        .get::<serde_json::Value>("/_apis/projects")
        .await
        .unwrap_err();
    let msg = ado_err(e).to_string();
    assert!(msg.contains(SIGN_IN_HINT), "{msg}");
    assert!(msg.contains("az login"), "{msg}");
    let args = runner.last_args("git");
    assert!(
        args.iter().any(|a| a == "credential.interactive=never"),
        "{args:?}"
    );
    assert_eq!(runner.count("az", "login"), 0);
    assert_eq!(runner.count("az", "get-access-token"), 1);
}

#[test]
fn other_failures_carry_no_sign_in_hint() {
    let msg = ado_err(AdoError::NotFound).to_string();
    assert!(!msg.contains("az login"), "{msg}");
}

fn item(id: u32, kind: &str, title: &str, parent: Option<u32>) -> WorkItem {
    WorkItem {
        id,
        kind: kind.into(),
        title: title.into(),
        state: "Active".into(),
        state_category: "InProgress".into(),
        assigned_to: Some("Jay Rich".into()),
        remaining: None,
        parent,
        board_column: None,
        description_md: String::new(),
        acceptance_md: String::new(),
        url: String::new(),
    }
}

#[test]
fn tasks_print_by_parent_with_the_next_step() {
    let list = TaskList {
        iteration: Some(Iteration {
            id: "i1".into(),
            name: "Sprint 42".into(),
            path: String::new(),
            start: None,
            finish: None,
            timeframe: None,
        }),
        stale: false,
        next: None,
        groups: vec![TaskGroup {
            parent: Some(item(100, "User Story", "GST on invoices", None)),
            items: vec![item(101, "Task", "Fix GST rounding", Some(100))],
        }],
        cached_at: None,
        cached_because: None,
        cached_reason: None,
    };
    let out = format_tasks(&list);
    assert!(out.starts_with("Sprint: Sprint 42\n"), "{out}");
    assert!(
        out.contains("User Story 100  [Active]  GST on invoices"),
        "{out}"
    );
    assert!(
        out.contains("  Task 101  [Active]  Fix GST rounding  (Jay Rich)"),
        "{out}"
    );
    assert!(out.contains("kete ado start-task <id>"), "{out}");
}

#[test]
fn a_saved_list_from_a_signed_out_org_says_how_to_sign_in() {
    let list = TaskList {
        iteration: None,
        stale: false,
        next: None,
        groups: vec![],
        cached_at: Some(1),
        cached_because: Some(ErrorKind::Auth),
        cached_reason: Some("signed out".into()),
    };
    let out = format_tasks(&list);
    assert!(
        out.contains("signed out") && out.contains(SIGN_IN_HINT),
        "{out}"
    );
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

fn temp_state(dir: &Path) -> AdoState {
    let store = Arc::new(kete_ado::store::AdoStore::open(dir.join("ado.json")));
    AdoState::new(store, Arc::new(TokenSource::new(Fake::new())))
}

#[tokio::test]
async fn the_context_comes_from_the_checkouts_ado_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("api");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://tnt@dev.azure.com/tnt/Platform/_git/api",
        ],
    );
    let sub = repo.join("src");
    std::fs::create_dir(&sub).unwrap();
    let ctx = context_at(&temp_state(tmp.path()), &sub).await.unwrap();
    assert_eq!(ctx.remote.org, "tnt");
    assert_eq!(ctx.remote.project, "Platform");
    assert_eq!(ctx.remote.repo, "api");
    assert_eq!(ctx.remote_name, "origin");

    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    git(&plain, &["init", "-q"]);
    let e = context_at(&temp_state(tmp.path()), &plain)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(e.contains("no Azure DevOps remote"), "{e}");
}

fn agent(id: &str, name: &str, role: AgentRole) -> Agent {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "worktree_id": "w1",
        "name": name,
        "status": serde_json::to_value(AgentStatus::Finished).unwrap(),
        "archived": false,
        "role": serde_json::to_value(role).unwrap(),
        "session_id": null,
        "sort_order": 0,
        "alive": true,
    }))
    .unwrap()
}

fn tree(agents: Vec<Agent>) -> Tree {
    Tree {
        worktrees: vec![],
        agents,
    }
}

fn spec(kind: &str) -> NewAgent {
    NewAgent {
        worktree: "w9".into(),
        kind: kind.into(),
        prompt: Some("  do it  ".into()),
        ..Default::default()
    }
}

#[test]
fn from_the_orchestrator_the_agent_is_its_child_in_the_new_worktree() {
    let cfg = nebula_config::Config::default();
    let t = tree(vec![agent("orch", "orchestrator", AgentRole::Orchestrator)]);
    let caller = AgentId("orch".into());
    let req = spawn_request(
        &cfg,
        &t,
        Some(&caller),
        54,
        &ProjectId("p1".into()),
        spec("codex"),
    )
    .unwrap();
    match req {
        ClientRequest::OrchestratorSpawnWith {
            caller,
            project,
            worktree,
            branch,
            kind,
            starting_prompt,
            ..
        } => {
            assert_eq!(caller.0, "orch");
            assert_eq!(project.0, "p1");
            assert_eq!(worktree, Some(WorktreeId("w9".into())));
            assert_eq!(branch, None);
            assert_eq!(kind, AgentKind::Codex);
            assert_eq!(starting_prompt, "do it");
        }
        other => panic!("{other:?}"),
    }
    let e = spawn_request(
        &cfg,
        &t,
        Some(&caller),
        53,
        &ProjectId("p1".into()),
        spec("codex"),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("kete reload"), "{e}");
}

#[test]
fn elsewhere_the_agent_is_a_plain_session_with_the_next_name() {
    let cfg = nebula_config::Config::default();
    let t = tree(vec![agent("a1", "agent-3", AgentRole::Worker)]);
    let worker = AgentId("a1".into());
    for caller in [None, Some(&worker)] {
        let req = spawn_request(
            &cfg,
            &t,
            caller,
            54,
            &ProjectId("p1".into()),
            spec("claude"),
        )
        .unwrap();
        match req {
            ClientRequest::CreateAgent {
                worktree,
                name,
                kind,
                auto_title,
                starting_prompt,
                role,
                ..
            } => {
                assert_eq!(worktree.0, "w9");
                assert_eq!(name, "agent-4");
                assert_eq!(kind, AgentKind::Claude);
                assert!(auto_title);
                assert_eq!(starting_prompt.as_deref(), Some("do it"));
                assert_eq!(role, AgentRole::Worker);
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(next_agent_name(&[]), "agent-1");
}

#[test]
fn no_harness_named_is_the_quick_prompts_and_an_unknown_one_is_refused() {
    let cfg = nebula_config::Config::default();
    assert_eq!(
        resolve_kind(&cfg, "").unwrap(),
        (cfg.quick_prompt_kind(), None)
    );
    assert_eq!(
        resolve_kind(&cfg, "codex").unwrap(),
        (AgentKind::Codex, None)
    );
    assert!(resolve_kind(&cfg, "no-such-harness").is_err());
}

#[test]
fn the_project_is_found_by_its_checkout_path() {
    let tmp = tempfile::tempdir().unwrap();
    let t = Tree {
        worktrees: vec![Worktree {
            id: WorktreeId("w1".into()),
            project_id: ProjectId("p1".into()),
            path: tmp.path().to_path_buf(),
            branch: "main".into(),
            is_main: true,
            sort_order: 0,
        }],
        agents: vec![],
    };
    let spelled = PathBuf::from(format!("{}/", tmp.path().display()));
    assert_eq!(
        project_for_checkout(&t, &spelled),
        Some(ProjectId("p1".into()))
    );
    assert_eq!(project_for_checkout(&t, &tmp.path().join("nope")), None);
}
