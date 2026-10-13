import { afterEach, expect, test, vi } from 'vitest';
import { errKind, errState, errText } from './errors';
import { createSignin } from './signin.svelte';

afterEach(() => {
  vi.useRealTimers();
});

test('errors read the same as objects and as strings', () => {
  const auth = { kind: 'auth', message: 'Azure DevOps did not accept your sign-in.' };
  expect(errText(auth)).toBe('Azure DevOps did not accept your sign-in.');
  expect(errKind(auth)).toBe('auth');
  expect(errText('that worktree is gone')).toBe('that worktree is gone');
  expect(errKind('that worktree is gone')).toBe('other');
  expect(errKind({ kind: 'nonsense', message: 'x' })).toBe('other');
  expect(errState({ kind: 'offline', message: 'down' })).toEqual({ text: 'down', kind: 'offline' });
  // Never "[object Object]".
  expect(errText({ kind: 'forbidden', message: 'no access' })).toBe('no access');
});

test('a prompting command counts as signing in only after a second', async () => {
  vi.useFakeTimers();
  const s = createSignin(async () => undefined, 1000);
  let finish!: () => void;
  const p = s.track(new Promise<void>((r) => (finish = r)));
  vi.advanceTimersByTime(900);
  expect(s.slow).toBe(false);
  vi.advanceTimersByTime(200);
  expect(s.slow).toBe(true);
  finish();
  await p;
  expect(s.slow).toBe(false);
});

test('a quick command never shows signing in', async () => {
  vi.useFakeTimers();
  const s = createSignin(async () => undefined, 1000);
  await s.track(Promise.reject(new Error('x'))).catch(() => {});
  vi.advanceTimersByTime(2000);
  expect(s.slow).toBe(false);
});

test('sign in again calls ado_sign_in and keeps its failure', async () => {
  const calls: [string, Record<string, unknown>][] = [];
  const ok = createSignin(async (cmd, args) => {
    calls.push([cmd, args]);
  });
  expect(await ok.signIn('wt-1')).toBe(true);
  expect(calls).toEqual([['ado_sign_in', { worktree: 'wt-1' }]]);
  expect(ok.busy).toBe(false);

  const bad = createSignin(async () => {
    throw { kind: 'auth', message: 'The Azure CLI (az) is not installed, so Kete cannot sign you in.' };
  });
  expect(await bad.signIn('wt-1')).toBe(false);
  expect(bad.error).toContain('not installed');
});
