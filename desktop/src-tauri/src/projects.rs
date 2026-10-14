//! Projects and worktrees from the window: a new worktree for any repo
//! (a typed branch, slugified as the TUI does, or a made-up name), delete
//! a linked worktree, rename, relocate or remove a project, open a
//! checkout with the project's open command, and open the repo's remote
//! page in the browser.

use crate::daemon::{Client, RpcTimeout};
use crate::sessions::ack;
use crate::{err, Res};
use nebula_core::branch_name;
use nebula_core::project_file::{self, ProjectCommand};
use nebula_core::{ClientRequest, EntityId, ProjectId, WorktreeId};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

/// How long a new worktree may take (a fetch and `git worktree add` of a
/// big repo) before the window stops waiting.
const CREATE_WORKTREE_TIMEOUT: std::time::Duration = crate::ado::tasks::CREATE_WORKTREE_TIMEOUT;

/// The branches a project's worktrees are on: names a new one can't take.
fn taken(client: &Client, project: &str) -> Vec<String> {
    client
        .snapshot()
        .worktrees
        .iter()
        .filter(|w| w.project_id.0 == project)
        .map(|w| w.branch.clone())
        .collect()
}

/// A made-up branch name no worktree of `project` is on, for the new
/// worktree box to offer (the TUI's prompt does the same).
pub fn suggest_branch_in(client: &Client, project: &str) -> String {
    branch_name::random_name(&taken(client, project))
}

/// The branch a typed name becomes: whitespace runs hyphenated, and
/// nothing typed takes a made-up name.
pub fn branch_for(typed: &str, taken: &[String]) -> String {
    let slug = branch_name::slugify(typed);
    if slug.is_empty() {
        branch_name::random_name(taken)
    } else {
        slug
    }
}

/// Cut a worktree on `branch` (typed, or empty for a made-up name) from
/// `base` (empty: the daemon's default base). The new worktree's id.
pub async fn create_worktree_in(
    client: &Client,
    project: String,
    branch: String,
    base: Option<String>,
) -> Res<String> {
    if !client.snapshot().projects.iter().any(|p| p.id.0 == project) {
        return Err("That repo is gone.".into());
    }
    let taken = taken(client, &project);
    let branch = branch_for(&branch, &taken);
    if taken.contains(&branch) {
        return Err(format!("A worktree is already on {branch}."));
    }
    let base = base.map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
    let req_id = client.req_id();
    let created = client
        .rpc_within(
            req_id,
            ClientRequest::CreateWorktree {
                req_id,
                project: ProjectId(project.clone()),
                branch: branch.clone(),
                base,
            },
            CREATE_WORKTREE_TIMEOUT,
        )
        .await;
    match created {
        Ok(Some(EntityId::Worktree(id))) => Ok(id.0),
        Ok(_) => Err("The daemon did not report the new worktree.".into()),
        // Still going: the row shows up when it is done. Use it if it
        // already has.
        Err(e) if e.is::<RpcTimeout>() => client
            .snapshot()
            .worktrees
            .iter()
            .find(|w| w.project_id.0 == project && w.branch == branch)
            .map(|w| w.id.0.clone())
            .ok_or_else(|| {
                format!(
                    "The worktree for {branch} is still being made; it appears when it is done."
                )
            }),
        Err(e) => Err(e.to_string()),
    }
}

pub async fn delete_worktree_in(client: &Client, worktree: String, force: bool) -> Res<()> {
    let model = client.snapshot();
    let Some(w) = model.worktrees.iter().find(|w| w.id.0 == worktree) else {
        return Err("That worktree is gone.".into());
    };
    if w.is_main {
        return Err("The repo's own checkout is not a worktree to delete.".into());
    }
    ack(client, |req_id| ClientRequest::DeleteWorktree {
        req_id,
        id: WorktreeId(worktree),
        force,
    })
    .await
    .map(drop)
}

/// Retitle a project's row; an empty name puts the folder's name back.
pub async fn rename_project_in(client: &Client, project: String, name: String) -> Res<()> {
    ack(client, |req_id| ClientRequest::RenameProject {
        req_id,
        id: ProjectId(project),
        name: name.trim().to_string(),
    })
    .await
    .map(drop)
}

/// Forget a project. Nothing on disk is touched.
pub async fn remove_project_in(client: &Client, project: String) -> Res<()> {
    ack(client, |req_id| ClientRequest::RemoveProject {
        req_id,
        id: ProjectId(project),
    })
    .await
    .map(drop)
}

/// Point a project at the folder its repo now lives in.
pub async fn set_project_path_in(client: &Client, project: String, path: String) -> Res<()> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Give the folder the repo lives in now.".into());
    }
    ack(client, |req_id| ClientRequest::SetProjectPath {
        req_id,
        id: ProjectId(project),
        path: PathBuf::from(path),
    })
    .await
    .map(drop)
}

/// A worktree's checkout and its project's main one.
fn checkout(client: &Client, worktree: &str) -> Res<(PathBuf, PathBuf)> {
    let model = client.snapshot();
    let w = model
        .worktrees
        .iter()
        .find(|w| w.id.0 == worktree)
        .ok_or_else(|| "That worktree is gone.".to_string())?;
    let main = model
        .projects
        .iter()
        .find(|p| p.id == w.project_id)
        .map_or_else(|| w.path.clone(), |p| p.repo_path.clone());
    Ok((w.path.clone(), main))
}

/// The OPEN COMMAND for a checkout, as the TUI resolves it: the project's
/// Open command setting, else the checkout's `.kete.json` `open`, else the
/// main checkout's.
pub fn open_command_for(
    cfg: &nebula_config::Config,
    worktree: &Path,
    main: &Path,
) -> Res<OpenCommand> {
    let setting = cfg.project(main).open_command;
    let setting = setting.trim();
    if !setting.is_empty() {
        return Ok(OpenCommand {
            command: setting.to_string(),
            from_file: false,
        });
    }
    project_file::lookup(worktree, main, ProjectCommand::Open)?
        .map(|command| OpenCommand {
            command,
            from_file: true,
        })
        .ok_or_else(|| {
            "No open command for this repo: set one in Settings, Project, or add \
             .kete.json with {\"open\": \"code .\"}."
                .to_string()
        })
}

/// An open command, and whether it came from a checkout's `.kete.json`
/// (anyone who can push to the repo wrote it) rather than the user's own
/// setting.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenCommand {
    pub command: String,
    pub from_file: bool,
}

/// What Open in editor did: ran `command`, or stopped to ask first.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOutcome {
    pub command: String,
    /// False: the command comes from the repo's .kete.json and hasn't been
    /// allowed for this repo yet; the window shows it and asks.
    pub opened: bool,
}

/// Whether Open in editor may run now, for the repo at `main`: a command
/// from the user's own setting always may; one from a .kete.json only once
/// allowed for this repo. `approve` allows it, and the approval is kept in
/// the window's prefs in the layers at `path` / `local`, so the same
/// command doesn't ask again (a changed one does).
pub fn open_allowed(
    path: &Path,
    local: &Path,
    main: &Path,
    open: &OpenCommand,
    approve: bool,
) -> Res<bool> {
    use nebula_config::desktop::DesktopPrefs;
    if !open.from_file {
        return Ok(true);
    }
    let repo = main.to_string_lossy().into_owned();
    if DesktopPrefs::load_at(path, local).open_approved(&repo, &open.command) {
        return Ok(true);
    }
    if !approve {
        return Ok(false);
    }
    DesktopPrefs::update_in(path, local, |p| {
        p.approve_open(&repo, &open.command);
        Ok(())
    })?;
    Ok(true)
}

/// Start `command` through the user's shell in `cwd`, with no console
/// window and nothing attached; a failure to start is the caller's.
fn spawn_open_command(command: &str, cwd: &Path) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let shell = nebula_core::shell::user_shell();
    #[cfg(windows)]
    let mut cmd = {
        let args = nebula_core::shell::shell_line_args(&shell, command);
        let launch = nebula_core::shell::launch(&shell, &args).map_err(std::io::Error::other)?;
        let mut cmd = Command::new(&launch.program);
        cmd.args(&launch.args)
            .envs(launch.env.iter().map(|(k, v)| (k, v)));
        nebula_core::platform::no_window(&mut cmd);
        cmd
    };
    #[cfg(unix)]
    let mut cmd = {
        let mut cmd = Command::new(shell);
        cmd.arg("-c").arg(command);
        cmd
    };
    let mut child = cmd
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The page `root`'s remote lives at: `origin`, else the first remote git
/// lists, made a browser address (credentials dropped).
pub fn remote_page(root: &Path) -> Res<String> {
    let remote = crate::git::remote_url(root)?;
    nebula_core::remote::web_url(&remote)
        .ok_or_else(|| format!("The remote has no web page: {remote}"))
}

#[tauri::command]
pub fn suggest_branch(client: State<'_, Arc<Client>>, project: String) -> String {
    suggest_branch_in(&client, &project)
}

#[tauri::command]
pub async fn create_worktree(
    client: State<'_, Arc<Client>>,
    project: String,
    branch: String,
    base: Option<String>,
) -> Res<String> {
    create_worktree_in(&client, project, branch, base).await
}

#[tauri::command]
pub async fn delete_worktree(
    client: State<'_, Arc<Client>>,
    worktree: String,
    force: bool,
) -> Res<()> {
    delete_worktree_in(&client, worktree, force).await
}

#[tauri::command]
pub async fn rename_project(
    client: State<'_, Arc<Client>>,
    project: String,
    name: String,
) -> Res<()> {
    rename_project_in(&client, project, name).await
}

#[tauri::command]
pub async fn remove_project(client: State<'_, Arc<Client>>, project: String) -> Res<()> {
    remove_project_in(&client, project).await
}

#[tauri::command]
pub async fn set_project_path(
    client: State<'_, Arc<Client>>,
    project: String,
    path: String,
) -> Res<()> {
    set_project_path_in(&client, project, path).await
}

/// Run the worktree's open command (an editor, usually); the command run.
#[tauri::command]
pub async fn open_worktree(
    client: State<'_, Arc<Client>>,
    worktree: String,
    approve: Option<bool>,
) -> Res<OpenOutcome> {
    let (path, main) = checkout(&client, &worktree)?;
    let open = open_command_for(&nebula_config::Config::load(), &path, &main)?;
    let (config, local) = nebula_config::layer_paths();
    if !open_allowed(&config, &local, &main, &open, approve.unwrap_or(false))? {
        return Ok(OpenOutcome {
            command: open.command,
            opened: false,
        });
    }
    let command = open.command;
    spawn_open_command(&command, &path).map_err(|e| format!("Could not run {command}: {e}"))?;
    Ok(OpenOutcome {
        command,
        opened: true,
    })
}

/// Open the repo's remote page in the browser; the address opened.
#[tauri::command]
pub async fn open_remote(
    app: tauri::AppHandle,
    client: State<'_, Arc<Client>>,
    worktree: String,
) -> Res<String> {
    let (path, _) = checkout(&client, &worktree)?;
    let url = tauri::async_runtime::spawn_blocking(move || remote_page(&path))
        .await
        .map_err(err)??;
    app.opener()
        .open_url(url.clone(), None::<&str>)
        .map_err(err)?;
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::testkit::{ok, run};
    use nebula_core::{Project, Worktree};

    fn model_with(client: &Client) {
        client.set_model(|m| {
            m.projects = vec![Project {
                id: ProjectId("p1".into()),
                name: "repo".into(),
                repo_path: PathBuf::from("/repo"),
                sort_order: 0,
            }];
            m.worktrees = vec![
                Worktree {
                    id: WorktreeId("w1".into()),
                    project_id: ProjectId("p1".into()),
                    path: PathBuf::from("/repo"),
                    branch: "main".into(),
                    is_main: true,
                    sort_order: 0,
                },
                Worktree {
                    id: WorktreeId("w2".into()),
                    project_id: ProjectId("p1".into()),
                    path: PathBuf::from("/repo-wt"),
                    branch: "fix-login".into(),
                    is_main: false,
                    sort_order: 1,
                },
            ];
        });
    }

    #[test]
    fn a_typed_branch_is_slugified_and_nothing_typed_is_made_up() {
        assert_eq!(
            branch_for("  fix login  redirect ", &[]),
            "fix-login-redirect"
        );
        let made = branch_for("  ", &["main".into()]);
        assert_eq!(made.split('-').count(), 3, "{made}");
    }

    #[tokio::test]
    async fn a_new_worktree_sends_the_slug_and_base_and_returns_its_id() {
        let (client, mut rx) = Client::connected_for_test();
        model_with(&client);
        let call = create_worktree_in(
            &client,
            "p1".into(),
            "add gst".into(),
            Some(" origin/main ".into()),
        );
        let (r, seen) = run(&client, &mut rx, call, |_| {
            Ok(Some(EntityId::Worktree(WorktreeId("w9".into()))))
        })
        .await;
        assert_eq!(r.unwrap(), "w9");
        assert!(
            matches!(&seen[0], ClientRequest::CreateWorktree { project, branch, base, .. }
            if project.0 == "p1" && branch == "add-gst" && base.as_deref() == Some("origin/main"))
        );

        // A branch a worktree is on, and a gone repo, are refused unsent.
        let (r, seen) = run(
            &client,
            &mut rx,
            create_worktree_in(&client, "p1".into(), "fix login".into(), None),
            ok,
        )
        .await;
        assert!(r.unwrap_err().contains("fix-login"));
        assert!(seen.is_empty());
        let (r, seen) = run(
            &client,
            &mut rx,
            create_worktree_in(&client, "p9".into(), "x".into(), None),
            ok,
        )
        .await;
        assert!(r.is_err() && seen.is_empty());
    }

    #[tokio::test]
    async fn worktree_and_project_verbs_send_their_requests() {
        let (client, mut rx) = Client::connected_for_test();
        model_with(&client);
        let (r, seen) = run(
            &client,
            &mut rx,
            delete_worktree_in(&client, "w2".into(), true),
            ok,
        )
        .await;
        r.unwrap();
        assert!(
            matches!(&seen[0], ClientRequest::DeleteWorktree { id, force: true, .. } if id.0 == "w2")
        );
        let (r, seen) = run(
            &client,
            &mut rx,
            delete_worktree_in(&client, "w1".into(), false),
            ok,
        )
        .await;
        assert!(r.is_err() && seen.is_empty(), "never the main checkout");

        let (r, seen) = run(
            &client,
            &mut rx,
            rename_project_in(&client, "p1".into(), " Ledger ".into()),
            ok,
        )
        .await;
        r.unwrap();
        assert!(
            matches!(&seen[0], ClientRequest::RenameProject { id, name, .. } if id.0 == "p1" && name == "Ledger")
        );

        let (r, seen) = run(
            &client,
            &mut rx,
            remove_project_in(&client, "p1".into()),
            ok,
        )
        .await;
        r.unwrap();
        assert!(matches!(&seen[0], ClientRequest::RemoveProject { id, .. } if id.0 == "p1"));

        let (r, seen) = run(
            &client,
            &mut rx,
            set_project_path_in(&client, "p1".into(), " /moved ".into()),
            ok,
        )
        .await;
        r.unwrap();
        assert!(
            matches!(&seen[0], ClientRequest::SetProjectPath { id, path, .. } if id.0 == "p1" && path == Path::new("/moved"))
        );
        let (r, seen) = run(
            &client,
            &mut rx,
            set_project_path_in(&client, "p1".into(), "  ".into()),
            ok,
        )
        .await;
        assert!(r.is_err() && seen.is_empty());
    }

    #[test]
    fn a_suggested_branch_avoids_every_worktree_branch() {
        let (client, _rx) = Client::connected_for_test();
        model_with(&client);
        let name = suggest_branch_in(&client, "p1");
        assert!(name != "main" && name != "fix-login");
    }

    #[test]
    fn the_open_command_comes_from_the_setting_then_the_project_file() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("repo");
        let wt = dir.path().join("repo-wt");
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        let cfg = nebula_config::Config::default();
        assert!(open_command_for(&cfg, &wt, &main)
            .unwrap_err()
            .contains("Settings"));
        std::fs::write(main.join(".kete.json"), r#"{"open": "code ."}"#).unwrap();
        let file = open_command_for(&cfg, &wt, &main).unwrap();
        assert_eq!(file.command, "code .");
        assert!(file.from_file);
        let mut cfg = nebula_config::Config::default();
        cfg.set_project_text(&main, nebula_config::SettingKind::OpenCommand, "cursor .");
        let setting = open_command_for(&cfg, &wt, &main).unwrap();
        assert_eq!(setting.command, "cursor .");
        assert!(!setting.from_file);
    }

    #[test]
    fn a_repo_files_open_command_asks_once_per_repo_and_command() {
        let dir = tempfile::tempdir().unwrap();
        let (path, local) = (
            dir.path().join("config.json"),
            dir.path().join("config.local.json"),
        );
        let main = dir.path().join("repo");
        let other = dir.path().join("other");
        let open = |command: &str, from_file| OpenCommand {
            command: command.into(),
            from_file,
        };
        // The user's own setting never asks.
        assert!(open_allowed(&path, &local, &main, &open("code .", false), false).unwrap());
        assert!(!path.exists(), "nothing recorded for a setting");
        // A repo file's asks until allowed, then not again.
        let file = open("code .", true);
        assert!(!open_allowed(&path, &local, &main, &file, false).unwrap());
        assert!(open_allowed(&path, &local, &main, &file, true).unwrap());
        assert!(open_allowed(&path, &local, &main, &file, false).unwrap());
        // Another repo, or a changed command, asks again.
        assert!(!open_allowed(&path, &local, &other, &file, false).unwrap());
        assert!(!open_allowed(&path, &local, &main, &open("make open", true), false).unwrap());
    }

    #[test]
    fn the_remote_page_drops_credentials_and_needs_a_web_remote() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        assert!(remote_page(dir.path()).is_err(), "no remote at all");
        git(&[
            "remote",
            "add",
            "upstream",
            "https://x:tok@github.com/o/r.git",
        ]);
        assert_eq!(remote_page(dir.path()).unwrap(), "https://github.com/o/r");
        git(&["remote", "add", "origin", "git@github.com:me/r.git"]);
        assert_eq!(remote_page(dir.path()).unwrap(), "https://github.com/me/r");
        git(&["remote", "set-url", "origin", "/srv/git/r"]);
        assert!(remote_page(dir.path()).unwrap_err().contains("no web page"));
    }
}
