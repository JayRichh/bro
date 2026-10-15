// A session row dragged onto a worktree's header moves it there (the
// terminal app's drag move; `m` is the keyboard way). The drag carries the
// session's id under this type, so nothing else dropped there counts.
export const DRAG_SESSION = 'application/x-kete-session';
