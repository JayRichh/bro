//! STOP NOTES (`nebula_core::stop`): why each Claude session's last turn
//! stopped short — a usage limit it is waiting out, a login it needs, an
//! API error — kept in memory beside the status machines and pushed to
//! 62+ subscribers as `SessionStops`. Nothing is persisted: a note is
//! about the running process, and a restarted daemon has none.
//!
//! The status machine decides when a note is set or cleared
//! (`Effect::SetStop`). This adds what only the daemon knows: the reset
//! time the session's screen printed (`pty::usage::LimitLine`, already in
//! the user's local clock), and the account's own reset from its last plan
//! reading (`registry::usage`). The same two tell a `rate_limit`
//! `StopFailure` what is known of the wait before the machine sees it
//! ([`Daemon::with_limit_outlook`]).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use nebula_core::stop::{SessionStop, StopNote, AUTO_WAIT_MAX_SECS, RATE_LIMIT};
use nebula_core::{AgentId, ServerEvent};

use super::Daemon;
use crate::pty::usage::LimitLine;
use crate::status::{HookEvent, LimitOutlook, StopCause};

/// How long a wait the screen showed still speaks for a `StopFailure` (or
/// a note) that lands after it: the hook and the footer of the same limit
/// arrive within moments of each other, in either order.
const SCREEN_FRESH: Duration = Duration::from_secs(120);

#[derive(Default)]
pub(crate) struct StopBook {
    notes: HashMap<AgentId, StopNote>,
    /// The latest usage-limit wait each session's screen showed.
    screens: HashMap<AgentId, Screen>,
}

/// One session's last usage-limit lines.
struct Screen {
    /// The reset time as printed (`3:45pm`).
    at: String,
    /// Claude said it will carry on by itself (`Continuing automatically`).
    continuing: bool,
    seen: Instant,
}

impl Screen {
    fn fresh(&self, now: Instant) -> bool {
        now.duration_since(self.seen) < SCREEN_FRESH
    }
}

/// `at` epoch seconds as an instant, seen from `now` (= `now_secs`).
fn instant_at(at: i64, now_secs: i64, now: Instant) -> Instant {
    now + Duration::from_secs((at - now_secs).max(0) as u64)
}

impl Daemon {
    /// Every session's note, for a subscriber's first look.
    pub fn session_stops(&self) -> Vec<SessionStop> {
        let book = self.stops.lock();
        let mut stops: Vec<SessionStop> = book
            .notes
            .iter()
            .map(|(agent, note)| SessionStop {
                agent: agent.clone(),
                note: Some(note.clone()),
            })
            .collect();
        stops.sort_by(|a, b| a.agent.0.cmp(&b.agent.0));
        stops
    }

    /// A `rate_limit` `StopFailure` as parsed says nothing of the wait:
    /// fill in what the screen and the account say, before the status
    /// machine reads it. Every other event passes through untouched.
    pub(super) fn with_limit_outlook(&self, agent: &AgentId, event: HookEvent) -> HookEvent {
        match event {
            HookEvent::StopFailure {
                error,
                detail,
                limit: LimitOutlook::Unknown,
            } if error.as_deref() == Some(RATE_LIMIT) => HookEvent::StopFailure {
                limit: self.limit_outlook(agent),
                error,
                detail,
            },
            other => other,
        }
    }

    /// What is known of `agent`'s usage-limit wait: the screen saying
    /// Claude will carry on decides it; otherwise the account's reset —
    /// within a day Claude waits for it by default, further off it opens
    /// its menu — and with no plan reading at all (an API-billed session)
    /// nothing says a wait is coming.
    fn limit_outlook(&self, agent: &AgentId) -> LimitOutlook {
        let now = Instant::now();
        let now_secs = nebula_core::clock::now_secs() as i64;
        let continuing = self
            .stops
            .lock()
            .screens
            .get(agent)
            .is_some_and(|s| s.continuing && s.fresh(now));
        let reset = self.account_limit_reset(agent);
        let until = reset
            .filter(|at| at - now_secs <= AUTO_WAIT_MAX_SECS)
            .map(|at| instant_at(at, now_secs, now));
        match reset {
            _ if continuing => LimitOutlook::Waits { until },
            Some(at) if at - now_secs > AUTO_WAIT_MAX_SECS => LimitOutlook::Menu,
            Some(_) => LimitOutlook::Waits { until },
            None => LimitOutlook::Unknown,
        }
    }

    /// The status machine set or cleared `agent`'s note.
    pub(super) fn apply_stop(&self, agent: &AgentId, cause: Option<StopCause>) {
        let note = cause.map(|cause| self.compose_note(agent, cause));
        self.put_note(agent, note);
    }

    /// A machine's cause, with the screen's reset time and the account's
    /// for a usage limit.
    fn compose_note(&self, agent: &AgentId, cause: StopCause) -> StopNote {
        let limit = cause.reason == RATE_LIMIT;
        let now = Instant::now();
        let resumes = limit
            .then(|| {
                self.stops
                    .lock()
                    .screens
                    .get(agent)
                    .filter(|s| s.fresh(now))
                    .map(|s| s.at.clone())
            })
            .flatten();
        let resets_at = limit.then(|| self.account_limit_reset(agent)).flatten();
        StopNote {
            reason: cause.reason,
            waiting: cause.waiting,
            resumes,
            resets_at,
            detail: cause.detail,
            at: nebula_core::clock::now_ms(),
        }
    }

    /// Keep `note` as `agent`'s (None drops it) and push it when it says
    /// something new: when it was noted alone is no news.
    fn put_note(&self, agent: &AgentId, note: Option<StopNote>) {
        let changed = {
            let mut book = self.stops.lock();
            let old = match &note {
                Some(n) => book.notes.insert(agent.clone(), n.clone()),
                None => book.notes.remove(agent),
            };
            match (&old, &note) {
                (Some(old), Some(new)) => {
                    StopNote {
                        at: new.at,
                        ..old.clone()
                    } != *new
                }
                (None, None) => false,
                _ => true,
            }
        };
        if changed {
            self.broadcast(ServerEvent::SessionStops {
                stops: vec![SessionStop {
                    agent: agent.clone(),
                    note,
                }],
            });
        }
    }

    /// A usage-limit line from `id`'s screen (`pty::usage`): remembered for
    /// the `StopFailure` it may precede, written into a limit note it may
    /// follow, and — when Claude says it will carry on, or that it will
    /// not — news for the status machine.
    pub(crate) fn on_pty_limit(&self, id: &AgentId, line: LimitLine) {
        let now = Instant::now();
        let says_continuing = matches!(line, LimitLine::Continuing { .. });
        match line {
            LimitLine::Continuing { at } | LimitLine::Resets { at } => {
                {
                    let mut book = self.stops.lock();
                    // The footer's two lines repaint together: the reset
                    // line must not take back what the other one said
                    // about the same reset.
                    let said = book
                        .screens
                        .get(id)
                        .is_some_and(|s| s.continuing && s.fresh(now) && s.at == at);
                    book.screens.insert(
                        id.clone(),
                        Screen {
                            at,
                            continuing: says_continuing || said,
                            seen: now,
                        },
                    );
                }
                // The scanner reports a line once until it changes, so each
                // of these is news; a machine already waiting ignores it.
                if says_continuing {
                    let now_secs = nebula_core::clock::now_secs() as i64;
                    let until = self
                        .account_limit_reset(id)
                        .filter(|at| at - now_secs <= AUTO_WAIT_MAX_SECS)
                        .map(|at| instant_at(at, now_secs, now));
                    self.apply_hook_event(id, HookEvent::LimitWaiting { until }, None);
                }
                self.refresh_limit_note(id);
            }
            LimitLine::Ended => {
                self.stops.lock().screens.remove(id);
                self.apply_hook_event(id, HookEvent::LimitWaitEnded, None);
            }
        }
    }

    /// A limit note already out learns the time the screen printed.
    fn refresh_limit_note(&self, id: &AgentId) {
        let Some(note) = self.stops.lock().notes.get(id).cloned() else {
            return;
        };
        if note.reason != RATE_LIMIT {
            return;
        }
        let resumes = self
            .stops
            .lock()
            .screens
            .get(id)
            .map(|s| s.at.clone())
            .or(note.resumes.clone());
        self.put_note(id, Some(StopNote { resumes, ..note }));
    }

    /// The CLI is starting afresh: what the old one waited out or was
    /// stuck on went with it (a resumed session does not restart its
    /// wait), so its note goes, and its machine forgets the stop.
    pub(super) fn reset_stop(&self, id: &AgentId) {
        if let Some(machine) = self.status_machines.lock().get_mut(id) {
            machine.forget_stop();
        }
        self.stops.lock().screens.remove(id);
        self.put_note(id, None);
    }

    /// Forget a deleted session's note: its removal tells the clients.
    pub(crate) fn forget_stop(&self, id: &AgentId) {
        let mut book = self.stops.lock();
        book.notes.remove(id);
        book.screens.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::{Agent, AgentKind, AgentStatus, Project, ProjectId, Worktree, WorktreeId};

    fn test_daemon(dir: &std::path::Path) -> std::sync::Arc<Daemon> {
        let daemon = Daemon::new(
            std::sync::Arc::new(crate::store::Store::open_in_memory().unwrap()),
            crate::hooks::HookEnv {
                port: 0,
                token: String::new(),
            },
        );
        daemon.set_usage_root(dir.to_path_buf());
        daemon
            .store
            .insert_project(&Project {
                id: ProjectId("p".into()),
                name: "demo".into(),
                repo_path: "/repo".into(),
                sort_order: 0,
            })
            .unwrap();
        daemon
            .store
            .insert_worktree(&Worktree {
                id: WorktreeId("w".into()),
                project_id: ProjectId("p".into()),
                path: "/repo".into(),
                branch: "main".into(),
                is_main: true,
                sort_order: 0,
            })
            .unwrap();
        daemon
    }

    /// A running Claude session on `profile`'s account.
    fn running(daemon: &Daemon, id: &str, profile: Option<&str>) -> AgentId {
        daemon
            .store
            .insert_agent(&Agent {
                id: AgentId(id.into()),
                worktree_id: WorktreeId("w".into()),
                name: id.into(),
                status: AgentStatus::Running,
                archived: false,
                archived_at: 0,
                unseen: false,
                status_changed_at: 0,
                kind: AgentKind::Claude,
                custom_harness: None,
                model: None,
                effort: None,
                session_id: Some("s1".into()),
                cloud_session_id: None,
                issue_url: None,
                role: nebula_core::AgentRole::Worker,
                sort_order: 0,
                alive: true,
                recent_prompts: Vec::new(),
                profile: profile.map(str::to_string),
                parent: None,
            })
            .unwrap();
        AgentId(id.into())
    }

    fn fail(daemon: &Daemon, id: &AgentId, error: &str) {
        daemon.apply_hook_event(
            id,
            HookEvent::StopFailure {
                error: Some(error.into()),
                detail: Some("API Error: Rate limit reached".into()),
                limit: LimitOutlook::Unknown,
            },
            Some("s1".into()),
        );
    }

    /// What was pushed since the last look: the stop notes, in order, and
    /// the statuses.
    fn pushed(
        events: &mut tokio::sync::broadcast::Receiver<ServerEvent>,
    ) -> (Vec<SessionStop>, Vec<AgentStatus>) {
        let (mut stops, mut statuses) = (Vec::new(), Vec::new());
        while let Ok(ev) = events.try_recv() {
            match ev {
                ServerEvent::SessionStops { stops: s } => stops.extend(s),
                ServerEvent::StatusChanged { status, .. } => statuses.push(status),
                _ => {}
            }
        }
        (stops, statuses)
    }

    fn reading(percent: f32, resets_in: i64) -> serde_json::Value {
        let at = nebula_core::clock::now_secs() as i64 + resets_in;
        serde_json::json!({
            "rate_limits": {"five_hour": {"used_percentage": percent, "resets_at": at}}
        })
    }

    /// The footer before the hook (they race): the limit waits, running,
    /// with the screen's own clock text and the account's reset beside it;
    /// Claude's continuation clears it, and the clear is pushed.
    #[test]
    fn a_waited_limit_carries_the_screens_time_and_the_accounts_reset() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = test_daemon(dir.path());
        let id = running(&daemon, "a1", None);
        daemon.on_statusline(&id, &reading(100.0, 3600));
        let mut events = daemon.events.subscribe();
        daemon.on_pty_limit(
            &id,
            LimitLine::Continuing {
                at: "3:45pm".into(),
            },
        );
        assert_eq!(pushed(&mut events), (vec![], vec![]), "no limit yet");
        fail(&daemon, &id, "rate_limit");
        let (stops, statuses) = pushed(&mut events);
        assert!(statuses.is_empty(), "still running: {statuses:?}");
        let note = stops[0].note.clone().expect("a note");
        assert!(note.waiting);
        assert_eq!(note.resumes.as_deref(), Some("3:45pm"));
        assert!(note.resets_at.is_some());
        assert_eq!(
            note.resets_at,
            daemon.account_limit_reset(&id),
            "the account's reset beside it"
        );
        assert_eq!(
            note.detail.as_deref(),
            Some("API Error: Rate limit reached")
        );
        assert_eq!(note.label(0), "resumes 3:45pm");
        assert_eq!(daemon.session_stops().len(), 1);

        // The footer repainting a later time moves the note with it.
        daemon.on_pty_limit(
            &id,
            LimitLine::Resets {
                at: "4:00pm".into(),
            },
        );
        let (stops, _) = pushed(&mut events);
        assert_eq!(
            stops[0].note.as_ref().unwrap().resumes.as_deref(),
            Some("4:00pm")
        );

        daemon.apply_hook_event(
            &id,
            HookEvent::Notification {
                notification_type: Some("quota_auto_resume_fired".into()),
            },
            Some("s1".into()),
        );
        let (stops, _) = pushed(&mut events);
        assert_eq!(
            stops,
            vec![SessionStop {
                agent: id,
                note: None
            }]
        );
        assert!(daemon.session_stops().is_empty());
    }

    /// A reset days off is Claude's menu (needs you); an account with no
    /// plan reading finishes the turn, until the screen says Claude waits
    /// after all, and a cancel on screen finishes it as the limit it was.
    #[test]
    fn the_accounts_reset_and_the_screen_decide_a_limits_row() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = test_daemon(dir.path());
        let weekly = running(&daemon, "wk", Some("work"));
        daemon.on_statusline(&weekly, &reading(100.0, 4 * 86_400));
        let mut events = daemon.events.subscribe();
        fail(&daemon, &weekly, "rate_limit");
        let (stops, statuses) = pushed(&mut events);
        assert_eq!(statuses, vec![AgentStatus::NeedsFeedback]);
        let note = stops[0].note.clone().unwrap();
        assert!(!note.waiting);
        assert_eq!(note.label(0), "weekly limit");

        let api = running(&daemon, "api", Some("api-key"));
        fail(&daemon, &api, "rate_limit");
        let (stops, statuses) = pushed(&mut events);
        assert_eq!(statuses, vec![AgentStatus::Finished]);
        assert_eq!(stops[0].note.as_ref().unwrap().label(0), "usage limit");
        daemon.on_pty_limit(
            &api,
            LimitLine::Continuing {
                at: "9:15am".into(),
            },
        );
        let (stops, statuses) = pushed(&mut events);
        assert_eq!(statuses, vec![AgentStatus::Running]);
        let note = stops.last().unwrap().note.clone().unwrap();
        assert!(note.waiting);
        assert_eq!(note.label(0), "resumes 9:15am");
        daemon.on_pty_limit(&api, LimitLine::Ended);
        let (stops, statuses) = pushed(&mut events);
        assert_eq!(statuses, vec![AgentStatus::Finished]);
        assert!(!stops.last().unwrap().note.as_ref().unwrap().waiting);
    }

    /// A fresh CLI drops the old one's note (pushed) and its hold; a
    /// deleted session's note is simply forgotten.
    #[test]
    fn a_fresh_cli_or_a_delete_drops_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = test_daemon(dir.path());
        let id = running(&daemon, "a1", None);
        fail(&daemon, &id, "authentication_failed");
        let mut events = daemon.events.subscribe();
        daemon.reset_stop(&id);
        let (stops, _) = pushed(&mut events);
        assert_eq!(
            stops,
            vec![SessionStop {
                agent: id.clone(),
                note: None
            }]
        );
        daemon.apply_hook_event(&id, HookEvent::Progress { busy: false }, None);
        let (_, statuses) = pushed(&mut events);
        assert_eq!(statuses, vec![AgentStatus::Finished], "no hold left");

        fail(&daemon, &id, "billing_error");
        assert_eq!(daemon.session_stops().len(), 1);
        daemon.forget_stop(&id);
        assert!(daemon.session_stops().is_empty());
    }
}
