import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MenuNode } from './hit';

vi.mock('../api', () => ({ api: {} }));
// No DOM here: openMenu tells an element from a point with instanceof.
(globalThis as { HTMLElement?: unknown }).HTMLElement ??= class {};

const { ui, openMenu, closeMenu } = await import('../ui/ui.svelte');
const router = await import('./router');

/** An element as the router reads it off an event's path. */
function el(tag: string, attrs: Record<string, string> = {}, classes: string[] = []): MenuNode & { focus: () => void; focused: number } {
  const n = {
    tagName: tag.toUpperCase(),
    type: attrs.type,
    classList: { contains: (c: string) => classes.includes(c) },
    getAttribute: (a: string) => attrs[a] ?? null,
    focused: 0,
    focus() {
      n.focused++;
    },
  };
  return n;
}

/** A right-click on `path[0]`, as `contextmenu` hands it over. */
function rightClick(path: unknown[], at = { x: 40, y: 60 }) {
  let prevented = false;
  return {
    clientX: at.x,
    clientY: at.y,
    composedPath: () => [...path, {}, {}],
    preventDefault: () => (prevented = true),
    get defaultPrevented() {
      return prevented;
    },
  };
}

let listener: ((e: unknown) => void) | null = null;
let stop: () => void;
const doc = {
  addEventListener: (_: string, f: (e: unknown) => void, capture?: boolean) => {
    expect(capture).toBe(true);
    listener = f;
  },
  removeEventListener: () => (listener = null),
  getSelection: () => null,
};

/** Fire `e` and let the menu open (it opens a tick later). */
async function fire(e: ReturnType<typeof rightClick>) {
  listener!(e);
  await new Promise((r) => setTimeout(r, 0));
  await Promise.resolve();
}

beforeEach(() => {
  closeMenu();
  stop = router.installMenuRouter(doc as never);
});
afterEach(() => stop());

describe('the right-click router', () => {
  it('stops the browser menu on every right-click, even with nothing to open', async () => {
    const e = rightClick([el('div')]);
    await fire(e);
    expect(e.defaultPrevented).toBe(true);
    expect(router.lastContextMenu).toMatchObject({ kind: 'none', prevented: true });
    expect(ui.menu).toBeNull();
  });

  it("opens a row's menu from its target's kind, with the id past the colon", async () => {
    const build = vi.fn((id: string) => ({ items: [{ label: `Open ${id}`, run: () => {} }], label: 'Thing' }));
    router.setRowBuilder('thing', build);
    const row = el('div', { 'data-target': 'thing:42:x' });
    await fire(rightClick([el('span'), row], { x: 5, y: 6 }));
    expect(build).toHaveBeenCalledWith('42:x', row, 'thing:42:x');
    expect(ui.menu).toMatchObject({ x: 5, y: 6, label: 'Thing' });
    expect(ui.menu?.items).toEqual([expect.objectContaining({ label: 'Open 42:x' })]);
  });

  it('focuses the clicked control inside a row that cannot take the focus', async () => {
    router.setRowBuilder('card', () => ({ items: [{ label: 'Archive', run: () => {} }] }));
    const button = el('button');
    await fire(rightClick([el('span'), button, el('div', { 'data-target': 'card:1', role: 'presentation' })]));
    expect(button.focused).toBe(1);
  });

  it('opens nothing for a kind no builder knows', async () => {
    await fire(rightClick([el('div', { 'data-target': 'nobody:1' })]));
    expect(ui.menu).toBeNull();
  });

  it("opens an element's own menu beside a target, and a target's inside it, until it is destroyed", async () => {
    router.setRowBuilder('band', () => ({ items: [{ label: 'Registry', run: () => {} }], label: 'chip' }));
    router.setRowBuilder('repo', () => ({ items: [{ label: 'Repo', run: () => {} }], label: 'repo' }));
    const box = el('div');
    const own = router.ownMenu(box as unknown as HTMLElement, () => ({ items: [{ label: 'Own', run: () => {} }], label: 'main' }));
    const outer = el('section', { 'data-target': 'repo:1' });
    await fire(rightClick([el('span'), box, outer]));
    expect(ui.menu?.label).toBe('main');
    closeMenu();
    // A target inside the own menu's box (a band's PR chip) has its own.
    await fire(rightClick([el('span'), el('div', { 'data-target': 'band:1' }), box, outer]));
    expect(ui.menu?.label).toBe('chip');
    closeMenu();
    own.destroy();
    await fire(rightClick([el('span'), box, outer]));
    expect(ui.menu?.items[0]).toMatchObject({ label: 'Repo' });
  });

  it('reopens on what is under the pointer when the right-click lands on the scrim', async () => {
    router.setRowBuilder('card', (id) => ({ items: [{ label: `Card ${id}`, run: () => {} }], label: `card ${id}` }));
    // Under the pointer: the scrim, the open menu's box, then the card.
    const card = el('div', { 'data-target': 'card:2' }) as ReturnType<typeof el> & { parentElement?: unknown };
    const label = el('span') as ReturnType<typeof el> & { parentElement?: unknown };
    label.parentElement = card;
    card.parentElement = null;
    const scrim = el('div', { 'data-menu-scrim': '' });
    const box = el('div', { role: 'menu' });
    const stack = [scrim, box, label];
    const d = { ...doc, elementsFromPoint: () => stack };
    stop();
    stop = router.installMenuRouter(d as never);
    openMenu([{ label: 'Card 1', run: () => {} }], { x: 1, y: 1 }, 'card 1');
    await fire(rightClick([scrim], { x: 70, y: 80 }));
    expect(ui.menu).toMatchObject({ label: 'card 2', x: 70, y: 80 });
    expect(router.lastContextMenu.kind).toBe('row');
  });

  it('closes the menu on a scrim right-click over nothing with a menu', async () => {
    const scrim = el('div', { 'data-menu-scrim': '' });
    const d = { ...doc, elementsFromPoint: () => [scrim] };
    stop();
    stop = router.installMenuRouter(d as never);
    openMenu([{ label: 'First', run: () => {} }], { x: 1, y: 1 }, 'Row');
    await fire(rightClick([scrim]));
    expect(ui.menu).toBeNull();
  });

  it('never opens a second menu while one is up: the Menu key after Shift+F10', async () => {
    const build = vi.fn(() => ({ items: [{ label: 'Again', run: () => {} }] }));
    router.setRowBuilder('row', build);
    // The row key (row_menu) opened this one on keydown...
    openMenu([{ label: 'First', run: () => {} }], { x: 1, y: 1 }, 'Row');
    const first = ui.menu;
    // ...and the browser's contextmenu for the same key follows it.
    const e = rightClick([el('div', { 'data-target': 'row:1' })]);
    await fire(e);
    expect(e.defaultPrevented).toBe(true);
    expect(router.lastContextMenu.kind).toBe('menu');
    expect(build).not.toHaveBeenCalled();
    expect(ui.menu).toBe(first);
  });

  it('opens nothing from inside a menu (its search field)', async () => {
    const box = el('div', { role: 'menu' });
    closeMenu();
    await fire(rightClick([el('input'), box]));
    expect(router.lastContextMenu.kind).toBe('menu');
    expect(ui.menu).toBeNull();
  });

  it('routes the terminal, a viewer and a field to their builders, keeping the focus', async () => {
    const seen: string[] = [];
    for (const kind of ['terminal', 'viewer', 'text'] as const) {
      router.setBuilder(kind, (hit) => (seen.push(hit.kind), { items: [{ label: kind, run: () => {} }], keep: true }));
    }
    await fire(rightClick([el('textarea', {}, ['xterm-helper-textarea']), el('div', {}, ['xterm'])]));
    expect(ui.menu).toMatchObject({ label: 'Actions', keep: true });
    closeMenu();
    await fire(rightClick([el('textarea', {}, ['inputarea']), el('div', {}, ['monaco-editor'])]));
    closeMenu();
    await fire(rightClick([el('input', { type: 'text' }), el('div', { 'data-target': 'row:1' })]));
    expect(seen).toEqual(['terminal', 'viewer', 'text']);
  });

  it('settles a row the menu opened without (Paste, once the clipboard is read)', async () => {
    let settle!: (items: { label: string; run: () => void; disabled?: boolean }[]) => void;
    router.setBuilder('text', () => ({
      items: [{ label: 'Paste', run: () => {} }],
      keep: true,
      later: new Promise((r) => (settle = r)),
    }));
    await fire(rightClick([el('textarea')]));
    expect(ui.menu?.items[0]).toMatchObject({ label: 'Paste' });
    settle([{ label: 'Paste', run: () => {}, disabled: true }]);
    await Promise.resolve();
    await Promise.resolve();
    expect(ui.menu?.items[0]).toMatchObject({ label: 'Paste', disabled: true });
  });

  it('leaves a newer menu alone when a late row settles', async () => {
    let settle!: (items: { label: string; run: () => void }[]) => void;
    router.setBuilder('text', () => ({ items: [{ label: 'Paste', run: () => {} }], later: new Promise((r) => (settle = r)) }));
    await fire(rightClick([el('textarea')]));
    openMenu([{ label: 'Other', run: () => {} }], { x: 0, y: 0 });
    settle([{ label: 'Late', run: () => {} }]);
    await Promise.resolve();
    await Promise.resolve();
    expect(ui.menu?.items[0]).toMatchObject({ label: 'Other' });
  });

  it('stops listening when stopped', () => {
    stop();
    expect(listener).toBeNull();
    stop = () => {};
  });
});
