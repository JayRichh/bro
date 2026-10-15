//! The WORKTREE SWEEP: a project's "Delete merged & clean worktrees…",
//! the desktop Worktrees view's sweep (`desktop/src/lib/worktrees/
//! verbs.ts` `sweepWorktrees`) in the terminal.
//!
//! The candidates are the project's linked worktrees with no live session
//! or terminal in them. Each is read off the loop with the DELETE CHECK's
//! own git (`delete_check::read`); one is swept when nothing in it is
//! uncommitted and either its pull request merged or its commits are all
//! on a remote with no pull request open. One confirm lists them; each
//! then goes the single delete's way, unforced and marked bulk, so the
//! DAEMON or git keeps one that picked up work since and the refusal only
//! flashes: a sweep never escalates to a forced delete.

use crate::app::{App, ConfirmDialog, Overlay, PendingAction};
use crate::delete_check::Findings;
use crate::pull_request::Standing;
use nebula_core::{ProjectId, WorktreeId};

/// Why a checkout is swept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Its pull request merged.
    Merged,
    /// Its commits are all on a remote and no pull request is open.
    Clean,
}

impl Why {
    fn word(self) -> &'static str {
        match self {
            Why::Merged => "PR merged",
            Why::Clean => "clean",
        }
    }
}

/// The desktop's `sweepReason`: nothing uncommitted, then a merged pull
/// request, else nothing unpushed and no open one. A read that failed is
/// never swept.
pub fn reason(found: &Result<Findings, String>, pr: Option<Standing>) -> Option<Why> {
    let found = found.as_ref().ok()?;
    if found.changes > 0 {
        return None;
    }
    if pr == Some(Standing::Merged) {
        return Some(Why::Merged);
    }
    let open = matches!(pr, Some(Standing::Open | Standing::Draft));
    (found.unpushed == Some(0) && !open).then_some(Why::Clean)
}

/// Whether anything live (an unarchived session, a terminal) is in `id`.
fn live(app: &App, id: &WorktreeId) -> bool {
    crate::delete_check::live_in(app, id).total() > 0
}

/// The checkouts a sweep asks git about: `project`'s linked worktrees,
/// nothing live in them, none still being cut.
pub fn candidates(app: &App, project: &ProjectId) -> Vec<(WorktreeId, std::path::PathBuf)> {
    app.tree
        .worktrees
        .iter()
        .filter(|w| &w.project_id == project && !w.is_main)
        .filter(|w| !app.is_placeholder_worktree(&w.id) && !live(app, &w.id))
        .map(|w| (w.id.clone(), w.path.clone()))
        .collect()
}

/// The menu row's verb: read the candidates off the loop (inline where no
/// loop runs, the unit tests), the confirm coming up when they land.
pub fn start(app: &mut App, project: &ProjectId) {
    let found = candidates(app, project);
    if found.is_empty() {
        app.chrome.flash =
            Some("no worktree here is free to delete: each has a session or terminal in it".into());
        return;
    }
    let read = move || {
        found
            .into_iter()
            .map(|(id, path)| {
                let result = crate::delete_check::read(&path);
                (id, result)
            })
            .collect::<Vec<_>>()
    };
    match app.jobs.view_jobs.clone() {
        Some(jobs) => {
            app.chrome.flash = Some("reading the worktrees…".into());
            let project = project.clone();
            jobs.run(move || {
                Some(crate::view_jobs::Answer::Sweep {
                    project,
                    results: read(),
                })
            });
        }
        None => land(app, project.clone(), read()),
    }
}

/// The reads landed: the confirm listing what goes, or why nothing does.
pub fn land(
    app: &mut App,
    project: ProjectId,
    results: Vec<(WorktreeId, Result<Findings, String>)>,
) {
    app.chrome.dirty = true;
    let mut picked = Vec::new();
    for (id, result) in results {
        // Gone, or something started in it while git ran.
        let Some(w) = app.tree.worktrees.iter().find(|w| w.id == id) else {
            continue;
        };
        if live(app, &id) {
            continue;
        }
        let pr = crate::launcher::worktree_pr(app, &id).map(|p| p.standing);
        if let Some(why) = reason(&result, pr) {
            let archived = app
                .tree
                .agents
                .iter()
                .filter(|a| a.worktree_id == id && a.archived)
                .count();
            picked.push((id, w.branch.clone(), why, archived));
        }
    }
    if picked.is_empty() {
        app.chrome.flash = Some(
            "nothing to clean: every free worktree has uncommitted work, unpushed commits or an open PR"
                .into(),
        );
        return;
    }
    // A confirm already up (one the user opened since) is left alone.
    if app.modals.overlay.is_some() {
        app.chrome.flash = Some(format!(
            "{} worktree{} free to sweep: close what is open and ask again",
            picked.len(),
            if picked.len() == 1 { " is" } else { "s are" }
        ));
        return;
    }
    let name = app
        .tree
        .projects
        .iter()
        .find(|p| p.id == project)
        .map(|p| p.name.clone())
        .unwrap_or_default();
    app.modals.overlay = Some(Overlay::Confirm(confirm(&name, &picked)));
}

/// Branches a confirm spells out before it says `+N more`.
const LISTED: usize = 8;

/// The one confirm: each checkout, why it goes, and the archived
/// sessions that go with it.
pub fn confirm(project: &str, picked: &[(WorktreeId, String, Why, usize)]) -> ConfirmDialog {
    let n = picked.len();
    let mut lines = vec![format!(
        "Delete {n} worktree{} of {project}? No session, terminal or uncommitted work is in them, and their commits are on a remote or their PR merged. Their folders go; the branches stay.",
        if n == 1 { "" } else { "s" }
    )];
    for (_, branch, why, archived) in picked.iter().take(LISTED) {
        let mut line = format!("  {branch} · {}", why.word());
        if *archived > 0 {
            line.push_str(&format!(
                " · {archived} archived session{} go{} with it",
                if *archived == 1 { "" } else { "s" },
                if *archived == 1 { "es" } else { "" }
            ));
        }
        lines.push(line);
    }
    if n > LISTED {
        lines.push(format!("  +{} more", n - LISTED));
    }
    ConfirmDialog {
        title: format!("Delete {n} worktree{}", if n == 1 { "" } else { "s" }),
        message: lines.join("\n"),
        action: PendingAction::SweepWorktrees(picked.iter().map(|p| p.0.clone()).collect()),
        area: ratatui::layout::Rect::default(),
    }
}

/// The confirmed sweep's checkouts that may still go: there, and nothing
/// started in them while the confirm was up.
pub fn still_free(app: &App, ids: Vec<WorktreeId>) -> Vec<WorktreeId> {
    ids.into_iter()
        .filter(|id| app.tree.worktrees.iter().any(|w| &w.id == id) && !live(app, id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(changes: usize, unpushed: Option<usize>) -> Result<Findings, String> {
        Ok(Findings { changes, unpushed })
    }

    #[test]
    fn a_checkout_goes_when_merged_or_pushed_and_never_with_work_in_it() {
        assert_eq!(
            reason(&found(0, Some(3)), Some(Standing::Merged)),
            Some(Why::Merged)
        );
        assert_eq!(reason(&found(0, Some(0)), None), Some(Why::Clean));
        assert_eq!(
            reason(&found(0, Some(0)), Some(Standing::Closed)),
            Some(Why::Clean)
        );
        assert_eq!(reason(&found(0, Some(0)), Some(Standing::Open)), None);
        assert_eq!(reason(&found(0, Some(0)), Some(Standing::Draft)), None);
        assert_eq!(reason(&found(1, Some(0)), Some(Standing::Merged)), None);
        assert_eq!(reason(&found(0, Some(2)), None), None);
        assert_eq!(reason(&found(0, None), None), None, "no remote to compare");
        assert_eq!(
            reason(&Err("no folder".into()), Some(Standing::Merged)),
            None
        );
    }

    #[test]
    fn the_confirm_lists_each_and_folds_the_rest() {
        let picked: Vec<_> = (0..10)
            .map(|i| {
                (
                    WorktreeId(format!("w{i}")),
                    format!("b{i}"),
                    if i == 0 { Why::Merged } else { Why::Clean },
                    usize::from(i == 1),
                )
            })
            .collect();
        let c = confirm("demo", &picked);
        assert_eq!(c.title, "Delete 10 worktrees");
        assert!(c.message.contains("  b0 · PR merged"), "{}", c.message);
        assert!(
            c.message
                .contains("  b1 · clean · 1 archived session goes with it"),
            "{}",
            c.message
        );
        assert!(c.message.contains("  +2 more"), "{}", c.message);
        assert!(!c.message.contains("b9"));
        match c.action {
            PendingAction::SweepWorktrees(ids) => assert_eq!(ids.len(), 10),
            other => panic!("{other:?}"),
        }
    }
}
