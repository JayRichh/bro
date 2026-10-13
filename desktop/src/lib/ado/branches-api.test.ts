import { expect, test } from 'vitest';
import { filterBranches, personOf, shortAge, syncWords, type BranchRow, type BranchTree } from './branches-api';

function row(name: string, over: Partial<BranchRow> = {}): BranchRow {
  return {
    name,
    person: personOf(name),
    local: true,
    remote: true,
    ahead: 0,
    behind: 0,
    gone: false,
    last_commit_age: 3600,
    last_author: 'Demo Dev',
    task: null,
    pr: null,
    worktree: null,
    session: null,
    ...over,
  };
}

const tree: BranchTree = {
  me: 'jari',
  people: [
    ['jari', [row('jari/gst-rounding'), row('jari/ledger-export')]],
    ['ga', [row('ga/flaky-login'), row('ga/gst-docs')]],
    ['other', [row('main')]],
  ],
};

const names = (t: BranchTree) => t.people.flatMap(([, rows]) => rows.map((r) => r.name));

test('ga/ narrows to person ga', () => {
  const t = filterBranches(tree, 'ga/');
  expect(t.people.map(([p]) => p)).toEqual(['ga']);
  expect(names(t)).toEqual(['ga/flaky-login', 'ga/gst-docs']);
});

test('a fuzzy gst matches jari/gst-rounding', () => {
  const t = filterBranches(tree, 'gst');
  expect(names(t)).toContain('jari/gst-rounding');
  expect(names(t)).not.toContain('jari/ledger-export');
  expect(names(filterBranches(tree, 'jgstrnd'))).toEqual(['jari/gst-rounding']);
});

test('person plus a fuzzy rest, and empty queries', () => {
  expect(names(filterBranches(tree, 'ga/gst'))).toEqual(['ga/gst-docs']);
  expect(names(filterBranches(tree, 'GA/'))).toEqual(['ga/flaky-login', 'ga/gst-docs']);
  expect(filterBranches(tree, '   ')).toBe(tree);
  expect(filterBranches(tree, 'zz/').people).toEqual([]);
  expect(filterBranches(tree, 'gst').me).toBe('jari');
});

test('sync words', () => {
  expect(syncWords(row('a', { ahead: 2, behind: 1 }))).toBe('\u21912 \u21931');
  expect(syncWords(row('a', { ahead: 2 }))).toBe('\u21912');
  expect(syncWords(row('a', { remote: false }))).toBe('local only');
  expect(syncWords(row('a', { local: false }))).toBe('remote only');
  expect(syncWords(row('a', { gone: true, remote: false }))).toBe('gone');
  expect(syncWords(row('a'))).toBe('in sync');
});

test('short ages', () => {
  expect(shortAge(30)).toBe('30 s');
  expect(shortAge(600)).toBe('10 min');
  expect(shortAge(3 * 3600)).toBe('3 h');
  expect(shortAge(5 * 86400)).toBe('5 d');
  expect(shortAge(30 * 86400)).toBe('4 wk');
});
