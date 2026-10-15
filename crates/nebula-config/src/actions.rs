//! The verb registry, as data: every key the TUI's grid reacts to, with
//! its stable config id, label, hint, Hotkeys-tab group, scope and default
//! chords, and what both apps need to say about a verb (its one word, what
//! it acts on, where it works, whether it destroys anything) and the
//! desktop window's own default keys for it. Verbs only the window has are
//! in [`DESKTOP_ONLY`]. The chord parsing and dispatch (`KeyChord`,
//! `Keymap`) stay in the TUI, which needs a terminal backend for them; the
//! window reads this table through the generated `desktop/src/lib/
//! actions.gen.ts` ([`render_ts`]).

use serde::Serialize;
use std::collections::BTreeMap;

/// Which mode a binding is live in. The same chord may mean different
/// things in each, so conflicts are only conflicts within one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The launcher grid, and a terminal pane that isn't input-locked.
    Global,
    /// Input-locked terminal pane, where every other key is forwarded to
    /// the child process.
    Terminal,
}

/// Everything a hotkey can do. Overlay-local keys (Esc to close, j/k inside
/// a picker, the line-editor bindings) are deliberately absent: they're the
/// modal grammar every overlay shares, not application hotkeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // navigate
    FocusNext,
    FocusLeft,
    FocusRight,
    MoveDown,
    MoveUp,
    HalfPageDown,
    HalfPageUp,
    Activate,
    Palette,
    /// `^K`: the COMMAND PALETTE — the `/` palette's projects, worktrees,
    /// sessions and pull requests, plus every action here by its label,
    /// Enter running it.
    CommandPalette,
    /// `⌥←`: BACK to the place (project tab, card, full-screen) the grid
    /// was on before this one.
    NavBack,
    /// `⌥→`: FORWARD again, after a back.
    NavForward,
    /// `.`: the next session in the PALETTE's attention order, no modal.
    NextAttention,
    /// `,`: the same walk backwards.
    PrevAttention,
    /// `]`: the LAUNCHER VIEW's next PROJECT TAB to the right — that
    /// project's sessions. The last tab goes no further.
    NextProjectTab,
    /// `[`: the tab to the left. The first tab goes no further.
    PrevProjectTab,
    /// `x`: close the PROJECT TAB the grid is on, landing on the tab that
    /// slides into its place.
    CloseProjectTab,
    /// `+` (and `⌘P` where the terminal sends ⌘): the PROJECT DROPDOWN, the
    /// list the `+` after the PROJECT TABS drops — type to narrow it, Enter
    /// or a click opens the project.
    ProjectDropdown,
    // projects & worktrees
    AddProject,
    New,
    GitDiff,
    OpenRepo,
    /// `Shift+V`: the pull request of the session card under the cursor,
    /// in the browser — the `#42 title` line on the card. `v` lists the
    /// project's pull requests in nebula; the shifted key goes to GitHub.
    OpenPullRequest,
    /// `Shift+I`: the GitHub issue the session card under the cursor was
    /// started from, in the browser. `i` lists the project's issues in
    /// nebula; the shifted key goes to GitHub.
    OpenIssue,
    /// `Shift+T`: a terminal *outside* nebula — a new Ghostty tab in the
    /// selected worktree's directory, where `t` opens one inside nebula; a
    /// silent no-op on a machine without Ghostty.
    OpenGhosttyTab,
    /// `Shift+R`: reload from GitHub now, past every timer — the project's
    /// open pull requests and issues, the worktree's PR, the one the pane
    /// is reading.
    RefreshPullRequests,
    /// `y`: reply — the COMMENT BOX on the card's pull request (the one
    /// `Shift+V` opens), or on the one the pane is reading, posted to
    /// GitHub with `gh pr comment` on Enter.
    CommentPullRequest,
    /// `i`: the ISSUES MODAL — the project's open GitHub issues, read in
    /// place, commented on, with a QUICK PROMPT or an AGENT PRESET launched
    /// on one.
    Issues,
    /// `v`: the PULL REQUESTS MODAL — the project's open pull requests,
    /// read in place, commented on, with a PR SESSION launched on one.
    PullRequests,
    /// `c`: the BRANCH SWITCHER — move the project's ROOT WORKTREE onto
    /// another branch, asking what to do with uncommitted changes.
    SwitchBranch,
    /// `⇧B`: the AZURE DEVOPS OVERLAY — the project's sprint tasks, its
    /// active pull requests and its branches, on a project whose remote
    /// is on Azure DevOps.
    Ado,
    /// `Shift+Enter` / `Shift+O` / `Alt+Enter`, on the grid: fire the
    /// selected worktree's OPEN COMMAND — the project's **Open command**
    /// setting, else its `.nebula.json` `open` (`open http://localhost:3000`,
    /// say). Three chords because only the kitty protocol carries a shifted
    /// Enter: Terminal.app sends a plain one and tmux flattens it, while
    /// `ESC CR` — what `/terminal-setup` gives Shift+Enter in VS Code, and
    /// Option+Enter on a Mac with Option as Meta — gets through both and
    /// parses as Alt+Enter. Its RUN COMMAND is the project menu's **Run**.
    OpenWorktree,
    /// `w`: the selected worktree's dev server, `http://localhost:<port>`,
    /// in a browser picked from the ones installed (`crate::localhost`).
    OpenInBrowser,
    /// `Shift+W`: a headless screenshot of that page, shown in the image
    /// modal.
    Screenshot,
    // sessions
    NewTerminal,
    Rename,
    Archive,
    Unarchive,
    ToggleArchived,
    Delete,
    DeleteAll,
    /// The AGENT PRESETS list: saved launch definitions for the checkout
    /// under the cursor.
    AgentPresets,
    /// The QUICK PROMPT: type a task, launch an agent on it.
    QuickPrompt,
    /// `Space`: the FOLLOW-UP MODAL for the selected session card — its
    /// next turn, typed in a box over the grid and sent on Enter.
    FollowUp,
    /// `Shift+P`: the QUICK PROMPT on the card under the cursor's settings
    /// — the same harness, model, effort and worktree, and the issue it
    /// was started from — so the task is all there is to type. `p` opens
    /// the box on the Agents tab defaults; the shifted key opens it on
    /// the card.
    DuplicateSession,
    /// `m`: move the card under the cursor onto another checkout, of its
    /// project or another one, picked from a list. Dragging the card onto
    /// another band or project tab does the same.
    MoveSession,
    /// `⌥↑`: the card, band or project tab under the cursor one place
    /// earlier in the user's order (`ClientRequest::Reorder`), as dragging
    /// it there does.
    ReorderUp,
    /// `⌥↓`: one place later.
    ReorderDown,
    /// `⇧S`: the SUBAGENTS OVERLAY — the project's sessions as a tree,
    /// each with the subagents running inside it.
    Subagents,
    // files
    FindFile,
    Grep,
    TreeBrowser,
    // terminal
    UnlockTerminal,
    // general
    /// `^~`: fold the LAUNCHER VIEW's PANE away and give the cards the
    /// whole body, or bring it back. Folding it also unselects the card
    /// under the cursor, so nothing is selected and nothing is read.
    ToggleLauncherPane,
    /// `F10`: toggle between the ORCHESTRATOR chat view and the grid.
    ToggleOrchestratorView,
    /// `⇧U`: the ACCOUNTS OVERLAY — each account's plan usage, login,
    /// and its on/off and default switches.
    Accounts,
    /// `⇧K`: the KANBAN — the grid as lanes by status — or back to the
    /// bands; saved as Settings → Appearance → **Kanban**.
    ToggleKanban,
    /// `^F`: FULL-SCREEN the session in the PANE — the grid and its header
    /// give way to the PTY — or bring it back down to the pane beside the
    /// cards. From the cards it full-screens the one under the cursor.
    ToggleFullScreen,
    /// `` ` ``: walk the TERMINAL cards of the cursor's checkout, one after
    /// another and back round to the first. What the pane READS, where
    /// `^~` is whether it is drawn at all.
    PaneTabs,
    /// Open the Nth PROJECT TAB (1-based) in the LAUNCHER VIEW's header.
    SelectProjectTab(u8),
    Hosts,
    Settings,
    Metrics,
    Help,
    Quit,
}

/// What a verb acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliesTo {
    /// Nothing in particular: the app itself, a view, a list.
    Global,
    Repo,
    Worktree,
    /// An agent session.
    Session,
    /// A shell terminal (and, in the TUI, the pane's PTY).
    Terminal,
    Account,
}

/// Where a verb means something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorksIn {
    /// The TUI's grid of cards; the desktop's overview.
    Grid,
    /// The kanban board, in either app.
    Board,
    /// A session's pane (the TUI's) or view (the desktop's).
    Pane,
    Diff,
    /// A modal over the rest.
    Overlay,
}

/// Which app answers a verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Both,
    /// The terminal app only: the desktop has no key for it.
    Tui,
    /// The desktop window only: never in the TUI's [`ACTIONS`].
    Desktop,
}

/// Where a desktop key counts, as `desktop/src/lib/shortcuts.ts` reads it:
/// `anywhere` even in a field or terminal, `fields` in a field but not a
/// terminal, `global` with neither, `row` on a focused row or card, and
/// `overview` / `board` for the home views' own walking keys (listed, not
/// rebindable: those views handle them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeskScope {
    Anywhere,
    Fields,
    Global,
    Row,
    Overview,
    Board,
}

/// One of the desktop window's key rows. A verb may have two when its
/// chords count in different places (Ctrl+K in fields, Ctrl+Shift+P even
/// in a terminal). `id` is the window's own and its config key under
/// [`DESKTOP_PREFIX`]: never rename one.
#[derive(Debug, Serialize)]
pub struct DeskKey {
    pub id: &'static str,
    /// What the window's help and its Hotkeys rows call it.
    pub label: &'static str,
    /// The window's help section.
    pub group: &'static str,
    pub scope: DeskScope,
    /// The diff editor (Monaco) binds this chord itself: the key stands
    /// down while the focus is in one.
    pub editor_owns: bool,
    /// Spelled as `chordOf` spells a key event: `ctrl+shift+b`,
    /// `alt+arrowleft`, `escape`, `?`.
    pub chords: &'static [&'static str],
}

/// A desktop key row that the diff editor doesn't own.
const fn dk(
    id: &'static str,
    label: &'static str,
    group: &'static str,
    scope: DeskScope,
    chords: &'static [&'static str],
) -> DeskKey {
    DeskKey {
        id,
        label,
        group,
        scope,
        editor_owns: false,
        chords,
    }
}

/// One rebindable row: what it does, where it's live, and what it starts as.
pub struct ActionSpec {
    pub action: Action,
    /// Stable key in the config file's `keybindings` object.
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// Section header in the Hotkeys tab.
    pub group: &'static str,
    pub scope: Scope,
    pub defaults: &'static [&'static str],
    /// The verb in a word, for a footer or the verb bar.
    pub word: &'static str,
    /// Loses something no key brings back: it asks first.
    pub destructive: bool,
    pub applies_to: &'static [AppliesTo],
    pub works_in: &'static [WorksIn],
    pub surface: Surface,
    /// The desktop window's default keys for this verb, empty on a
    /// [`Surface::Tui`] row.
    pub desktop: &'static [DeskKey],
}

/// A verb only the desktop window has: an [`ActionSpec`] less the TUI's
/// action, scope and chords.
pub struct DesktopVerb {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub group: &'static str,
    pub word: &'static str,
    pub destructive: bool,
    pub applies_to: &'static [AppliesTo],
    pub works_in: &'static [WorksIn],
    pub desktop: &'static [DeskKey],
}

/// One positional PROJECT TAB shortcut. `⌘N` is what a browser user
/// reaches for — but it is [`Reach::Blocked`] in Terminal.app and most
/// other emulators, which never encode ⌘ into pty bytes at all. The bare
/// digit is bound alongside it and is the chord that actually fires there;
/// digits are otherwise unbound on the grid.
macro_rules! project_tab_slot {
    ($n:literal, $id:literal, $label:literal, $cmd:literal, $digit:literal) => {
        ActionSpec {
            action: Action::SelectProjectTab($n),
            id: $id,
            label: $label,
            hint: "Open the project on that tab of the header, counting from the left (⌘N only in emulators that send ⌘)",
            group: "NAVIGATE",
            scope: Scope::Global,
            // The digit first: it arrives everywhere, and ⌘N is a silent
            // alias Help and the footer leave out (`shown_chords`).
            defaults: &[$digit, $cmd],
            word: "tab",
            destructive: false,
            applies_to: &[AppliesTo::Repo],
            works_in: &[WorksIn::Grid, WorksIn::Board],
            surface: Surface::Tui,
            desktop: &[],
        }
    };
}

pub const ACTIONS: &[ActionSpec] = &[
    // ---- NAVIGATE ----
    ActionSpec {
        action: Action::FocusNext,
        id: "focus_next",
        label: "Open / fold checkout",
        hint: "Open every card of the checkout under the cursor in place, or fold it back to one row: one checkout open at a time",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["tab"],
        word: "fold",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::FocusLeft,
        id: "focus_left",
        label: "Focus left",
        hint: "Step to the card on the left, stopping at the row's first",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["h", "left"],
        word: "left",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::FocusRight,
        id: "focus_right",
        label: "Focus right",
        hint: "Step to the card on the right, stopping at the row's last",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["l", "right"],
        word: "right",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::MoveDown,
        id: "move_down",
        label: "Move down",
        hint: "Move the cursor down a row of cards; twice from the project tabs, into the one under their cursor",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["j", "down"],
        word: "down",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::MoveUp,
        id: "move_up",
        label: "Move up",
        hint: "Move the cursor up a row of cards, stopping at the first; twice there, up to the project tabs",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["k", "up"],
        word: "up",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::HalfPageDown,
        id: "half_page_down",
        label: "Half page down",
        hint: "Jump the cursor down two rows of cards, stopping at the last",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["ctrl+d"],
        word: "page",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::HalfPageUp,
        id: "half_page_up",
        label: "Half page up",
        hint: "Jump the cursor up two rows of cards, stopping at the first",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["ctrl+u"],
        word: "page",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Activate,
        id: "activate",
        label: "Open / attach",
        hint: "Into the pane on the card under the cursor, or lock the pane's input",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["enter"],
        word: "open",
        destructive: false,
        applies_to: &[AppliesTo::Session, AppliesTo::Terminal],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Palette,
        id: "palette",
        label: "Fuzzy jump",
        hint: "Search every project, worktree and session at once",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["/"],
        word: "jump",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::CommandPalette,
        id: "command_palette",
        label: "Command palette",
        hint: "Search every project, worktree, session and pull request, and every action by its name: Enter jumps to the row or runs the action",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["ctrl+k"],
        word: "commands",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        desktop: &[
            DeskKey { id: "palette", label: "Command palette", group: "General", scope: DeskScope::Fields, editor_owns: true, chords: &["ctrl+k"] },
            dk("palette_alt", "Command palette", "General", DeskScope::Anywhere, &["ctrl+shift+p"]),
        ],
    },
    ActionSpec {
        action: Action::NavBack,
        id: "nav_back",
        label: "Back",
        hint: "Go back to the place the grid was on before (its project tab, the card under the cursor, full-screen or not), as the header's ‹ does. < is bound beside ⌥←, which needs Option as Meta on a Mac",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["alt+left", "<"],
        word: "back",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        desktop: &[
            dk("back", "Back", "Views", DeskScope::Fields, &["alt+arrowleft"]),
        ],
    },
    ActionSpec {
        action: Action::NavForward,
        id: "nav_forward",
        label: "Forward",
        hint: "Go forward again after a back, as the header's › does. > is bound beside ⌥→, which needs Option as Meta on a Mac",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["alt+right", ">"],
        word: "forward",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        desktop: &[
            dk("forward", "Forward", "Views", DeskScope::Fields, &["alt+arrowright"]),
        ],
    },
    ActionSpec {
        action: Action::NextAttention,
        id: "next_attention",
        label: "Next session needing you",
        hint: "Jump to the next session in the palette's attention order (needs feedback, running, unseen, then recency) in any project, wrapping",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["."],
        word: "next",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Both,
        desktop: &[
            dk("next_attention", "Next session needing you", "Views", DeskScope::Global, &["."]),
        ],
    },
    ActionSpec {
        action: Action::PrevAttention,
        id: "prev_attention",
        label: "Prev session needing you",
        hint: "The same walk backwards: the session before this one in the palette's attention order, wrapping at the top",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &[","],
        word: "prev",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Both,
        desktop: &[
            dk("prev_attention", "Previous session needing you", "Views", DeskScope::Global, &[","]),
        ],
    },
    ActionSpec {
        action: Action::NextProjectTab,
        id: "next_project_tab",
        label: "Next project tab",
        hint: "Open the project on the tab to the right of this one in the header, stopping at the last",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["]"],
        word: "tab",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::PrevProjectTab,
        id: "prev_project_tab",
        label: "Prev project tab",
        hint: "Open the project on the tab to the left, stopping at the first",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["["],
        word: "tab",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    project_tab_slot!(1, "project_tab_1", "Project tab 1", "cmd+1", "1"),
    project_tab_slot!(2, "project_tab_2", "Project tab 2", "cmd+2", "2"),
    project_tab_slot!(3, "project_tab_3", "Project tab 3", "cmd+3", "3"),
    project_tab_slot!(4, "project_tab_4", "Project tab 4", "cmd+4", "4"),
    project_tab_slot!(5, "project_tab_5", "Project tab 5", "cmd+5", "5"),
    project_tab_slot!(6, "project_tab_6", "Project tab 6", "cmd+6", "6"),
    project_tab_slot!(7, "project_tab_7", "Project tab 7", "cmd+7", "7"),
    project_tab_slot!(8, "project_tab_8", "Project tab 8", "cmd+8", "8"),
    project_tab_slot!(9, "project_tab_9", "Project tab 9", "cmd+9", "9"),
    ActionSpec {
        action: Action::CloseProjectTab,
        id: "close_project_tab",
        label: "Close project tab",
        hint: "Drop this project's tab from the header and open the tab beside it. Nothing is deleted: + opens it again",
        group: "NAVIGATE",
        scope: Scope::Global,
        defaults: &["x"],
        word: "close",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::ProjectDropdown,
        id: "project_dropdown",
        label: "Switch project",
        hint: "Drop the list of every project under the + in the header: type to narrow it, Enter or a click opens one. + is the header's own button and arrives in every terminal and in kete browser; ⌘P does the same from inside the pane, where the terminal sends ⌘ at all (Ghostty/kitty do, Terminal.app and the browser never do)",
        group: "NAVIGATE",
        scope: Scope::Global,
        // `+`, the header button's own glyph, is the key the app shows —
        // it arrives everywhere, where `⌘P` is the browser's print dialog
        // and Terminal.app's nothing. `⌘P` stays bound behind it as the
        // one chord a LOCKED PANE lets through in Ghostty and kitty.
        defaults: &["+", "cmd+p"],
        word: "projects",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    // ---- PROJECTS & WORKTREES ----
    ActionSpec {
        action: Action::AddProject,
        id: "add_project",
        label: "Open a folder as a project",
        hint: "Open a folder as a project in kete, from anywhere; ⇧O opens a checkout outside it, in your editor",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["o"],
        word: "add",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::New,
        id: "new",
        label: "New session",
        hint: "Pick the harness first (→ drills into its model and effort), then the quick prompt opens set to it, in the checkout under the cursor; on the first-run splash, add a project",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["n"],
        word: "new",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::GitDiff,
        id: "git_diff",
        label: "Git diff",
        hint: "Open the diff viewer for the checkout under the cursor, or, with the pane reading a pull request, that pull request's diff",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["g"],
        word: "diff",
        destructive: false,
        applies_to: &[AppliesTo::Worktree, AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        desktop: &[
            dk("open_diff", "Diff the checkout in context: the focused row, the session, the repo (again: back from the diff)", "Views", DeskScope::Anywhere, &["ctrl+shift+d"]),
        ],
    },
    ActionSpec {
        action: Action::OpenRepo,
        id: "open_repo",
        label: "Open repo in browser",
        hint: "Send the selected repo's git remote (GitHub, GitLab, …) to your browser",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["shift+g"],
        word: "remote",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("open_remote", "Open the repo remote in the browser", "Sessions", DeskScope::Row, &["shift+g"]),
        ],
    },
    ActionSpec {
        action: Action::OpenPullRequest,
        id: "open_pull_request",
        label: "Open pull request in browser",
        hint: "Send the pull request of the session card under the cursor (its checkout's branch) to your browser, as a click on the card's #42 line does. v lists the pull requests in kete; ⇧V goes to GitHub",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["shift+v"],
        word: "PR",
        destructive: false,
        applies_to: &[AppliesTo::Worktree, AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::OpenIssue,
        id: "open_issue",
        label: "Open issue in browser",
        hint: "Send the GitHub issue the session card under the cursor was started from to your browser. i lists the issues in kete; ⇧I goes to GitHub",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["shift+i"],
        word: "issue",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::RefreshPullRequests,
        id: "refresh_pull_requests",
        label: "Reload from GitHub",
        hint: "Ask GitHub again now for the project's open pull requests and issues, the selected worktree's PR and the one the pane is reading",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["shift+r"],
        word: "reload",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::CommentPullRequest,
        id: "comment_pull_request",
        label: "Comment on pull request",
        hint: "Open a box to type a comment and post it through gh on the card's pull request (the one ⇧V opens) or, with the pane reading a pull request (a / jump lands on one), on that one",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["y"],
        word: "comment",
        destructive: false,
        applies_to: &[AppliesTo::Worktree, AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Issues,
        id: "issues",
        label: "GitHub issues",
        hint: "List the project's open issues; Enter prompts an agent on one, ⇧Tab launches a preset on it",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["i"],
        word: "issues",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::PullRequests,
        id: "pull_requests",
        label: "GitHub pull requests",
        hint: "List the project's open pull requests; Enter prompts a PR session on one, ⇧Tab launches a preset on it, Tab picks a harness",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["v"],
        word: "PRs",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::SwitchBranch,
        id: "switch_branch",
        label: "Switch root branch",
        hint: "Fuzzy-pick a branch or remote for the project's root checkout; uncommitted changes get stash / bring / commit / discard",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["c"],
        word: "branch",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("switch_branch", "Switch the repo root checkout to another branch", "Sessions", DeskScope::Row, &["c"]),
        ],
    },
    ActionSpec {
        action: Action::Ado,
        id: "azure_devops",
        label: "Azure DevOps",
        hint: "On a project whose remote is on Azure DevOps (the header shows an ado button): your sprint's tasks, the active pull requests and the branches. Enter opens a task or pull request in the browser and checks a branch out in a worktree; s twice starts a task (branch, worktree, agent)",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["shift+b"],
        word: "ADO",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::OpenWorktree,
        id: "open_worktree",
        label: "Open checkout in editor",
        hint: "Open the selected checkout outside kete, usually in your editor: its project's Open command (Settings → Project), else .kete.json \"open\", e.g. open http://localhost:3000 (⇧Enter needs the kitty protocol; ⇧O arrives everywhere; ⌥Enter is the ESC CR that VS Code's Shift+Enter setup sends and tmux passes through)",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["shift+enter", "shift+o", "alt+enter"],
        word: "editor",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("open_editor", "Open its worktree in the editor", "Sessions", DeskScope::Row, &["shift+o"]),
        ],
    },
    ActionSpec {
        action: Action::OpenInBrowser,
        id: "open_in_browser",
        label: "Open localhost in a browser",
        hint: "Open the selected worktree's dev server (http://localhost:<port>, the port its sessions listen on) in a browser picked from Chrome, Edge, Firefox or the system default; the pick is remembered per project",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["w"],
        word: "browser",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Screenshot,
        id: "screenshot",
        label: "Screenshot localhost",
        hint: "Take a headless Edge or Chrome screenshot (1280×800) of the selected worktree's dev server and show it in the image view",
        group: "PROJECTS & WORKTREES",
        scope: Scope::Global,
        defaults: &["shift+w"],
        word: "shot",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    // ---- SESSIONS ----
    ActionSpec {
        action: Action::NewTerminal,
        id: "new_terminal",
        label: "New shell terminal",
        hint: "Spawn a plain shell inside kete, in the selected worktree's directory; ⇧T opens one outside it, in a Ghostty tab",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["t"],
        word: "terminal",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("new_terminal", "New terminal in its worktree", "Sessions", DeskScope::Row, &["t"]),
        ],
    },
    // `t`'s shift pair: the same terminal, outside nebula.
    ActionSpec {
        action: Action::OpenGhosttyTab,
        id: "open_ghostty_tab",
        label: "Terminal in a Ghostty tab",
        hint: "Open a new Ghostty tab in the selected worktree's directory: t's terminal, outside kete; does nothing without Ghostty.app",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["shift+t"],
        word: "ghostty",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Rename,
        id: "rename",
        label: "Rename",
        hint: "Rename the session under the cursor (the checkout's run command is the menu's Run / Stop)",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["r"],
        word: "rename",
        destructive: false,
        applies_to: &[AppliesTo::Session, AppliesTo::Terminal],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("rename", "Rename", "Sessions", DeskScope::Row, &["r"]),
        ],
    },
    ActionSpec {
        action: Action::Archive,
        id: "archive",
        label: "Archive session",
        hint: "Archive the selected agent (its PTY is released)",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["a"],
        word: "archive",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Both,
        desktop: &[
            dk("archive", "Archive", "Sessions", DeskScope::Row, &["a"]),
        ],
    },
    ActionSpec {
        action: Action::Unarchive,
        id: "unarchive",
        label: "Unarchive session",
        hint: "Bring an archived agent back into the list",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["u"],
        word: "unarchive",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Both,
        desktop: &[
            dk("unarchive", "Unarchive", "Sessions", DeskScope::Row, &["u"]),
        ],
    },
    ActionSpec {
        action: Action::ToggleArchived,
        id: "toggle_archived",
        label: "Show / hide archived",
        hint: "Swap the grid between the live sessions and the archived ones",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["shift+a"],
        word: "archived",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("toggle_archived", "Show or hide archived sessions", "Views", DeskScope::Global, &["shift+a"]),
        ],
    },
    ActionSpec {
        action: Action::Delete,
        id: "delete",
        label: "Delete selected",
        hint: "Remove the selected row, behind a confirmation. With the PROJECT TABS holding the keys, close the tab under their cursor, behind the same kind of confirmation: x closes it outright",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["d", "delete", "backspace"],
        word: "delete",
        destructive: true,
        applies_to: &[AppliesTo::Session, AppliesTo::Terminal],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("delete", "Delete (asks first)", "Sessions", DeskScope::Row, &["delete"]),
        ],
    },
    ActionSpec {
        action: Action::DeleteAll,
        id: "delete_all",
        label: "Delete all sessions",
        hint: "Remove every session listed for the checkout under the cursor, behind a confirmation",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["shift+d"],
        word: "delete all",
        destructive: true,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::AgentPresets,
        id: "agent_presets",
        label: "Agent presets",
        hint: "Saved launch presets (CLI, model, effort, prefix/postfix) for the checkout under the cursor; Enter asks for an optional task, or skips it; from the pull requests list, a PR session in that branch's worktree",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["e"],
        word: "presets",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::QuickPrompt,
        id: "quick_prompt",
        label: "Quick prompt",
        hint: "Type a prompt; Enter starts an agent on it (Settings > Agents picks which)",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["p"],
        word: "prompt",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Both,
        desktop: &[
            dk("quick_prompt", "Quick prompt", "General", DeskScope::Global, &["p", "n"]),
            dk("quick_prompt_alt", "Quick prompt", "General", DeskScope::Anywhere, &["ctrl+shift+n"]),
        ],
    },
    ActionSpec {
        action: Action::FollowUp,
        id: "follow_up",
        label: "Follow-up prompt",
        hint: "Open a box over the grid for the selected session's next turn, sent to the running agent on Enter; the pane stays as it is",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["space"],
        word: "reply",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Both,
        desktop: &[
            dk("follow_up", "Follow-up prompt", "Sessions", DeskScope::Row, &["f"]),
        ],
    },
    ActionSpec {
        action: Action::DuplicateSession,
        id: "duplicate_session",
        label: "Duplicate session",
        hint: "Open the quick prompt set to launch what the selected card runs (the same harness, model, effort and worktree) so only the task is left to type. p opens the box on the defaults; ⇧P on the card",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["shift+p"],
        word: "duplicate",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("duplicate", "Duplicate into the quick prompt", "Sessions", DeskScope::Row, &["shift+p"]),
        ],
    },
    ActionSpec {
        action: Action::MoveSession,
        id: "move_session",
        label: "Move session",
        hint: "Pick another worktree, of this project or another, for the selected session; it resumes there, after its current turn when it is mid-turn. Dragging the card onto another band or project tab does the same",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["m"],
        word: "move",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("move", "Move to another worktree", "Sessions", DeskScope::Row, &["m"]),
        ],
    },
    ActionSpec {
        action: Action::ReorderUp,
        id: "reorder_up",
        label: "Move item up",
        hint: "Move the card under the cursor one place earlier in its band (on the board, among its worktree's cards in its lane), or on a band with no card to be on (empty or folded) that band, or with the project tabs holding the keys the tab under their cursor one place left; the menus' Move up and dragging do the same. ⌥↑ needs Option as Meta on a Mac; ⇧↑ is bound beside it",
        group: "SESSIONS",
        scope: Scope::Global,
        // ⌥↑ is the desktop's chord; ⇧↑ beside it arrives where Option is
        // not Meta.
        defaults: &["alt+up", "shift+up"],
        word: "up",
        destructive: false,
        applies_to: &[AppliesTo::Session, AppliesTo::Worktree, AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("move_up", "Move up: the session, worktree or repo (or drag it)", "Sessions", DeskScope::Row, &["alt+arrowup"]),
        ],
    },
    ActionSpec {
        action: Action::ReorderDown,
        id: "reorder_down",
        label: "Move item down",
        hint: "Move the card under the cursor one place later in its band (on the board, in its lane), or the project tab under the header's cursor one place right; the menus' Move down and dragging do the same",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["alt+down", "shift+down"],
        word: "down",
        destructive: false,
        applies_to: &[AppliesTo::Session, AppliesTo::Worktree, AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("move_down", "Move down: the session, worktree or repo (or drag it)", "Sessions", DeskScope::Row, &["alt+arrowdown"]),
        ],
    },
    ActionSpec {
        action: Action::Subagents,
        id: "subagents",
        label: "Subagents and session tree",
        hint: "The project's sessions as a tree, each session under the one that started it and the subagents running inside it under that; Enter opens the session, → and ← open and fold, Tab every project. A card's ⑂ chip opens it on that session",
        group: "SESSIONS",
        scope: Scope::Global,
        defaults: &["shift+s"],
        word: "tree",
        destructive: false,
        applies_to: &[AppliesTo::Repo, AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    // ---- FILES ----
    ActionSpec {
        action: Action::FindFile,
        id: "find_file",
        label: "Find file",
        hint: "Fuzzy file finder for the selected worktree",
        group: "FILES",
        scope: Scope::Global,
        defaults: &["f"],
        word: "find",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Grep,
        id: "grep",
        label: "Find in files",
        hint: "git grep across the selected worktree",
        group: "FILES",
        scope: Scope::Global,
        defaults: &["shift+f"],
        word: "grep",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::TreeBrowser,
        id: "tree_browser",
        label: "File tree browser",
        hint: "Browse the worktree's files with a preview pane",
        group: "FILES",
        scope: Scope::Global,
        defaults: &["b"],
        word: "files",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    // ---- TERMINAL ----
    ActionSpec {
        action: Action::UnlockTerminal,
        id: "unlock_terminal",
        label: "Unlock terminal input",
        hint: "Leave the locked pane and go back to the card (^q always works; ^⇧H needs the kitty protocol)",
        group: "TERMINAL",
        scope: Scope::Terminal,
        defaults: &["ctrl+q", "ctrl+]", "ctrl+shift+h"],
        word: "unlock",
        destructive: false,
        applies_to: &[AppliesTo::Session, AppliesTo::Terminal],
        works_in: &[WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    // ---- GENERAL ----
    ActionSpec {
        action: Action::ToggleLauncherPane,
        id: "toggle_launcher_pane",
        label: "Launcher session pane",
        hint: "Fold the pane under the cards away (which also unselects the card it was reading) or bring it back. From inside the pane the ctrl chords take two presses: the first hands the keys back to the card, the second folds the pane. ^` and ^~ need the kitty protocol; ~ is bound alongside them, the shift of the ` that walks the pane's tabs",
        group: "GENERAL",
        scope: Scope::Global,
        // `^`` is the chord to reach for — out of the pane, then the pane
        // away — and `^~` the same key for the terminals that report it
        // shifted with ctrl held. Only the kitty protocol carries either:
        // a stock terminal has no encoding for ctrl and this key, and
        // sends the same NUL it sends for ^Space. So the bare `~` is bound
        // beside them — the shift of the `` ` `` that walks the pane's
        // tabs, which is the key everything else about the pane is on.
        defaults: &["ctrl+`", "ctrl+~", "~"],
        word: "pane",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::ToggleKanban,
        id: "toggle_kanban",
        label: "Kanban board",
        hint: "Lay the grid out as lanes by status (Needs you, Running, Unread, Done, PR open) or back to a band per worktree; the Overview | Board toggle at the right of the grid's header does the same by click. On the board h/l walk the lanes, j/k the cards, Tab flips between this project and every project",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["shift+k"],
        word: "board",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Both,
        desktop: &[
            dk("toggle_home", "Switch overview and board", "Views", DeskScope::Global, &["shift+k"]),
        ],
    },
    ActionSpec {
        action: Action::ToggleOrchestratorView,
        id: "toggle_orchestrator_view",
        label: "Orchestrator view",
        hint: "Toggle between the chat-first orchestrator view and the grid",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["f10"],
        word: "orchestrator",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        // F10 as here, but never inside a shell on the desktop (htop and mc
        // quit on it); 0 beside it.
        desktop: &[
            dk("orchestrator", "Orchestrator (start one when there is none)", "Views", DeskScope::Global, &["0"]),
            dk("orchestrator_f10", "Orchestrator (start one when there is none)", "Views", DeskScope::Fields, &["f10"]),
        ],
    },
    ActionSpec {
        action: Action::Accounts,
        id: "accounts",
        label: "Accounts and usage",
        hint: "Every account (profiles, and each CLI's own login) with its 5h and weekly plan usage, resets and login; Space switches a profile on or off, d makes it its harness's default",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["shift+u"],
        word: "accounts",
        destructive: false,
        applies_to: &[AppliesTo::Account],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        surface: Surface::Both,
        desktop: &[
            dk("accounts", "Accounts: every login, its use, and who runs on it", "Views", DeskScope::Global, &["shift+u"]),
        ],
    },
    ActionSpec {
        action: Action::ToggleFullScreen,
        id: "toggle_full_screen",
        label: "Full-screen session",
        hint: "Give the session in the pane the whole screen, or bring it back down beside the cards, from inside the pane too, where it is never forwarded to the agent. From the cards it full-screens the one under the cursor. ^q and ^` also bring a full-screen session back down",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["ctrl+f"],
        word: "full",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        // On the desktop ^F is the diff editor's find and the shell's
        // forward-char, so the window's chord takes Shift too (its terminal
        // pane lets it through).
        desktop: &[
            dk("maximize", "Full pane: the session or diff takes the window, or comes back down", "Views", DeskScope::Anywhere, &["ctrl+shift+m"]),
        ],
    },
    ActionSpec {
        action: Action::PaneTabs,
        id: "pane_tabs",
        label: "Terminals in the pane",
        hint: "Walk the cursor through the terminal cards of its checkout, one after another and back round to the first, the pane reads each",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["`"],
        word: "terminals",
        destructive: false,
        applies_to: &[AppliesTo::Terminal],
        works_in: &[WorksIn::Grid, WorksIn::Pane],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Hosts,
        id: "hosts",
        label: "SSH hosts",
        hint: "Connect to a saved ssh host (restarts kete over ssh)",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["shift+h"],
        word: "hosts",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Settings,
        id: "settings",
        label: "Settings",
        hint: "Open this settings overlay",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["s"],
        word: "settings",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        desktop: &[
            dk("settings", "Settings", "General", DeskScope::Anywhere, &["ctrl+,"]),
        ],
    },
    ActionSpec {
        action: Action::Metrics,
        id: "metrics",
        label: "Memory usage",
        hint: "RAM used by kete and every live agent's process tree",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["shift+m"],
        word: "memory",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
    ActionSpec {
        action: Action::Help,
        id: "help",
        label: "Help",
        hint: "Toggle the keyboard help overlay",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["?"],
        word: "help",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        surface: Surface::Both,
        desktop: &[
            dk("help", "Keyboard shortcuts", "General", DeskScope::Global, &["?"]),
            dk("help_f1", "Keyboard shortcuts", "General", DeskScope::Anywhere, &["f1"]),
        ],
    },
    ActionSpec {
        action: Action::Quit,
        id: "quit",
        label: "Quit kete",
        hint: "Leave the TUI (sessions keep running in the daemon)",
        group: "GENERAL",
        scope: Scope::Global,
        defaults: &["q", "ctrl+c"],
        word: "quit",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        surface: Surface::Tui,
        desktop: &[],
    },
];

pub fn spec_at(index: usize) -> Option<&'static ActionSpec> {
    ACTIONS.get(index)
}

pub fn index_of(action: Action) -> Option<usize> {
    ACTIONS.iter().position(|s| s.action == action)
}

/// The row for an action: its label and hint, for anything that names
/// what a key did (the KEY COMBO DISPLAY, for one).
pub fn spec_of(action: Action) -> Option<&'static ActionSpec> {
    index_of(action).and_then(spec_at)
}

/// Verbs only the desktop window has: its views, its theme, its split and
/// the home views' walking keys. Never in [`ACTIONS`], which the TUI's
/// keymap indexes by position.
pub const DESKTOP_ONLY: &[DesktopVerb] = &[
    DesktopVerb {
        id: "cycle_theme",
        label: "Cycle theme",
        hint: "Step the window's theme through system, light and dark",
        group: "GENERAL",
        word: "theme",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk("cycle_theme", "Cycle theme: system, light, dark", "General", DeskScope::Anywhere, &["ctrl+shift+l"])],
    },
    DesktopVerb {
        id: "new_worktree",
        label: "New worktree",
        hint: "Cut a new worktree in the focused or shown repo",
        group: "PROJECTS & WORKTREES",
        word: "worktree",
        destructive: false,
        applies_to: &[AppliesTo::Repo],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk("new_worktree", "New worktree in this repo", "General", DeskScope::Global, &["shift+n"])],
    },
    DesktopVerb {
        id: "home_board",
        label: "Board",
        hint: "Show the board: the sessions as lanes by status",
        group: "VIEWS",
        word: "board",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk("home_board", "Board", "Views", DeskScope::Anywhere, &["ctrl+shift+b"])],
    },
    DesktopVerb {
        id: "home_overview",
        label: "Overview",
        hint: "Show the overview: the repos, their worktrees and sessions",
        group: "VIEWS",
        word: "overview",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk("home_overview", "Overview", "Views", DeskScope::Anywhere, &["ctrl+shift+o"])],
    },
    DesktopVerb {
        id: "open_session",
        label: "Show the session",
        hint: "Bring back the session last shown, in the split",
        group: "VIEWS",
        word: "session",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Diff],
        desktop: &[dk("open_session", "Show the session", "Views", DeskScope::Anywhere, &["ctrl+shift+t"])],
    },
    DesktopVerb {
        id: "go_home",
        label: "Home",
        hint: "The home view, overview or board, with nothing open beside it",
        group: "VIEWS",
        word: "home",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk("go_home", "Home", "Views", DeskScope::Global, &["1"])],
    },
    DesktopVerb {
        id: "go_session",
        label: "Session",
        hint: "The session last shown",
        group: "VIEWS",
        word: "session",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk("go_session", "Session", "Views", DeskScope::Global, &["2"])],
    },
    DesktopVerb {
        id: "go_diff",
        label: "Diff",
        hint: "The diff last shown, or the shown session's worktree's",
        group: "VIEWS",
        word: "diff",
        destructive: false,
        applies_to: &[AppliesTo::Worktree],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk("go_diff", "Diff", "Views", DeskScope::Global, &["3"])],
    },
    DesktopVerb {
        id: "close_split",
        label: "Close the split",
        hint: "Close the session or diff beside the home view, or Settings; a full pane comes back down first",
        group: "VIEWS",
        word: "close",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Pane, WorksIn::Diff],
        desktop: &[dk(
            "close_split",
            "Close the split or Settings (a full pane comes back down first)",
            "Views",
            DeskScope::Global,
            &["escape"],
        )],
    },
    DesktopVerb {
        id: "restart",
        label: "Restart the agent",
        hint: "Restart the focused session's agent, resuming its conversation",
        group: "SESSIONS",
        word: "restart",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Grid, WorksIn::Board, WorksIn::Pane],
        desktop: &[dk("restart", "Restart the agent", "Sessions", DeskScope::Row, &["shift+r"])],
    },
    DesktopVerb {
        id: "row_menu",
        label: "Actions menu",
        hint: "The focused row's or card's actions, as a right-click shows them",
        group: "SESSIONS",
        word: "actions",
        destructive: false,
        applies_to: &[AppliesTo::Repo, AppliesTo::Worktree, AppliesTo::Session, AppliesTo::Terminal],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        desktop: &[dk("row_menu", "Actions menu", "Sessions", DeskScope::Row, &["shift+f10", "contextmenu"])],
    },
    DesktopVerb {
        id: "list_move",
        label: "Move through the rows",
        hint: "Walk the overview's rows, or the board's cards in a lane",
        group: "OVERVIEW AND BOARD",
        word: "move",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        desktop: &[dk(
            "list_move",
            "Move through the rows (cards on the board)",
            "Overview and board",
            DeskScope::Overview,
            &["j", "k", "arrowdown", "arrowup"],
        )],
    },
    DesktopVerb {
        id: "list_open",
        label: "Open the session",
        hint: "Open the session on the focused row or card",
        group: "OVERVIEW AND BOARD",
        word: "open",
        destructive: false,
        applies_to: &[AppliesTo::Session, AppliesTo::Terminal],
        works_in: &[WorksIn::Grid, WorksIn::Board],
        desktop: &[dk("list_open", "Open the session", "Overview and board", DeskScope::Overview, &["enter", "space"])],
    },
    DesktopVerb {
        id: "overview_fold",
        label: "Unfold, fold",
        hint: "Unfold or fold the overview row's nested sessions",
        group: "OVERVIEW AND BOARD",
        word: "fold",
        destructive: false,
        applies_to: &[AppliesTo::Worktree, AppliesTo::Session],
        works_in: &[WorksIn::Grid],
        desktop: &[dk(
            "overview_fold",
            "Overview: unfold, fold nested sessions",
            "Overview and board",
            DeskScope::Overview,
            &["arrowright", "arrowleft"],
        )],
    },
    DesktopVerb {
        id: "board_lanes",
        label: "Across lanes",
        hint: "Step to the board's lane on either side",
        group: "OVERVIEW AND BOARD",
        word: "lanes",
        destructive: false,
        applies_to: &[AppliesTo::Global],
        works_in: &[WorksIn::Board],
        desktop: &[dk(
            "board_lanes",
            "Board: across lanes (also the side arrows)",
            "Overview and board",
            DeskScope::Board,
            &["h", "l"],
        )],
    },
    DesktopVerb {
        id: "board_diff",
        label: "Diff the card",
        hint: "Diff the selected card's worktree",
        group: "OVERVIEW AND BOARD",
        word: "diff",
        destructive: false,
        applies_to: &[AppliesTo::Session],
        works_in: &[WorksIn::Board],
        desktop: &[dk("board_diff", "Board: diff the selected card", "Overview and board", DeskScope::Board, &["d"])],
    },
];

/// The desktop window's key rows in the order its help lists them and its
/// key handler tries them (the first row a chord matches wins).
pub const DESKTOP_ORDER: &[&str] = &[
    "palette",
    "palette_alt",
    "help",
    "help_f1",
    "settings",
    "quick_prompt",
    "quick_prompt_alt",
    "cycle_theme",
    "new_worktree",
    "back",
    "forward",
    "home_board",
    "home_overview",
    "toggle_home",
    "open_session",
    "open_diff",
    "go_home",
    "go_session",
    "go_diff",
    "orchestrator",
    "orchestrator_f10",
    "close_split",
    "maximize",
    "toggle_archived",
    "next_attention",
    "prev_attention",
    "accounts",
    "rename",
    "archive",
    "unarchive",
    "delete",
    "restart",
    "move",
    "duplicate",
    "follow_up",
    "new_terminal",
    "open_editor",
    "open_remote",
    "switch_branch",
    "row_menu",
    "move_up",
    "move_down",
    "list_move",
    "list_open",
    "overview_fold",
    "board_lanes",
    "board_diff",
];

/// One registry row, either table's, as the window reads it.
#[derive(Debug, Serialize)]
pub struct Verb {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub group: &'static str,
    pub word: &'static str,
    pub destructive: bool,
    pub applies_to: &'static [AppliesTo],
    pub works_in: &'static [WorksIn],
    pub surface: Surface,
    /// The TUI's default chords, in its own spelling; empty on a desktop
    /// verb.
    pub tui: &'static [&'static str],
    pub tui_scope: Option<Scope>,
    pub desktop: &'static [DeskKey],
}

/// Every verb: the TUI's [`ACTIONS`], then [`DESKTOP_ONLY`].
pub fn verbs() -> Vec<Verb> {
    let tui = ACTIONS.iter().map(|s| Verb {
        id: s.id,
        label: s.label,
        hint: s.hint,
        group: s.group,
        word: s.word,
        destructive: s.destructive,
        applies_to: s.applies_to,
        works_in: s.works_in,
        surface: s.surface,
        tui: s.defaults,
        tui_scope: Some(s.scope),
        desktop: s.desktop,
    });
    let desk = DESKTOP_ONLY.iter().map(|d| Verb {
        id: d.id,
        label: d.label,
        hint: d.hint,
        group: d.group,
        word: d.word,
        destructive: d.destructive,
        applies_to: d.applies_to,
        works_in: d.works_in,
        surface: Surface::Desktop,
        tui: &[],
        tui_scope: None,
        desktop: d.desktop,
    });
    tui.chain(desk).collect()
}

/// The desktop's key rows in [`DESKTOP_ORDER`], each with its verb's id.
pub fn desktop_keys() -> Vec<(&'static str, &'static DeskKey)> {
    let all: Vec<(&'static str, &'static DeskKey)> = ACTIONS
        .iter()
        .flat_map(|s| s.desktop.iter().map(move |k| (s.id, k)))
        .chain(
            DESKTOP_ONLY
                .iter()
                .flat_map(|d| d.desktop.iter().map(move |k| (d.id, k))),
        )
        .collect();
    DESKTOP_ORDER
        .iter()
        .filter_map(|id| all.iter().find(|(_, k)| k.id == *id).copied())
        .collect()
}

// ---- the desktop's overrides ----

/// The desktop window's overrides share the config file's `keybindings`
/// object with the TUI's, as flat string entries under this prefix
/// (`"desktop.palette": "ctrl+k, ctrl+shift+p"`): a nested object would
/// fail to parse as the map of strings older builds read, and drop the
/// TUI's overrides with it. An empty string is a deliberate unbind, as
/// in the TUI.
pub const DESKTOP_PREFIX: &str = "desktop.";

/// The desktop's entries of a `keybindings` map: key id to its chords.
pub fn desktop_overrides(keybindings: &BTreeMap<String, String>) -> BTreeMap<String, Vec<String>> {
    keybindings
        .iter()
        .filter_map(|(k, v)| {
            let id = k.strip_prefix(DESKTOP_PREFIX)?;
            Some((id.to_string(), split_chords(v)))
        })
        .collect()
}

/// A stored chord list, `"ctrl+k, ctrl+shift+p"`, as its chords. A lone
/// `,` is a chord (prev_attention's): an empty part after one is it.
fn split_chords(list: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut part = String::new();
    let mut chars = list.chars().peekable();
    while let Some(c) = chars.next() {
        // `,` ends a chord unless it is the chord's key: the first thing
        // in the part, or after a `+`.
        let key = c == ',' && (part.trim().is_empty() || part.ends_with('+'));
        if c == ',' && !key {
            out.push(std::mem::take(&mut part));
            continue;
        }
        part.push(c);
        if key {
            // Its own separator, if one follows.
            while chars.peek() == Some(&' ') {
                chars.next();
            }
            if chars.peek() == Some(&',') {
                chars.next();
                out.push(std::mem::take(&mut part));
            }
        }
    }
    out.push(part);
    out.into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// `tui` (the TUI keymap's overrides) as the whole of the TUI's part of
/// `keybindings`, the desktop's entries kept as they were.
pub fn merge_tui_overrides(
    keybindings: &mut BTreeMap<String, String>,
    tui: BTreeMap<String, String>,
) {
    keybindings.retain(|k, _| k.starts_with(DESKTOP_PREFIX));
    keybindings.extend(tui);
}

/// Set (`Some`) or clear back to its default (`None`) one desktop key's
/// override. Refuses an unknown id and a home view's walking keys (those
/// views handle them themselves).
pub fn set_desktop_override(
    keybindings: &mut BTreeMap<String, String>,
    id: &str,
    chords: Option<&[String]>,
) -> Result<(), String> {
    let (_, key) = desktop_keys()
        .into_iter()
        .find(|(_, k)| k.id == id)
        .ok_or_else(|| format!("no desktop key {id:?}"))?;
    if matches!(key.scope, DeskScope::Overview | DeskScope::Board) {
        return Err(format!("{} is fixed: the view handles it", key.label));
    }
    let entry = format!("{DESKTOP_PREFIX}{id}");
    let Some(chords) = chords else {
        keybindings.remove(&entry);
        return Ok(());
    };
    let mut list: Vec<String> = Vec::new();
    for c in chords {
        let c = c.trim().to_ascii_lowercase();
        if !c.is_empty() && !list.contains(&c) {
            list.push(c);
        }
    }
    if list
        .iter()
        .map(String::as_str)
        .eq(key.chords.iter().copied())
    {
        keybindings.remove(&entry);
    } else {
        keybindings.insert(entry, list.join(", "));
    }
    Ok(())
}

// ---- the window's copy ----

/// Where the window's copy of this table lives, from this crate.
pub const TS_PATH: &str = "../../desktop/src/lib/actions.gen.ts";

/// A string as a TS literal in plain ASCII (the window's sources must be:
/// `desktop/scripts/check-encoding.mjs`): JSON, with every other character
/// a `\u` escape.
fn ts_str(s: &str) -> String {
    let json = serde_json::to_string(s).expect("a string serializes");
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

fn ts_enum<T: Serialize>(v: &T) -> String {
    let value = serde_json::to_value(v).expect("a variant serializes");
    ts_str(value.as_str().unwrap_or_default())
}

fn ts_list<T: Serialize>(items: &[T]) -> String {
    let items: Vec<String> = items.iter().map(ts_enum).collect();
    format!("[{}]", items.join(", "))
}

fn ts_strs(items: &[&str]) -> String {
    let items: Vec<String> = items.iter().map(|s| ts_str(s)).collect();
    format!("[{}]", items.join(", "))
}

fn ts_union(name: &str, items: &[String]) -> String {
    let mut out = format!("export type {name} =\n");
    for (i, item) in items.iter().enumerate() {
        let end = if i + 1 == items.len() { ";" } else { "" };
        out.push_str(&format!("  | {item}{end}\n"));
    }
    out
}

const TS_HEAD: &str = "\
// Generated from crates/nebula-config/src/actions.rs: do not edit. The
// nebula-config test actions_gen_ts_is_current fails when this file and
// that table differ; KETE_BLESS=1 cargo test -p nebula-config actions_gen
// rewrites it.

";

const TS_TYPES: &str = "\
/** One verb. `tui` is the terminal app's default chords in its own
 *  spelling; `desktop` names the window's key rows for it. */
export interface ActionRow {
  id: ActionId;
  label: string;
  hint: string;
  group: string;
  word: string;
  destructive: boolean;
  appliesTo: readonly AppliesTo[];
  worksIn: readonly WorksIn[];
  surface: Surface;
  tui: readonly string[];
  desktop: readonly DeskKeyId[];
}

/** One of the window's key rows: its default chords, spelled as
 *  `chordOf` spells a key event, and where they count. */
export interface DeskKey {
  id: DeskKeyId;
  action: ActionId;
  label: string;
  group: DeskGroup;
  scope: DeskScope;
  editorOwns: boolean;
  chords: readonly string[];
}

";

/// `desktop/src/lib/actions.gen.ts`: the registry as the window imports it.
/// The `actions_gen_ts_is_current` test fails when the committed file and
/// this differ; `KETE_BLESS=1` rewrites it.
pub fn render_ts() -> String {
    let verbs = verbs();
    let keys = desktop_keys();
    let mut groups: Vec<&str> = Vec::new();
    for (_, k) in &keys {
        if !groups.contains(&k.group) {
            groups.push(k.group);
        }
    }
    let ids: Vec<String> = verbs.iter().map(|v| ts_str(v.id)).collect();
    let key_ids: Vec<String> = keys.iter().map(|(_, k)| ts_str(k.id)).collect();
    let group_names: Vec<String> = groups.iter().map(|g| ts_str(g)).collect();
    let mut out = String::from(TS_HEAD);
    out.push_str(&ts_union("ActionId", &ids));
    out.push('\n');
    out.push_str(&ts_union("DeskKeyId", &key_ids));
    out.push('\n');
    out.push_str(&ts_union("DeskGroup", &group_names));
    out.push('\n');
    let applies = [
        AppliesTo::Global,
        AppliesTo::Repo,
        AppliesTo::Worktree,
        AppliesTo::Session,
        AppliesTo::Terminal,
        AppliesTo::Account,
    ];
    let works = [
        WorksIn::Grid,
        WorksIn::Board,
        WorksIn::Pane,
        WorksIn::Diff,
        WorksIn::Overlay,
    ];
    let surfaces = [Surface::Both, Surface::Tui, Surface::Desktop];
    let scopes = [
        DeskScope::Anywhere,
        DeskScope::Fields,
        DeskScope::Global,
        DeskScope::Row,
        DeskScope::Overview,
        DeskScope::Board,
    ];
    let alt = |names: Vec<String>| names.join(" | ");
    out.push_str(&format!(
        "export type AppliesTo = {};\nexport type WorksIn = {};\nexport type Surface = {};\nexport type DeskScope = {};\n\n",
        alt(applies.iter().map(ts_enum).collect()),
        alt(works.iter().map(ts_enum).collect()),
        alt(surfaces.iter().map(ts_enum).collect()),
        alt(scopes.iter().map(ts_enum).collect()),
    ));
    out.push_str(TS_TYPES);
    out.push_str("export const ACTIONS: readonly ActionRow[] = [\n");
    for v in &verbs {
        let desk: Vec<&str> = v.desktop.iter().map(|k| k.id).collect();
        out.push_str("  {\n");
        out.push_str(&format!("    id: {},\n", ts_str(v.id)));
        out.push_str(&format!("    label: {},\n", ts_str(v.label)));
        out.push_str(&format!("    hint: {},\n", ts_str(v.hint)));
        out.push_str(&format!("    group: {},\n", ts_str(v.group)));
        out.push_str(&format!("    word: {},\n", ts_str(v.word)));
        out.push_str(&format!("    destructive: {},\n", v.destructive));
        out.push_str(&format!("    appliesTo: {},\n", ts_list(v.applies_to)));
        out.push_str(&format!("    worksIn: {},\n", ts_list(v.works_in)));
        out.push_str(&format!("    surface: {},\n", ts_enum(&v.surface)));
        out.push_str(&format!("    tui: {},\n", ts_strs(v.tui)));
        out.push_str(&format!("    desktop: {},\n", ts_strs(&desk)));
        out.push_str("  },\n");
    }
    out.push_str("];\n\n/** The window's key rows, in its help's order. */\n");
    out.push_str("export const DESK_KEYS: readonly DeskKey[] = [\n");
    for (action, k) in &keys {
        out.push_str(&format!(
            "  {{ id: {}, action: {}, label: {}, group: {}, scope: {}, editorOwns: {}, chords: {} }},\n",
            ts_str(k.id),
            ts_str(action),
            ts_str(k.label),
            ts_str(k.group),
            ts_enum(&k.scope),
            k.editor_owns,
            ts_strs(k.chords),
        ));
    }
    out.push_str("];\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ids_are_unique_across_both_tables() {
        let mut seen = BTreeSet::new();
        for v in verbs() {
            assert!(seen.insert(v.id), "verb id {} twice", v.id);
        }
        let mut keys = BTreeSet::new();
        for (_, k) in desktop_keys() {
            assert!(keys.insert(k.id), "desktop key {} twice", k.id);
        }
    }

    #[test]
    fn every_desktop_key_is_in_the_order_once() {
        let mut listed: Vec<&str> = ACTIONS
            .iter()
            .flat_map(|s| s.desktop.iter().map(|k| k.id))
            .chain(
                DESKTOP_ONLY
                    .iter()
                    .flat_map(|d| d.desktop.iter().map(|k| k.id)),
            )
            .collect();
        let mut ordered: Vec<&str> = DESKTOP_ORDER.to_vec();
        listed.sort_unstable();
        ordered.sort_unstable();
        assert_eq!(ordered, listed);
        assert_eq!(desktop_keys().len(), DESKTOP_ORDER.len());
    }

    #[test]
    fn every_row_says_what_it_is() {
        for s in ACTIONS {
            match s.surface {
                Surface::Tui => assert!(s.desktop.is_empty(), "{} is TUI-only", s.id),
                Surface::Both => {}
                Surface::Desktop => panic!("{} is in the TUI's table", s.id),
            }
            assert!(!s.word.is_empty(), "{}", s.id);
            assert!(
                !s.applies_to.is_empty() && !s.works_in.is_empty(),
                "{}",
                s.id
            );
        }
        for d in DESKTOP_ONLY {
            assert!(!d.desktop.is_empty(), "{} has no key", d.id);
            assert!(!d.word.is_empty(), "{}", d.id);
            assert!(
                !d.applies_to.is_empty() && !d.works_in.is_empty(),
                "{}",
                d.id
            );
        }
    }

    #[test]
    fn the_tui_table_keeps_its_ids() {
        // Users' overrides are keyed by these: a rename strands them. Some
        // read as the desktop's other verbs (`palette` is the TUI's fuzzy
        // jump, the window's Ctrl+K is `command_palette`; `move_up` walks
        // the cursor where the window's reorders, `reorder_up`): the
        // window's key ids are their own namespace.
        assert_eq!(ACTIONS.len(), 73);
        for id in [
            "palette",
            "move_up",
            "move_down",
            "delete",
            "quit",
            "reorder_up",
        ] {
            assert!(ACTIONS.iter().any(|s| s.id == id), "{id}");
        }
    }

    #[test]
    fn chord_lists_split_on_their_commas_only() {
        assert_eq!(
            split_chords("ctrl+k, ctrl+shift+p"),
            vec!["ctrl+k", "ctrl+shift+p"]
        );
        assert_eq!(split_chords(","), vec![","]);
        assert_eq!(split_chords(",, ."), vec![",", "."]);
        assert_eq!(split_chords("ctrl+,, f2"), vec!["ctrl+,", "f2"]);
        assert_eq!(split_chords("f2, ,"), vec!["f2", ","]);
        assert!(split_chords("").is_empty());
    }

    #[test]
    fn desktop_overrides_live_beside_the_tuis() {
        let mut kb: BTreeMap<String, String> = BTreeMap::new();
        kb.insert("quit".into(), "ctrl+q".into());
        let palette = ["Ctrl+J".to_string(), "ctrl+k".to_string()];
        set_desktop_override(&mut kb, "palette", Some(&palette)).unwrap();
        set_desktop_override(&mut kb, "help_f1", Some(&[])).unwrap();
        assert_eq!(kb["desktop.palette"], "ctrl+j, ctrl+k");
        assert_eq!(kb["desktop.help_f1"], "");
        let over = desktop_overrides(&kb);
        assert_eq!(over["palette"], vec!["ctrl+j", "ctrl+k"]);
        assert!(over["help_f1"].is_empty());
        assert!(!over.contains_key("quit"));
        // Back to the defaults drops the entry.
        set_desktop_override(&mut kb, "palette", Some(&["ctrl+k".into()])).unwrap();
        assert!(!kb.contains_key("desktop.palette"));
        set_desktop_override(&mut kb, "help_f1", None).unwrap();
        assert!(!kb.contains_key("desktop.help_f1"));
        assert!(set_desktop_override(&mut kb, "nope", None).is_err());
        assert!(set_desktop_override(&mut kb, "board_diff", Some(&["x".into()])).is_err());
        // A TUI save replaces its own part only.
        set_desktop_override(&mut kb, "rename", Some(&["f2".into(), ",".into()])).unwrap();
        merge_tui_overrides(
            &mut kb,
            BTreeMap::from([("help".to_string(), "f1".to_string())]),
        );
        assert_eq!(kb["desktop.rename"], "f2, ,");
        assert_eq!(desktop_overrides(&kb)["rename"], vec!["f2", ","]);
        assert_eq!(kb["help"], "f1");
        assert!(!kb.contains_key("quit"));
    }

    #[test]
    fn the_generated_file_is_plain_ascii() {
        assert!(render_ts().is_ascii());
        assert_eq!(ts_str("\u{2318}P \u{1F600}"), "\"\\u2318P \\ud83d\\ude00\"");
    }

    #[test]
    fn actions_gen_ts_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(TS_PATH);
        let want = render_ts();
        if std::env::var_os("KETE_BLESS").is_some() {
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (run with KETE_BLESS=1)", path.display()))
            .replace('\r', "");
        assert!(
            have == want,
            "{} is stale: run KETE_BLESS=1 cargo test -p nebula-config actions_gen",
            path.display()
        );
    }
}
