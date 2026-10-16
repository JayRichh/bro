import { describe, expect, it, vi } from 'vitest';
import { fieldItems, selectionItems, terminalItems, viewerItems } from './items';
import type { FieldState, FieldVerbs, TermState, TermVerbs, ViewerState, ViewerVerbs } from './items';
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';

const rows = (es: MenuEntry[]): MenuItem[] => es.filter((e): e is MenuItem => typeof e === 'object' && 'label' in e);
const labels = (es: MenuEntry[]) => es.map((e) => (e === 'sep' ? 'sep' : 'heading' in e ? e.heading : e.label));

function item(es: MenuEntry[], label: string): MenuItem {
  const r = rows(es).find((x) => x.label === label);
  if (!r) throw new Error(`no row labelled ${label}`);
  return r;
}

/** The labels of the greyed-out rows, in menu order. */
const disabledRows = (es: MenuEntry[]) => rows(es).filter((r) => r.disabled).map((r) => r.label);
/** Every greyed-out row says why. */
const allExplained = (es: MenuEntry[]) => rows(es).filter((r) => r.disabled).every((r) => !!r.title);

const field = (o: Partial<FieldState> = {}): FieldState => ({
  editable: true,
  secret: false,
  selected: true,
  empty: false,
  canUndo: true,
  clipboard: null,
  ...o,
});
const fieldVerbs = () => ({ undo: vi.fn(), cut: vi.fn(), copy: vi.fn(), paste: vi.fn(), selectAll: vi.fn() });

describe('fieldItems', () => {
  it('lists Undo, Cut, Copy, Paste and Select all in the browser order with their keys', () => {
    const items = fieldItems(field(), fieldVerbs());
    expect(labels(items)).toEqual(['Undo', 'sep', 'Cut', 'Copy', 'Paste', 'sep', 'Select all']);
    expect(item(items, 'Undo').keys).toEqual(['ctrl+z']);
    expect(item(items, 'Cut').keys).toEqual(['ctrl+x']);
    expect(item(items, 'Copy').keys).toEqual(['ctrl+c']);
    expect(item(items, 'Paste').keys).toEqual(['ctrl+v']);
    expect(item(items, 'Select all').keys).toEqual(['ctrl+a']);
  });

  it('enables every row for an editable field with a selection and an unknown clipboard', () => {
    expect(disabledRows(fieldItems(field(), fieldVerbs()))).toEqual([]);
  });

  it('greys out Cut and Copy with nothing selected, and keeps Paste and Select all', () => {
    const items = fieldItems(field({ selected: false }), fieldVerbs());
    expect(disabledRows(items)).toEqual(['Cut', 'Copy']);
    expect(allExplained(items)).toBe(true);
  });

  it('greys out Undo, Cut and Paste in a read-only field, and keeps Copy and Select all', () => {
    const items = fieldItems(field({ editable: false }), fieldVerbs());
    expect(disabledRows(items)).toEqual(['Undo', 'Cut', 'Paste']);
    expect(allExplained(items)).toBe(true);
  });

  it('greys out Cut and Copy in a password field with a selection', () => {
    const items = fieldItems(field({ secret: true }), fieldVerbs());
    expect(disabledRows(items)).toEqual(['Cut', 'Copy']);
    expect(allExplained(items)).toBe(true);
  });

  it('greys out Paste when the clipboard holds no text, and enables it when it does', () => {
    const empty = fieldItems(field({ clipboard: false }), fieldVerbs());
    expect(disabledRows(empty)).toEqual(['Paste']);
    expect(item(empty, 'Paste').title).toBeTruthy();
    expect(disabledRows(fieldItems(field({ clipboard: true }), fieldVerbs()))).toEqual([]);
  });

  it('greys out Select all in an empty field', () => {
    const items = fieldItems(field({ empty: true, selected: false }), fieldVerbs());
    expect(disabledRows(items)).toEqual(['Cut', 'Copy', 'Select all']);
    expect(item(items, 'Select all').title).toBeTruthy();
  });

  it('greys out Undo when there is nothing to undo', () => {
    const items = fieldItems(field({ canUndo: false }), fieldVerbs());
    expect(disabledRows(items)).toEqual(['Undo']);
    expect(item(items, 'Undo').title).toBeTruthy();
  });

  const wiring: [string, keyof FieldVerbs][] = [
    ['Undo', 'undo'],
    ['Cut', 'cut'],
    ['Copy', 'copy'],
    ['Paste', 'paste'],
    ['Select all', 'selectAll'],
  ];
  it('ends on Open this line in the editor, at the clicked line, from the new side only', () => {
    const openLine = vi.fn();
    const v = { ...viewVerbs(), openLine };
    const at = viewerItems(view({ path: '/home/me/app/src/a.ts', line: 12, editor: 'VS Code' }), v);
    expect(labels(at)).toEqual(['Copy', 'Select all', 'sep', 'Copy path:line', 'Open this line in VS Code']);
    const row = item(at, 'Open this line in VS Code');
    expect(row.title).toBe('/home/me/app/src/a.ts:12');
    row.run();
    expect(openLine).toHaveBeenCalledTimes(1);
    // No line under the pointer: the file, in the editor the user has.
    expect(labels(viewerItems(view({ path: '/a.ts', line: null, editor: 'Cursor' }), v))).toContain('Open in Cursor');
    // A diff's old side: its lines are not the file's now.
    expect(labels(viewerItems(view({ path: '/a.ts', line: 3, old: true, editor: 'VS Code' }), v))).not.toContain('Open this line in VS Code');
    // No editor installed, or no file: no row.
    expect(labels(viewerItems(view({ path: '/a.ts', line: 3, editor: null }), v)).at(-1)).toBe('Copy path:line');
    expect(labels(viewerItems(view({ editor: 'VS Code' }), v))).toEqual(['Copy', 'Select all']);
  });

  it.each(wiring)('runs %s through the %s verb', (label, verb) => {
    const v = fieldVerbs();
    item(fieldItems(field(), v), label).run();
    expect(v[verb]).toHaveBeenCalledTimes(1);
    for (const [name, fn] of Object.entries(v)) if (name !== verb) expect(fn).not.toHaveBeenCalled();
  });
});

const term = (o: Partial<TermState> = {}): TermState => ({
  selected: true,
  live: true,
  clipboard: null,
  folder: null,
  link: null,
  ...o,
});
const termVerbs = () => ({
  copy: vi.fn(),
  paste: vi.fn(),
  selectAll: vi.fn(),
  clear: vi.fn(),
  copyFolder: vi.fn(),
  openLink: vi.fn(),
  copyLink: vi.fn(),
});

describe('terminalItems', () => {
  it('lists Copy, Paste, Select all and Clear, all enabled for a live pane with a selection', () => {
    const items = terminalItems(term(), termVerbs());
    expect(labels(items)).toEqual(['Copy', 'Paste', 'sep', 'Select all', 'Clear']);
    expect(item(items, 'Copy').keys).toEqual(['ctrl+c']);
    expect(item(items, 'Paste').keys).toEqual(['ctrl+v']);
    expect(disabledRows(items)).toEqual([]);
  });

  it('greys out Copy without a selection', () => {
    const items = terminalItems(term({ selected: false }), termVerbs());
    expect(disabledRows(items)).toEqual(['Copy']);
    expect(allExplained(items)).toBe(true);
  });

  it('greys out Paste when the session is not running, saying so', () => {
    const items = terminalItems(term({ live: false }), termVerbs());
    expect(disabledRows(items)).toEqual(['Paste']);
    expect(item(items, 'Paste').title).toMatch(/session/i);
  });

  it('greys out Paste when the clipboard holds no text, and enables it when it does', () => {
    const empty = terminalItems(term({ clipboard: false }), termVerbs());
    expect(disabledRows(empty)).toEqual(['Paste']);
    expect(item(empty, 'Paste').title).toBeTruthy();
    expect(disabledRows(terminalItems(term({ clipboard: true }), termVerbs()))).toEqual([]);
  });

  it('never greys out Select all or Clear', () => {
    const items = terminalItems(term({ selected: false, live: false, clipboard: false }), termVerbs());
    expect(disabledRows(items)).toEqual(['Copy', 'Paste']);
    expect(allExplained(items)).toBe(true);
  });

  it('adds Copy folder path only when the pane has a folder, titled with it', () => {
    const items = terminalItems(term({ folder: '/home/me/app' }), termVerbs());
    expect(labels(items)).toEqual(['Copy', 'Paste', 'sep', 'Select all', 'Clear', 'sep', 'Copy folder path']);
    expect(item(items, 'Copy folder path').title).toBe('/home/me/app');
    expect(labels(terminalItems(term({ folder: null }), termVerbs()))).not.toContain('Copy folder path');
  });

  it('adds Open link and Copy link after a separator only when a link is under the pointer', () => {
    const url = 'https://example.com/pr/7';
    const items = terminalItems(term({ link: url }), termVerbs());
    expect(labels(items)).toEqual(['Copy', 'Paste', 'sep', 'Select all', 'Clear', 'sep', 'Open link', 'Copy link']);
    expect(item(items, 'Open link').title).toBe(url);
    expect(item(items, 'Copy link').title).toBe(url);
    expect(labels(terminalItems(term({ link: null }), termVerbs()))).not.toContain('Open link');
  });

  it('puts the folder row before the link rows when both are there', () => {
    const items = terminalItems(term({ folder: '/home/me/app', link: 'https://example.com' }), termVerbs());
    expect(labels(items).slice(5)).toEqual(['sep', 'Copy folder path', 'sep', 'Open link', 'Copy link']);
  });

  const wiring: [string, keyof TermVerbs][] = [
    ['Copy', 'copy'],
    ['Paste', 'paste'],
    ['Select all', 'selectAll'],
    ['Clear', 'clear'],
    ['Copy folder path', 'copyFolder'],
    ['Open link', 'openLink'],
    ['Copy link', 'copyLink'],
  ];
  it.each(wiring)('runs %s through the %s verb', (label, verb) => {
    const v = termVerbs();
    item(terminalItems(term({ folder: '/home/me/app', link: 'https://example.com' }), v), label).run();
    expect(v[verb]).toHaveBeenCalledTimes(1);
    for (const [name, fn] of Object.entries(v)) if (name !== verb) expect(fn).not.toHaveBeenCalled();
  });
});

const view = (o: Partial<ViewerState> = {}): ViewerState => ({ selected: true, path: null, line: null, old: false, ...o });
const viewVerbs = () => ({ copy: vi.fn(), selectAll: vi.fn(), copyPath: vi.fn() });

describe('viewerItems', () => {
  it('lists Copy and Select all with their keys when there is no path', () => {
    const items = viewerItems(view(), viewVerbs());
    expect(labels(items)).toEqual(['Copy', 'Select all']);
    expect(item(items, 'Copy').keys).toEqual(['ctrl+c']);
    expect(item(items, 'Select all').keys).toEqual(['ctrl+a']);
  });

  it('greys out Copy without a selection, saying why', () => {
    const items = viewerItems(view({ selected: false }), viewVerbs());
    expect(disabledRows(items)).toEqual(['Copy']);
    expect(item(items, 'Copy').title).toBeTruthy();
    expect(disabledRows(viewerItems(view({ selected: true }), viewVerbs()))).toEqual([]);
  });

  it('always enables Select all', () => {
    expect(item(viewerItems(view({ selected: false }), viewVerbs()), 'Select all').disabled).toBeFalsy();
    expect(item(viewerItems(view({ selected: true }), viewVerbs()), 'Select all').disabled).toBeFalsy();
  });

  it('adds Copy path:line for a path and a line, titled with both, copying with the line', () => {
    const v = viewVerbs();
    const items = viewerItems(view({ path: '/home/me/app/src/a.ts', line: 42 }), v);
    expect(labels(items)).toEqual(['Copy', 'Select all', 'sep', 'Copy path:line']);
    const row = item(items, 'Copy path:line');
    expect(row.title).toBe('/home/me/app/src/a.ts:42');
    row.run();
    expect(v.copyPath).toHaveBeenCalledTimes(1);
    expect(v.copyPath).toHaveBeenCalledWith(true);
  });

  it('offers Copy path without the line on the old side of a diff', () => {
    const v = viewVerbs();
    const items = viewerItems(view({ path: '/home/me/app/src/a.ts', line: 42, old: true }), v);
    expect(labels(items)).toEqual(['Copy', 'Select all', 'sep', 'Copy path']);
    const row = item(items, 'Copy path');
    expect(row.title).toContain('/home/me/app/src/a.ts');
    row.run();
    expect(v.copyPath).toHaveBeenCalledWith(false);
  });

  it('offers Copy path when the pointer is on no line', () => {
    const v = viewVerbs();
    const items = viewerItems(view({ path: '/home/me/app/src/a.ts', line: null }), v);
    const row = item(items, 'Copy path');
    expect(labels(items)).not.toContain('Copy path:line');
    expect(row.title).toBe('/home/me/app/src/a.ts');
    row.run();
    expect(v.copyPath).toHaveBeenCalledWith(false);
  });

  const wiring: [string, Exclude<keyof ViewerVerbs, 'openLine'>][] = [
    ['Copy', 'copy'],
    ['Select all', 'selectAll'],
  ];
  it.each(wiring)('runs %s through the %s verb', (label, verb) => {
    const v = viewVerbs();
    item(viewerItems(view(), v), label).run();
    expect(v[verb]).toHaveBeenCalledTimes(1);
    expect(v.copyPath).not.toHaveBeenCalled();
  });
});

describe('selectionItems', () => {
  it('is one Copy row with Ctrl+C that calls the function', () => {
    const copy = vi.fn();
    const items = selectionItems(copy);
    expect(items).toHaveLength(1);
    const row = item(items, 'Copy');
    expect(row.keys).toEqual(['ctrl+c']);
    expect(row.disabled).toBeFalsy();
    row.run();
    expect(copy).toHaveBeenCalledTimes(1);
  });
});
