//! The ORCHESTRATOR VIEW: F10's chat-first layout around the machine's one
//! pinned orchestrator session. Its terminal owns the screen (the same
//! full-screen pane any session gets with `^F`), with a STATUS STRIP of
//! chips across the top — every other live session, in the attention order
//! of the `/` palette — whose click or Enter opens the review modal on it.
//!
//! This module is the view's state, what it reads off the tree, and its
//! drawing; the keys and requests live in `event_loop::orchestrator`.

use nebula_core::{Agent, AgentId, AgentKind, AgentStatus, ReviewTabKind, SessionRef};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, HitTarget};

/// Below this many columns the strip collapses to one summary chip.
pub const STRIP_MIN_WIDTH: u16 = 60;

/// The view's own state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrchState {
    /// F10 put the view up; it holds while the orchestrator's session is
    /// the full-screen pane.
    pub view: bool,
    /// The chip the keyboard is on, with the pane unlocked (`^Q`).
    pub chip: usize,
    /// A review the orchestrator opened while the user was typing into
    /// another session: held, with a toast, until F10 shows it.
    pub pending_review: Option<(AgentId, Vec<SessionRef>, Vec<ReviewTabKind>)>,
    /// What the pane showed before a review moved it onto the session
    /// under review: put back when the review closes, however it closes.
    pub review_restore: Option<SessionRef>,
    /// The protocol the daemon answered the handshake with; None before
    /// one (tests). Below [`ORCHESTRATOR_PROTOCOL`] it cannot decode the
    /// provisioning requests, and sending one would drop the connection.
    pub daemon_protocol: Option<u32>,
}

/// The protocol that grew `StartOrchestrator`, `SetOrchestrator` and
/// `ClearOrchestrator`.
pub const ORCHESTRATOR_PROTOCOL: u32 = 54;

/// What F10 and the card menu say to a daemon too old for provisioning.
pub const DAEMON_TOO_OLD: &str = "the running daemon predates the orchestrator: run `kete reload` (every session keeps running), then reopen kete";

/// Can the daemon take the provisioning requests?
pub fn daemon_provisions(app: &App) -> bool {
    app.orch
        .daemon_protocol
        .is_none_or(|v| v >= ORCHESTRATOR_PROTOCOL)
}

/// The pinned orchestrator, when one is live (archived rows never count).
pub fn orchestrator(app: &App) -> Option<&Agent> {
    app.tree
        .agents
        .iter()
        .find(|a| a.role.is_orchestrator() && !a.archived)
}

/// Is the ORCHESTRATOR VIEW on screen: F10's flag, a full-screen pane, and
/// the orchestrator's session in it.
pub fn active(app: &App) -> bool {
    let Some(orch) = orchestrator(app) else {
        return false;
    };
    app.orch.view
        && app.pane.collapsed
        && app
            .pane
            .term
            .as_ref()
            .is_some_and(|t| t.sref == SessionRef::Agent(orch.id.clone()))
}

/// One chip on the strip.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub sref: SessionRef,
    pub name: String,
    pub status: AgentStatus,
    pub unseen: bool,
}

/// Every live session but the orchestrator, in attention order:
/// needs-feedback, running, finished-unread, the rest.
pub fn chips(app: &App) -> Vec<Chip> {
    let orch = orchestrator(app).map(|a| a.id.clone());
    crate::palette::attention_sessions(&app.tree)
        .into_iter()
        .filter(|id| Some(id) != orch.as_ref())
        .filter_map(|id| app.tree.agents.iter().find(|a| a.id == id))
        .map(|a| Chip {
            sref: SessionRef::Agent(a.id.clone()),
            name: a.name.clone(),
            status: a.status,
            unseen: a.unseen,
        })
        .collect()
}

/// The strip's row, carved off the top of `body` while the view is up;
/// None leaves the body whole.
pub fn split(app: &App, body: Rect) -> (Option<Rect>, Rect) {
    if !active(app) || body.height < 4 {
        return (None, body);
    }
    let strip = Rect { height: 1, ..body };
    let rest = Rect {
        y: body.y + 1,
        height: body.height - 1,
        ..body
    };
    (Some(strip), rest)
}

/// What a narrow strip says instead of chips: the count, and how many
/// need the user.
fn summary(chips: &[Chip]) -> String {
    let asking = chips
        .iter()
        .filter(|c| c.status == AgentStatus::NeedsFeedback)
        .count();
    let sessions = if chips.len() == 1 {
        "session"
    } else {
        "sessions"
    };
    if asking > 0 {
        format!(" ◆ {} {sessions} · {asking} need you ", chips.len())
    } else {
        format!(" ◆ {} {sessions} ", chips.len())
    }
}

/// Draw the STATUS STRIP into `area`, registering a click target per chip.
pub fn draw_strip(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.chrome.theme;
    let chips = chips(app);
    if app.orch.chip >= chips.len() {
        app.orch.chip = chips.len().saturating_sub(1);
    }
    let keyboard = !app.pane.term_locked;
    let mut spans = vec![Span::styled(
        " orchestrator ",
        Style::default()
            .fg(th.on_accent)
            .bg(th.accent)
            .add_modifier(Modifier::BOLD),
    )];
    let mut x = area.x + Span::raw(" orchestrator ").width() as u16;
    if chips.is_empty() {
        spans.push(Span::styled(
            "  no other sessions",
            Style::default().fg(th.dim),
        ));
    } else if area.width < STRIP_MIN_WIDTH {
        let text = summary(&chips);
        let w = Span::raw(text.as_str()).width() as u16;
        // The summary chip opens the review on the first session in need.
        app.chrome.hits.push((
            Rect {
                x,
                y: area.y,
                width: w.min(area.right().saturating_sub(x)),
                height: 1,
            },
            HitTarget::OrchestratorChip(chips[0].sref.clone()),
        ));
        spans.push(Span::styled(text, Style::default().fg(th.text)));
    } else {
        for (i, chip) in chips.iter().enumerate() {
            let dot = crate::ui::status_color(Some(chip.status), chip.unseen, th);
            let name = crate::ui::fit_to_width(&chip.name, 18);
            let label = format!(" ● {name} ");
            let w = Span::raw(label.as_str()).width() as u16 + 1;
            if x + w > area.right() {
                let more = format!(" +{}", chips.len() - i);
                spans.push(Span::styled(more, Style::default().fg(th.dim)));
                break;
            }
            spans.push(Span::raw(" "));
            let selected = keyboard && i == app.orch.chip;
            let base = if selected {
                Style::default().bg(th.accent).fg(th.on_accent)
            } else {
                Style::default().fg(th.text)
            };
            spans.push(Span::styled(
                " ● ",
                base.fg(if selected { th.on_accent } else { dot }),
            ));
            spans.push(Span::styled(format!("{name} "), base));
            app.chrome.hits.push((
                Rect {
                    x: x + 1,
                    y: area.y,
                    width: w - 1,
                    height: 1,
                },
                HitTarget::OrchestratorChip(chip.sref.clone()),
            ));
            x += w;
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The F10 prompt that starts the first orchestrator: a harness and a
/// profile, Enter to start.
#[derive(Debug, Clone, PartialEq)]
pub struct StartView {
    pub harnesses: Vec<AgentKind>,
    pub harness: usize,
    /// Profile names for the chosen harness; index 0 is the harness's own
    /// default (no profile named).
    pub profiles: Vec<nebula_core::profiles::Profile>,
    pub profile: usize,
    /// 0: the harness row has the keys; 1: the profile row.
    pub row: usize,
    pub area: Rect,
}

impl StartView {
    pub fn new(profiles: Vec<nebula_core::profiles::Profile>) -> Self {
        Self {
            harnesses: AgentKind::ALL
                .into_iter()
                .filter(|k| *k != AgentKind::Custom)
                .collect(),
            harness: 0,
            profiles,
            profile: 0,
            row: 0,
            area: Rect::default(),
        }
    }

    pub fn kind(&self) -> AgentKind {
        self.harnesses
            .get(self.harness)
            .copied()
            .unwrap_or(AgentKind::Claude)
    }

    /// The profiles the chosen harness can run under.
    pub fn harness_profiles(&self) -> Vec<&nebula_core::profiles::Profile> {
        nebula_core::profiles::for_harness(&self.profiles, self.kind().as_str())
    }

    /// The profile named for the start; None = the harness's default.
    pub fn profile_name(&self) -> Option<String> {
        if self.profile == 0 {
            return None;
        }
        self.harness_profiles()
            .get(self.profile - 1)
            .map(|p| p.name.clone())
    }

    pub fn profile_label(&self) -> String {
        self.profile_name()
            .unwrap_or_else(|| "harness default".to_string())
    }

    /// Step the focused row's value by `delta`, wrapping.
    pub fn step(&mut self, delta: i64) {
        if self.row == 0 {
            let n = self.harnesses.len().max(1) as i64;
            self.harness = (self.harness as i64 + delta).rem_euclid(n) as usize;
            self.profile = 0;
        } else {
            let n = self.harness_profiles().len() as i64 + 1;
            self.profile = (self.profile as i64 + delta).rem_euclid(n) as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, harness: &str) -> nebula_core::profiles::Profile {
        nebula_core::profiles::Profile {
            name: name.into(),
            harness: harness.into(),
            env: Default::default(),
            default: false,
            use_default: false,
        }
    }

    #[test]
    fn the_start_prompt_cycles_harnesses_and_their_profiles() {
        let mut view = StartView::new(vec![profile("work", "claude"), profile("alt", "codex")]);
        assert_eq!(view.kind(), AgentKind::Claude);
        assert_eq!(view.profile_name(), None);
        view.row = 1;
        view.step(1);
        assert_eq!(view.profile_name().as_deref(), Some("work"));
        view.step(1);
        assert_eq!(view.profile_name(), None, "wraps back to the default");
        view.row = 0;
        view.step(1);
        assert_eq!(view.kind(), AgentKind::Codex);
        view.row = 1;
        view.step(1);
        assert_eq!(view.profile_name().as_deref(), Some("alt"));
        view.row = 0;
        view.step(-2);
        assert!(!view.harnesses.contains(&AgentKind::Custom));
        assert_eq!(view.profile, 0, "a harness change drops the profile");
    }

    #[test]
    fn a_narrow_strip_counts_sessions_and_who_needs_you() {
        let chip = |status| Chip {
            sref: SessionRef::Agent(AgentId("a".into())),
            name: "a".into(),
            status,
            unseen: false,
        };
        assert_eq!(summary(&[chip(AgentStatus::Running)]), " ◆ 1 session ");
        assert_eq!(
            summary(&[chip(AgentStatus::NeedsFeedback), chip(AgentStatus::Running)]),
            " ◆ 2 sessions · 1 need you "
        );
    }
}
