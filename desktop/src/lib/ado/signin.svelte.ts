// Signing in to Azure DevOps, as the views show it. A command that may
// open a sign-in window (GCM's, or az's) and is still pending after about
// a second is probably waiting on one, so the views say "Signing in".
// "Sign in again" runs `ado_sign_in` (az login for the org's tenant).

import { invoke } from '@tauri-apps/api/core';
import { errText, SIGNING_IN_AFTER_MS } from './errors';

type Invoke = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;

export function createSignin(call: Invoke = invoke, after: number = SIGNING_IN_AFTER_MS) {
  const s = $state({
    /** Prompting commands pending for longer than `after`. */
    slow: 0,
    /** `ado_sign_in` is running (a browser window is open). */
    busy: false,
    /** Why the last "Sign in again" failed, if it did. */
    error: null as string | null,
  });

  return {
    get slow() {
      return s.slow > 0;
    },
    get busy() {
      return s.busy;
    },
    get error() {
      return s.error;
    },
    /** Hand back `p`, counting it as slow while it runs past `after`. */
    track<T>(p: Promise<T>): Promise<T> {
      let counted = false;
      const timer = setTimeout(() => {
        counted = true;
        s.slow++;
      }, after);
      const done = () => {
        clearTimeout(timer);
        if (counted) s.slow--;
      };
      p.then(done, done);
      return p;
    },
    /** "Sign in again" for the repo of `worktree`; true when it worked. */
    async signIn(worktree: string): Promise<boolean> {
      s.busy = true;
      s.error = null;
      try {
        await call('ado_sign_in', { worktree });
        return true;
      } catch (e) {
        s.error = errText(e);
        return false;
      } finally {
        s.busy = false;
      }
    },
  };
}

export const signin = createSignin();

/** `invoke` for a command that may open a sign-in window. */
export function promptingInvoke<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  return signin.track(invoke<T>(cmd, args));
}
