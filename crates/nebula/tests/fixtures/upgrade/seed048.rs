//! Seeds a Kete 0.48 data dir for the upgrade fixtures in
//! `crates/nebula/tests/fixtures/upgrade/` (see its README). Not built by
//! the workspace: copy it into a v0.48.0 checkout as
//! `crates/nebula/examples/seed048.rs`, add `rusqlite = { version = "0.40",
//! features = ["bundled"] }` to that crate's `[dev-dependencies]`, and run
//!
//!   cargo run --example seed048 -- <root> <old kete.exe> <fake agent exe>
//!
//! `<root>` must be empty: the repos, their origins, the data and runtime
//! dirs and a home all go under it, and NEBULA_* point there, so nothing
//! touches the user's own Kete. Commits are dated and authored the same on
//! every run (and the same as the test that rebuilds the repos), so their
//! SHAs, which the reviewed marks are scoped to, come out the same.

use nebula_core::codec::{read_frame, write_frame};
use nebula_core::{
    AgentId, AgentKind, AgentRole, ClientRequest, EntityId, ProjectId, ReorderScope, ServerEvent,
    WorktreeId,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Kete Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Kete Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo with one commit on `main`, pushed to a bare origin beside it.
fn make_repo(root: &Path, name: &str) -> PathBuf {
    let origin = root.join("origins").join(format!("{name}.git"));
    let repo = root.join("repos").join(name);
    std::fs::create_dir_all(&origin).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    git(&origin, &["init", "--bare", "-q", "-b", "main"]);
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), format!("# {name}\n")).unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    git(
        &repo,
        &["remote", "add", "origin", &origin.display().to_string()],
    );
    git(&repo, &["push", "-q", "-u", "origin", "main"]);
    repo
}

struct Client {
    stream: nebula_tui::platform::Stream,
    next: u64,
}

impl Client {
    async fn request(&mut self, make: impl FnOnce(u64) -> ClientRequest) -> Option<EntityId> {
        self.next += 1;
        let id = self.next;
        write_frame(&mut self.stream, &make(id)).await.unwrap();
        loop {
            match read_frame::<ServerEvent, _>(&mut self.stream).await.unwrap() {
                Some(ServerEvent::Ack { req_id, created }) if req_id == id => return created,
                Some(ServerEvent::Error {
                    req_id: Some(r),
                    message,
                }) if r == id => panic!("request {id}: {message}"),
                Some(_) => {}
                None => panic!("daemon hung up"),
            }
        }
    }

    async fn send(&mut self, req: ClientRequest) {
        write_frame(&mut self.stream, &req).await.unwrap();
    }
}

fn kete(exe: &Path, args: &[&str]) {
    let out = Command::new(exe).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "kete {args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(&args[0]);
    let kete_exe = PathBuf::from(&args[1]);
    let agent_exe = &args[2];
    let data = root.join("data");
    let run = root.join("rt");
    let home = root.join("home");
    for d in [&data, &run, &home] {
        std::fs::create_dir_all(d).unwrap();
    }
    // Ours and every child's: the old kete CLI, its daemon, git.
    std::env::set_var("NEBULA_DATA_DIR", &data);
    std::env::set_var("NEBULA_RUNTIME_DIR", &run);
    std::env::set_var("NEBULA_UPDATE_CHECK_SECS", "0");
    std::env::set_var("NEBULA_WORKTREE_SYNC_MS", "200");
    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);
    std::env::set_var("GIT_TERMINAL_PROMPT", "0");
    std::env::set_var("GCM_INTERACTIVE", "never");
    for var in ["NEBULA_AGENT_ID", "NEBULA_API_URL", "NEBULA_API_TOKEN"] {
        std::env::remove_var(var);
    }

    let alpha = make_repo(&root, "alpha");
    let beta = make_repo(&root, "beta");

    // The settings a 0.48 user had made, written by 0.48's own config code.
    let (cfg_path, cfg_local) = nebula_config::Config::layer_paths();
    nebula_config::Config::update_in(&cfg_path, &cfg_local, |c| {
        c.keybindings = [
            // A plain rebind, and one onto a key 0.49 gave another verb by
            // default (`z`, full screen).
            ("quick_prompt", "ctrl+n"),
            ("screenshot", "z"),
            // Help moved from `?` to F1 in 0.49; theirs stays theirs.
            ("help", "f12"),
            // A deliberate unbind, and a list with a comma chord.
            ("archive", ""),
            ("rename", "f2, r"),
            ("reorder_up", "alt+k"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        c.theme = "default".into();
        c.worktree_base_branch = "main".into();
    })
    .unwrap();
    // The fake agent as the `claude` harness's program, and the desktop
    // window's own preferences beside the terminal app's.
    let path = data.join("config.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["harnesses"] = serde_json::json!({ "claude": { "program": agent_exe } });
    json["desktop"] = serde_json::json!({ "theme": "dark", "home": "board", "collapsed": ["alpha"] });
    std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    // The daemon first, its handles all null: one `kete add` spawned would
    // inherit that command's stdout pipe and hold our read of it open.
    Command::new(&kete_exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pipe = run.join("daemon.sock");
    for _ in 0..100 {
        if nebula_tui::platform::connect(&pipe).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Projects and accounts through the 0.48 CLI.
    kete(&kete_exe, &["add", &alpha.display().to_string()]);
    kete(&kete_exe, &["add", &beta.display().to_string()]);
    kete(&kete_exe, &["profile", "add", "work", "--harness", "claude", "--default"]);
    kete(
        &kete_exe,
        &["profile", "add", "spare", "--harness", "claude", "--env", "FIXTURE=1"],
    );
    kete(&kete_exe, &["profile", "disable", "spare"]);
    kete(
        &kete_exe,
        &["profile", "set-project", &beta.display().to_string(), "spare"],
    );

    let conn = nebula_tui::ipc::connect_or_spawn().await.unwrap();
    let mut c = Client {
        stream: conn.stream,
        next: 100,
    };
    c.send(ClientRequest::Subscribe).await;
    let (projects, worktrees) = loop {
        if let Some(ServerEvent::Snapshot {
            projects, worktrees, ..
        }) = read_frame::<ServerEvent, _>(&mut c.stream).await.unwrap()
        {
            break (projects, worktrees);
        }
    };
    let pid = |repo: &Path| -> ProjectId {
        projects
            .iter()
            .find(|p| p.repo_path.ends_with(repo.file_name().unwrap()))
            .unwrap()
            .id
            .clone()
    };
    let (alpha_id, beta_id) = (pid(&alpha), pid(&beta));
    let root_wt = |p: &ProjectId| -> WorktreeId {
        worktrees
            .iter()
            .find(|w| &w.project_id == p && w.is_main)
            .unwrap()
            .id
            .clone()
    };
    let alpha_main = root_wt(&alpha_id);
    let beta_main = root_wt(&beta_id);

    async fn wt(c: &mut Client, project: &ProjectId, branch: &str) -> WorktreeId {
        let project = project.clone();
        let branch = branch.to_string();
        match c
            .request(|req_id| ClientRequest::CreateWorktree {
                req_id,
                project,
                branch,
                base: None,
            })
            .await
        {
            Some(EntityId::Worktree(id)) => id,
            other => panic!("worktree: {other:?}"),
        }
    }
    let login = wt(&mut c, &alpha_id, "feat/login").await;
    let typo = wt(&mut c, &alpha_id, "fix-typo").await;
    let spike = wt(&mut c, &beta_id, "spike").await;

    async fn agent(
        c: &mut Client,
        worktree: &WorktreeId,
        name: &str,
        profile: Option<&str>,
    ) -> AgentId {
        let worktree = worktree.clone();
        let name = name.to_string();
        let profile = profile.map(str::to_string);
        match c
            .request(|req_id| ClientRequest::CreateAgent {
                req_id,
                worktree,
                name,
                kind: AgentKind::Claude,
                custom_harness: None,
                model: None,
                effort: None,
                auto_title: false,
                cloud_prompt: None,
                starting_prompt: None,
                issue_url: None,
                role: AgentRole::default(),
                profile,
            })
            .await
        {
            Some(EntityId::Agent(id)) => id,
            other => panic!("agent: {other:?}"),
        }
    }
    let root_chat = agent(&mut c, &alpha_main, "root chat", None).await;
    let form = agent(&mut c, &login, "login form", None).await;
    let tests = agent(&mut c, &login, "login tests", Some("work")).await;
    let typo_agent = agent(&mut c, &typo, "typo", None).await;
    let spike_agent = agent(&mut c, &spike, "spike it", Some("spare")).await;
    let _beta_chat = agent(&mut c, &beta_main, "beta chat", None).await;

    // Let the sessions start: the daemon writes Claude's hooks into each
    // checkout first.
    tokio::time::sleep(Duration::from_secs(3)).await;

    c.request(|req_id| ClientRequest::RenameAgent {
        req_id,
        id: form.clone(),
        name: "login form (renamed)".into(),
    })
    .await;
    c.request(|req_id| ClientRequest::ArchiveAgent {
        req_id,
        id: typo_agent.clone(),
    })
    .await;
    // The user's order: beta's tab first, fix-typo above feat/login, and
    // the tests session above the form.
    c.request(|req_id| ClientRequest::Reorder {
        req_id,
        scope: ReorderScope::Projects,
        ids: vec![beta_id.to_string(), alpha_id.to_string()],
    })
    .await;
    c.request(|req_id| ClientRequest::Reorder {
        req_id,
        scope: ReorderScope::Worktrees(alpha_id.clone()),
        ids: vec![alpha_main.to_string(), typo.to_string(), login.to_string()],
    })
    .await;
    c.request(|req_id| ClientRequest::Reorder {
        req_id,
        scope: ReorderScope::Agents(login.clone()),
        ids: vec![tests.to_string(), form.to_string()],
    })
    .await;
    c.send(ClientRequest::MarkAgentSeen {
        id: root_chat.clone(),
    })
    .await;
    c.send(ClientRequest::SaveUiState {
        json: serde_json::json!({
            "project": alpha_id.to_string(),
            "worktree": login.to_string(),
            "session_agent": tests.to_string(),
            "show_archived": true,
            "collapsed": false,
            "diff_tree": true,
            "launcher_tabs": [beta_id.to_string(), alpha_id.to_string()],
        })
        .to_string(),
    })
    .await;
    let _ = spike_agent;

    // Before 0.52 an agent's `git add -A` committed the hooks file Kete
    // wrote: the feat/login checkout now tracks it.
    let login_dir = worktrees_dir(&alpha).join("feat-login");
    assert!(login_dir.join(".claude/settings.local.json").exists());
    std::fs::write(login_dir.join("login.rs"), "fn login() {}\n").unwrap();
    git(&login_dir, &["add", "-A"]);
    git(&login_dir, &["commit", "-q", "-m", "login"]);

    // Reviewed marks in the root checkout, made against its HEAD.
    std::fs::write(alpha.join("README.md"), "# alpha\n\nedited\n").unwrap();
    let head = git(&alpha, &["rev-parse", "HEAD"]);
    let marks = HashMap::from([("README.md".to_string(), 0x1234_5678_u64)]);
    nebula_tui::review::store_marks(&alpha, &head, &marks);

    tokio::time::sleep(Duration::from_secs(1)).await;
    drop(c);
    kete(&kete_exe, &["kill"]);
    tokio::time::sleep(Duration::from_secs(1)).await;

    dump_db(&data.join("nebula.db"), &root.join("kete.sql"));
    println!("seeded {}", root.display());
}

fn worktrees_dir(repo: &Path) -> PathBuf {
    repo.parent()
        .unwrap()
        .join(format!("{}-worktrees", repo.file_name().unwrap().to_string_lossy()))
}

/// The database as SQL text: schema, rows, then indexes and triggers, and
/// its `user_version` last.
fn dump_db(db: &Path, out: &Path) {
    use rusqlite::types::ValueRef;
    let conn = rusqlite::Connection::open(db).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    let mut text = String::from("PRAGMA foreign_keys=OFF;\nBEGIN;\n");
    let mut objects: Vec<(String, String, String)> = conn
        .prepare("SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY rowid")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    objects.sort_by_key(|(kind, _, _)| kind != "table");
    for (kind, name, sql) in &objects {
        text.push_str(sql);
        text.push_str(";\n");
        if kind != "table" {
            continue;
        }
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{name}\"")).unwrap();
        let cols = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let vals: Vec<String> = (0..cols)
                .map(|i| match row.get_ref(i).unwrap() {
                    ValueRef::Null => "NULL".into(),
                    ValueRef::Integer(n) => n.to_string(),
                    ValueRef::Real(f) => f.to_string(),
                    ValueRef::Text(t) => {
                        format!("'{}'", String::from_utf8_lossy(t).replace('\'', "''"))
                    }
                    ValueRef::Blob(b) => {
                        format!("X'{}'", b.iter().map(|x| format!("{x:02x}")).collect::<String>())
                    }
                })
                .collect();
            text.push_str(&format!("INSERT INTO \"{name}\" VALUES({});\n", vals.join(",")));
        }
    }
    text.push_str(&format!("PRAGMA user_version={version};\nCOMMIT;\n"));
    std::fs::write(out, text).unwrap();
}
