import { expect, test } from 'vitest';
import {
  branchProblem,
  ownsKey,
  filterTasks,
  scopeFromKey,
  scopeKey,
  scopeLabel,
  slugify,
  staleNotice,
  suggestBranch,
  type Iteration,
  type TaskGroup,
  type WorkItem,
} from './tasks-api';

const sprint = (name: string, finish: string | null): Iteration => ({
  id: name.toLowerCase().replace(' ', '-'),
  name,
  path: `Platform\\${name}`,
  start: null,
  finish,
  timeframe: null,
});

test('scopeLabel names each scope', () => {
  expect(scopeLabel('currentSprint')).toBe('Current sprint');
  expect(scopeLabel('myOpen')).toBe('My open');
  expect(scopeLabel('sprintAll')).toBe('Sprint, everyone');
  expect(scopeLabel({ iteration: 'sprint-41' }, [sprint('Sprint 41', null)])).toBe('Sprint 41');
  expect(scopeLabel({ iteration: 'gone' }, [])).toBe('Chosen iteration');
});

test('scope keys round-trip', () => {
  for (const s of ['currentSprint', 'myOpen', 'sprintAll', { iteration: 'abc' }] as const)
    expect(scopeFromKey(scopeKey(s))).toEqual(s);
  expect(scopeFromKey(undefined)).toBe('currentSprint');
  expect(scopeFromKey('iteration:')).toBe('currentSprint');
});

test('staleNotice says how long ago the sprint ended', () => {
  const now = Date.parse('2026-10-14T09:30:00Z');
  const list = { stale: true, iteration: sprint('Sprint 42', '2026-10-11T00:00:00Z') };
  expect(staleNotice(list, now)).toBe('Sprint 42 ended 3 days ago');
  expect(staleNotice(list, now)).not.toContain('\u2014');
  expect(staleNotice({ ...list, iteration: sprint('Sprint 42', '2026-10-13T00:00:00Z') }, now)).toBe(
    'Sprint 42 ended 1 day ago',
  );
  expect(staleNotice({ ...list, iteration: sprint('Sprint 42', null) }, now)).toBe('Sprint 42 has ended');
  expect(staleNotice({ ...list, stale: false }, now)).toBeNull();
  expect(staleNotice({ stale: true, iteration: null }, now)).toBeNull();
});

test('slugify follows the backend rules', () => {
  expect(slugify('GST rounding on invoices')).toBe('gst-rounding-on-invoices');
  expect(slugify('  Caf\u00e9: d\u00e9j\u00e0 vu!  ')).toBe('cafe-deja-vu');
  const long = slugify('Refund webhook retries back off exponentially when the gateway is down');
  expect(long.length).toBeLessThanOrEqual(40);
  expect(long.endsWith('-')).toBe(false);
  expect(long).toBe('refund-webhook-retries-back-off');
  expect(suggestBranch('jari', 'GST rounding')).toBe('jari/gst-rounding');
  expect(suggestBranch('', 'GST rounding')).toBe('gst-rounding');
});

test('branchProblem applies check-ref-format and clashes', () => {
  expect(branchProblem('jari/gst-rounding')).toBeNull();
  expect(branchProblem('jari/gst-rounding', ['jari/gst-rounding'])).toMatch(/already exists/);
  expect(branchProblem('')).toBe('Name the branch.');
  expect(branchProblem('jari/gst rounding')).toMatch(/No spaces/);
  expect(branchProblem('jari/../x')).toMatch(/\.\./);
  expect(branchProblem('jari/x.lock')).toMatch(/\.lock/);
  expect(branchProblem('jari/')).toMatch(/slashes/);
  expect(branchProblem('-x')).toMatch(/start with/);
});

const item = (id: number, title: string, parent: number | null = null): WorkItem => ({
  id,
  kind: 'Task',
  title,
  state: 'New',
  stateCategory: 'Proposed',
  assignedTo: null,
  remaining: null,
  parent,
  boardColumn: null,
  descriptionMd: '',
  acceptanceMd: '',
  url: '',
});

/** A stand-in element that matches the selectors its tags appear in. */
const el = (...tags: string[]) => ({
  closest: (sel: string) => (sel.split(',').some((s) => tags.includes(s.trim())) ? {} : null),
});

test('ownsKey leaves Enter to a focused control and every key to a text field', () => {
  expect(ownsKey(el('button'), 'Enter')).toBe(true);
  expect(ownsKey(el('button'), ' ')).toBe(true);
  expect(ownsKey(el('a'), 'Enter')).toBe(true);
  expect(ownsKey(el('[role="radio"]'), 'Enter')).toBe(true);
  expect(ownsKey(el('[contenteditable]'), 's')).toBe(true);
  expect(ownsKey(el('input'), 'j')).toBe(true);
  // Letter shortcuts still work on a focused row button, and Enter on the page.
  expect(ownsKey(el('button'), 's')).toBe(false);
  expect(ownsKey(el('body'), 'Enter')).toBe(false);
  expect(ownsKey(null, 'Enter')).toBe(false);
});

test('filterTasks matches id and fuzzy title', () => {
  const groups: TaskGroup[] = [
    { parent: item(300, 'Invoices'), items: [item(101, 'GST rounding', 300), item(102, 'Ledger export', 300)] },
    { parent: null, items: [item(103, 'Flaky login test')] },
  ];
  expect(filterTasks(groups, 'gst')[0].items.map((i) => i.id)).toEqual([101]);
  expect(filterTasks(groups, '103')[0].items.map((i) => i.id)).toEqual([103]);
  expect(filterTasks(groups, 'invoices')[0].items).toHaveLength(2);
  expect(filterTasks(groups, 'zzz')).toEqual([]);
  expect(filterTasks(groups, '  ')).toBe(groups);
});
