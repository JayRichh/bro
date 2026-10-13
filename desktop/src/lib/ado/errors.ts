// ADO command errors as the window reads them. The ADO commands reject with
// `{ kind, message }` (CmdError in client.rs); other commands, and older
// paths, reject with a plain string. Both read the same way here.

/** Why an ADO command failed: `auth` offers "Sign in again". */
export type ErrKind = 'auth' | 'forbidden' | 'offline' | 'other';

const KINDS: ErrKind[] = ['auth', 'forbidden', 'offline', 'other'];

/** The failure in words, whatever shape it came in. */
export function errText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e && typeof e === 'object' && 'message' in e && typeof (e as { message: unknown }).message === 'string')
    return (e as { message: string }).message;
  return String(e);
}

/** The failure's kind; a plain string or anything unknown is `other`. */
export function errKind(e: unknown): ErrKind {
  if (e && typeof e === 'object' && 'kind' in e) {
    const k = (e as { kind: unknown }).kind;
    if (KINDS.includes(k as ErrKind)) return k as ErrKind;
  }
  return 'other';
}

/** A failure as views keep it: the words and whether signing in helps. */
export interface ErrState {
  text: string;
  kind: ErrKind;
}

export function errState(e: unknown): ErrState {
  return { text: errText(e), kind: errKind(e) };
}

/** How long a prompting command may be pending before the view says it is signing in. */
export const SIGNING_IN_AFTER_MS = 1000;
