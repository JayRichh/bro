import { describe, expect, it } from 'vitest';
import { chordLabel, hotkeyGroups, matches, segmented, sequence, targetKey } from './settings';
import type { HotkeyRow, SettingRow } from '../api';

const row = (labels: string[]): SettingRow => ({
  target: { scope: 'global', key: 'k' },
  label: 'L',
  hint: '',
  control: { type: 'choice', options: labels, labels },
  value: labels[0] ?? '',
  default: labels[0] ?? '',
  restart: null,
  is_new: false,
});

describe('settings helpers', () => {
  it('keys every target scope apart', () => {
    const keys = [
      targetKey({ scope: 'global', key: 'theme' }),
      targetKey({ scope: 'desktop', key: 'theme' }),
      targetKey({ scope: 'agent', harness: 'claude', field: 'model' }),
      targetKey({ scope: 'project', path: 'C:\\r', field: 'run_command' }),
    ];
    expect(new Set(keys).size).toBe(4);
  });

  it('segments short choices and lists long ones', () => {
    expect(segmented(row(['System', 'Light', 'Dark']))).toBe(true);
    expect(segmented(row(['off', '1m', '5m', '15m', '30m', '1h']))).toBe(false);
    expect(segmented(row([]))).toBe(false);
  });

  it('spells chords for people', () => {
    expect(chordLabel('ctrl+shift+k')).toBe('Ctrl+Shift+K');
    expect(chordLabel('j')).toBe('j');
    expect(chordLabel('down')).toBe('\u2193');
    expect(chordLabel('f10')).toBe('F10');
    expect(chordLabel('ctrl++')).toBe('Ctrl++');
  });

  it('groups hotkeys in order and filters loosely', () => {
    const h = (id: string, group: string): HotkeyRow => ({
      id,
      label: id,
      hint: '',
      group,
      defaults: [],
      bound: [],
      overridden: false,
    });
    const g = hotkeyGroups([h('a', 'NAVIGATE'), h('b', 'NAVIGATE'), h('c', 'GENERAL')]);
    expect(g.map((x) => [x.group, x.rows.length])).toEqual([
      ['NAVIGATE', 2],
      ['GENERAL', 1],
    ]);
    expect(matches('idle', 'Idle session timeout')).toBe(true);
    expect(matches('', 'anything')).toBe(true);
    expect(matches('zzz', 'Theme', 'hint')).toBe(false);
  });
});

describe('sequence', () => {
  it('applies only the newest of overlapping loads', () => {
    const seq = sequence();
    const first = seq.next();
    const second = seq.next();
    expect(seq.isLatest(first)).toBe(false);
    expect(seq.isLatest(second)).toBe(true);
  });
});
