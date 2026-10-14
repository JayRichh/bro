import { describe, expect, it } from 'vitest';
import type { Agent, Model, PrInfo, TerminalTab, Worktree } from '../api';
import type { Branch } from './api';
import { FETCH_GAP_MS, branchName, fetchDue, localName, press, refusal, rows } from './branches';
import { cardsLine, dirty, liveCardsIn, offerLine, stateLines, worktreeOffer } from './offer';
import { laneOf, prWord } from '../status';
import { keysOf, shortcutFor } from '../shortcuts';

const wt = (id: string, extra: Partial<Worktree> = {}): Worktree => ({
  id,
  project_id: 'p',
  path: `C:/r/${id}`,
  branch: id,
  is_main: false,
  sort_order: 0,
  ...extra,
});
const agent = (id: string, worktree_id: string, extra: Partial<Agent> = {}): Agent =>
  ({ id, worktree_id, name: id, status: 'finished', archived: false, unseen: false, ...extra }) as Agent;
const term = (id: string, worktree_id: string): TerminalTab => ({ id, worktree_id, name: id, sort_order: 0, alive: true, run_command: null });
const model = (m: Partial<Model>): Model => ({ worktrees: [], agents: [], terminals: [], ...m }) as Model;

describe('the emptied worktree offer', () => {
  const base = model({
    worktrees: [wt('main', { is_main: true }), wt('topic')],
    agents: [agent('a', 'topic'), agent('old', 'topic', { archived: true }), agent('m', 'main')],
  });

  it('asks when the delete takes the last live card', () => {
    const o = worktreeOffer(base, 'topic', 1, ['a'], false);
    expect(o).toMatchObject({ auto: false, archived: 1 });
    expect(offerLine(o!)).toContain('1 archived session would go');
  });

  it('never offers the main checkout, or a worktree that keeps a card', () => {
    expect(worktreeOffer(base, 'main', 1, ['m'], true)).toBeNull();
    const busy = model({ ...base, terminals: [term('t', 'topic')] });
    expect(liveCardsIn(busy, 'topic')).toBe(2);
    expect(worktreeOffer(busy, 'topic', 1, ['a'], false)).toBeNull();
    expect(worktreeOffer(busy, 'topic', 2, ['a', 't'], false)).not.toBeNull();
  });

  it('goes without a question when the setting is on and nothing archived is left', () => {
    expect(worktreeOffer(base, 'topic', 1, ['a'], true)?.auto).toBe(false);
    // Deleting the archived one with the live one: nothing is left behind.
    const o = worktreeOffer(base, 'topic', 1, ['a', 'old'], true);
    expect(o?.auto).toBe(true);
    expect(offerLine(o!)).toContain('goes with it');
  });

  it('offers on deleting an archived session from an already empty worktree', () => {
    const empty = model({ worktrees: [wt('topic')], agents: [agent('old', 'topic', { archived: true })] });
    expect(worktreeOffer(empty, 'topic', 0, ['old'], false)).toMatchObject({ auto: false, archived: 0 });
  });
});

describe('what a worktree delete would throw away', () => {
  it('spells the git state and the cards', () => {
    expect(stateLines({ changed: 2, untracked: 1, unpushed: 3 })).toEqual([
      '2 uncommitted changes, 1 untracked file',
      '3 commits on no remote (the branch keeps them)',
    ]);
    expect(stateLines({ changed: 0, untracked: 0, unpushed: null })).toEqual(['No uncommitted changes']);
    expect(stateLines(null)).toEqual(['Could not read its git status.']);
    expect(dirty({ changed: 0, untracked: 1, unpushed: 0 })).toBe(true);
    expect(dirty(null)).toBe(false);
    expect(cardsLine(2, 1)).toBe('2 sessions, 1 terminal in it');
    expect(cardsLine(0, 0)).toBeNull();
  });
});

const br = (name: string, extra: Partial<Branch> = {}): Branch => ({
  name,
  remote: false,
  current: false,
  checked_out_at: null,
  committed: 0,
  subject: '',
  ...extra,
});

describe('the branch switcher', () => {
  const list = [br('main', { current: true }), br('feature-x', { checked_out_at: 'C:/r/wt' }), br('origin/release', { remote: true })];

  it('lists matches and offers to create a name no branch has', () => {
    const r = rows(list, 'feat');
    expect(r.map((x) => (x.kind === 'branch' ? x.branch.name : `+${x.name}`))).toEqual(['feature-x', '+feat']);
    expect(rows(list, 'zzz new thing')).toEqual([{ kind: 'create', name: 'zzz-new-thing' }]);
  });

  it('does not offer to create a branch that exists, locally or as a remote twin', () => {
    expect(rows(list, 'feature-x').some((x) => x.kind === 'create')).toBe(false);
    expect(rows(list, 'release').some((x) => x.kind === 'create')).toBe(false);
    expect(rows(list, '').some((x) => x.kind === 'create')).toBe(false);
    expect(localName(list[2])).toBe('release');
    expect(branchName('  my  fix ')).toBe('my-fix');
  });

  it('refuses the current branch and one another worktree holds', () => {
    expect(refusal(list[0])).toContain('Already on');
    expect(refusal(list[1])).toContain('C:/r/wt');
    expect(refusal(list[2])).toBeNull();
  });

  it('is c on a row, as the terminal app has it', () => {
    expect(keysOf('switch_branch')).toEqual(['c']);
    expect(shortcutFor({ key: 'c', ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { inTerminal: false, typing: false, onRow: true })?.id).toBe(
      'switch_branch',
    );
  });

  it('fetches at most once a minute', () => {
    expect(fetchDue(undefined, 1000)).toBe(true);
    expect(fetchDue(1000, 1000 + FETCH_GAP_MS - 1)).toBe(false);
    expect(fetchDue(1000, 1000 + FETCH_GAP_MS)).toBe(true);
  });

  it('discards only on a second press, and any other choice disarms', () => {
    let s = press(false, 'discard');
    expect(s).toEqual({ armed: true, run: false });
    s = press(s.armed, 'discard');
    expect(s).toEqual({ armed: false, run: true });
    expect(press(true, 'stash')).toEqual({ armed: false, run: true });
    expect(press(false, 'bring')).toEqual({ armed: false, run: true });
  });
});

describe('pull request states', () => {
  const pr = (state?: PrInfo['state'], draft = false): PrInfo => ({ number: 1, url: '', title: '', draft, state });

  it('keeps merged and closed PRs out of the PR open lane', () => {
    const a = agent('a', 'w');
    expect(laneOf(a, pr('open'))).toBe('pr');
    expect(laneOf(a, pr(undefined))).toBe('pr');
    expect(laneOf(a, pr('open', true))).toBe('pr');
    expect(laneOf(a, pr('merged'))).toBe('done');
    expect(laneOf(a, pr('closed'))).toBe('done');
  });

  it('names the state', () => {
    expect([pr('open'), pr('open', true), pr('merged'), pr('closed')].map(prWord)).toEqual(['PR', 'draft PR', 'merged PR', 'closed PR']);
  });
});
