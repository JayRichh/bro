//! `kete ado` against a loopback mock of Azure DevOps (`KETE_ADO_BASE`, a
//! debug-build-only switch that also hands every request a dummy token, so
//! no real credential or request is ever involved), in a throwaway repo
//! whose `origin` is an Azure DevOps remote, with throwaway data and
//! runtime dirs.

use httpmock::prelude::*;
use nebula_core::env::{CONFIG_FILE, DATA_DIR, RUNTIME_DIR};
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};

struct Sandbox {
    _tmp: tempfile::TempDir,
    data: std::path::PathBuf,
    runtime: std::path::PathBuf,
    repo: std::path::PathBuf,
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

fn sandbox(remote: Option<&str>) -> Sandbox {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let runtime = tmp.path().join("run");
    let repo = tmp.path().join("api");
    for d in [&data, &runtime, &repo] {
        std::fs::create_dir_all(d).unwrap();
    }
    git(&repo, &["init", "-q"]);
    if let Some(url) = remote {
        git(&repo, &["remote", "add", "origin", url]);
    }
    Sandbox {
        _tmp: tmp,
        data,
        runtime,
        repo,
    }
}

fn kete(s: &Sandbox, base: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kete"));
    cmd.args(args)
        .current_dir(&s.repo)
        .env(DATA_DIR, &s.data)
        .env(RUNTIME_DIR, &s.runtime)
        .env_remove(CONFIG_FILE)
        .env_remove("NEBULA_AGENT_ID")
        .env("GIT_TERMINAL_PROMPT", "0");
    match base {
        Some(b) => cmd.env("KETE_ADO_BASE", b),
        None => cmd.env_remove("KETE_ADO_BASE"),
    };
    cmd.output().expect("failed to run kete")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

const REMOTE: &str = "https://tnt@dev.azure.com/tnt/Platform/_git/api";

fn pr_json(id: u32, draft: bool) -> Value {
    json!({
        "pullRequestId": id,
        "title": "Fix GST rounding",
        "sourceRefName": "refs/heads/jr/fix-gst",
        "targetRefName": "refs/heads/main",
        "isDraft": draft,
        "status": "active",
        "createdBy": {"displayName": "Jay Rich"},
        "reviewers": [{"id": "r1", "displayName": "Sam Lee", "vote": 10, "isRequired": true}],
        "repository": {"name": "api", "project": {"name": "Platform"}},
    })
}

/// The repo and identity lookups every PR command starts with.
fn ids(s: &MockServer) {
    s.mock(|w, t| {
        w.method(GET)
            .path("/tnt/Platform/_apis/git/repositories/api");
        t.status(200)
            .json_body(json!({"id": "repo-1", "project": {"id": "proj-1"}}));
    });
    s.mock(|w, t| {
        w.method(GET).path("/tnt/_apis/connectionData");
        t.status(200)
            .json_body(json!({"authenticatedUser": {"id": "me-1"}}));
    });
}

#[test]
fn prs_lists_mine_and_my_reviews_as_json_and_text() {
    let server = MockServer::start();
    ids(&server);
    server.mock(|w, t| {
        w.method(GET)
            .path("/tnt/_apis/git/repositories/repo-1/pullrequests")
            .query_param("searchCriteria.creatorId", "me-1");
        t.status(200)
            .json_body(json!({"value": [pr_json(7, true)]}));
    });
    server.mock(|w, t| {
        w.method(GET)
            .path("/tnt/_apis/git/repositories/repo-1/pullrequests")
            .query_param("searchCriteria.reviewerId", "me-1");
        t.status(200).json_body(json!({"value": []}));
    });
    let s = sandbox(Some(REMOTE));
    let base = server.base_url();

    let out = kete(&s, Some(&base), &["ado", "prs", "--json"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["mine"][0]["id"], 7);
    assert_eq!(v["mine"][0]["source"], "jr/fix-gst");
    assert_eq!(v["mine"][0]["isDraft"], true);
    assert_eq!(v["review"], json!([]));

    let out = kete(&s, Some(&base), &["ado", "prs"]);
    let stdout = text(&out.stdout);
    assert!(stdout.contains("Mine (1)"), "{stdout}");
    assert!(
        stdout.contains("!7 draft jr/fix-gst -> main  Fix GST rounding  [Sam Lee approved]"),
        "{stdout}"
    );
    assert!(stdout.contains("Waiting on my review (0)"), "{stdout}");
}

#[test]
fn pr_shows_detail_policies_and_threads_and_vote_and_reply_post() {
    let server = MockServer::start();
    ids(&server);
    server.mock(|w, t| {
        w.method(GET)
            .path("/tnt/_apis/git/repositories/repo-1/pullrequests/7");
        t.status(200).json_body(pr_json(7, false));
    });
    server.mock(|w, t| {
        w.method(GET)
            .path("/tnt/_apis/git/repositories/repo-1/pullrequests/7/threads");
        t.status(200).json_body(json!({"value": [{
            "id": 3,
            "status": "active",
            "threadContext": {"filePath": "/src/gst.rs", "rightFileStart": {"line": 12}},
            "comments": [{"author": {"displayName": "Sam Lee"}, "content": "Round half up?", "publishedDate": "2026-10-14T00:00:00Z", "commentType": "text"}]
        }]}));
    });
    server.mock(|w, t| {
        w.method(GET)
            .path("/tnt/_apis/git/repositories/repo-1/pullrequests/7/workitems");
        t.status(200).json_body(json!({"value": [{"id": "4211"}]}));
    });
    server.mock(|w, t| {
        w.method(GET).path("/tnt/proj-1/_apis/policy/evaluations");
        t.status(200).json_body(json!({"value": [{
            "status": "approved",
            "configuration": {"isBlocking": true, "type": {"displayName": "Minimum number of reviewers"}}
        }]}));
    });
    let vote = server.mock(|w, t| {
        w.method(PUT)
            .path("/tnt/_apis/git/repositories/repo-1/pullrequests/7/reviewers/me-1")
            .json_body_partial(r#"{"vote": 10}"#);
        t.status(200).json_body(json!({"vote": 10}));
    });
    let reply = server.mock(|w, t| {
        w.method(POST)
            .path("/tnt/_apis/git/repositories/repo-1/pullrequests/7/threads/3/comments")
            .json_body_partial(r#"{"content": "fixed in the latest push"}"#);
        t.status(200).json_body(json!({
            "author": {"displayName": "Jay Rich"},
            "content": "fixed in the latest push",
            "publishedDate": "2026-10-14T01:00:00Z"
        }));
    });
    let s = sandbox(Some(REMOTE));
    let base = server.base_url();

    let out = kete(&s, Some(&base), &["ado", "pr", "7", "--json"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["pr"]["id"], 7);
    assert_eq!(v["threads"][0]["id"], 3);
    assert_eq!(v["threads"][0]["comments"][0]["text"], "Round half up?");
    assert_eq!(v["policies"][0]["name"], "Minimum number of reviewers");
    assert_eq!(v["policies"][0]["state"], "approved");

    let out = kete(&s, Some(&base), &["ado", "pr", "7"]);
    let stdout = text(&out.stdout);
    assert!(stdout.contains("!7 Fix GST rounding  (active)"), "{stdout}");
    assert!(stdout.contains("thread 3 [active]"), "{stdout}");
    assert!(stdout.contains("Sam Lee: Round half up?"), "{stdout}");

    let out = kete(&s, Some(&base), &["ado", "vote", "7", "approve"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("Voted approved on !7."));
    vote.assert();

    let out = kete(
        &s,
        Some(&base),
        &[
            "ado", "reply", "7", "3", "fixed", "in", "the", "latest", "push",
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    reply.assert();
}

#[test]
fn a_checkout_without_an_ado_remote_is_told_so() {
    let s = sandbox(Some("https://github.com/JayRichh/bro.git"));
    let out = kete(&s, Some("http://127.0.0.1:9"), &["ado", "tasks"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("no Azure DevOps remote"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn pr_create_refuses_without_a_task_or_reviewer_before_pushing() {
    let server = MockServer::start();
    let s = sandbox(Some(REMOTE));
    git(&s.repo, &["checkout", "-q", "-b", "jr/fix-gst"]);
    let out = kete(&s, Some(&server.base_url()), &["ado", "pr", "create"]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(
        err.contains("--task") && err.contains("--reviewer"),
        "{err}"
    );
}
