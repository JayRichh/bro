import { describe, expect, it } from 'vitest';
import { SHORTCUTS, chordOf, helpGroups, inScope, keyLabel, shortcutFor, type KeyContext, type Scope } from './shortcuts';

const key = (k: string, m: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({
  key: k,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...m,
});
const idle: KeyContext = { inTerminal: false, typing: false, onRow: false };
const row: KeyContext = { ...idle, onRow: true };
const field: KeyContext = { inTerminal: false, typing: true, onRow: false };
const term: KeyContext = { inTerminal: true, typing: true, onRow: false };

describe('the shortcut table', () => {
  it('never binds one chord twice where the scopes overlap', () => {
    const contexts = [idle, row, field, term];
    const clashes: string[] = [];
    for (const ctx of contexts) {
      const seen = new Map<string, string>();
      for (const s of SHORTCUTS) {
        if (!inScope(s.scope, ctx)) continue;
        for (const k of s.keys) {
          const other = seen.get(k);
          if (other && other !== s.id) clashes.push(`${k}: ${other} and ${s.id}`);
          seen.set(k, s.id);
        }
      }
    }
    expect(clashes).toEqual([]);
  });

  it('never lets a window key take a home view walking key (Board d, j/k, h/l, Enter)', () => {
    const home = SHORTCUTS.filter((s) => s.scope === 'overview' || s.scope === 'board').flatMap((s) => s.keys);
    const window = SHORTCUTS.filter((s) => s.scope !== 'overview' && s.scope !== 'board').flatMap((s) => s.keys);
    expect(home).toContain('d');
    expect(window.filter((k) => home.includes(k))).toEqual([]);
    expect(shortcutFor(key('d'), row)).toBeUndefined();
  });

  it('has unique ids and every action a key', () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const s of SHORTCUTS) expect(s.keys.length, s.id).toBeGreaterThan(0);
  });

  it('keeps Board d as diff and the terminal app row letters', () => {
    const keys = (id: string) => SHORTCUTS.find((s) => s.id === id)!.keys;
    expect(SHORTCUTS.filter((s) => s.keys.includes('d')).map((s) => s.id)).toEqual(['board_diff']);
    expect(keys('rename')).toEqual(['r']);
    expect(keys('archive')).toEqual(['a']);
    expect(keys('unarchive')).toEqual(['u']);
    expect(keys('move')).toEqual(['m']);
    expect(keys('duplicate')).toEqual(['shift+p']);
  });
});

describe('chordOf and shortcutFor', () => {
  it('spells modifiers and keys as the table does', () => {
    expect(chordOf(key('B', { ctrlKey: true, shiftKey: true }))).toBe('ctrl+shift+b');
    expect(chordOf(key('K', { shiftKey: true }))).toBe('shift+k');
    expect(chordOf(key('?', { shiftKey: true }))).toBe('?');
    expect(chordOf(key(',', { ctrlKey: true }))).toBe('ctrl+,');
    expect(chordOf(key('F1'))).toBe('f1');
    expect(chordOf(key('ArrowLeft', { altKey: true }))).toBe('alt+arrowleft');
    expect(chordOf(key(' '))).toBe('space');
  });

  it('answers by scope: row keys on a row, Ctrl+K never inside a terminal', () => {
    expect(shortcutFor(key('r'), row)?.id).toBe('rename');
    expect(shortcutFor(key('r'), idle)).toBeUndefined();
    expect(shortcutFor(key('p'), field)).toBeUndefined();
    expect(shortcutFor(key('k', { ctrlKey: true }), field)?.id).toBe('palette');
    expect(shortcutFor(key('k', { ctrlKey: true }), term)).toBeUndefined();
    expect(shortcutFor(key('P', { ctrlKey: true, shiftKey: true }), term)?.id).toBe('palette_alt');
    expect(shortcutFor(key('F1'), term)?.id).toBe('help_f1');
    expect(shortcutFor(key('?', { shiftKey: true }), field)).toBeUndefined();
  });

  it('labels chords for people', () => {
    expect(keyLabel('ctrl+shift+b')).toBe('Ctrl+Shift+B');
    expect(keyLabel('alt+arrowleft')).toBe('Alt+\u2190');
    expect(keyLabel('?')).toBe('?');
    expect(keyLabel('delete')).toBe('Del');
  });

  it('groups the help in table order', () => {
    const groups = helpGroups();
    expect(groups.map((g) => g.group)).toEqual(['General', 'Views', 'Sessions', 'Overview and board']);
    const keys = (n: number, g: { rows: { keys: string[] }[] }) => n + g.rows.reduce((m, r) => m + r.keys.length, 0);
    expect(groups.reduce(keys, 0)).toBe(SHORTCUTS.reduce((n, s) => n + s.keys.length, 0));
    const palette = groups[0].rows.find((r) => r.label === 'Command palette')!;
    expect(palette.keys).toEqual(['ctrl+k', 'ctrl+shift+p']);
    expect(palette.scope).toBeNull();
  });

  it('every scope is reachable', () => {
    const scopes = new Set<Scope>(SHORTCUTS.map((s) => s.scope));
    expect([...scopes].sort()).toEqual(['anywhere', 'board', 'fields', 'global', 'overview', 'row']);
  });
});

describe('inside the diff editor', () => {
  const editor: KeyContext = { inTerminal: false, inEditor: true, typing: true, onRow: false };
  const key = (k: string, mods: Partial<KeyboardEvent> = {}) =>
    shortcutFor({ key: k, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods }, editor)?.id;
  it('leaves Ctrl+K to Monaco and keeps Ctrl+Shift+P, Back and Forward', () => {
    expect(key('k', { ctrlKey: true })).toBeUndefined();
    expect(key('P', { ctrlKey: true, shiftKey: true })).toBe('palette_alt');
    expect(key('ArrowLeft', { altKey: true })).toBe('back');
    expect(key('ArrowRight', { altKey: true })).toBe('forward');
    expect(shortcutFor({ key: 'k', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false }, field)?.id).toBe('palette');
  });
});
