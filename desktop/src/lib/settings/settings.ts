// Pure helpers for the Settings view: how a row is keyed, how a chord
// reads, and how a hotkey list groups. The rows themselves come from the
// Rust side (`settings_schema`); nothing here restates them.

import type { HotkeyRow, SettingRow, SettingTarget } from '../api';

/** A stable key for a row, for keyed each blocks and pending state. */
export function targetKey(t: SettingTarget): string {
  switch (t.scope) {
    case 'global':
      return `g:${t.key}`;
    case 'desktop':
      return `d:${t.key}`;
    case 'agent':
      return `a:${t.harness}:${t.field}`;
    case 'project':
      return `p:${t.path}:${t.field}`;
  }
}

/** A choice row short enough to show as a segmented control. */
export function segmented(row: SettingRow): boolean {
  if (row.control.type !== 'choice') return false;
  const { labels } = row.control;
  return labels.length > 0 && labels.length <= 4 && labels.join('').length <= 34;
}

/** The row differs from its default. */
export function changed(row: SettingRow): boolean {
  return row.value !== row.default;
}

const KEY_WORD: Record<string, string> = {
  ctrl: 'Ctrl',
  shift: 'Shift',
  alt: 'Alt',
  cmd: 'Cmd',
  super: 'Super',
  enter: 'Enter',
  esc: 'Esc',
  tab: 'Tab',
  backtab: 'Shift+Tab',
  space: 'Space',
  left: '\u2190',
  right: '\u2192',
  up: '\u2191',
  down: '\u2193',
  pageup: 'PgUp',
  pagedown: 'PgDn',
  home: 'Home',
  end: 'End',
  backspace: 'Backspace',
  delete: 'Del',
};

/** A chord as the config spells it (`ctrl+shift+k`), for display (`Ctrl+Shift+K`). */
export function chordLabel(spec: string): string {
  const parts = spec.trim().split('+');
  // `ctrl++` binds the plus key itself.
  if (spec.trim().endsWith('++')) parts.splice(parts.length - 2, 2, '+');
  return parts
    .filter((p) => p !== '')
    .map((p) => {
      const lower = p.toLowerCase();
      if (KEY_WORD[lower]) return KEY_WORD[lower];
      if (/^f\d{1,2}$/.test(lower)) return lower.toUpperCase();
      // A letter under a modifier reads as the key cap (Ctrl+K); alone it
      // keeps its case, since `K` and `k` are different chords there.
      if (p.length === 1) return parts.length > 1 ? p.toUpperCase() : p;
      return p[0].toUpperCase() + p.slice(1);
    })
    .join('+');
}

/** Hotkey rows under their group headers, in table order. */
export function hotkeyGroups(rows: HotkeyRow[]): { group: string; rows: HotkeyRow[] }[] {
  const out: { group: string; rows: HotkeyRow[] }[] = [];
  for (const r of rows) {
    const last = out[out.length - 1];
    if (last && last.group === r.group) last.rows.push(r);
    else out.push({ group: r.group, rows: [r] });
  }
  return out;
}

/** Rows whose label, hint or key matches a filter, case-insensitively. */
export function matches(text: string, ...fields: string[]): boolean {
  const q = text.trim().toLowerCase();
  if (!q) return true;
  return fields.some((f) => f.toLowerCase().includes(q));
}
