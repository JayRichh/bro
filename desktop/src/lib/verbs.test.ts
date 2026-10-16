import { describe, expect, it } from 'vitest';
import { barVerbs, filterRows, groupOf, menuIds, menuLabel, sectionOf, topVerbs, verbKeys, wordOf } from './verbs';
import { MENU_SECTION_ORDER, MENUS, type AppliesTo } from './actions.gen';
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
    expect(menuIds('terminal')).toEqual(['activate', 'copy_path', 'rename', 'delete']);
    expect(menuIds('account')).toEqual(['scope_account', 'accounts', 'wake_account', 'wake_schedule']);
    expect(menuIds('folder')).toEqual(['open_in_vscode', 'reveal_in_explorer', 'copy_path', 'copy_relative_path']);
    expect(menuIds('file')[0]).toBe('open_file_in_kete');
    // A row alone says the whole thing; in its submenu, the short word.
    expect(menuLabel('open_in_vscode', 'worktree')).toBe('Open in VS Code');
    expect(menuLabel('mark_seen', 'repo')).toBe('Mark all seen');
  });

  it('reads the sections and submenus the way the terminal app does', () => {
    expect(sectionOf('activate', 'session')).toBe('act');
    expect(sectionOf('delete', 'session')).toBe('away');
    // The kind's own section first.
    expect(sectionOf('wake_schedule', 'account')).toBe('change');
    expect(sectionOf('wake_schedule', 'global')).toBe('go');
    const vs = groupOf('open_in_vscode', 'worktree');
    expect([vs?.group.title, vs?.label]).toEqual(['Open in', 'VS Code']);
    // A file's menu never folds.
    expect(groupOf('open_in_vscode', 'file')).toBeUndefined();
    expect(groupOf('open_repo', 'repo')?.group.id).toBe('remote');
    expect(groupOf('open_repo', 'worktree')).toBeUndefined();
    // Every menu runs through the sections in order.
    const order = MENU_SECTION_ORDER;
    for (const [kind, ids] of Object.entries(MENUS)) {
      const at = (ids ?? []).map((id) => order.indexOf(sectionOf(id, kind as AppliesTo)!));
      expect(at.every((s, i) => s >= 0 && (i === 0 || s >= at[i - 1])), kind).toBe(true);
    }
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
