//! The desktop window's client of the Kete daemon.
//!
//! The wire format is nebula-core's: `codec` frames `ClientRequest` /
//! `ServerEvent` as length-prefixed MessagePack, and `paths` names the
//! endpoint (a named pipe on Windows, a unix socket elsewhere) exactly as
//! the TUI derives it, so `NEBULA_RUNTIME_DIR` isolates a test daemon here
//! too. One connection carries everything: the Subscribe stream that keeps
//! [`Model`] current, the PTY plane of every attached pane, and RPCs
//! answered by `Ack` / `Error` with a matching `req_id`.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use nebula_core::codec::{read_frame, write_frame};
use nebula_core::ports::WorktreePorts;
use nebula_core::{
    paths, Agent, ClientRequest, Entity, EntityId, Project, ServerEvent, SessionRef, TerminalTab,
    Worktree, PROTOCOL_VERSION,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};

/// How long a freshly spawned daemon gets to start listening.
const BOOT_WAIT: Duration = Duration::from_secs(5);
const POLL_STEP: Duration = Duration::from_millis(50);
/// How long an RPC waits for its Ack before the UI is told it failed.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Everything the frontend draws, mirrored from the daemon's Snapshot and
/// the deltas after it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Model {
    pub connected: bool,
    /// Why there is no connection, in words the window can show.
    pub error: Option<String>,
    pub daemon_pid: u32,
    pub protocol_version: u32,
    pub runtime_dir: String,
    pub projects: Vec<Project>,
    pub worktrees: Vec<Worktree>,
    pub agents: Vec<Agent>,
    pub terminals: Vec<TerminalTab>,
    pub ports: Vec<WorktreePorts>,
    /// Open pull request per worktree id, looked up with `gh`.
    pub prs: HashMap<String, PrInfo>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub draft: bool,
}

/// What a pane hears about its session, on the Channel it attached with.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PtyEvent {
    /// The ring replayed on attach (base64); the pane resets first.
    Scrollback {
        data: String,
    },
    /// Live output (base64).
    Output {
        data: String,
    },
    Exited {
        code: Option<i32>,
    },
    Refused {
        message: String,
    },
    KittyFlags {
        flags: u8,
    },
}

type Pending = HashMap<u64, oneshot::Sender<Result<Option<EntityId>, String>>>;

/// Shared between the Tauri commands and the connection task.
pub struct Client {
    pub model: Mutex<Model>,
    tx: Mutex<Option<mpsc::UnboundedSender<ClientRequest>>>,
    pending: Mutex<Pending>,
    panes: Mutex<HashMap<SessionRef, Channel<PtyEvent>>>,
    next_req: AtomicU64,
    /// Bumped on every model change; the emitter task watches it.
    pub changed: tokio::sync::Notify,
    /// Pinged to make the connection loop try again at once.
    pub retry: tokio::sync::Notify,
}

impl Client {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            model: Mutex::new(Model {
                runtime_dir: paths::runtime_dir().display().to_string(),
                ..Model::default()
            }),
            tx: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            panes: Mutex::new(HashMap::new()),
            next_req: AtomicU64::new(1),
            changed: tokio::sync::Notify::new(),
            retry: tokio::sync::Notify::new(),
        })
    }

    pub fn snapshot(&self) -> Model {
        self.model.lock().expect("model lock").clone()
    }

    fn touch(&self) {
        self.changed.notify_one();
    }

    /// Fire-and-forget request.
    pub fn send(&self, req: ClientRequest) -> Result<()> {
        let tx = self.tx.lock().expect("tx lock");
        let tx = tx
            .as_ref()
            .ok_or_else(|| anyhow!("not connected to the kete daemon"))?;
        tx.send(req)
            .map_err(|_| anyhow!("the daemon connection closed"))
    }

    pub fn req_id(&self) -> u64 {
        self.next_req.fetch_add(1, Ordering::Relaxed)
    }

    /// Send a request carrying `req_id` and wait for its Ack.
    pub async fn rpc(&self, req_id: u64, req: ClientRequest) -> Result<Option<EntityId>> {
        let (done_tx, done_rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("pending lock")
            .insert(req_id, done_tx);
        if let Err(e) = self.send(req) {
            self.pending.lock().expect("pending lock").remove(&req_id);
            return Err(e);
        }
        match tokio::time::timeout(RPC_TIMEOUT, done_rx).await {
            Ok(Ok(Ok(created))) => Ok(created),
            Ok(Ok(Err(message))) => Err(anyhow!(message)),
            Ok(Err(_)) => bail!("the daemon connection closed before it answered"),
            Err(_) => {
                self.pending.lock().expect("pending lock").remove(&req_id);
                bail!("the daemon did not answer in time")
            }
        }
    }

    pub fn attach(
        &self,
        session: SessionRef,
        cols: u16,
        rows: u16,
        pane: Channel<PtyEvent>,
    ) -> Result<()> {
        self.panes
            .lock()
            .expect("panes lock")
            .insert(session.clone(), pane);
        self.send(ClientRequest::Attach {
            session,
            from_seq: None,
            cols,
            rows,
        })
    }

    pub fn detach(&self, session: SessionRef) -> Result<()> {
        self.panes.lock().expect("panes lock").remove(&session);
        self.send(ClientRequest::Detach { session })
    }

    pub fn set_prs(&self, prs: HashMap<String, PrInfo>) {
        let mut model = self.model.lock().expect("model lock");
        if model.prs != prs {
            model.prs = prs;
            drop(model);
            self.touch();
        }
    }

    fn pane(&self, session: &SessionRef) -> Option<Channel<PtyEvent>> {
        self.panes.lock().expect("panes lock").get(session).cloned()
    }

    fn set_disconnected(&self, error: Option<String>) {
        *self.tx.lock().expect("tx lock") = None;
        for (_, waiter) in self.pending.lock().expect("pending lock").drain() {
            let _ = waiter.send(Err("the daemon connection closed".into()));
        }
        self.panes.lock().expect("panes lock").clear();
        let mut model = self.model.lock().expect("model lock");
        model.connected = false;
        model.error = error;
        drop(model);
        self.touch();
    }

    fn handle(&self, event: ServerEvent) {
        let b64 = |data: &[u8]| base64::engine::general_purpose::STANDARD.encode(data);
        match event {
            ServerEvent::Snapshot {
                projects,
                worktrees,
                agents,
                terminals,
                ..
            } => {
                let mut m = self.model.lock().expect("model lock");
                m.projects = projects;
                m.worktrees = worktrees;
                m.agents = agents;
                m.terminals = terminals;
                drop(m);
                self.touch();
            }
            ServerEvent::EntityUpserted { entity } => {
                let mut m = self.model.lock().expect("model lock");
                match entity {
                    Entity::Project(p) => upsert(&mut m.projects, p, |x| x.id.0.clone()),
                    Entity::Worktree(w) => upsert(&mut m.worktrees, w, |x| x.id.0.clone()),
                    Entity::Agent(a) => upsert(&mut m.agents, a, |x| x.id.0.clone()),
                    Entity::Terminal(t) => upsert(&mut m.terminals, t, |x| x.id.0.clone()),
                    Entity::Link(_) => {}
                }
                drop(m);
                self.touch();
            }
            ServerEvent::EntityRemoved { id } => {
                let mut m = self.model.lock().expect("model lock");
                match id {
                    EntityId::Project(id) => m.projects.retain(|x| x.id != id),
                    EntityId::Worktree(id) => m.worktrees.retain(|x| x.id != id),
                    EntityId::Agent(id) => m.agents.retain(|x| x.id != id),
                    EntityId::Terminal(id) => m.terminals.retain(|x| x.id != id),
                    EntityId::Link(_) => {}
                }
                drop(m);
                self.touch();
            }
            ServerEvent::StatusChanged {
                agent,
                status,
                changed_at,
                unseen,
            } => {
                let mut m = self.model.lock().expect("model lock");
                if let Some(a) = m.agents.iter_mut().find(|a| a.id == agent) {
                    a.status = status;
                    a.status_changed_at = changed_at;
                    a.unseen = unseen;
                }
                drop(m);
                self.touch();
            }
            ServerEvent::WorktreePorts { ports } => {
                self.model.lock().expect("model lock").ports = ports;
                self.touch();
            }
            ServerEvent::Ack { req_id, created } => {
                if let Some(w) = self.pending.lock().expect("pending lock").remove(&req_id) {
                    let _ = w.send(Ok(created));
                }
            }
            ServerEvent::Error {
                req_id: Some(req_id),
                message,
            } => {
                if let Some(w) = self.pending.lock().expect("pending lock").remove(&req_id) {
                    let _ = w.send(Err(message));
                }
            }
            ServerEvent::Scrollback { session, data, .. } => {
                if let Some(p) = self.pane(&session) {
                    let _ = p.send(PtyEvent::Scrollback { data: b64(&data) });
                }
            }
            ServerEvent::Output { session, data, .. } => {
                if let Some(p) = self.pane(&session) {
                    let _ = p.send(PtyEvent::Output { data: b64(&data) });
                }
            }
            ServerEvent::SessionExited { session, exit_code } => {
                if let Some(p) = self.pane(&session) {
                    let _ = p.send(PtyEvent::Exited { code: exit_code });
                }
            }
            ServerEvent::AttachRefused { session, message } => {
                if let Some(p) = self.pane(&session) {
                    let _ = p.send(PtyEvent::Refused { message });
                }
            }
            ServerEvent::KittyFlags { session, flags } => {
                if let Some(p) = self.pane(&session) {
                    let _ = p.send(PtyEvent::KittyFlags { flags });
                }
            }
            _ => {}
        }
    }
}

fn upsert<T>(list: &mut Vec<T>, item: T, key: impl Fn(&T) -> String) {
    let k = key(&item);
    match list.iter_mut().find(|x| key(x) == k) {
        Some(slot) => *slot = item,
        None => list.push(item),
    }
}

// ------------------------------------------------------------- connection

trait Duplex: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Duplex for T {}
type Stream = Box<dyn Duplex>;

/// Connect to the daemon's endpoint for this runtime dir.
#[cfg(windows)]
async fn connect() -> std::io::Result<Stream> {
    use tokio::net::windows::named_pipe::ClientOptions;
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
    let name = paths::pipe_name_for_socket(&paths::socket_path());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        match ClientOptions::new().open(&name) {
            Ok(client) => return Ok(Box::new(client)),
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(unix)]
async fn connect() -> std::io::Result<Stream> {
    Ok(Box::new(
        tokio::net::UnixStream::connect(paths::socket_path()).await?,
    ))
}

/// The `kete` binary that runs the daemon: `KETE_BIN` (or the older
/// `BRO_BIN`), else a `kete` beside this exe, else one on PATH; `bro`, the
/// old name of the same program, after each `kete`.
pub fn find_kete() -> Option<PathBuf> {
    for var in ["KETE_BIN", "BRO_BIN"] {
        if let Some(p) = std::env::var_os(var).map(PathBuf::from) {
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let names: &[&str] = if cfg!(windows) {
        &["kete.exe", "bro.exe"]
    } else {
        &["kete", "bro"]
    };
    let beside = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from));
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    beside
        .into_iter()
        .chain(on_path)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|p| p.is_file())
}

/// Start `kete daemon` detached, the way the TUI does: no console, its own
/// process group, out of this process's job when the job allows it.
#[cfg(windows)]
fn spawn_daemon(kete: &std::path::Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{
        CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS,
    };
    let flags = CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW;
    let spawn = |flags: u32| {
        std::process::Command::new(kete)
            .arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    spawn(flags | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(flags))
        .context("start the kete daemon")?;
    Ok(())
}

#[cfg(unix)]
fn spawn_daemon(kete: &std::path::Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(kete);
    cmd.arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // A session of its own, as the TUI's spawn: no controlling terminal.
    unsafe {
        cmd.pre_exec(|| {
            extern "C" {
                fn setsid() -> i32;
            }
            if setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn().context("start the kete daemon")?;
    Ok(())
}

async fn connect_or_spawn(spawn: bool) -> Result<Stream> {
    if let Ok(s) = connect().await {
        return Ok(s);
    }
    if !spawn {
        bail!("the kete daemon is not running");
    }
    let kete = find_kete().ok_or_else(|| {
        anyhow!("no kete daemon is running, and no kete binary was found beside this app or on PATH to start one")
    })?;
    spawn_daemon(&kete)?;
    let deadline = tokio::time::Instant::now() + BOOT_WAIT;
    loop {
        match connect().await {
            Ok(s) => return Ok(s),
            Err(_) if tokio::time::Instant::now() < deadline => tokio::time::sleep(POLL_STEP).await,
            Err(e) => {
                return Err(anyhow!(e)).with_context(|| {
                    format!(
                        "the daemon did not come up; check {}",
                        paths::daemon_log_path().display()
                    )
                })
            }
        }
    }
}

async fn handshake(stream: &mut Stream) -> Result<(u32, u32)> {
    write_frame(
        stream,
        &ClientRequest::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await?;
    match read_frame::<ServerEvent, _>(stream).await? {
        Some(ServerEvent::HelloOk {
            protocol_version,
            daemon_pid,
        }) if nebula_core::protocol_compatible_with(protocol_version) => {
            Ok((protocol_version, daemon_pid))
        }
        Some(ServerEvent::HelloOk {
            protocol_version, ..
        })
        | Some(ServerEvent::Incompatible {
            daemon_protocol_version: protocol_version,
        }) => bail!(
            "the running daemon speaks protocol {protocol_version}, this app speaks {PROTOCOL_VERSION}; \
             run `kete kill` and reopen, or update the older side"
        ),
        other => bail!("unexpected handshake reply: {other:?}"),
    }
}

/// One connection's life: handshake, Subscribe, then pump events until the
/// daemon hangs up.
async fn session(client: &Arc<Client>, spawn: bool) -> Result<()> {
    let mut stream = connect_or_spawn(spawn).await?;
    let (version, pid) = handshake(&mut stream).await?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::unbounded_channel::<ClientRequest>();
    tx.send(ClientRequest::Subscribe).ok();
    *client.tx.lock().expect("tx lock") = Some(tx);
    {
        let mut m = client.model.lock().expect("model lock");
        m.connected = true;
        m.error = None;
        m.daemon_pid = pid;
        m.protocol_version = version;
    }
    client.touch();

    let writer_task = tokio::spawn(async move {
        while let Some(req) = rx.recv().await {
            if write_frame(&mut writer, &req).await.is_err() {
                break;
            }
        }
    });
    let result = loop {
        match read_frame::<ServerEvent, _>(&mut reader).await {
            Ok(Some(event)) => client.handle(event),
            Ok(None) => break Ok(()),
            Err(e) => break Err(anyhow!(e)),
        }
    };
    writer_task.abort();
    result
}

/// Keep a connection up for the life of the app. The daemon is started
/// only on the first try (and when the user asks): a daemon someone
/// stopped with `kete kill` stays stopped until then.
pub async fn run(client: Arc<Client>) {
    let mut spawn = true;
    loop {
        let outcome = session(&client, spawn).await;
        let message = match outcome {
            Ok(()) => "the daemon closed the connection".to_string(),
            Err(e) => format!("{e:#}"),
        };
        client.set_disconnected(Some(message));
        spawn = false;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            _ = client.retry.notified() => { spawn = true; }
        }
    }
}
