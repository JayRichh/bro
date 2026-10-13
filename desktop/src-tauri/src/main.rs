//! bro-desktop: Bro's native window over the same daemon the TUI drives.
//! The Rust side owns the daemon connection (`daemon`) and the git reads
//! (`git`); the web view draws the kanban, the live terminals and the
//! diffs, and talks to this side through the commands below.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daemon;
mod git;

use daemon::{Client, Model, PtyEvent};
use nebula_core::{AgentId, AgentKind, ClientRequest, EntityId, SessionRef, WorktreeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
fn get_state(client: State<'_, Arc<Client>>) -> Model {
    client.snapshot()
}

#[tauri::command]
fn retry_connect(client: State<'_, Arc<Client>>) {
    client.retry.notify_one();
}

#[tauri::command]
fn attach(
    client: State<'_, Arc<Client>>,
    session: SessionRef,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
) -> Res<()> {
    client
        .attach(session, cols.max(2), rows.max(2), on_event)
        .map_err(err)
}

#[tauri::command]
fn detach(client: State<'_, Arc<Client>>, session: SessionRef) -> Res<()> {
    client.detach(session).map_err(err)
}

/// Keystrokes from the pane. `binary` input arrives as a string of
/// byte-valued code units (xterm.js's `onBinary`); anything else is UTF-8.
#[tauri::command]
fn input(
    client: State<'_, Arc<Client>>,
    session: SessionRef,
    data: String,
    binary: bool,
) -> Res<()> {
    let data = if binary {
        data.chars().map(|c| c as u32 as u8).collect()
    } else {
        data.into_bytes()
    };
    client
        .send(ClientRequest::Input { session, data })
        .map_err(err)
}

#[tauri::command]
fn resize(client: State<'_, Arc<Client>>, session: SessionRef, cols: u16, rows: u16) -> Res<()> {
    client
        .send(ClientRequest::Resize {
            session,
            cols: cols.max(2),
            rows: rows.max(2),
        })
        .map_err(err)
}

#[tauri::command]
fn mark_seen(client: State<'_, Arc<Client>>, agent: String) -> Res<()> {
    client
        .send(ClientRequest::MarkAgentSeen { id: AgentId(agent) })
        .map_err(err)
}

fn created_id(created: Option<EntityId>) -> String {
    match created {
        Some(EntityId::Agent(id)) => id.0,
        Some(EntityId::Terminal(id)) => id.0,
        Some(EntityId::Project(id)) => id.0,
        Some(EntityId::Worktree(id)) => id.0,
        Some(EntityId::Link(id)) => id.0,
        None => String::new(),
    }
}

/// The next free `agent-N`, the default name a session takes until its
/// agent titles it.
fn default_agent_name(model: &Model) -> String {
    let n = model
        .agents
        .iter()
        .filter_map(|a| a.name.strip_prefix("agent-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("agent-{}", n + 1)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewAgent {
    worktree: String,
    kind: String,
    profile: Option<String>,
    prompt: Option<String>,
}

#[tauri::command]
async fn create_agent(client: State<'_, Arc<Client>>, spec: NewAgent) -> Res<String> {
    let kind =
        AgentKind::parse(&spec.kind).ok_or_else(|| format!("unknown harness {}", spec.kind))?;
    let name = default_agent_name(&client.snapshot());
    let prompt = spec
        .prompt
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let req_id = client.req_id();
    let created = client
        .rpc(
            req_id,
            ClientRequest::CreateAgent {
                req_id,
                worktree: WorktreeId(spec.worktree),
                name,
                kind,
                custom_harness: None,
                model: None,
                effort: None,
                auto_title: true,
                cloud_prompt: None,
                starting_prompt: prompt,
                issue_url: None,
                role: Default::default(),
                profile: spec.profile.filter(|p| !p.is_empty()),
            },
        )
        .await
        .map_err(err)?;
    Ok(created_id(created))
}

#[tauri::command]
async fn create_terminal(client: State<'_, Arc<Client>>, worktree: String) -> Res<String> {
    let req_id = client.req_id();
    let created = client
        .rpc(
            req_id,
            ClientRequest::CreateTerminal {
                req_id,
                worktree: WorktreeId(worktree),
                name: None,
            },
        )
        .await
        .map_err(err)?;
    Ok(created_id(created))
}

#[tauri::command]
async fn add_project(client: State<'_, Arc<Client>>, path: String) -> Res<String> {
    let req_id = client.req_id();
    let created = client
        .rpc(
            req_id,
            ClientRequest::AddProject {
                req_id,
                path: PathBuf::from(path.trim()),
                name: None,
                create_missing: false,
            },
        )
        .await
        .map_err(err)?;
    Ok(created_id(created))
}

#[tauri::command]
async fn start_run(client: State<'_, Arc<Client>>, worktree: String) -> Res<String> {
    let req_id = client.req_id();
    let created = client
        .rpc(
            req_id,
            ClientRequest::StartRun {
                req_id,
                worktree: WorktreeId(worktree),
            },
        )
        .await
        .map_err(err)?;
    Ok(created_id(created))
}

fn worktree_path(client: &Client, worktree: &str) -> Res<PathBuf> {
    client
        .snapshot()
        .worktrees
        .iter()
        .find(|w| w.id.0 == worktree)
        .map(|w| w.path.clone())
        .ok_or_else(|| "that worktree is gone".to_string())
}

#[tauri::command]
async fn diff_files(client: State<'_, Arc<Client>>, worktree: String) -> Res<Vec<git::DiffFile>> {
    let root = worktree_path(&client, &worktree)?;
    tauri::async_runtime::spawn_blocking(move || git::changed_files(&root))
        .await
        .map_err(err)?
}

#[derive(Deserialize)]
struct FileRef {
    path: String,
    from: Option<String>,
    status: char,
}

#[tauri::command]
async fn diff_pair(
    client: State<'_, Arc<Client>>,
    worktree: String,
    file: FileRef,
) -> Res<git::DiffPair> {
    let root = worktree_path(&client, &worktree)?;
    let file = git::DiffFile {
        path: file.path,
        from: file.from,
        status: file.status,
        added: None,
        removed: None,
    };
    tauri::async_runtime::spawn_blocking(move || git::file_pair(&root, &file))
        .await
        .map_err(err)?
}

#[tauri::command]
fn open_url(app: tauri::AppHandle, url: String) -> Res<()> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("only http(s) links open from here".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

#[derive(Serialize)]
struct HarnessInfo {
    id: String,
    label: String,
}

#[derive(Serialize)]
struct ProfileInfo {
    name: String,
    harness: String,
    default: bool,
}

#[derive(Serialize)]
struct Launchables {
    harnesses: Vec<HarnessInfo>,
    profiles: Vec<ProfileInfo>,
}

/// What the quick prompt offers: the built-in harnesses and every profile.
#[tauri::command]
fn launchables() -> Launchables {
    let harnesses = nebula_core::harness::builtins()
        .into_iter()
        .filter(|h| AgentKind::parse(&h.id).is_some())
        .map(|h| HarnessInfo {
            label: h.display_label().to_string(),
            id: h.id,
        })
        .collect();
    let profiles = nebula_core::profiles::load()
        .into_iter()
        .map(|p| ProfileInfo {
            name: p.name,
            harness: p.harness,
            default: p.default,
        })
        .collect();
    Launchables {
        harnesses,
        profiles,
    }
}

/// Command-line options: `--view board|session|diff`, `--session <name or
/// id>`, `--theme light|dark|system`. They pick what the window opens on.
#[derive(Serialize, Default)]
struct Startup {
    view: Option<String>,
    session: Option<String>,
    theme: Option<String>,
    diff_mode: Option<String>,
}

#[tauri::command]
fn startup() -> Startup {
    let mut out = Startup::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let slot = match a.as_str() {
            "--view" => &mut out.view,
            "--session" => &mut out.session,
            "--theme" => &mut out.theme,
            "--diff" => &mut out.diff_mode,
            _ => continue,
        };
        *slot = args.next();
    }
    out
}

/// Ask `gh` for each worktree's open pull request, now and then, for the
/// PR open lane. Only checkouts with agent sessions are asked.
async fn poll_prs(client: Arc<Client>) {
    let have_gh = tauri::async_runtime::spawn_blocking(git::have_gh)
        .await
        .unwrap_or(false);
    if !have_gh {
        return;
    }
    loop {
        let model = client.snapshot();
        let wanted: Vec<(String, PathBuf, String)> = model
            .worktrees
            .iter()
            .filter(|w| !w.is_main && model.agents.iter().any(|a| a.worktree_id == w.id))
            .map(|w| (w.id.0.clone(), w.path.clone(), w.branch.clone()))
            .collect();
        let prs = tauri::async_runtime::spawn_blocking(move || {
            wanted
                .into_iter()
                .filter_map(|(id, path, branch)| Some((id, git::open_pr(&path, &branch)?)))
                .collect::<HashMap<_, _>>()
        })
        .await
        .unwrap_or_default();
        client.set_prs(prs);
        tokio::time::sleep(Duration::from_secs(90)).await;
    }
}

/// Push the model to the window whenever it changes, at most every 50 ms.
async fn emit_state(app: tauri::AppHandle, client: Arc<Client>) {
    loop {
        client.changed.notified().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = app.emit("bro://state", client.snapshot());
    }
}

fn main() {
    let client = Client::new();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(client.clone())
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(daemon::run(client.clone()));
            tauri::async_runtime::spawn(emit_state(handle, client.clone()));
            tauri::async_runtime::spawn(poll_prs(client.clone()));
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title("bro");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            retry_connect,
            attach,
            detach,
            input,
            resize,
            mark_seen,
            create_agent,
            create_terminal,
            add_project,
            start_run,
            diff_files,
            diff_pair,
            open_url,
            launchables,
            startup,
        ])
        .run(tauri::generate_context!())
        .expect("bro-desktop failed to start");
}
