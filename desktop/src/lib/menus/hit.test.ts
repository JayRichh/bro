import { describe, expect, it } from 'vitest';
import { isTextField, menuPoint, resolveHit, targetKind } from './hit';
import type { Hit, MenuNode } from './hit';

interface NodeOpts {
  attrs?: Record<string, string>;
  classes?: string[];
  type?: string;
  disabled?: boolean;
  editable?: boolean;
}

/** A plain object with as much of an element as hit.ts reads. */
function node(tag: string, o: NodeOpts = {}): MenuNode {
  const attrs = new Map(Object.entries(o.attrs ?? {}));
  const classes = o.classes ?? [];
  return {
    tagName: tag.toUpperCase(),
    type: o.type,
    disabled: o.disabled,
    isContentEditable: o.editable,
    classList: { contains: (c) => classes.includes(c) },
    getAttribute: (name) => attrs.get(name) ?? null,
  };
}

/** A composedPath(): the elements innermost first, then document and window. */
const path = (...els: MenuNode[]): unknown[] => [...els, {}, {}];

const target = (t: string) => node('div', { attrs: { 'data-target': t } });
const elOf = (h: Hit): MenuNode | null => ('el' in h ? h.el : null);
const focusOf = (h: Hit): MenuNode | null | undefined => ('focus' in h ? h.focus : undefined);

describe('resolveHit: menus', () => {
  it('is a menu whatever the path when one is open', () => {
    const field = node('input', { type: 'text' });
    const row = target('agent:7');
    expect(resolveHit(path(field, row), { menuOpen: true })).toEqual({ kind: 'menu' });
    expect(resolveHit(path(row), { menuOpen: true })).toEqual({ kind: 'menu' });
    expect(resolveHit([], { menuOpen: true })).toEqual({ kind: 'menu' });
  });

  it('is a menu inside an element with role menu, such as the verbs menu search', () => {
    const search = node('input', { type: 'text' });
    const menu = node('div', { attrs: { role: 'menu' } });
    const row = target('agent:7');
    expect(resolveHit(path(search, menu))).toEqual({ kind: 'menu' });
    expect(resolveHit(path(search, menu, row))).toEqual({ kind: 'menu' });
  });
});

describe('resolveHit: terminal and viewer', () => {
  it('reads the terminal helper textarea as the terminal, not a field', () => {
    const helper = node('textarea', { classes: ['xterm-helper-textarea'] });
    const xterm = node('div', { classes: ['xterm'] });
    const row = target('terminal:t1');
    const h = resolveHit(path(helper, xterm, row));
    expect(h.kind).toBe('terminal');
    expect(elOf(h)).toBe(xterm);
  });

  it('reads Monaco input area as the viewer, not a field', () => {
    const area = node('textarea', { classes: ['inputarea'] });
    const monaco = node('div', { classes: ['monaco-editor'] });
    const row = target('file:wt:src/a.ts');
    const h = resolveHit(path(area, monaco, row));
    expect(h.kind).toBe('viewer');
    expect(elOf(h)).toBe(monaco);
  });

  it("reads Monaco's other hidden inputs as the viewer too", () => {
    const monaco = node('div', { classes: ['monaco-editor'] });
    expect(resolveHit(path(node('div', { classes: ['native-edit-context'], editable: true }), monaco)).kind).toBe('viewer');
    expect(resolveHit(path(node('textarea', { classes: ['ime-text-area'] }), monaco)).kind).toBe('viewer');
  });

  it("reads a real field inside Monaco (its find box) as a field", () => {
    const find = node('textarea', { classes: ['input'] });
    const monaco = node('div', { classes: ['monaco-editor'] });
    const h = resolveHit(path(find, node('div', { classes: ['find-widget'] }), monaco));
    expect(h.kind).toBe('text');
    expect(elOf(h)).toBe(find);
  });
});

describe('resolveHit: text fields', () => {
  it('reads a plain input as text even inside a row', () => {
    const input = node('input', { type: 'text' });
    const row = target('agent:7');
    const h = resolveHit(path(input, row));
    expect(h.kind).toBe('text');
    expect(elOf(h)).toBe(input);
  });

  it.each(['text', 'search', 'url', 'tel', 'email', 'password', 'number', ''])('counts an input of type %j as a text field', (type) => {
    expect(isTextField(node('input', { type }))).toBe(true);
    expect(resolveHit(path(node('input', { type }))).kind).toBe('text');
  });

  it('counts an input with no type as a text field', () => {
    expect(isTextField(node('input'))).toBe(true);
  });

  it('ignores the case of the type', () => {
    expect(isTextField(node('INPUT', { type: 'TEXT' }))).toBe(true);
    expect(isTextField(node('input', { type: 'CheckBox' }))).toBe(false);
  });

  it.each(['checkbox', 'radio', 'button', 'range', 'file'])('does not count an input of type %j', (type) => {
    expect(isTextField(node('input', { type }))).toBe(false);
    expect(resolveHit(path(node('input', { type }))).kind).toBe('none');
  });

  it('does not count a disabled input or textarea', () => {
    expect(isTextField(node('input', { type: 'text', disabled: true }))).toBe(false);
    expect(isTextField(node('textarea', { disabled: true }))).toBe(false);
  });

  it('counts a textarea', () => {
    expect(isTextField(node('textarea'))).toBe(true);
    expect(resolveHit(path(node('textarea'))).kind).toBe('text');
  });

  it('counts a span inside a contenteditable, which reports isContentEditable itself', () => {
    const span = node('span', { editable: true });
    expect(isTextField(span)).toBe(true);
    const h = resolveHit(path(span, node('div', { editable: true })));
    expect(h.kind).toBe('text');
    expect(elOf(h)).toBe(span);
  });

  it('does not count other elements', () => {
    expect(isTextField(node('div'))).toBe(false);
    expect(isTextField(node('button'))).toBe(false);
  });
});

describe('resolveHit: selection', () => {
  it('is a selection when the pointer is on selected text', () => {
    const span = node('span');
    const h = resolveHit(path(span, node('div')), { selected: (n) => n === span });
    expect(h.kind).toBe('selection');
    expect(elOf(h)).toBe(span);
  });

  it('does not beat a text field', () => {
    const input = node('input', { type: 'text' });
    const h = resolveHit(path(input, target('agent:7')), { selected: () => true });
    expect(h.kind).toBe('text');
  });

  it('beats an own menu and a row', () => {
    const span = node('span');
    const row = target('worktree:w1');
    const band = node('div');
    const ctx = { selected: (n: MenuNode) => n === span, own: (n: MenuNode) => n === band };
    const h = resolveHit(path(span, row, band), ctx);
    expect(h.kind).toBe('selection');
    expect(elOf(h)).toBe(span);
    expect(resolveHit(path(span, row), ctx).kind).toBe('selection');
  });

  it('falls through to the row when the selection is not under the pointer', () => {
    const span = node('span');
    const row = target('worktree:w1');
    expect(resolveHit(path(span, row), { selected: () => false }).kind).toBe('row');
  });
});

describe('resolveHit: own menu and rows', () => {
  it('prefers an element with a menu of its own over a target inside it', () => {
    const inner = target('worktree:w1');
    const band = node('div');
    const h = resolveHit(path(inner, band), { own: (n) => n === band });
    expect(h.kind).toBe('own');
    expect(elOf(h)).toBe(band);
  });

  it('gives an own menu the innermost focusable element inside it', () => {
    const chip = node('button');
    const inner = node('span');
    const band = node('div');
    const h = resolveHit(path(inner, chip, band), { own: (n) => n === band });
    expect(h.kind).toBe('own');
    expect(focusOf(h)).toBe(chip);
  });

  it('is a row with the nearest data-target', () => {
    const label = node('span');
    const row = target('worktree:w1');
    const h = resolveHit(path(label, row, node('div')));
    expect(h).toMatchObject({ kind: 'row', target: 'worktree:w1' });
    expect(elOf(h)).toBe(row);
  });

  it('takes the innermost of two nested targets', () => {
    const inner = target('file:wt:src/a.ts');
    const outer = target('worktree:w1');
    const h = resolveHit(path(node('span'), inner, outer));
    expect(h).toMatchObject({ kind: 'row', target: 'file:wt:src/a.ts' });
    expect(elOf(h)).toBe(inner);
  });
});

describe('resolveHit: the row focus', () => {
  it('is the row itself when it has a tabindex', () => {
    const row = node('div', { attrs: { 'data-target': 'file:wt:src/a.ts', tabindex: '0' } });
    const inner = node('button');
    expect(focusOf(resolveHit(path(inner, row)))).toBe(row);
  });

  it('is the row itself when it is a button', () => {
    const row = node('button', { attrs: { 'data-target': 'agent:7' } });
    expect(focusOf(resolveHit(path(node('span'), row)))).toBe(row);
  });

  it('is the innermost focusable element inside a row that is not focusable', () => {
    const row = node('div', { attrs: { 'data-target': 'agent:7', role: 'presentation' } });
    const inner = node('button');
    const outer = node('button');
    const h = resolveHit(path(node('span'), inner, outer, row));
    expect(h.kind).toBe('row');
    expect(focusOf(h)).toBe(inner);
  });

  it('is null when nothing on the way is focusable', () => {
    const h = resolveHit(path(node('span'), node('div'), target('agent:7')));
    expect(h.kind).toBe('row');
    expect(focusOf(h)).toBeNull();
  });

  it('ignores focusable elements outside the row', () => {
    const row = target('agent:7');
    const h = resolveHit(path(node('span'), row, node('button')));
    expect(focusOf(h)).toBeNull();
  });

  it('skips a disabled button', () => {
    const row = target('agent:7');
    const off = node('button', { disabled: true });
    const on = node('button');
    expect(focusOf(resolveHit(path(off, row)))).toBeNull();
    expect(focusOf(resolveHit(path(off, on, row)))).toBe(on);
  });

  it('counts an anchor only with an href', () => {
    const row = target('agent:7');
    const bare = node('a');
    const link = node('a', { attrs: { href: '/x' } });
    expect(focusOf(resolveHit(path(bare, row)))).toBeNull();
    expect(focusOf(resolveHit(path(link, row)))).toBe(link);
  });
});

describe('resolveHit: nothing', () => {
  it('is none with the innermost element', () => {
    const span = node('span');
    const h = resolveHit(path(span, node('div')));
    expect(h.kind).toBe('none');
    expect(elOf(h)).toBe(span);
  });

  it('is none with no element for an empty path', () => {
    expect(resolveHit([])).toEqual({ kind: 'none', el: null });
  });

  it('is none with no element for a path of only document and window', () => {
    expect(resolveHit([{}, {}])).toEqual({ kind: 'none', el: null });
  });
});

describe('targetKind', () => {
  it('is the part before the first colon', () => {
    expect(targetKind('agent:7')).toBe('agent');
    expect(targetKind('file:wt:src/a.ts')).toBe('file');
  });

  it('is the whole target when it has no colon', () => {
    expect(targetKind('plain')).toBe('plain');
  });
});

describe('menuPoint', () => {
  const rect = { getBoundingClientRect: () => ({ left: 40, bottom: 90 }) };

  it('uses the pointer when either coordinate is set', () => {
    expect(menuPoint({ clientX: 10, clientY: 20 }, rect)).toEqual({ x: 10, y: 20 });
    expect(menuPoint({ clientX: 0, clientY: 20 }, rect)).toEqual({ x: 0, y: 20 });
    expect(menuPoint({ clientX: 10, clientY: 0 }, rect)).toEqual({ x: 10, y: 0 });
  });

  it('opens under the element when there is no pointer', () => {
    expect(menuPoint({ clientX: 0, clientY: 0 }, rect)).toEqual({ x: 40, y: 90 });
  });

  it('falls back to the origin with no pointer and no element', () => {
    expect(menuPoint({ clientX: 0, clientY: 0 }, null)).toEqual({ x: 0, y: 0 });
    expect(menuPoint({ clientX: 0, clientY: 0 }, {})).toEqual({ x: 0, y: 0 });
  });
});
