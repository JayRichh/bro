import { describe, expect, it } from 'vitest';
import type { DropTarget, DropVerb, PrInfo, ShipStatus } from '../api';
import {
  asShipError,
  commitsWord,
  createBlocked,
  dropHint,
  dropVerbOf,
  errorAction,
  GH_INSTALL_URL,
  laneLook,
  newPrOf,
  prChip,
  reviewVerbs,
  seedForm,
} from './ship';

const drops: Partial<Record<DropTarget, DropVerb>> = {
  unseen: { act: 'reorder' },
  pr: { act: 'open_pr' },
  done: { act: 'refuse', reason: 'Its worktree has changes no pull request carries.' },
  running: { act: 'refuse', reason: 'Only the agent puts a card in Running.' },
  archived: { act: 'archive' },
};

const status = (extra: Partial<ShipStatus> = {}): ShipStatus => ({
  forge: 'github',
  remote: 'origin',
  branch: 'fix-login',
  base: 'main',
  unpushed: 2,
  pr: null,
  error: null,
  suggestion: { title: 'Fix the login', body: 'Body' },
  ...extra,
});

describe('board drops', () => {
  it('reads the snapshot, refusing a target it has no verdict for', () => {
    expect(dropVerbOf(drops, 'pr')).toEqual({ act: 'open_pr' });
    expect(dropVerbOf(undefined, 'done').act).toBe('refuse');
  });

  it('marks a lane that acts, dims one that refuses, leaves its own alone', () => {
    expect(laneLook(drops, 'unseen', 'pr')).toBe('act');
    expect(laneLook(drops, 'unseen', 'done')).toBe('refuse');
    expect(laneLook(drops, 'unseen', 'unseen')).toBe('none');
    expect(laneLook(drops, 'unseen', 'archived')).toBe('act');
  });

  it('says on the lane what the drop does, or why not', () => {
    expect(dropHint({ act: 'open_pr' })).toMatch(/pull request/);
    expect(dropHint(drops.done!)).toBe('Its worktree has changes no pull request carries.');
    expect(dropHint({ act: 'reorder' })).toBe('');
  });
});

describe('ship errors', () => {
  it('keeps a ShipError, wraps anything else as other', () => {
    const e = { kind: 'signIn', forge: 'ado', message: 'sign in', url: null };
    expect(asShipError(e)).toEqual(e);
    expect(asShipError('boom')).toEqual({ kind: 'other', forge: null, message: 'boom', url: null });
    expect(asShipError(new Error('x')).message).toBe('x');
  });

  it('gives each kind its button', () => {
    expect(errorAction(asShipError({ kind: 'signIn', forge: 'github', message: '', url: null }))).toEqual({ kind: 'signIn', label: 'Sign in', forge: 'github' });
    expect(errorAction(asShipError({ kind: 'noTool', forge: 'github', message: '', url: null }))).toEqual({ kind: 'open', label: 'How to install gh', url: GH_INSTALL_URL });
    expect(errorAction(asShipError({ kind: 'mergeInAdo', forge: 'ado', message: '', url: 'https://dev.azure.com/x' }))).toEqual({
      kind: 'open',
      label: 'Open in ADO',
      url: 'https://dev.azure.com/x',
    });
    expect(errorAction(asShipError({ kind: 'offline', forge: null, message: '', url: null }))).toBeNull();
  });
});

describe('the composer', () => {
  it('starts from the suggestion and the review base, not a draft', () => {
    expect(seedForm(status())).toEqual({ title: 'Fix the login', body: 'Body', base: 'main', draft: false });
    expect(seedForm(status({ suggestion: null })).title).toBe('fix-login');
  });

  it('sends the base only when it was changed', () => {
    const f = { title: ' T ', body: 'b', base: 'main', draft: true };
    expect(newPrOf(f, status())).toEqual({ title: 'T', body: 'b', draft: true, base: null });
    expect(newPrOf({ ...f, base: 'release' }, status()).base).toBe('release');
  });

  it('says why Create cannot go', () => {
    const f = seedForm(status());
    expect(createBlocked(f, status())).toBeNull();
    expect(createBlocked(f, null)).toMatch(/Reading/);
    expect(createBlocked(f, status({ forge: null }))).toMatch(/neither/);
    expect(createBlocked(f, status({ branch: null }))).toMatch(/detached/);
    expect(createBlocked({ ...f, title: ' ' }, status())).toMatch(/title/);
    expect(createBlocked({ ...f, base: 'fix-login' }, status())).toMatch(/base/);
    const pr = { forge: 'github', number: 1, url: 'u', title: 't', state: 'open', draft: false, head: 'h', base: 'main', headSha: null } as const;
    expect(createBlocked(f, status({ pr }))).toMatch(/already/);
  });
});

describe('review verbs', () => {
  const pr = (extra: Partial<PrInfo> = {}): PrInfo => ({ number: 7, url: 'u', title: 't', draft: false, state: 'open', ...extra });

  it('opens a PR without one, views and merges an open one', () => {
    expect(reviewVerbs(null, { act: 'open_pr' }, false)).toEqual({ pr: { kind: 'open', why: null }, merge: null, deleteWorktree: true });
    const open = reviewVerbs(pr(), { act: 'reorder' }, false);
    expect(open.pr.kind).toBe('view');
    expect(open.merge).toEqual({ why: null });
    expect(reviewVerbs(pr({ state: 'merged' }), undefined, false).merge).toBeNull();
  });

  it('greys a draft merge and a PR from the base branch, never offers to delete the root', () => {
    expect(reviewVerbs(pr({ draft: true }), undefined, false).merge?.why).toMatch(/draft/);
    const base = reviewVerbs(null, { act: 'refuse', reason: 'This is the base branch: no.' }, true);
    expect(base.pr.why).toMatch(/base branch/);
    expect(base.deleteWorktree).toBe(false);
    expect(reviewVerbs(null, { act: 'refuse', reason: 'Running is the agent\u2019s' }, false).pr.why).toBeNull();
  });

  it('words chips and commit counts', () => {
    expect(prChip(pr()).word).toBe('PR #7');
    expect(prChip(pr({ draft: true })).tone).toBe('draft');
    expect(prChip(pr({ state: 'merged' })).word).toBe('merged #7');
    expect(commitsWord(0)).toBeNull();
    expect(commitsWord(1)).toBe('1 commit');
    expect(commitsWord(3)).toBe('3 commits');
  });
});
