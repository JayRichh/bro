//! Reaching a user who is not looking at the window: when a session turns
//! done or stops to ask while the window is in the background, ring the
//! configured sound and post a native notification; a click on it brings
//! the window up on that session. The terminal app's DONE SOUND, FEEDBACK
//! SOUND and desktop notification (crates/nebula-tui/src/event_loop/
//! alerts.rs), on the same two keys: `done_sound` and `feedback_sound`,
//! each of which, "off", silences its sound and its notification.
//!
//! The edges come from `daemon::turn`. Turns are gathered for
//! [`DEBOUNCE`] and posted as one: a burst of sessions finishing together
//! is one notification and one sound, not a stack. Everything here fails
//! soft: a notifier that won't start is a missing nicety, not an error.

use crate::daemon::{Model, Turn};
use nebula_config::{Config, Sound};

/// How long turns gather before they are posted together.
pub const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(1500);

/// The most session names one notification lists.
const NAMED: usize = 4;

/// One session's turn, as its row reads on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub agent: String,
    pub turn: Turn,
    pub session: String,
    /// `<repo> · <branch>`; empty when the row's place is unknown.
    pub place: String,
}

/// The alert for `agent`, or `None` for a row the model no longer holds.
pub fn alert_for(model: &Model, agent: &str, turn: Turn) -> Option<Alert> {
    let a = model.agents.iter().find(|a| a.id.0 == agent)?;
    let w = model.worktrees.iter().find(|w| w.id == a.worktree_id);
    let p = w.and_then(|w| model.projects.iter().find(|p| p.id == w.project_id));
    let place = match (p, w) {
        (Some(p), Some(w)) => format!("{} \u{b7} {}", p.name, w.branch),
        (None, Some(w)) => w.branch.clone(),
        _ => String::new(),
    };
    Some(Alert {
        agent: agent.to_string(),
        turn,
        session: a.name.clone(),
        place,
    })
}

/// The turns gathered since the last post.
#[derive(Debug, Default)]
pub struct Batch(Vec<Alert>);

impl Batch {
    /// Gather `alert`; true when it opens a new batch (the caller posts
    /// the batch [`DEBOUNCE`] later). A session's later turn replaces its
    /// earlier one: what it is doing now is the news.
    pub fn push(&mut self, alert: Alert) -> bool {
        let first = self.0.is_empty();
        self.0.retain(|a| a.agent != alert.agent);
        self.0.push(alert);
        first
    }

    pub fn take(&mut self) -> Vec<Alert> {
        std::mem::take(&mut self.0)
    }
}

/// One notification and its sound.
#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub title: String,
    pub body: String,
    /// The session a click opens: the first that needs you, else the first done.
    pub open: String,
    pub sound: Sound,
}

/// What a batch posts under these sounds (`None`: off). A turn whose
/// sound is off drops out; nothing left posts nothing. Sessions that need
/// you lead, and their sound rings for the lot.
pub fn summarise(alerts: &[Alert], done: Option<Sound>, feedback: Option<Sound>) -> Option<Post> {
    let needs: Vec<&Alert> = match feedback {
        Some(_) => alerts.iter().filter(|a| a.turn == Turn::NeedsYou).collect(),
        None => Vec::new(),
    };
    let dones: Vec<&Alert> = match done {
        Some(_) => alerts.iter().filter(|a| a.turn == Turn::Done).collect(),
        None => Vec::new(),
    };
    let first = needs.first().or(dones.first())?;
    let sound = if needs.is_empty() { done } else { feedback }?;
    let all: Vec<&Alert> = needs.iter().chain(dones.iter()).copied().collect();
    if all.len() == 1 {
        let a = all[0];
        let title = match a.turn {
            Turn::NeedsYou => format!("{} needs you", a.session),
            Turn::Done => format!("{} is done", a.session),
        };
        return Some(Post {
            title,
            body: a.place.clone(),
            open: a.agent.clone(),
            sound,
        });
    }
    let title = match (needs.len(), dones.len()) {
        (0, d) => format!("{d} sessions done"),
        (1, 0) => unreachable!("one alert is posted above"),
        (n, 0) => format!("{n} sessions need you"),
        (1, d) => format!("1 session needs you, {d} done"),
        (n, d) => format!("{n} sessions need you, {d} done"),
    };
    let mut names: Vec<String> = all.iter().take(NAMED).map(|a| a.session.clone()).collect();
    if all.len() > NAMED {
        names.push(format!("and {} more", all.len() - NAMED));
    }
    Some(Post {
        title,
        body: names.join(", "),
        open: first.agent.clone(),
        sound,
    })
}

/// Post a batch: the sounds are read fresh from config.json, as the TUI
/// reads them each frame.
pub fn post(alerts: &[Alert], identifier: &str, on_click: Box<dyn Fn(String) + Send + Sync>) {
    let cfg = Config::load();
    let Some(post) = summarise(alerts, cfg.done_sound(), cfg.feedback_sound()) else {
        return;
    };
    play(&post.sound);
    notify(&post, identifier, on_click);
}

// ------------------------------------------------------------- the OS side

fn spawn_quiet(program: &str, args: &[&str]) {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Ring `sound`: a macOS system sound through `afplay`; the bell as the
/// system's default beep on Windows and `osascript`'s `beep` on macOS.
/// Elsewhere the bell is the notification's own sound hint.
fn play(sound: &Sound) {
    match sound {
        Sound::File(path) => spawn_quiet("afplay", &[&path.to_string_lossy()]),
        Sound::Bell => {
            #[cfg(windows)]
            // SAFETY: MessageBeep takes a sound id and touches no memory of ours.
            unsafe {
                windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(
                    windows_sys::Win32::UI::WindowsAndMessaging::MB_OK,
                );
            }
            if cfg!(target_os = "macos") {
                spawn_quiet("osascript", &["-e", "beep"]);
            }
        }
    }
}

#[cfg(windows)]
fn notify(post: &Post, identifier: &str, on_click: Box<dyn Fn(String) + Send + Sync>) {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_default();
    if let Err(e) = toast::show(post, &toast_app_id(&exe_dir, identifier), on_click) {
        eprintln!("kete-desktop: notification not posted: {e}");
    }
}

/// `osascript` on macOS, `notify-send` elsewhere, as the terminal app's:
/// no click there, the window is a dock or taskbar click away.
#[cfg(not(windows))]
fn notify(post: &Post, _identifier: &str, _on_click: Box<dyn Fn(String) + Send + Sync>) {
    if cfg!(target_os = "macos") {
        let script = format!(
            "display notification {} with title \"kete\" subtitle {}",
            applescript_str(&post.body),
            applescript_str(&post.title)
        );
        spawn_quiet("osascript", &["-e", &script]);
    } else {
        spawn_quiet(
            "notify-send",
            &[
                "--app-name=kete",
                "--hint=string:sound-name:bell",
                &post.title,
                &post.body,
            ],
        );
    }
}

/// `s` as an AppleScript string literal (the TUI's escaping).
#[cfg_attr(windows, allow(dead_code))]
fn applescript_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `s` as XML text: the five specials escaped, control characters dropped.
#[cfg_attr(not(windows), allow(dead_code))]
fn xml_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// The toast: a title and a body, silent, since [`play`] rang the
/// configured sound already.
#[cfg_attr(not(windows), allow(dead_code))]
fn toast_xml(post: &Post) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual><audio silent=\"true\"/></toast>",
        xml_text(&post.title),
        xml_text(&post.body)
    )
}

/// Which app a toast is posted as. The installer's shortcuts carry the
/// bundle identifier as their AppUserModelID, so the installed window
/// posts as itself; a build run from `target/` has no shortcut, and posts
/// as PowerShell, as Tauri's own notification plugin does there.
#[cfg_attr(not(windows), allow(dead_code))]
const POWERSHELL_APP_ID: &str =
    "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";

#[cfg_attr(not(windows), allow(dead_code))]
fn toast_app_id(exe_dir: &std::path::Path, identifier: &str) -> String {
    let dev = ["debug", "release"]
        .iter()
        .any(|d| exe_dir.ends_with(std::path::Path::new("target").join(d)));
    if dev {
        POWERSHELL_APP_ID.into()
    } else {
        identifier.into()
    }
}

#[cfg(windows)]
mod toast {
    use super::{toast_xml, Post};
    use std::sync::Mutex;
    use windows::core::{IInspectable, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    /// The last few toasts, held so their click handlers outlive the call.
    static SHOWN: Mutex<Vec<ToastNotification>> = Mutex::new(Vec::new());

    /// Post `post` as the app `app_id`; a click calls `on_click` with the
    /// session to open.
    pub fn show(
        post: &Post,
        app_id: &str,
        on_click: Box<dyn Fn(String) + Send + Sync>,
    ) -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(toast_xml(post)))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        let open = post.open.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, _| {
                on_click(open.clone());
                Ok(())
            },
        ))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?
            .Show(&toast)?;
        let mut shown = SHOWN.lock().expect("toast lock");
        shown.push(toast);
        let extra = shown.len().saturating_sub(8);
        shown.drain(..extra);
        Ok(())
    }

    /// Puts a real toast on this desktop, so it only runs by hand:
    /// `cargo test -- --ignored posts_a_real_toast`.
    #[test]
    #[ignore]
    fn posts_a_real_toast() {
        let post = Post {
            title: "kete test needs you".into(),
            body: "demo \u{b7} main".into(),
            open: "a".into(),
            sound: nebula_config::Sound::Bell,
        };
        show(&post, super::POWERSHELL_APP_ID, Box::new(|_| {})).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::{
        Agent, AgentId, AgentKind, AgentRole, AgentStatus, Project, ProjectId, Worktree, WorktreeId,
    };
    use std::path::PathBuf;

    fn alert(agent: &str, turn: Turn) -> Alert {
        Alert {
            agent: agent.into(),
            turn,
            session: format!("s-{agent}"),
            place: "demo \u{b7} main".into(),
        }
    }

    const BELL: Option<Sound> = Some(Sound::Bell);

    #[test]
    fn one_turn_names_its_session_and_place() {
        let p = summarise(&[alert("a", Turn::NeedsYou)], BELL, BELL).unwrap();
        assert_eq!(p.title, "s-a needs you");
        assert_eq!(p.body, "demo \u{b7} main");
        assert_eq!(p.open, "a");
        let p = summarise(&[alert("b", Turn::Done)], BELL, BELL).unwrap();
        assert_eq!(p.title, "s-b is done");
    }

    #[test]
    fn a_burst_is_one_post_led_by_who_needs_you() {
        let burst = [
            alert("d1", Turn::Done),
            alert("n1", Turn::NeedsYou),
            alert("d2", Turn::Done),
            alert("n2", Turn::NeedsYou),
            alert("d3", Turn::Done),
        ];
        let p = summarise(&burst, BELL, BELL).unwrap();
        assert_eq!(p.title, "2 sessions need you, 3 done");
        assert_eq!(p.body, "s-n1, s-n2, s-d1, s-d2, and 1 more");
        assert_eq!(p.open, "n1");
        let p = summarise(&burst[..3], BELL, BELL).unwrap();
        assert_eq!(p.title, "1 session needs you, 2 done");
        let p = summarise(
            &[alert("d1", Turn::Done), alert("d2", Turn::Done)],
            BELL,
            BELL,
        )
        .unwrap();
        assert_eq!(p.title, "2 sessions done");
    }

    /// Each key silences its own pair; both off is nothing at all. The
    /// needs-you sound rings for a mixed batch.
    #[test]
    fn an_off_sound_drops_its_turns() {
        let file = Some(Sound::File(PathBuf::from("/S/Sosumi.aiff")));
        let mixed = [alert("d", Turn::Done), alert("n", Turn::NeedsYou)];
        assert_eq!(
            summarise(&mixed, BELL, file.clone()).unwrap().sound,
            file.clone().unwrap()
        );
        let p = summarise(&mixed, None, BELL).unwrap();
        assert_eq!(p.title, "s-n needs you");
        let p = summarise(&mixed, BELL, None).unwrap();
        assert_eq!(p.title, "s-d is done");
        assert_eq!(summarise(&mixed, None, None), None);
        assert_eq!(summarise(&[], BELL, BELL), None);
    }

    #[test]
    fn a_batch_opens_once_and_keeps_each_sessions_latest_turn() {
        let mut b = Batch::default();
        assert!(b.push(alert("a", Turn::NeedsYou)));
        assert!(!b.push(alert("b", Turn::Done)));
        assert!(!b.push(alert("a", Turn::Done)));
        let got = b.take();
        assert_eq!(
            got.iter()
                .map(|a| (a.agent.as_str(), a.turn))
                .collect::<Vec<_>>(),
            [("b", Turn::Done), ("a", Turn::Done)]
        );
        assert!(b.push(alert("c", Turn::Done)), "a new batch after a take");
    }

    #[test]
    fn alerts_read_as_the_row_does() {
        let mut model = Model::default();
        model.projects.push(Project {
            id: ProjectId("p".into()),
            name: "demo".into(),
            repo_path: PathBuf::from("/demo"),
            sort_order: 0,
        });
        model.worktrees.push(Worktree {
            id: WorktreeId("w".into()),
            project_id: ProjectId("p".into()),
            path: PathBuf::from("/demo"),
            branch: "fix-login".into(),
            is_main: false,
            sort_order: 0,
        });
        model.agents.push(Agent {
            id: AgentId("a1".into()),
            worktree_id: WorktreeId("w".into()),
            name: "Fix Login".into(),
            status: AgentStatus::Running,
            archived: false,
            archived_at: 0,
            unseen: false,
            status_changed_at: 0,
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
            recent_prompts: Vec::new(),
            profile: None,
            parent: None,
        });
        let got = alert_for(&model, "a1", Turn::NeedsYou).unwrap();
        assert_eq!(got.session, "Fix Login");
        assert_eq!(got.place, "demo \u{b7} fix-login");
        assert_eq!(alert_for(&model, "gone", Turn::Done), None);
    }

    #[test]
    fn notification_text_is_escaped() {
        assert_eq!(
            xml_text(r#"a<b> & "c" 'd'"#),
            "a&lt;b&gt; &amp; &quot;c&quot; &apos;d&apos;"
        );
        assert_eq!(xml_text("one\ntwo"), "onetwo");
        let p = Post {
            title: "<x> needs you".into(),
            body: "demo".into(),
            open: "x".into(),
            sound: Sound::Bell,
        };
        assert!(toast_xml(&p).contains("<text>&lt;x&gt; needs you</text>"));
        assert!(toast_xml(&p).contains("<audio silent=\"true\"/>"));
        assert_eq!(applescript_str(r#"say "hi""#), r#""say \"hi\"""#);
    }

    #[test]
    fn a_dev_build_posts_as_powershell() {
        let dev = PathBuf::from("C:/code/kete/desktop/target/debug");
        assert!(toast_app_id(&dev, "nz.kete.desktop").contains("powershell.exe"));
        let installed = PathBuf::from("C:/Users/me/AppData/Local/Kete");
        assert_eq!(
            toast_app_id(&installed, "nz.kete.desktop"),
            "nz.kete.desktop"
        );
    }
}
