//! The REVIEW MODAL the orchestrator opens (`kete orchestrator review`, or a
//! chip on the ORCHESTRATOR VIEW's strip): one or more sessions, each with
//! four tabs — Terminal (the session's own pane), Diff (its worktree's
//! uncommitted changes), History (its transcript-aware recent context) and
//! PR (the pull request on its checkout).
//!
//! This module is the modal's state and its keys and clicks, all pure:
//! anything that needs the daemon or the pane (attaching a session, asking
//! for its history, putting the pane back) is left on the view for
//! `event_loop::orchestrator::settle_review` to carry out after the input.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use nebula_core::{AgentId, ReviewTabKind, SessionRef};
use ratatui::layout::Rect;

use crate::app::{App, Overlay};

/// Something a tab reads off the machine: asked for, in, or refused.
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded {
    Loading,
    Ready(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewView {
    /// The orchestrator that opened it; None when the user did (a chip).
    pub opener: Option<AgentId>,
    pub sessions: Vec<SessionRef>,
    pub session: usize,
    pub tabs: Vec<ReviewTabKind>,
    pub tab: usize,
    pub area: Rect,
    pub body_area: Rect,
    pub tab_hits: Vec<(u16, u16)>,
    /// Columns of each session's name on the session row.
    pub session_hits: Vec<(u16, u16)>,
    /// Lines the Diff / History / PR body is scrolled down.
    pub scroll: u16,
    pub diffs: HashMap<SessionRef, Loaded>,
    pub histories: HashMap<SessionRef, Loaded>,
    /// The ReadSession in flight, by request id.
    pub history_req: Option<(u64, SessionRef)>,
    /// Enter (or a click) on the Terminal tab: close the modal and type
    /// into this session.
    pub jump: Option<SessionRef>,
    /// Enter on the PR tab: open this pull request in the browser.
    pub open_url: Option<String>,
    /// The selected session's pull request, as the last draw found it.
    pub pr_url: Option<String>,
}

impl ReviewView {
    pub fn new(
        opener: Option<AgentId>,
        sessions: Vec<SessionRef>,
        tabs: Vec<ReviewTabKind>,
    ) -> Self {
        let tabs = if tabs.is_empty() {
            vec![
                ReviewTabKind::Terminal,
                ReviewTabKind::Diff,
                ReviewTabKind::History,
                ReviewTabKind::PullRequest,
            ]
        } else {
            tabs
        };
        Self {
            opener,
            sessions,
            session: 0,
            tabs,
            tab: 0,
            area: Rect::default(),
            body_area: Rect::default(),
            tab_hits: Vec::new(),
            session_hits: Vec::new(),
            scroll: 0,
            diffs: HashMap::new(),
            histories: HashMap::new(),
            history_req: None,
            jump: None,
            open_url: None,
            pr_url: None,
        }
    }

    pub fn selected_session(&self) -> Option<&SessionRef> {
        self.sessions.get(self.session)
    }

    pub fn selected_tab(&self) -> Option<ReviewTabKind> {
        self.tabs.get(self.tab).copied()
    }

    fn select_tab(&mut self, index: i64) {
        if self.tabs.is_empty() {
            return;
        }
        self.tab = index.rem_euclid(self.tabs.len() as i64) as usize;
        self.scroll = 0;
    }

    fn select_session(&mut self, index: i64) {
        if self.sessions.is_empty() {
            return;
        }
        self.session = index.rem_euclid(self.sessions.len() as i64) as usize;
        self.scroll = 0;
    }

    fn scroll_by(&mut self, delta: i32) {
        self.scroll = (i32::from(self.scroll) + delta).clamp(0, i32::from(u16::MAX)) as u16;
    }
}

/// The review modal over whatever is on screen.
pub fn open(
    app: &mut App,
    opener: Option<AgentId>,
    sessions: Vec<SessionRef>,
    tabs: Vec<ReviewTabKind>,
) {
    app.modals.overlay = Some(Overlay::Review(ReviewView::new(opener, sessions, tabs)));
    app.chrome.dirty = true;
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(Overlay::Review(view)) = &mut app.modals.overlay else {
        return false;
    };
    let page = i32::from(view.body_area.height.max(2)) - 1;
    match key.code {
        KeyCode::Esc => {
            app.modals.overlay = None;
        }
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
            view.select_tab(view.tab as i64 - 1)
        }
        KeyCode::BackTab => view.select_tab(view.tab as i64 - 1),
        KeyCode::Tab => view.select_tab(view.tab as i64 + 1),
        KeyCode::Char(c) if ('1'..='9').contains(&c) => {
            let index = c as usize - '1' as usize;
            if index < view.tabs.len() {
                view.select_tab(index as i64);
            }
        }
        // The sessions under review, one after another.
        KeyCode::Left | KeyCode::Char('[') | KeyCode::Char('h') => {
            view.select_session(view.session as i64 - 1)
        }
        KeyCode::Right | KeyCode::Char(']') | KeyCode::Char('l') => {
            view.select_session(view.session as i64 + 1)
        }
        KeyCode::Down | KeyCode::Char('j') => view.scroll_by(1),
        KeyCode::Up | KeyCode::Char('k') => view.scroll_by(-1),
        KeyCode::PageDown | KeyCode::Char(' ') => view.scroll_by(page),
        KeyCode::PageUp => view.scroll_by(-page),
        KeyCode::Home | KeyCode::Char('g') => view.scroll = 0,
        KeyCode::Enter => enter(view),
        _ => return false,
    }
    app.chrome.dirty = true;
    true
}

/// What Enter does on the selected tab: type into the session from the
/// Terminal tab, open the pull request from the PR tab (the draw left its
/// URL on the view).
fn enter(view: &mut ReviewView) {
    match view.selected_tab() {
        Some(ReviewTabKind::Terminal) => view.jump = view.selected_session().cloned(),
        Some(ReviewTabKind::PullRequest) => view.open_url = view.pr_url.clone(),
        _ => {}
    }
}

pub fn handle_mouse(app: &mut App, mouse: MouseEvent) -> bool {
    let Some(Overlay::Review(view)) = &mut app.modals.overlay else {
        return false;
    };
    let (x, y) = (mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if !contains(view.area, x, y) {
                app.modals.overlay = None;
            } else if let Some(idx) = (y == view.area.y).then(|| hit(&view.tab_hits, x)).flatten() {
                view.select_tab(idx as i64);
            } else if let Some(idx) = (y == view.area.y + 1)
                .then(|| hit(&view.session_hits, x))
                .flatten()
            {
                view.select_session(idx as i64);
            } else if contains(view.body_area, x, y)
                && view.selected_tab() == Some(ReviewTabKind::Terminal)
            {
                // The session's own screen: a click goes to it, to type.
                view.jump = view.selected_session().cloned();
            }
        }
        MouseEventKind::ScrollDown if contains(view.area, x, y) => view.scroll_by(3),
        MouseEventKind::ScrollUp if contains(view.area, x, y) => view.scroll_by(-3),
        _ => return false,
    }
    app.chrome.dirty = true;
    true
}

fn hit(hits: &[(u16, u16)], x: u16) -> Option<usize> {
    hits.iter().position(|(a, b)| x >= *a && x < *b)
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

pub fn tab_label(tab: ReviewTabKind) -> &'static str {
    match tab {
        ReviewTabKind::Terminal => "Terminal",
        ReviewTabKind::Diff => "Diff",
        ReviewTabKind::History => "History",
        ReviewTabKind::PullRequest => "PR",
    }
}

/// A worktree's uncommitted changes as one unified diff, every changed
/// file in git's order, capped so a huge change stays a modal's worth.
pub fn read_worktree_diff(root: &std::path::Path) -> Result<String, String> {
    const MAX_LINES: usize = 4000;
    let files = crate::git_diff::changed_files(root)?;
    if files.is_empty() {
        return Ok(String::new());
    }
    let head_ok = crate::git_diff::has_head(root);
    let mut out = String::new();
    for file in &files {
        out.push_str(&crate::git_diff::diff_for(root, file, head_ok, None));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if out.lines().count() > MAX_LINES {
            break;
        }
    }
    Ok(crate::git_diff::cap_lines(&out, MAX_LINES, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn agent(id: &str) -> SessionRef {
        SessionRef::Agent(AgentId(id.into()))
    }

    fn app_with_review() -> App {
        let mut app = App::new();
        open(
            &mut app,
            Some(AgentId("orch".into())),
            vec![agent("a1"), agent("a2")],
            Vec::new(),
        );
        app
    }

    fn view(app: &App) -> &ReviewView {
        match &app.modals.overlay {
            Some(Overlay::Review(view)) => view,
            other => panic!("expected review overlay, got {other:?}"),
        }
    }

    fn press(app: &mut App, code: KeyCode) -> bool {
        handle_key(app, KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn tab_and_number_keys_switch_tabs() {
        let mut app = app_with_review();
        assert_eq!(view(&app).tab, 0);
        assert!(press(&mut app, KeyCode::Tab));
        assert_eq!(view(&app).tab, 1);
        assert!(handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT)
        ));
        assert_eq!(view(&app).tab, 0);
        assert!(press(&mut app, KeyCode::Char('4')));
        assert_eq!(view(&app).tab, 3);
    }

    #[test]
    fn arrows_and_brackets_walk_the_sessions_under_review() {
        let mut app = app_with_review();
        assert_eq!(view(&app).selected_session(), Some(&agent("a1")));
        press(&mut app, KeyCode::Right);
        assert_eq!(view(&app).selected_session(), Some(&agent("a2")));
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(view(&app).selected_session(), Some(&agent("a1")), "wraps");
        press(&mut app, KeyCode::Char('['));
        assert_eq!(view(&app).selected_session(), Some(&agent("a2")));
    }

    #[test]
    fn enter_on_the_terminal_tab_asks_to_type_into_that_session() {
        let mut app = app_with_review();
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Enter);
        assert_eq!(view(&app).jump, Some(agent("a2")));
    }

    #[test]
    fn esc_closes_review_modal() {
        let mut app = app_with_review();
        assert!(press(&mut app, KeyCode::Esc));
        assert!(app.modals.overlay.is_none());
    }

    #[test]
    fn clicks_inside_reach_the_tabs_the_session_row_and_the_terminal() {
        let mut app = app_with_review();
        if let Some(Overlay::Review(v)) = &mut app.modals.overlay {
            v.area = Rect::new(0, 0, 80, 20);
            v.body_area = Rect::new(1, 3, 78, 16);
            v.tab_hits = vec![(2, 12), (13, 20), (21, 30), (31, 35)];
            v.session_hits = vec![(2, 10), (11, 19)];
        }
        let click = |x, y| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        };
        assert!(handle_mouse(&mut app, click(14, 1)), "the session row");
        assert_eq!(view(&app).selected_session(), Some(&agent("a2")));
        assert!(handle_mouse(&mut app, click(5, 10)), "the terminal body");
        assert_eq!(view(&app).jump, Some(agent("a2")));
        assert!(handle_mouse(&mut app, click(22, 0)), "a tab");
        assert_eq!(view(&app).selected_tab(), Some(ReviewTabKind::History));
    }
}
