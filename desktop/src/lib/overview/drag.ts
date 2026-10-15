// A session row dragged onto a worktree's header moves it there (the
// terminal app's drag move; `m` is the keyboard way). The drag carries the
// session's id under this type, so nothing else dropped there counts.
export const DRAG_SESSION = 'application/x-kete-session';
/** A worktree group's header, dragged to reorder a repo's checkouts. */
export const DRAG_WORKTREE = 'application/x-kete-worktree';
/** A repo (rail row or overview header), dragged to reorder the repos. */
export const DRAG_PROJECT = 'application/x-kete-project';
