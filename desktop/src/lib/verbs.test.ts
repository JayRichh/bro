import { describe, expect, it } from 'vitest';
import { barVerbs, filterRows, menuIds, menuLabel, topVerbs, verbKeys, wordOf } from './verbs';
import type { Shortcut } from './shortcuts';
import { fuzzyMatch } from './fuzzy';

describe('the registry words', () => {
  it('one word per verb in both apps, the kind own first', () => {
    expect(menuLabel('delete', 'terminal')).toBe('Close terminal\u2026');
    expect(menuLabel('delete', 'session')).toBe('Delete\u2026');
    expect(menuLabel('activate', 'session')).toBe('Open');
    expect(menuLabel('open_worktree', 'worktree')).toBe('Open in editor');
    expect(menuLabel('remove_project', 'repo')).toBe('Remove from Kete\u2026');
    // A verb with no menu words goes by its label.
    expect(menuLabel('subagents', 'session')).toBe('Subagents and session tree');
    expect(wordOf('follow_up')).toBe('reply');
  });

  it('lists each kind menu in the registry order, the destructive rows last', () => {
    const session = menuIds('session');
    expect(session[0]).toBe('activate');
    expect(session.slice(-1)).toEqual(['delete']);
    expect(menuIds('terminal')).toEqual(['activate', 'rename', 'delete']);
    expect(menuIds('account')).toEqual([]);
  });
});

describe('the verb bar', () => {
  const table: Shortcut[] = [
    { id: 'diff', keys: ['d'], scope: 'global', label: 'Diff', group: 'Views' },
    { id: 'open_diff', keys: ['ctrl+shift+d'], scope: 'anywhere', label: 'Diff', group: 'Views' },
    { id: 'follow_up', keys: ['space'], scope: 'row', label: 'Follow-up', group: 'Sessions' },
    { id: 'archive', keys: ['a'], scope: 'row', label: 'Archive', group: 'Sessions' },
    { id: 'delete', keys: ['delete'], scope: 'row', label: 'Delete', group: 'Sessions' },
  ];

  it('leads with the first verbs of the kind the row can run, four of them', () => {
    expect(topVerbs('session', () => true)).toEqual(['activate', 'follow_up', 'git_diff', 'archive']);
    // An archived session: no open, reply or archive.
    const archived = new Set(['unarchive', 'duplicate_session', 'delete']);
    expect(topVerbs('session', (id) => archived.has(id))).toEqual(['unarchive', 'delete']);
    expect(topVerbs('repo', () => true, 2)).toEqual(['quick_prompt', 'new_worktree']);
  });

  it('names each verb word and its shortest key from the live table', () => {
    const bar = barVerbs('session', (id) => id !== 'activate', table);
    expect(bar.map((v) => [v.word, v.key])).toEqual([
      ['reply', 'space'],
      ['diff', 'd'],
      ['archive', 'a'],
      ['unarchive', ''],
    ]);
    expect(verbKeys('git_diff', table)).toEqual(['d', 'ctrl+shift+d']);
    expect(barVerbs('session', (id) => id === 'delete', table)[0].danger).toBe(true);
  });
});

describe('a searchable menu', () => {
  const rows = [{ label: 'Open' }, { label: 'Archive' }, { label: 'Rename\u2026' }, { label: 'All keys\u2026', pinned: true }];
  const score = (q: string, s: string) => fuzzyMatch(q, s)?.score ?? null;

  it('narrows to the rows that match, the pinned row kept last', () => {
    expect(filterRows(rows, '', score).map((r) => r.label)).toEqual(['Open', 'Archive', 'Rename\u2026', 'All keys\u2026']);
    expect(filterRows(rows, 'arc', score).map((r) => r.label)).toEqual(['Archive', 'All keys\u2026']);
    expect(filterRows(rows, 'zzz', score).map((r) => r.label)).toEqual(['All keys\u2026']);
  });
});
