import { afterEach, describe, expect, it } from 'vitest';
import { ACTIONS, DESK_KEYS } from './actions.gen';
import { SHORTCUTS, actionOf, applyOverrides, clashes, defaultKeys, isFixed, isModifierKey, keysOf, overlaps, shortcutFor, withKey } from './shortcuts';

const idle = { inTerminal: false, typing: false, onRow: false };
const press = (k: string) => ({ key: k, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false });

describe('the registry behind the table', () => {
  afterEach(() => applyOverrides({}));

  it('derives one row per desktop key, in its order, each on a registry verb', () => {
    expect(SHORTCUTS.map((s) => s.id)).toEqual(DESK_KEYS.map((k) => k.id));
    for (const k of DESK_KEYS) {
      const verb = ACTIONS.find((a) => a.id === k.action);
      expect(verb, k.id).toBeDefined();
      expect(verb!.desktop).toContain(k.id);
      expect(verb!.surface).not.toBe('tui');
    }
    expect(actionOf('palette')).toBe('command_palette');
    expect(actionOf('move_up')).toBe('reorder_up');
  });

  it('lays the user bindings over the defaults, and takes them back off', () => {
    applyOverrides({ help: ['f2'], settings: [], board_diff: ['x'], gone: ['q'] });
    expect(keysOf('help')).toEqual(['f2']);
    expect(keysOf('settings')).toEqual([]);
    // A home view's walking key is the view's: never overridden.
    expect(keysOf('board_diff')).toEqual(['d']);
    expect(shortcutFor(press('F2'), idle)?.id).toBe('help');
    expect(shortcutFor(press('?'), idle)).toBeUndefined();
    expect(withKey('Keyboard shortcuts', ['help', 'help_f1'])).toBe('Keyboard shortcuts (F2 or F1)');
    applyOverrides({});
    expect(keysOf('help')).toEqual(defaultKeys('help'));
    expect(keysOf('settings')).toEqual(['ctrl+,']);
  });

  it('names the rows a chord would clash with where their scopes meet', () => {
    expect(clashes('rename', 'a').map((s) => s.id)).toEqual(['archive']);
    expect(clashes('help', 'f9')).toEqual([]);
    // A window key may not take a home view's walking key.
    expect(clashes('help', 'j').map((s) => s.id)).toEqual(['list_move']);
    expect(overlaps('row', 'global')).toBe(true);
    expect(overlaps('anywhere', 'fields')).toBe(true);
    expect(isFixed('board')).toBe(true);
    expect(isModifierKey('Shift')).toBe(true);
    expect(isModifierKey('k')).toBe(false);
  });
});
