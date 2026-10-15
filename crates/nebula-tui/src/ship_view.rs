//! The SHIP OVERLAY: the board's pull request verbs, in the terminal. Two
//! forms over one checkout:
//!
//! - the PULL REQUEST COMPOSER (Review dropped on Ship, or **Create pull
//!   request…**): title, a multi-line body, the base branch and a draft
//!   toggle, prefilled from the session and the commits since the base
//!   (`kete_ado::ship::suggest_title_body`); Enter pushes the branch and
//!   opens the pull request;
//! - the MERGE CONFIRM (Ship dropped on Done, or **Merge pull request…**):
//!   the method and whether the remote branch goes.
//!
//! The work is `kete_ado::ship`'s, the code `kete ship` and the desktop run:
//! GitHub through `gh`, Azure DevOps through its API, never an interactive
//! sign-in. It runs off the loop as a [`ShipJob`] and lands as a
//! [`ShipAnswer`] (`event_loop::ship`). A failure is a typed
//! [`ShipError`] the form shows with what to do about it: a sign-in opens a
//! terminal running the login, a missing `gh` or a merge Azure DevOps
//! keeps for its page opens a browser.
//!
//! This module is the state and its keys, all pure: the event loop carries
//! out what a key asks for ([`Outcome`]).

use crate::text_input::TextInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use kete_ado::ship::{
    Created, ForgeKind, MergeMethod, MergeOpts, Merged, NewPr, ShipError, ShipErrorKind, Suggestion,
};
use nebula_core::{AgentId, WorktreeId};
use ratatui::layout::Rect;
use std::path::PathBuf;

/// The overlay: which checkout, which form, and where the work stands.
#[derive(Debug, Clone)]
pub struct ShipView {
    pub worktree: WorktreeId,
    /// The card it was opened from, when it was: the session whose name
    /// and last prompt the suggestion reads, and the card the board
    /// returns to.
    pub agent: Option<AgentId>,
    pub branch: String,
    pub dir: PathBuf,
    pub form: Form,
    pub stage: Stage,
    /// The job this view waits on: an answer with another ticket is an
    /// earlier view's and only flashes.
    pub ticket: u64,
    /// The REVIEW UNIT's diff the form was opened from, put back when it
    /// closes without shipping.
    pub back: Option<Box<crate::app::DiffView>>,
    /// Written back by the draw: the box, and each button's cells.
    pub area: Rect,
    pub buttons: Vec<(Rect, Button)>,
}

#[derive(Debug, Clone)]
pub enum Form {
    Compose(Composer),
    Merge(MergeForm),
}

#[derive(Debug, Clone)]
pub struct Composer {
    pub title: TextInput,
    pub body: TextInput,
    /// The branch it merges into; empty is the review base.
    pub base: TextInput,
    pub draft: bool,
    pub field: Field,
    /// The title or body has been typed in: the suggestion landing late
    /// leaves them alone.
    pub edited: bool,
    pub base_edited: bool,
    /// The suggestion (commits since the base) is still being read.
    pub suggesting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Body,
    Base,
    Draft,
}

impl Field {
    const ORDER: [Field; 4] = [Field::Title, Field::Body, Field::Base, Field::Draft];

    fn step(self, by: i64) -> Field {
        let i = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0) as i64;
        Self::ORDER[(i + by).rem_euclid(Self::ORDER.len() as i64) as usize]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeForm {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub method: MergeMethod,
    pub delete_branch: bool,
}

/// The merge methods, in the order ←/→ walk them.
pub const METHODS: [MergeMethod; 3] =
    [MergeMethod::Squash, MergeMethod::Merge, MergeMethod::Rebase];

pub fn method_word(m: MergeMethod) -> &'static str {
    match m {
        MergeMethod::Squash => "squash",
        MergeMethod::Merge => "merge",
        MergeMethod::Rebase => "rebase",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Editing,
    /// The push, the create or the merge is running.
    Working,
    /// It failed: what, and (by its kind) what to do about it.
    Failed(ShipError),
}

/// A clickable cell of the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Field(Field),
    Method(MergeMethod),
    DeleteBranch,
    Submit,
    Cancel,
    /// The failure's own way out ([`error_action`]).
    Fix,
    /// Back from the failure to the form.
    Dismiss,
}

/// What a key or a click asks the event loop for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// Close the overlay (back to the review unit when it came from one).
    Close,
    /// Run the form: create the pull request, or merge it.
    Submit,
    /// The failure's fix.
    Fix(Fix),
    /// Say this in the footer and stay.
    Say(&'static str),
}

/// What a failure offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fix {
    /// Open a terminal in the checkout running this sign-in.
    SignIn(String),
    OpenUrl(String),
    Retry,
}

/// Where `gh` comes from.
pub const GH_INSTALL_URL: &str = "https://cli.github.com";

/// The fix a failure offers, with the words its button says: a sign-in
/// opens a terminal on the forge's login, a missing `gh` its download
/// page, a merge Azure DevOps keeps for itself the pull request's page,
/// and a dropped connection or a refusal another try. None where trying
/// again cannot help (no forge, no branch, no pull request).
pub fn error_action(err: &ShipError) -> Option<(Fix, String)> {
    match err.kind {
        ShipErrorKind::SignIn => {
            let line = match err.forge {
                Some(ForgeKind::Ado) => "az login",
                _ => "gh auth login",
            };
            Some((
                Fix::SignIn(line.into()),
                format!("open a terminal running `{line}`"),
            ))
        }
        ShipErrorKind::NoTool => Some((
            Fix::OpenUrl(GH_INSTALL_URL.into()),
            "open cli.github.com to install gh".into(),
        )),
        ShipErrorKind::MergeInAdo => err.url.clone().map(|url| {
            (
                Fix::OpenUrl(url),
                "finish it on the pull request's page".into(),
            )
        }),
        ShipErrorKind::Offline | ShipErrorKind::Other => Some((Fix::Retry, "try again".into())),
        ShipErrorKind::NoForge | ShipErrorKind::NoBranch | ShipErrorKind::NoPr => None,
    }
}

impl Composer {
    /// A composer prefilled from `suggestion`, the base left to the review
    /// base until the read says what it is.
    pub fn new(suggestion: Suggestion) -> Self {
        let mut body = TextInput::multiline_with_text(suggestion.body);
        body.cursor_to_start();
        Self {
            title: TextInput::with_text(suggestion.title),
            body,
            base: TextInput::new(),
            draft: false,
            field: Field::Title,
            edited: false,
            base_edited: false,
            suggesting: true,
        }
    }

    /// What `create_pr` is asked for.
    pub fn new_pr(&self) -> NewPr {
        let base = self.base.as_str().trim();
        NewPr {
            title: self.title.as_str().trim().to_string(),
            body: self.body.as_str().to_string(),
            draft: self.draft,
            base: (!base.is_empty()).then(|| base.to_string()),
        }
    }

    /// The read of the commits since the base landed: it fills what the
    /// user has not typed in.
    pub fn suggested(&mut self, suggestion: Suggestion, base: Option<String>) {
        self.suggesting = false;
        if !self.edited {
            self.title.set_text(suggestion.title);
            self.body.set_text(suggestion.body);
            self.body.cursor_to_start();
        }
        if !self.base_edited && self.base.as_str().is_empty() {
            if let Some(base) = base {
                self.base.set_text(base);
            }
        }
    }
}

impl MergeForm {
    pub fn opts(&self) -> MergeOpts {
        MergeOpts {
            method: self.method,
            delete_branch: self.delete_branch,
        }
    }

    fn step_method(&mut self, by: i64) {
        let i = METHODS.iter().position(|m| *m == self.method).unwrap_or(0) as i64;
        self.method = METHODS[(i + by).rem_euclid(METHODS.len() as i64) as usize];
    }
}

impl ShipView {
    pub fn is_compose(&self) -> bool {
        matches!(self.form, Form::Compose(_))
    }

    /// The box's title.
    pub fn title(&self) -> String {
        match &self.form {
            Form::Compose(_) => format!(" Pull request: {} ", self.branch),
            Form::Merge(m) => format!(" Merge #{}: {} ", m.number, self.branch),
        }
    }

    /// A key, in whatever stage the view is.
    pub fn key(&mut self, key: &KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match &self.stage {
            // The job runs whatever happens to the box; Esc only takes
            // the box away, and what lands says itself.
            Stage::Working => match key.code {
                KeyCode::Esc => Outcome::Close,
                _ => Outcome::Say("still working: Esc closes this, the result still lands"),
            },
            Stage::Failed(err) => match key.code {
                KeyCode::Enter => match error_action(err) {
                    Some((fix, _)) => Outcome::Fix(fix),
                    None => {
                        self.stage = Stage::Editing;
                        Outcome::None
                    }
                },
                KeyCode::Esc => {
                    self.stage = Stage::Editing;
                    Outcome::None
                }
                _ => Outcome::None,
            },
            Stage::Editing => match &mut self.form {
                Form::Compose(c) => compose_key(c, key, ctrl),
                Form::Merge(m) => merge_key(m, key),
            },
        }
    }

    /// A click on one of the form's cells.
    pub fn click(&mut self, button: Button) -> Outcome {
        match button {
            Button::Cancel => self.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Button::Dismiss => {
                self.stage = Stage::Editing;
                Outcome::None
            }
            Button::Fix | Button::Submit => {
                self.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            }
            _ if self.stage != Stage::Editing => Outcome::None,
            Button::Field(field) => {
                if let Form::Compose(c) = &mut self.form {
                    if field == Field::Draft && c.field == Field::Draft {
                        c.draft = !c.draft;
                    }
                    c.field = field;
                }
                Outcome::None
            }
            Button::Method(method) => {
                if let Form::Merge(m) = &mut self.form {
                    m.method = method;
                }
                Outcome::None
            }
            Button::DeleteBranch => {
                if let Form::Merge(m) = &mut self.form {
                    m.delete_branch = !m.delete_branch;
                }
                Outcome::None
            }
        }
    }
}

fn compose_key(c: &mut Composer, key: &KeyEvent, ctrl: bool) -> Outcome {
    match key.code {
        KeyCode::Esc => return Outcome::Close,
        KeyCode::Tab => {
            c.field = c.field.step(1);
            return Outcome::None;
        }
        KeyCode::BackTab => {
            c.field = c.field.step(-1);
            return Outcome::None;
        }
        // Shift+Enter (and the chords standing in for it) break a line in
        // the body; Enter anywhere opens the pull request.
        KeyCode::Enter if !(c.field == Field::Body && c.body.takes_newline(key)) => {
            return if c.title.as_str().trim().is_empty() {
                c.field = Field::Title;
                Outcome::Say("a pull request needs a title")
            } else {
                Outcome::Submit
            };
        }
        _ => {}
    }
    match c.field {
        Field::Draft => {
            if matches!(key.code, KeyCode::Char(' ') | KeyCode::Char('x')) && !ctrl {
                c.draft = !c.draft;
            }
        }
        Field::Title => {
            if c.title.handle_key(key).changed() {
                c.edited = true;
            }
        }
        Field::Body => {
            if c.body.handle_key(key).changed() {
                c.edited = true;
            }
        }
        Field::Base => {
            if c.base.handle_key(key).changed() {
                c.base_edited = true;
            }
        }
    }
    Outcome::None
}

fn merge_key(m: &mut MergeForm, key: &KeyEvent) -> Outcome {
    match key.code {
        KeyCode::Esc => Outcome::Close,
        KeyCode::Enter => Outcome::Submit,
        KeyCode::Left | KeyCode::BackTab => {
            m.step_method(-1);
            Outcome::None
        }
        KeyCode::Right | KeyCode::Tab => {
            m.step_method(1);
            Outcome::None
        }
        KeyCode::Char('s') => {
            m.method = MergeMethod::Squash;
            Outcome::None
        }
        KeyCode::Char('m') => {
            m.method = MergeMethod::Merge;
            Outcome::None
        }
        KeyCode::Char('r') => {
            m.method = MergeMethod::Rebase;
            Outcome::None
        }
        KeyCode::Char(' ') | KeyCode::Char('d') => {
            m.delete_branch = !m.delete_branch;
            Outcome::None
        }
        _ => Outcome::None,
    }
}

// ---- the work ----

/// What runs off the loop for the overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShipJob {
    /// The title and body from the commits since the base, and the base.
    Suggest {
        dir: PathBuf,
        base_setting: String,
        name: Option<String>,
        prompt: Option<String>,
    },
    /// Push the branch and open its pull request.
    Create {
        dir: PathBuf,
        base_setting: String,
        pr: NewPr,
    },
    Merge {
        dir: PathBuf,
        base_setting: String,
        opts: MergeOpts,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShipOutcome {
    Suggested {
        suggestion: Suggestion,
        base: Option<String>,
    },
    Created(Result<Created, ShipError>),
    Merged(Result<Merged, ShipError>),
}

/// A job's result, back on the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipAnswer {
    pub ticket: u64,
    pub worktree: WorktreeId,
    pub outcome: ShipOutcome,
}

/// Run `job` against the real forge: `kete_ado::ship` with its silent
/// sign-ins, as `kete ship` runs it. Never called by a test.
pub async fn run(job: ShipJob) -> ShipOutcome {
    use kete_ado::ship::{Place, Ship};
    static ADO: std::sync::OnceLock<std::sync::Arc<kete_ado::AdoState>> =
        std::sync::OnceLock::new();
    let state = ADO
        .get_or_init(|| std::sync::Arc::new(kete_ado::AdoState::system()))
        .clone();
    let ship = Ship::new(&state, true);
    match job {
        ShipJob::Suggest {
            dir,
            base_setting,
            name,
            prompt,
        } => {
            let place = Place::new(dir, base_setting);
            let suggestion = place.suggest(name.as_deref(), prompt.as_deref()).await;
            let base = place.base_branch().await;
            ShipOutcome::Suggested { suggestion, base }
        }
        ShipJob::Create {
            dir,
            base_setting,
            pr,
        } => ShipOutcome::Created(ship.create_pr(&Place::new(dir, base_setting), pr).await),
        ShipJob::Merge {
            dir,
            base_setting,
            opts,
        } => ShipOutcome::Merged(ship.merge_pr(&Place::new(dir, base_setting), opts).await),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(form: Form) -> ShipView {
        ShipView {
            worktree: WorktreeId("w1".into()),
            agent: None,
            branch: "feat".into(),
            dir: "/nowhere".into(),
            form,
            stage: Stage::Editing,
            ticket: 1,
            back: None,
            area: Rect::default(),
            buttons: Vec::new(),
        }
    }

    fn press(v: &mut ShipView, code: KeyCode, mods: KeyModifiers) -> Outcome {
        v.key(&KeyEvent::new(code, mods))
    }

    fn type_in(v: &mut ShipView, text: &str) {
        for c in text.chars() {
            press(v, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    fn composer(v: &ShipView) -> &Composer {
        match &v.form {
            Form::Compose(c) => c,
            Form::Merge(_) => panic!("a composer"),
        }
    }

    /// Tab walks the fields round; typing edits the one the cursor is in;
    /// Shift+Enter breaks a line in the body and Enter submits from
    /// anywhere, once there is a title. A late suggestion leaves what was
    /// typed alone but still fills an untouched base.
    #[test]
    fn the_composer_walks_its_fields_and_submits_with_a_title() {
        let mut v = view(Form::Compose(Composer::new(Suggestion {
            title: "Fix login".into(),
            body: "## Summary\nfix it".into(),
        })));
        assert_eq!(composer(&v).title.as_str(), "Fix login");
        assert!(composer(&v).suggesting);
        press(&mut v, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(composer(&v).field, Field::Body);
        press(&mut v, KeyCode::End, KeyModifiers::CONTROL);
        type_in(&mut v, "more");
        press(&mut v, KeyCode::Enter, KeyModifiers::SHIFT);
        type_in(&mut v, "x");
        assert!(composer(&v).body.as_str().contains('\n'));
        assert!(composer(&v).edited);
        press(&mut v, KeyCode::Tab, KeyModifiers::NONE);
        press(&mut v, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(composer(&v).field, Field::Draft);
        press(&mut v, KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(composer(&v).draft);
        press(&mut v, KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(composer(&v).field, Field::Base);

        if let Form::Compose(c) = &mut v.form {
            c.suggested(
                Suggestion {
                    title: "Other".into(),
                    body: "other".into(),
                },
                Some("main".into()),
            );
        }
        assert_eq!(composer(&v).title.as_str(), "Fix login", "typed: kept");
        assert_eq!(composer(&v).base.as_str(), "main", "untouched: filled");
        assert!(!composer(&v).suggesting);
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Submit
        );
        let pr = composer(&v).new_pr();
        assert_eq!(pr.title, "Fix login");
        assert_eq!(pr.base.as_deref(), Some("main"));
        assert!(pr.draft);

        // No title: Enter says so and puts the cursor there.
        if let Form::Compose(c) = &mut v.form {
            c.title.clear();
            c.field = Field::Body;
        }
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Say("a pull request needs a title")
        );
        assert_eq!(composer(&v).field, Field::Title);
        assert_eq!(
            press(&mut v, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Close
        );
    }

    /// The merge confirm walks the methods, toggles the remote branch and
    /// merges on Enter.
    #[test]
    fn the_merge_confirm_picks_a_method_and_the_branch() {
        let mut v = view(Form::Merge(MergeForm {
            number: 7,
            title: "t".into(),
            url: "u".into(),
            method: MergeMethod::Squash,
            delete_branch: true,
        }));
        press(&mut v, KeyCode::Right, KeyModifiers::NONE);
        press(&mut v, KeyCode::Char(' '), KeyModifiers::NONE);
        let Form::Merge(m) = &v.form else { panic!() };
        assert_eq!(m.opts().method, MergeMethod::Merge);
        assert!(!m.opts().delete_branch);
        press(&mut v, KeyCode::Left, KeyModifiers::NONE);
        press(&mut v, KeyCode::Left, KeyModifiers::NONE);
        let Form::Merge(m) = &v.form else { panic!() };
        assert_eq!(m.method, MergeMethod::Rebase, "round the end");
        v.click(Button::Method(MergeMethod::Squash));
        let Form::Merge(m) = &v.form else { panic!() };
        assert_eq!(m.method, MergeMethod::Squash);
        assert_eq!(v.click(Button::Submit), Outcome::Submit);
    }

    /// A failure offers its kind's fix on Enter; Esc goes back to the form;
    /// while working, keys wait and Esc only closes.
    #[test]
    fn a_failure_offers_its_fix() {
        let mut v = view(Form::Compose(Composer::new(Suggestion::default())));
        let fail = |kind, forge| ShipError {
            kind,
            forge,
            message: "no".into(),
            url: None,
        };
        v.stage = Stage::Failed(fail(ShipErrorKind::SignIn, Some(ForgeKind::GitHub)));
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Fix(Fix::SignIn("gh auth login".into()))
        );
        v.stage = Stage::Failed(fail(ShipErrorKind::SignIn, Some(ForgeKind::Ado)));
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Fix(Fix::SignIn("az login".into()))
        );
        v.stage = Stage::Failed(fail(ShipErrorKind::NoTool, Some(ForgeKind::GitHub)));
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Fix(Fix::OpenUrl(GH_INSTALL_URL.into()))
        );
        let mut ado = fail(ShipErrorKind::MergeInAdo, Some(ForgeKind::Ado));
        ado.url = Some("https://dev.azure.com/pr/1".into());
        v.stage = Stage::Failed(ado);
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Fix(Fix::OpenUrl("https://dev.azure.com/pr/1".into()))
        );
        v.stage = Stage::Failed(fail(ShipErrorKind::Offline, None));
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Fix(Fix::Retry)
        );
        v.stage = Stage::Failed(fail(ShipErrorKind::NoForge, None));
        assert_eq!(
            press(&mut v, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::None
        );
        assert_eq!(
            v.stage,
            Stage::Editing,
            "nothing to retry: back to the form"
        );
        v.stage = Stage::Failed(fail(ShipErrorKind::Other, None));
        press(&mut v, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(v.stage, Stage::Editing);

        v.stage = Stage::Working;
        assert!(matches!(
            press(&mut v, KeyCode::Char('a'), KeyModifiers::NONE),
            Outcome::Say(_)
        ));
        assert_eq!(
            press(&mut v, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Close
        );
    }
}
