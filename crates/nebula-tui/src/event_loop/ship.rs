//! The SHIP OVERLAY's side of the loop (`crate::ship_view`): opening the
//! pull request composer and the merge confirm on a checkout, running
//! what they ask for off the loop, and landing the answer — on the form
//! when it is still up, in the footer otherwise.

use super::*;
use crate::ship_view::{
    Composer, Fix, Form, MergeForm, Outcome, ShipAnswer, ShipJob, ShipOutcome, ShipView, Stage,
};
use kete_ado::ship::{MergeMethod, PrInfo};

/// What a composer asked for on a checkout already carrying an open pull
/// request says.
pub(crate) fn already_open(number: u64) -> String {
    format!("#{number} is already open on this branch: \u{21e7}V opens it, merge it from the board")
}

/// What the merge asked for on a checkout with no open pull request says.
pub(crate) const NO_OPEN_PR: &str = "no open pull request on this checkout to merge";

/// What a ship verb on the base branch itself says.
pub(crate) const ON_THE_BASE: &str =
    "this checkout is on its base branch: nothing here ships as a pull request";

/// The checkout `worktree`'s open pull request, as the card shows it.
fn open_pr_of(app: &App, worktree: &WorktreeId) -> Option<crate::launcher::RowPr> {
    crate::launcher::worktree_pr(app, worktree).filter(|pr| {
        matches!(
            pr.standing,
            crate::pull_request::Standing::Open | crate::pull_request::Standing::Draft
        )
    })
}

/// The session a suggestion is written from: `agent`, else the
/// checkout's most recently active one.
fn session_words(
    app: &App,
    worktree: &WorktreeId,
    agent: Option<&AgentId>,
) -> (Option<String>, Option<String>) {
    kete_ado::ship::session_words(&app.tree.agents, worktree, agent)
}

/// Hand `job` to the runner: off the loop in the running app, queued for
/// the test to answer in the unit tests. Its ticket.
pub(crate) fn start_job(app: &mut App, worktree: &WorktreeId, job: ShipJob) -> u64 {
    app.github.ship_ticket += 1;
    let ticket = app.github.ship_ticket;
    match app.github.ship_tx.clone() {
        Some(tx) => {
            let worktree = worktree.clone();
            tokio::spawn(async move {
                let outcome = crate::ship_view::run(job).await;
                let _ = tx.send(ShipAnswer {
                    ticket,
                    worktree,
                    outcome,
                });
            });
        }
        None => app.github.ship_queued.push((ticket, worktree.clone(), job)),
    }
    ticket
}

/// Where a ship verb on `worktree` stands before any form opens: its
/// checkout and branch, or why there is nothing to ship.
fn checkout(app: &App, worktree: &WorktreeId) -> Result<(std::path::PathBuf, String), String> {
    let Some(w) = app.tree.worktrees.iter().find(|w| &w.id == worktree) else {
        return Err("that worktree is gone".into());
    };
    if app.is_placeholder_worktree(worktree) {
        return Err("the worktree is still being cut".into());
    }
    if app
        .jobs
        .worktree_base
        .get(worktree)
        .is_some_and(|b| b.on_default_branch)
    {
        return Err(ON_THE_BASE.into());
    }
    Ok((w.path.clone(), w.branch.clone()))
}

/// The PULL REQUEST COMPOSER on `worktree`, prefilled from `agent` (or
/// the checkout's latest session) at once and from the commits since the
/// base once they are read. `back` is the REVIEW UNIT it stands over.
pub(crate) fn open_composer(
    app: &mut App,
    worktree: &WorktreeId,
    agent: Option<AgentId>,
    back: Option<Box<crate::app::DiffView>>,
) {
    app.chrome.dirty = true;
    if let Some(pr) = open_pr_of(app, worktree) {
        app.chrome.flash = Some(already_open(pr.number));
        return;
    }
    let (dir, branch) = match checkout(app, worktree) {
        Ok(found) => found,
        Err(why) => {
            app.chrome.flash = Some(why);
            return;
        }
    };
    let (name, prompt) = session_words(app, worktree, agent.as_ref());
    let first =
        kete_ado::ship::suggest_title_body(name.as_deref(), prompt.as_deref(), Some(&branch), &[]);
    let ticket = start_job(
        app,
        worktree,
        ShipJob::Suggest {
            dir: dir.clone(),
            base_setting: crate::config::Config::load().worktree_base_branch,
            name,
            prompt,
        },
    );
    app.modals.overlay = Some(Overlay::Ship(ShipView {
        worktree: worktree.clone(),
        agent,
        branch,
        dir,
        form: Form::Compose(Composer::new(first)),
        stage: Stage::Editing,
        ticket,
        back,
        area: ratatui::layout::Rect::default(),
        buttons: Vec::new(),
    }));
}

/// The MERGE CONFIRM on `worktree`'s open pull request.
pub(crate) fn open_merge(
    app: &mut App,
    worktree: &WorktreeId,
    agent: Option<AgentId>,
    back: Option<Box<crate::app::DiffView>>,
) {
    app.chrome.dirty = true;
    let Some(pr) = open_pr_of(app, worktree) else {
        app.chrome.flash = Some(NO_OPEN_PR.into());
        return;
    };
    let Some(w) = app.tree.worktrees.iter().find(|w| &w.id == worktree) else {
        return;
    };
    app.modals.overlay = Some(Overlay::Ship(ShipView {
        worktree: worktree.clone(),
        agent,
        branch: w.branch.clone(),
        dir: w.path.clone(),
        form: Form::Merge(MergeForm {
            number: pr.number,
            title: pr.title,
            url: pr.url,
            method: MergeMethod::Squash,
            // The local branch and its worktree stay either way: the
            // remote's copy is what a merged pull request leaves behind.
            delete_branch: true,
        }),
        stage: Stage::Editing,
        ticket: 0,
        back,
        area: ratatui::layout::Rect::default(),
        buttons: Vec::new(),
    }));
}

/// A key on the SHIP OVERLAY.
pub(crate) fn handle_key(app: &mut App, key: KeyEvent, out: &mut Vec<ClientRequest>) {
    let Some(Overlay::Ship(view)) = &mut app.modals.overlay else {
        return;
    };
    let outcome = view.key(&key);
    act(app, outcome, out);
}

/// A click on the SHIP OVERLAY: a button is its key; outside the box
/// closes it, as every modal's outside does.
pub(crate) fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    pos: ratatui::layout::Position,
    out: &mut Vec<ClientRequest>,
) -> bool {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return true;
    }
    let Some(Overlay::Ship(view)) = &mut app.modals.overlay else {
        return false;
    };
    let hit = view
        .buttons
        .iter()
        .find(|(r, _)| r.contains(pos))
        .map(|(_, b)| *b);
    let outcome = match hit {
        Some(button) => view.click(button),
        None if !view.area.contains(pos) && view.stage != Stage::Working => Outcome::Close,
        None => Outcome::None,
    };
    act(app, outcome, out);
    true
}

/// Carry out what the form asked for.
fn act(app: &mut App, outcome: Outcome, out: &mut Vec<ClientRequest>) {
    app.chrome.dirty = true;
    match outcome {
        Outcome::None => {}
        Outcome::Say(words) => app.chrome.flash = Some(words.into()),
        Outcome::Close => close(app, out),
        Outcome::Submit => submit(app),
        Outcome::Fix(fix) => run_fix(app, fix, out),
    }
}

/// Close the overlay onto what it stood over: the REVIEW UNIT it was
/// opened from, else the board at its card.
pub(crate) fn close(app: &mut App, out: &mut Vec<ClientRequest>) {
    let Some(Overlay::Ship(view)) = app.modals.overlay.take() else {
        return;
    };
    match view.back {
        Some(diff) => app.modals.overlay = Some(Overlay::Diff(*diff)),
        None => super::board::return_to_card(app, view.agent.as_ref(), out),
    }
}

/// Enter on the form: the create or the merge, off the loop.
fn submit(app: &mut App) {
    let Some(Overlay::Ship(view)) = &app.modals.overlay else {
        return;
    };
    if app.github.ship_inflight.contains(&view.worktree) {
        app.chrome.flash = Some("still shipping this checkout: wait for it to land".into());
        return;
    }
    let base_setting = crate::config::Config::load().worktree_base_branch;
    let job = match &view.form {
        Form::Compose(c) => ShipJob::Create {
            dir: view.dir.clone(),
            base_setting,
            pr: c.new_pr(),
        },
        Form::Merge(m) => ShipJob::Merge {
            dir: view.dir.clone(),
            base_setting,
            opts: m.opts(),
        },
    };
    let worktree = view.worktree.clone();
    let said = match &view.form {
        Form::Compose(_) => format!("pushing {} and opening its pull request…", view.branch),
        Form::Merge(m) => format!(
            "merging #{} ({})…",
            m.number,
            crate::ship_view::method_word(m.method)
        ),
    };
    app.github.ship_inflight.insert(worktree.clone());
    let ticket = start_job(app, &worktree, job);
    if let Some(Overlay::Ship(view)) = &mut app.modals.overlay {
        view.ticket = ticket;
        view.stage = Stage::Working;
    }
    app.chrome.flash = Some(said);
}

/// The failure's fix: a sign-in in a terminal of the checkout, a page in
/// the browser, or another try.
fn run_fix(app: &mut App, fix: Fix, out: &mut Vec<ClientRequest>) {
    match fix {
        Fix::Retry => submit(app),
        Fix::OpenUrl(url) => open_link(app, &url, out),
        Fix::SignIn(line) => {
            let Some(Overlay::Ship(view)) = &app.modals.overlay else {
                return;
            };
            let worktree = view.worktree.clone();
            app.modals.overlay = None;
            app.chrome.flash = Some(format!(
                "signing in: finish `{line}` in the terminal, then ship again"
            ));
            send_with(app, out, PendingIntent::SignIn { line }, |req_id| {
                ClientRequest::CreateTerminal {
                    req_id,
                    worktree,
                    name: Some("sign in".into()),
                }
            });
        }
    }
}

/// The pull request `info` as the board's rows read one: what makes the
/// card move to Ship (or Done) the moment the answer lands, before the
/// next lookup confirms it.
fn adopt(app: &mut App, worktree: &WorktreeId, info: &PrInfo) {
    use crate::pull_request::{STATE_CLOSED, STATE_MERGED, STATE_OPEN};
    let state = match info.state.as_str() {
        "merged" => STATE_MERGED,
        "closed" => STATE_CLOSED,
        _ => STATE_OPEN,
    };
    let pr = crate::pull_request::PullRequest {
        number: info.number,
        url: info.url.clone(),
        title: info.title.clone(),
        state: state.into(),
        is_draft: info.draft,
        head: info.head.clone(),
        head_sha: info.head_sha.clone().unwrap_or_default(),
        base: info.base.clone(),
        health: Default::default(),
        activity: Vec::new(),
    };
    app.github.pull_requests.insert(worktree.clone(), Some(pr));
    // And ask again soon, for what the forge says about it now.
    app.github.pr_recheck.remove(worktree);
}

/// A ship job landed.
pub(crate) fn land_ship(app: &mut App, answer: ShipAnswer, out: &mut Vec<ClientRequest>) {
    let ShipAnswer {
        ticket,
        worktree,
        outcome,
    } = answer;
    app.chrome.dirty = true;
    let mine = matches!(&app.modals.overlay, Some(Overlay::Ship(v)) if v.ticket == ticket);
    match outcome {
        ShipOutcome::Suggested { suggestion, base } => {
            if let Some(Overlay::Ship(ShipView {
                form: Form::Compose(c),
                ..
            })) = &mut app.modals.overlay
            {
                if mine {
                    c.suggested(suggestion, base);
                }
            }
        }
        ShipOutcome::Created(result) => {
            app.github.ship_inflight.remove(&worktree);
            match result {
                Ok(created) => {
                    adopt(app, &worktree, &created.pr);
                    let mut said = if created.existing {
                        format!(
                            "pushed: #{} was already open and has the new commits",
                            created.pr.number
                        )
                    } else {
                        format!("opened #{} {}", created.pr.number, created.pr.title)
                    };
                    if let Some(note) = created.note {
                        said.push_str(&format!(" ({note})"));
                    }
                    if mine {
                        close(app, out);
                    }
                    app.chrome.flash = Some(said);
                }
                Err(err) => fail(app, mine, err),
            }
        }
        ShipOutcome::Merged(result) => {
            app.github.ship_inflight.remove(&worktree);
            match result {
                Ok(merged) => {
                    adopt(app, &worktree, &merged.pr);
                    let mut said = if merged.state == "queued" {
                        format!("#{} is queued to merge behind its checks", merged.pr.number)
                    } else {
                        format!("merged #{} {}", merged.pr.number, merged.pr.title)
                    };
                    if merged.branch_deleted {
                        said.push_str(", the remote branch deleted");
                    }
                    if let Some(note) = merged.note {
                        said.push_str(&format!(" ({note})"));
                    }
                    if mine {
                        close(app, out);
                    }
                    app.chrome.flash = Some(said);
                }
                Err(err) => fail(app, mine, err),
            }
        }
    }
}

/// A create or merge refused: on the form, with its fix, when it is
/// still up; in the footer otherwise.
fn fail(app: &mut App, mine: bool, err: kete_ado::ship::ShipError) {
    match &mut app.modals.overlay {
        Some(Overlay::Ship(view)) if mine => view.stage = Stage::Failed(err),
        _ => app.chrome.flash = Some(err.message),
    }
}
