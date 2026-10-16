import { describe, expect, it, vi } from 'vitest';
import {
  attentionItems,
  branchItems,
  fileItems,
  laneItems,
  localMenu,
  orchestratorItems,
  portItems,
  prItems,
  taskItems,
  viewItems,
} from './rows';
import type { BranchState, FileState, LaneId, LaneState, PortState, PrStanding, PrState, PrVerbs, TaskState } from './rows';
import type { Row } from './layout';
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';

const SEP = 'sep';

const rows = (es: MenuEntry[]): MenuItem[] => es.filter((e): e is MenuItem => typeof e === 'object' && 'label' in e);
const labels = (es: MenuEntry[]) => es.map((e) => (e === 'sep' ? 'sep' : 'heading' in e ? e.heading : e.label));

function item(es: MenuEntry[], label: string): MenuItem {
  const r = rows(es).find((x) => x.label === label);
  if (!r) throw new Error(`no row labelled ${label}`);
  return r;
}

/** The rows inside the submenu row `label`. */
function kids(es: MenuEntry[], label: string): MenuItem[] {
  const r = item(es, label);
  if (!r.children) throw new Error(`${label} has no submenu`);
  return rows(r.children);
}

/** The labels of the greyed-out rows, in menu order. */
const disabledRows = (es: MenuEntry[]) => rows(es).filter((r) => r.disabled).map((r) => r.label);
/** Every greyed-out row says why. */
const allExplained = (es: MenuEntry[]) => rows(es).filter((r) => r.disabled).every((r) => !!r.title);
/** The rows that open a submenu. */
const folds = (es: MenuEntry[]) => rows(es).filter((r) => r.children?.length).map((r) => r.label);

const DRAFT_MERGE = 'Draft pull request: publish it first';
const DRAFT_VOTE = 'Draft pull request: votes open once it is published';
const CHECKED_OUT = 'Checked out in a worktree: remove the worktree first';

// ---- pull requests ----

const URL_GH = 'https://github.com/o/r/pull/42';
const URL_ADO = 'https://dev.azure.com/o/p/_git/r/pullrequest/42';

/** An open GitHub pull request with a local worktree and no live session. */
const pr = (o: Partial<PrState> = {}): PrState => ({
  number: 42,
  url: URL_GH,
  standing: 'open',
  branch: 'feat/x',
  sessions: [],
  worktree: { id: 'w1', branch: 'feat/x' },
  canCheckout: false,
  ...o,
});
/** An Azure DevOps one someone else opened, with no local worktree yet. */
const adoPr = (o: Partial<PrState> = {}): PrState => pr({ url: URL_ADO, worktree: null, canCheckout: true, ado: { mine: false }, ...o });
const sessions = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `s${i + 1}`, name: `work ${i + 1}` }));

const ghVerbs = () => ({ open: vi.fn(), goSession: vi.fn(), goWorktree: vi.fn(), copy: vi.fn(), merge: vi.fn(), refresh: vi.fn() });
const adoVerbs = () => ({ ...ghVerbs(), review: vi.fn(), checkout: vi.fn(), vote: vi.fn(), publish: vi.fn() });

describe('prItems: a GitHub pull request', () => {
  it('lists Open in browser, Go to worktree, the Copy rows, Merge and Refresh, a rule between sections', () => {
    expect(labels(prItems(pr(), ghVerbs()))).toEqual([
      'Open in browser',
      SEP,
      'Go to worktree feat/x',
      SEP,
      'Copy link',
      'Copy number (#42)',
      'Copy branch name',
      SEP,
      'Merge\u2026',
      'Refresh pull requests',
    ]);
  });

  it('titles Open in browser with the link, and offers no Review, Vote or Publish', () => {
    const items = prItems(pr(), ghVerbs());
    expect(item(items, 'Open in browser').title).toBe(URL_GH);
    for (const l of ['Review', 'Vote', 'Publish']) expect(labels(items)).not.toContain(l);
  });

  it('adds a Go to session row for each live session, three at most, ahead of Go to worktree', () => {
    const items = prItems(pr({ sessions: sessions(5) }), ghVerbs());
    expect(labels(items).slice(0, 7)).toEqual([
      'Open in browser',
      SEP,
      'Go to session work 1',
      'Go to session work 2',
      'Go to session work 3',
      'Go to worktree feat/x',
      SEP,
    ]);
    expect(labels(items).filter((l) => l.startsWith('Go to session'))).toHaveLength(3);
  });

  it('leaves out Go to worktree, Merge and Check out without a local worktree', () => {
    const items = prItems(pr({ worktree: null, canCheckout: true }), ghVerbs());
    expect(labels(items)).toEqual([
      'Open in browser',
      SEP,
      'Copy link',
      'Copy number (#42)',
      'Copy branch name',
      SEP,
      'Refresh pull requests',
    ]);
  });

  it('leaves out the link without a url, and the branch name without a branch', () => {
    expect(labels(prItems(pr({ url: null }), ghVerbs()))).toEqual([
      'Go to worktree feat/x',
      SEP,
      'Copy number (#42)',
      'Copy branch name',
      SEP,
      'Merge\u2026',
      'Refresh pull requests',
    ]);
    const noBranch = labels(prItems(pr({ branch: null }), ghVerbs()));
    expect(noBranch).not.toContain('Copy branch name');
    expect(noBranch).toContain('Copy link');
  });

  it('runs Open, Go to session, Go to worktree, the Copy rows, Merge and Refresh through their verbs', () => {
    const v = ghVerbs();
    const items = prItems(pr({ sessions: sessions(1) }), v);
    item(items, 'Open in browser').run();
    expect(v.open).toHaveBeenCalledTimes(1);
    item(items, 'Go to session work 1').run();
    expect(v.goSession).toHaveBeenCalledWith('s1');
    item(items, 'Go to worktree feat/x').run();
    expect(v.goWorktree).toHaveBeenCalledWith('w1');
    item(items, 'Merge\u2026').run();
    expect(v.merge).toHaveBeenCalledTimes(1);
    item(items, 'Refresh pull requests').run();
    expect(v.refresh).toHaveBeenCalledTimes(1);
    expect(v.copy).not.toHaveBeenCalled();
  });

  it('copies the link, the number and the branch, each saying what it copied', () => {
    const v = ghVerbs();
    const items = prItems(pr(), v);
    item(items, 'Copy link').run();
    expect(v.copy).toHaveBeenLastCalledWith(URL_GH, URL_GH);
    item(items, 'Copy number (#42)').run();
    expect(v.copy).toHaveBeenLastCalledWith('#42', '#42');
    item(items, 'Copy branch name').run();
    expect(v.copy).toHaveBeenLastCalledWith('feat/x', 'feat/x');
    expect(v.copy).toHaveBeenCalledTimes(3);
  });

  it('greys out Merge on a draft, saying to publish it first, and keeps the rest', () => {
    const items = prItems(pr({ standing: 'draft' }), ghVerbs());
    expect(disabledRows(items)).toEqual(['Merge\u2026']);
    expect(item(items, 'Merge\u2026').title).toBe(DRAFT_MERGE);
    expect(item(items, 'Refresh pull requests').disabled).toBeFalsy();
    expect(item(items, 'Open in browser').disabled).toBeFalsy();
  });

  it('leaves Merge enabled on an open pull request', () => {
    const items = prItems(pr(), ghVerbs());
    expect(disabledRows(items)).toEqual([]);
    expect(item(items, 'Merge\u2026').title).toBeUndefined();
  });

  it('leaves Merge out when the caller has no merge verb', () => {
    const v: PrVerbs = ghVerbs();
    delete v.merge;
    expect(labels(prItems(pr(), v))).not.toContain('Merge\u2026');
  });

  it('offers no Vote on a GitHub pull request, whatever verbs it is given', () => {
    expect(labels(prItems(pr(), adoVerbs()))).not.toContain('Vote');
  });
});

describe('prItems: merged and closed', () => {
  const ended: [PrStanding, string][] = [
    ['merged', 'Open merged pull request'],
    ['closed', 'Open closed pull request'],
  ];

  it.each(ended)('a %s pull request keeps only Open, Go to and Copy: %s', (standing, open) => {
    const items = prItems(pr({ standing, sessions: sessions(1) }), ghVerbs());
    expect(labels(items)).toEqual([
      open,
      SEP,
      'Go to session work 1',
      'Go to worktree feat/x',
      SEP,
      'Copy link',
      'Copy number (#42)',
      'Copy branch name',
    ]);
    expect(item(items, open).title).toBe(URL_GH);
  });

  const endings: PrStanding[] = ['merged', 'closed'];

  it.each(endings)('a %s Azure DevOps pull request keeps Review but offers no Vote, Publish, Check out, Merge or Refresh', (standing) => {
    const items = prItems(adoPr({ standing }), adoVerbs());
    expect(labels(items)).toEqual([
      'Review',
      `Open ${standing} pull request`,
      SEP,
      'Copy link',
      'Copy number (#42)',
      'Copy branch name',
    ]);
  });

  it('offers no Publish on a closed pull request of mine', () => {
    expect(labels(prItems(adoPr({ standing: 'closed', ado: { mine: true } }), adoVerbs()))).not.toContain('Publish');
  });
});

describe('prItems: an Azure DevOps pull request', () => {
  it('puts Review first, then Open, Check out, Copy, Vote and Refresh', () => {
    const items = prItems(adoPr(), adoVerbs());
    expect(labels(items)).toEqual([
      'Review',
      'Open in browser',
      SEP,
      'Check out in new worktree\u2026',
      SEP,
      'Copy',
      SEP,
      'Vote',
      'Refresh pull requests',
    ]);
    expect(folds(items)).toEqual(['Copy', 'Vote']);
  });

  it('runs Review and Check out through their verbs', () => {
    const v = adoVerbs();
    const items = prItems(adoPr(), v);
    item(items, 'Review').run();
    expect(v.review).toHaveBeenCalledTimes(1);
    item(items, 'Check out in new worktree\u2026').run();
    expect(v.checkout).toHaveBeenCalledTimes(1);
  });

  it('lists the five votes in a Vote submenu', () => {
    const items = prItems(adoPr(), adoVerbs());
    expect(kids(items, 'Vote').map((r) => r.label)).toEqual(['Approve', 'Approve with suggestions', 'Wait for author', 'Reject', 'Reset']);
    expect(item(items, 'Vote').disabled).toBeFalsy();
  });

  it('casts 10, 5, -5, -10 and 0 for Approve, Approve with suggestions, Wait for author, Reject and Reset', () => {
    const v = adoVerbs();
    const votes = kids(prItems(adoPr(), v), 'Vote');
    for (const r of votes) r.run();
    expect(v.vote.mock.calls).toEqual([[10], [5], [-5], [-10], [0]]);
  });

  it('greys out Vote on a draft, saying votes open once it is published, and keeps its rows', () => {
    const items = prItems(adoPr({ standing: 'draft' }), adoVerbs());
    expect(disabledRows(items)).toEqual(['Vote']);
    expect(item(items, 'Vote').title).toBe(DRAFT_VOTE);
    expect(kids(items, 'Vote')).toHaveLength(5);
    expect(allExplained(items)).toBe(true);
  });

  it('offers no Vote on my own pull request, and none without a vote verb', () => {
    expect(labels(prItems(adoPr({ ado: { mine: true } }), adoVerbs()))).not.toContain('Vote');
    const v: PrVerbs = adoVerbs();
    delete v.vote;
    expect(labels(prItems(adoPr(), v))).not.toContain('Vote');
  });

  it('offers Publish only on my own draft', () => {
    const mineDraft = prItems(adoPr({ standing: 'draft', ado: { mine: true }, canCheckout: false }), adoVerbs());
    expect(labels(mineDraft)).toEqual([
      'Review',
      'Open in browser',
      SEP,
      'Copy link',
      'Copy number (#42)',
      'Copy branch name',
      SEP,
      'Publish',
      'Refresh pull requests',
    ]);
    expect(labels(mineDraft)).not.toContain('Vote');
    expect(labels(prItems(adoPr({ standing: 'open', ado: { mine: true } }), adoVerbs()))).not.toContain('Publish');
    expect(labels(prItems(adoPr({ standing: 'draft', ado: { mine: false } }), adoVerbs()))).not.toContain('Publish');
    const v: PrVerbs = adoVerbs();
    delete v.publish;
    expect(labels(prItems(adoPr({ standing: 'draft', ado: { mine: true } }), v))).not.toContain('Publish');
  });

  it('runs Publish through its verb', () => {
    const v = adoVerbs();
    item(prItems(adoPr({ standing: 'draft', ado: { mine: true } }), v), 'Publish').run();
    expect(v.publish).toHaveBeenCalledTimes(1);
  });

  it('offers Check out in new worktree only with no local worktree and a branch Kete can cut', () => {
    const co = 'Check out in new worktree\u2026';
    expect(labels(prItems(adoPr(), adoVerbs()))).toContain(co);
    // A local worktree is there: go to it instead.
    const local = labels(prItems(adoPr({ worktree: { id: 'w1', branch: 'feat/x' } }), adoVerbs()));
    expect(local).not.toContain(co);
    expect(local).toContain('Go to worktree feat/x');
    // Kete cannot cut one there.
    expect(labels(prItems(adoPr({ canCheckout: false }), adoVerbs()))).not.toContain(co);
    // No checkout verb to run.
    const v: PrVerbs = adoVerbs();
    delete v.checkout;
    expect(labels(prItems(adoPr(), v))).not.toContain(co);
  });

  it('offers Merge on a pull request with a local worktree, greyed on a draft', () => {
    const open = prItems(adoPr({ worktree: { id: 'w1', branch: 'feat/x' } }), adoVerbs());
    expect(item(open, 'Merge\u2026').disabled).toBeFalsy();
    const draft = prItems(adoPr({ standing: 'draft', worktree: { id: 'w1', branch: 'feat/x' } }), adoVerbs());
    expect(item(draft, 'Merge\u2026').title).toBe(DRAFT_MERGE);
    expect(labels(prItems(adoPr(), adoVerbs()))).not.toContain('Merge\u2026');
  });
});

describe('localMenu', () => {
  const row = (id: string, section: Row['section'], label: string, group?: Row['group']): Row => ({
    id,
    section,
    item: { label, run: vi.fn() },
    ...(group ? { group } : {}),
  });
  const copy = (label: string, short: string) => row(`copy.${short}`, 'copy', label, { id: 'copy', title: 'Copy', label: short });

  it('keeps the rows flat under eight, with a rule where the section changes', () => {
    const items = localMenu([row('a', 'act', 'A'), row('b', 'go', 'B'), copy('Copy x', 'X'), copy('Copy y', 'Y'), row('c', 'change', 'C'), row('d', 'change', 'D'), row('e', 'away', 'E')]);
    expect(labels(items)).toEqual(['A', SEP, 'B', SEP, 'Copy x', 'Copy y', SEP, 'C', 'D', SEP, 'E']);
    expect(folds(items)).toEqual([]);
  });

  it('folds a group of two or more rows into one submenu row from eight rows up', () => {
    const items = localMenu([
      row('a', 'act', 'A'),
      row('b', 'go', 'B'),
      copy('Copy x', 'X'),
      copy('Copy y', 'Y'),
      row('c', 'change', 'C'),
      row('d', 'change', 'D'),
      row('e', 'change', 'E'),
      row('f', 'away', 'F'),
    ]);
    expect(labels(items)).toEqual(['A', SEP, 'B', SEP, 'Copy', SEP, 'C', 'D', 'E', SEP, 'F']);
    expect(kids(items, 'Copy').map((r) => r.label)).toEqual(['X', 'Y']);
    expect(kids(items, 'Copy').map((r) => r.full)).toEqual(['Copy x', 'Copy y']);
  });

  it('does not fold a group with one row left, even at eight rows', () => {
    const items = localMenu(
      [row('a', 'act', 'A'), row('b', 'go', 'B'), copy('Copy x', 'X'), row('c', 'change', 'C'), row('d', 'change', 'D'), row('e', 'change', 'E'), row('f', 'change', 'F'), row('g', 'away', 'G')],
    );
    expect(labels(items)).toContain('Copy x');
    expect(folds(items)).toEqual([]);
  });

  it('puts the head entries ahead of the rows', () => {
    const items = localMenu([row('a', 'act', 'A')], [{ heading: 'needs you' }]);
    expect(labels(items)).toEqual(['needs you', 'A']);
  });

  it('folds a GitHub pull request at eight rows and not at seven', () => {
    const seven = prItems(pr(), ghVerbs());
    expect(rows(seven)).toHaveLength(7);
    expect(folds(seven)).toEqual([]);
    const eight = prItems(pr({ sessions: sessions(1) }), ghVerbs());
    // Eight rows, the three Copy rows folded into one.
    expect(rows(eight)).toHaveLength(6);
    expect(folds(eight)).toEqual(['Copy']);
  });

  it('folds the three Copy rows of an eight-row pull request into Copy: Link, Number (#42), Branch name', () => {
    const v = ghVerbs();
    const items = prItems(pr({ sessions: sessions(1) }), v);
    expect(labels(items)).toEqual([
      'Open in browser',
      SEP,
      'Go to session work 1',
      'Go to worktree feat/x',
      SEP,
      'Copy',
      SEP,
      'Merge\u2026',
      'Refresh pull requests',
    ]);
    const copies = kids(items, 'Copy');
    expect(copies.map((r) => r.label)).toEqual(['Link', 'Number (#42)', 'Branch name']);
    expect(copies.map((r) => r.full)).toEqual(['Copy link', 'Copy number (#42)', 'Copy branch name']);
    for (const r of copies) r.run();
    expect(v.copy.mock.calls).toEqual([
      [URL_GH, URL_GH],
      ['#42', '#42'],
      ['feat/x', 'feat/x'],
    ]);
  });
});

// ---- branches ----

const URL_BRANCH = 'https://dev.azure.com/o/p/_git/r?version=GBja%2Ffix';

/** A local branch checked out nowhere, with nothing linked. */
const branch = (o: Partial<BranchState> = {}): BranchState => ({
  name: 'ja/fix',
  local: true,
  worktree: null,
  session: null,
  pr: null,
  task: null,
  canDiff: true,
  url: URL_BRANCH,
  ...o,
});
const branchVerbs = () => ({
  goWorktree: vi.fn(),
  checkout: vi.fn(),
  goSession: vi.fn(),
  goPr: vi.fn(),
  draftPr: vi.fn(),
  goTask: vi.fn(),
  diff: vi.fn(),
  copy: vi.fn(),
  open: vi.fn(),
  deleteLocal: vi.fn(),
});

describe('branchItems', () => {
  it('starts with Check out in new worktree when the branch is checked out nowhere', () => {
    expect(labels(branchItems(branch(), branchVerbs()))).toEqual([
      'Check out in new worktree\u2026',
      SEP,
      'Draft PR\u2026',
      'Diff vs main',
      SEP,
      'Open in Azure DevOps',
      SEP,
      'Copy name',
      SEP,
      'Delete local\u2026',
    ]);
  });

  it('starts with Go to worktree when it is checked out, and offers no Check out', () => {
    const items = branchItems(branch({ worktree: { id: 'w1', branch: 'ja/fix' } }), branchVerbs());
    expect(labels(items)[0]).toBe('Go to worktree ja/fix');
    expect(labels(items)).not.toContain('Check out in new worktree\u2026');
  });

  it('lists the session, pull request and task it is linked to, in order, and no Draft PR', () => {
    const items = branchItems(
      branch({
        worktree: { id: 'w1', branch: 'ja/fix' },
        session: { id: 's1', name: 'api work' },
        pr: { id: 7 },
        task: 12,
      }),
      branchVerbs(),
    );
    expect(labels(items)).toEqual([
      'Go to worktree ja/fix',
      SEP,
      'Go to session api work',
      'Go to pull request #7',
      'Go to task #12',
      'Diff vs main',
      SEP,
      'Open in Azure DevOps',
      SEP,
      'Copy name',
      SEP,
      'Delete local\u2026',
    ]);
    expect(folds(items)).toEqual([]);
  });

  it('offers Draft PR only for a local branch with no pull request', () => {
    expect(labels(branchItems(branch(), branchVerbs()))).toContain('Draft PR\u2026');
    expect(labels(branchItems(branch({ pr: { id: 7 } }), branchVerbs()))).not.toContain('Draft PR\u2026');
    expect(labels(branchItems(branch({ local: false }), branchVerbs()))).not.toContain('Draft PR\u2026');
  });

  it('offers Delete local only for a local branch', () => {
    expect(labels(branchItems(branch({ local: false }), branchVerbs()))).not.toContain('Delete local\u2026');
  });

  it('marks Delete local as danger, enabled when the branch is checked out nowhere', () => {
    const del = item(branchItems(branch(), branchVerbs()), 'Delete local\u2026');
    expect(del.danger).toBe(true);
    expect(del.disabled).toBeFalsy();
  });

  it('greys out Delete local while the branch is checked out, saying to remove the worktree first', () => {
    const items = branchItems(branch({ worktree: { id: 'w1', branch: 'ja/fix' } }), branchVerbs());
    const del = item(items, 'Delete local\u2026');
    expect(del.disabled).toBe(true);
    expect(del.title).toBe(CHECKED_OUT);
    expect(del.danger).toBe(true);
    expect(disabledRows(items)).toEqual(['Delete local\u2026']);
  });

  it('leaves out Diff vs main when there is nothing to diff against, and Open in Azure DevOps without a url', () => {
    const labelsOf = labels(branchItems(branch({ canDiff: false, url: null }), branchVerbs()));
    expect(labelsOf).not.toContain('Diff vs main');
    expect(labelsOf).not.toContain('Open in Azure DevOps');
    expect(labelsOf).toContain('Copy name');
  });

  it('titles Open in Azure DevOps with the page it opens', () => {
    expect(item(branchItems(branch(), branchVerbs()), 'Open in Azure DevOps').title).toBe(URL_BRANCH);
  });

  it('runs each row through its verb', () => {
    const v = branchVerbs();
    const full = branch({ worktree: { id: 'w1', branch: 'ja/fix' }, session: { id: 's1', name: 'api work' }, pr: { id: 7 }, task: 12 });
    const items = branchItems(full, v);
    item(items, 'Go to worktree ja/fix').run();
    expect(v.goWorktree).toHaveBeenCalledWith('w1');
    item(items, 'Go to session api work').run();
    expect(v.goSession).toHaveBeenCalledWith('s1');
    item(items, 'Go to pull request #7').run();
    expect(v.goPr).toHaveBeenCalledWith(7);
    item(items, 'Go to task #12').run();
    expect(v.goTask).toHaveBeenCalledWith(12);
    item(items, 'Diff vs main').run();
    expect(v.diff).toHaveBeenCalledTimes(1);
    item(items, 'Open in Azure DevOps').run();
    expect(v.open).toHaveBeenCalledWith(URL_BRANCH);
    item(items, 'Copy name').run();
    expect(v.copy).toHaveBeenCalledWith('ja/fix', 'ja/fix');
    expect(v.checkout).not.toHaveBeenCalled();
    expect(v.draftPr).not.toHaveBeenCalled();
    expect(v.deleteLocal).not.toHaveBeenCalled();
  });

  it('runs Check out, Draft PR and Delete local through their verbs', () => {
    const v = branchVerbs();
    const items = branchItems(branch(), v);
    item(items, 'Check out in new worktree\u2026').run();
    expect(v.checkout).toHaveBeenCalledTimes(1);
    item(items, 'Draft PR\u2026').run();
    expect(v.draftPr).toHaveBeenCalledTimes(1);
    item(items, 'Delete local\u2026').run();
    expect(v.deleteLocal).toHaveBeenCalledTimes(1);
  });
});

// ---- tasks ----

const URL_TASK = 'https://dev.azure.com/o/p/_workitems/edit/1234';

/** An open task with nothing linked. */
const task = (o: Partial<TaskState> = {}): TaskState => ({
  id: 1234,
  title: 'Fix GST rounding',
  url: URL_TASK,
  closed: false,
  session: null,
  branch: null,
  pr: null,
  ...o,
});
const taskVerbs = () => ({
  start: vi.fn(),
  goSession: vi.fn(),
  goBranch: vi.fn(),
  goPr: vi.fn(),
  draftPr: vi.fn(),
  open: vi.fn(),
  copy: vi.fn(),
});

describe('taskItems', () => {
  it('lists Start session, Open in Azure DevOps and the three Copy rows for a fresh task', () => {
    expect(labels(taskItems(task(), taskVerbs()))).toEqual([
      'Start session\u2026',
      SEP,
      'Open in Azure DevOps',
      SEP,
      'Copy id (#1234)',
      'Copy link',
      'Copy title',
    ]);
  });

  it('offers Start session only for a task that is not closed and has no session', () => {
    expect(labels(taskItems(task({ closed: true }), taskVerbs()))).toEqual([
      'Open in Azure DevOps',
      SEP,
      'Copy id (#1234)',
      'Copy link',
      'Copy title',
    ]);
    expect(labels(taskItems(task(), taskVerbs()))).toContain('Start session\u2026');
  });

  it('shows Go to session in the place of Start session when a session is linked', () => {
    const items = taskItems(task({ session: { id: 's1', name: 'api work' } }), taskVerbs());
    expect(labels(items)[0]).toBe('Go to session api work');
    expect(labels(items)).not.toContain('Start session\u2026');
  });

  it('keeps Go to session on a closed task that has one, and no Start session', () => {
    const items = taskItems(task({ closed: true, session: { id: 's1', name: 'api work' } }), taskVerbs());
    expect(labels(items)[0]).toBe('Go to session api work');
    expect(labels(items)).not.toContain('Start session\u2026');
  });

  it('offers Draft PR for a branch with no pull request, after Go to branch', () => {
    expect(labels(taskItems(task({ branch: 'ja/fix' }), taskVerbs()))).toEqual([
      'Start session\u2026',
      SEP,
      'Go to branch ja/fix',
      'Draft PR\u2026',
      SEP,
      'Open in Azure DevOps',
      SEP,
      'Copy id (#1234)',
      'Copy link',
      'Copy title',
    ]);
  });

  it('offers Go to pull request in the place of Draft PR once the branch has one', () => {
    const l = labels(taskItems(task({ branch: 'ja/fix', pr: 5 }), taskVerbs()));
    expect(l).toContain('Go to branch ja/fix');
    expect(l).toContain('Go to pull request #5');
    expect(l).not.toContain('Draft PR\u2026');
  });

  it('offers no Draft PR without a branch', () => {
    expect(labels(taskItems(task(), taskVerbs()))).not.toContain('Draft PR\u2026');
  });

  it('leaves out the page and the link without a url', () => {
    expect(labels(taskItems(task({ url: null }), taskVerbs()))).toEqual(['Start session\u2026', SEP, 'Copy id (#1234)', 'Copy title']);
  });

  it('never folds: even the fullest task has seven rows, the Copy rows flat', () => {
    const items = taskItems(task({ session: { id: 's1', name: 'api work' }, branch: 'ja/fix', pr: 5 }), taskVerbs());
    expect(rows(items)).toHaveLength(7);
    expect(folds(items)).toEqual([]);
    expect(labels(items).slice(-3)).toEqual(['Copy id (#1234)', 'Copy link', 'Copy title']);
  });

  it('titles Open in Azure DevOps with the page it opens', () => {
    expect(item(taskItems(task(), taskVerbs()), 'Open in Azure DevOps').title).toBe(URL_TASK);
  });

  it('runs each row through its verb', () => {
    const v = taskVerbs();
    const items = taskItems(task({ branch: 'ja/fix' }), v);
    item(items, 'Start session\u2026').run();
    expect(v.start).toHaveBeenCalledTimes(1);
    item(items, 'Go to branch ja/fix').run();
    expect(v.goBranch).toHaveBeenCalledWith('ja/fix');
    item(items, 'Draft PR\u2026').run();
    expect(v.draftPr).toHaveBeenCalledTimes(1);
    item(items, 'Open in Azure DevOps').run();
    expect(v.open).toHaveBeenCalledWith(URL_TASK);
    item(items, 'Copy id (#1234)').run();
    expect(v.copy).toHaveBeenLastCalledWith('#1234', '#1234');
    item(items, 'Copy link').run();
    expect(v.copy).toHaveBeenLastCalledWith(URL_TASK, URL_TASK);
    item(items, 'Copy title').run();
    expect(v.copy).toHaveBeenLastCalledWith('Fix GST rounding', 'the title');
    expect(v.copy).toHaveBeenCalledTimes(3);
  });

  it('runs Go to session and Go to pull request through their verbs', () => {
    const v = taskVerbs();
    const items = taskItems(task({ session: { id: 's1', name: 'api work' }, branch: 'ja/fix', pr: 5 }), v);
    item(items, 'Go to session api work').run();
    expect(v.goSession).toHaveBeenCalledWith('s1');
    item(items, 'Go to pull request #5').run();
    expect(v.goPr).toHaveBeenCalledWith(5);
  });
});

// ---- ports ----

const port = (o: Partial<PortState> = {}): PortState => ({ port: 5173, runTerminal: false, running: null, ...o });
const portVerbs = () => ({ open: vi.fn(), copy: vi.fn(), showRun: vi.fn(), toggleRun: vi.fn() });

describe('portItems', () => {
  it('lists Open in browser and Copy URL for a port with no run command', () => {
    const items = portItems(port(), portVerbs());
    expect(labels(items)).toEqual(['Open localhost:5173 in browser', SEP, 'Copy URL']);
    expect(item(items, 'Open localhost:5173 in browser').title).toBe('http://localhost:5173');
  });

  it('opens and copies http://localhost:N', () => {
    const v = portVerbs();
    const items = portItems(port({ port: 3000 }), v);
    item(items, 'Open localhost:3000 in browser').run();
    expect(v.open).toHaveBeenCalledWith('http://localhost:3000');
    item(items, 'Copy URL').run();
    expect(v.copy).toHaveBeenCalledWith('http://localhost:3000', 'http://localhost:3000');
  });

  it('shows Run while the checkout has a run command that is not running', () => {
    const v = portVerbs();
    const items = portItems(port({ running: false }), v);
    expect(labels(items)).toEqual(['Open localhost:5173 in browser', SEP, 'Copy URL', SEP, 'Run']);
    item(items, 'Run').run();
    expect(v.toggleRun).toHaveBeenCalledTimes(1);
  });

  it('shows Stop run while it is running', () => {
    const v = portVerbs();
    const items = portItems(port({ running: true }), v);
    expect(labels(items)).toContain('Stop run');
    expect(labels(items)).not.toContain('Run');
    item(items, 'Stop run').run();
    expect(v.toggleRun).toHaveBeenCalledTimes(1);
  });

  it('shows neither Run nor Stop run when there is no run command', () => {
    const l = labels(portItems(port({ running: null, runTerminal: true }), portVerbs()));
    expect(l).not.toContain('Run');
    expect(l).not.toContain('Stop run');
  });

  it('shows Show run terminal only with a run terminal, after Open', () => {
    expect(labels(portItems(port(), portVerbs()))).not.toContain('Show run terminal');
    const v = portVerbs();
    const items = portItems(port({ runTerminal: true, running: true }), v);
    expect(labels(items)).toEqual(['Open localhost:5173 in browser', SEP, 'Show run terminal', SEP, 'Copy URL', SEP, 'Stop run']);
    item(items, 'Show run terminal').run();
    expect(v.showRun).toHaveBeenCalledTimes(1);
  });
});

// ---- board lanes ----

const lane = (l: LaneId, o: Partial<LaneState> = {}): LaneState => ({ lane: l, count: 2, unread: 1, done: 1, ...o });
const laneVerbs = () => ({ next: vi.fn(), markSeen: vi.fn(), refresh: vi.fn(), archiveDone: vi.fn() });

describe('laneItems', () => {
  const single: [LaneId, string, 'next' | 'markSeen' | 'refresh' | 'archiveDone'][] = [
    ['needs', 'Open next needing you', 'next'],
    ['running', 'Open next running', 'next'],
    ['unseen', 'Mark all seen', 'markSeen'],
    ['pr', 'Refresh pull requests', 'refresh'],
    ['done', 'Archive done sessions\u2026', 'archiveDone'],
  ];

  it.each(single)('the %s lane has one row, %s, that runs %s', (id, label, verb) => {
    const v = laneVerbs();
    const items = laneItems(lane(id), v);
    expect(labels(items)).toEqual([label]);
    expect(disabledRows(items)).toEqual([]);
    item(items, label).run();
    expect(v[verb]).toHaveBeenCalledTimes(1);
    for (const [name, fn] of Object.entries(v)) if (name !== verb) expect(fn).not.toHaveBeenCalled();
  });

  const empty: [LaneId, Partial<LaneState>, string, string][] = [
    ['needs', { count: 0 }, 'Open next needing you', 'Nothing needs you'],
    ['running', { count: 0 }, 'Open next running', 'Nothing is running'],
    ['unseen', { unread: 0 }, 'Mark all seen', 'Nothing unread here'],
    ['done', { done: 0 }, 'Archive done sessions\u2026', 'No finished, read sessions here'],
  ];

  it.each(empty)('the %s lane greys its row out when it is empty, saying why', (id, o, label, why) => {
    const items = laneItems(lane(id, o), laneVerbs());
    expect(disabledRows(items)).toEqual([label]);
    expect(item(items, label).title).toBe(why);
  });

  it('never greys out Refresh pull requests', () => {
    expect(disabledRows(laneItems(lane('pr', { count: 0, unread: 0, done: 0 }), laneVerbs()))).toEqual([]);
  });

  it('gives Open next needing you the keys it is handed', () => {
    expect(item(laneItems(lane('needs'), laneVerbs(), { next: ['.'] }), 'Open next needing you').keys).toEqual(['.']);
    expect(item(laneItems(lane('needs'), laneVerbs()), 'Open next needing you').keys).toBeUndefined();
  });
});

// ---- the attention strip ----

describe('attentionItems', () => {
  const list = [
    { id: 'a1', name: 'api work', where: 'Kete / feat/x' },
    { id: 'a2', name: 'ui polish', where: 'Kete / main' },
  ];

  it('lists the sessions, each titled with where it is', () => {
    const items = attentionItems({ sessions: list, unread: false }, { open: vi.fn(), markSeen: vi.fn() });
    expect(labels(items)).toEqual(['api work', 'ui polish']);
    expect(rows(items).map((r) => r.title)).toEqual(['Kete / feat/x', 'Kete / main']);
  });

  it('ends with Mark all seen behind a rule only for the unread list', () => {
    const v = { open: vi.fn(), markSeen: vi.fn() };
    const items = attentionItems({ sessions: list, unread: true }, v);
    expect(labels(items)).toEqual(['api work', 'ui polish', SEP, 'Mark all seen']);
    item(items, 'Mark all seen').run();
    expect(v.markSeen).toHaveBeenCalledTimes(1);
    expect(labels(attentionItems({ sessions: list, unread: false }, v))).not.toContain('Mark all seen');
  });

  it('is empty for an unread count with no sessions', () => {
    expect(attentionItems({ sessions: [], unread: true }, { open: vi.fn(), markSeen: vi.fn() })).toEqual([]);
  });

  it('opens the session whose row is run', () => {
    const v = { open: vi.fn(), markSeen: vi.fn() };
    item(attentionItems({ sessions: list, unread: false }, v), 'ui polish').run();
    expect(v.open).toHaveBeenCalledWith('a2');
  });
});

// ---- the orchestrator ----

describe('orchestratorItems', () => {
  const orchVerbs = () => ({ open: vi.fn(), reviewWaiting: vi.fn(), start: vi.fn(), unpin: vi.fn() });
  const session = (): MenuEntry[] => [
    { label: 'Follow-up prompt\u2026', run: vi.fn() },
    { label: 'Restart agent', run: vi.fn() },
  ];

  it('offers only Start an orchestrator when none is pinned, whatever is waiting', () => {
    const v = orchVerbs();
    const items = orchestratorItems({ name: null, waiting: 3 }, v);
    expect(labels(items)).toEqual(['Start an orchestrator\u2026']);
    item(items, 'Start an orchestrator\u2026').run();
    expect(v.start).toHaveBeenCalledTimes(1);
  });

  it('on the panel background: Open orchestrator, a rule, Start a new orchestrator', () => {
    const items = orchestratorItems({ name: 'Boss', waiting: 0 }, orchVerbs());
    expect(labels(items)).toEqual(['Open orchestrator', SEP, 'Start a new orchestrator\u2026']);
    expect(item(items, 'Open orchestrator').title).toBe('Boss');
  });

  it('on the panel background, Review all waiting appears with reviews waiting', () => {
    const v = orchVerbs();
    const items = orchestratorItems({ name: 'Boss', waiting: 2 }, v);
    expect(labels(items)).toEqual(['Open orchestrator', 'Review all waiting (2)\u2026', SEP, 'Start a new orchestrator\u2026']);
    item(items, 'Review all waiting (2)\u2026').run();
    expect(v.reviewWaiting).toHaveBeenCalledTimes(1);
  });

  it('on the rail entry: Open, Review waiting, the Session submenu, Start a new orchestrator, Unpin', () => {
    const v = orchVerbs();
    const sessionRows = session();
    const items = orchestratorItems({ name: 'Boss', waiting: 2, session: sessionRows }, v);
    expect(labels(items)).toEqual([
      'Open orchestrator',
      'Review waiting (2)\u2026',
      SEP,
      'Session Boss',
      SEP,
      'Start a new orchestrator\u2026',
      'Unpin orchestrator',
    ]);
    expect(kids(items, 'Session Boss').map((r) => r.label)).toEqual(['Follow-up prompt\u2026', 'Restart agent']);
    item(items, 'Open orchestrator').run();
    expect(v.open).toHaveBeenCalledTimes(1);
    item(items, 'Review waiting (2)\u2026').run();
    expect(v.reviewWaiting).toHaveBeenCalledTimes(1);
    item(items, 'Start a new orchestrator\u2026').run();
    expect(v.start).toHaveBeenCalledTimes(1);
    item(items, 'Unpin orchestrator').run();
    expect(v.unpin).toHaveBeenCalledTimes(1);
  });

  it('on the rail entry, leaves Review waiting out when nothing waits', () => {
    const l = labels(orchestratorItems({ name: 'Boss', waiting: 0, session: session() }, orchVerbs()));
    expect(l.some((x) => x.startsWith('Review'))).toBe(false);
    expect(l).toContain('Unpin orchestrator');
  });

  it('offers Unpin only on the rail entry, where the session rows are', () => {
    expect(labels(orchestratorItems({ name: 'Boss', waiting: 0 }, orchVerbs()))).not.toContain('Unpin orchestrator');
  });

  it('gives Open and Start the keys they are handed', () => {
    const keys = { open: ['ctrl+shift+o'] };
    expect(item(orchestratorItems({ name: 'Boss', waiting: 0 }, orchVerbs(), keys), 'Open orchestrator').keys).toEqual(['ctrl+shift+o']);
    expect(item(orchestratorItems({ name: null, waiting: 0 }, orchVerbs(), keys), 'Start an orchestrator\u2026').keys).toEqual(['ctrl+shift+o']);
  });
});

// ---- files ----

describe('fileItems', () => {
  const registry = (): Row[] => [
    { id: 'activate', section: 'act', item: { label: 'Open in Kete', run: vi.fn() } },
    { id: 'copy_path', section: 'copy', item: { label: 'Copy path', run: vi.fn() } },
  ];
  const file = (o: Partial<FileState> = {}): FileState => ({
    path: 'src/a.ts',
    deleted: false,
    untracked: false,
    stage: { unstaged: true, staged: false },
    ...o,
  });
  const fileVerbs = () => ({ stage: vi.fn(), unstage: vi.fn(), discard: vi.fn() });
  const head = ['Open in Kete', SEP, 'Copy path', SEP];

  it('adds Stage file and, behind a rule, Discard changes for an unstaged tracked change', () => {
    const items = fileItems(registry(), file(), fileVerbs());
    expect(labels(items)).toEqual([...head, 'Stage file', SEP, 'Discard changes\u2026']);
    expect(disabledRows(items)).toEqual([]);
  });

  it('marks Discard changes as danger and nothing else', () => {
    const items = fileItems(registry(), file(), fileVerbs());
    expect(rows(items).filter((r) => r.danger).map((r) => r.label)).toEqual(['Discard changes\u2026']);
  });

  it('adds Unstage file alone for a fully staged file', () => {
    expect(labels(fileItems(registry(), file({ stage: { unstaged: false, staged: true } }), fileVerbs()))).toEqual([...head, 'Unstage file']);
  });

  it('adds Stage file, Unstage file and Discard changes for a partly staged file', () => {
    expect(labels(fileItems(registry(), file({ stage: { unstaged: true, staged: true } }), fileVerbs()))).toEqual([
      ...head,
      'Stage file',
      'Unstage file',
      SEP,
      'Discard changes\u2026',
    ]);
  });

  it.each([{ unstaged: true, staged: false }, { unstaged: false, staged: false }])(
    'greys out Stage file for an untracked file (%o), saying why, and offers no Discard changes',
    (stage) => {
      const items = fileItems(registry(), file({ untracked: true, stage }), fileVerbs());
      expect(labels(items)).toEqual([...head, 'Stage file']);
      expect(disabledRows(items)).toEqual(['Stage file']);
      expect(item(items, 'Stage file').title).toMatch(/untracked/i);
    },
  );

  it('adds no staging rows where the diff cannot stage', () => {
    expect(labels(fileItems(registry(), file({ stage: null }), fileVerbs()))).toEqual(['Open in Kete', SEP, 'Copy path']);
  });

  it('adds nothing for a tracked file with no hunks either side', () => {
    expect(labels(fileItems(registry(), file({ stage: { unstaged: false, staged: false } }), fileVerbs()))).toEqual(['Open in Kete', SEP, 'Copy path']);
  });

  it('keeps the registry rows in their order and never folds them', () => {
    const group = { id: 'copy', title: 'Copy', label: 'x' };
    const reg: Row[] = [
      { id: 'copy_path', section: 'copy', group: { ...group, label: 'Path' }, item: { label: 'Copy path', run: vi.fn() } },
      { id: 'copy_relative_path', section: 'copy', group: { ...group, label: 'Relative path' }, item: { label: 'Copy relative path', run: vi.fn() } },
    ];
    const items = fileItems(reg, file({ stage: null }), fileVerbs());
    expect(labels(items)).toEqual(['Copy path', 'Copy relative path']);
    expect(folds(items)).toEqual([]);
  });

  it('runs Stage, Unstage and Discard through their verbs', () => {
    const v = fileVerbs();
    const items = fileItems(registry(), file({ stage: { unstaged: true, staged: true } }), v);
    item(items, 'Stage file').run();
    expect(v.stage).toHaveBeenCalledTimes(1);
    expect(v.unstage).not.toHaveBeenCalled();
    item(items, 'Unstage file').run();
    expect(v.unstage).toHaveBeenCalledTimes(1);
    item(items, 'Discard changes\u2026').run();
    expect(v.discard).toHaveBeenCalledTimes(1);
    expect(v.stage).toHaveBeenCalledTimes(1);
  });
});

// ---- the breadcrumb's views ----

describe('viewItems', () => {
  it('lists the views as radio rows with the current one ticked', () => {
    const items = viewItems([
      { label: 'Overview', on: true, run: vi.fn() },
      { label: 'Board', on: false, run: vi.fn() },
    ]);
    expect(labels(items)).toEqual(['Overview', 'Board']);
    expect(rows(items).map((r) => r.checked)).toEqual([true, false]);
    expect(rows(items).every((r) => r.radio === true)).toBe(true);
  });

  it('carries keys only for the views that have them', () => {
    const items = viewItems([
      { label: 'Overview', on: true, run: vi.fn(), keys: ['g o'] },
      { label: 'Board', on: false, run: vi.fn() },
    ]);
    expect(item(items, 'Overview').keys).toEqual(['g o']);
    expect('keys' in item(items, 'Board')).toBe(false);
  });

  it('runs the view that is picked', () => {
    const go = vi.fn();
    const stay = vi.fn();
    item(
      viewItems([
        { label: 'Overview', on: true, run: stay },
        { label: 'Board', on: false, run: go },
      ]),
      'Board',
    ).run();
    expect(go).toHaveBeenCalledTimes(1);
    expect(stay).not.toHaveBeenCalled();
  });

  it('is empty for no views', () => {
    expect(viewItems([])).toEqual([]);
  });
});
