import { describe, expect, it, vi } from 'vitest';

vi.mock('../api', () => ({ api: {} }));

const { aimsAt, inTriangle, leafItems, NAV0, narrow, navKey, navKeyOf, placeSub, step, submenu, typedChar } = await import('./model');
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';
import { fuzzyMatch } from '../fuzzy';

const row = (label: string, extra: Partial<MenuItem> = {}): MenuItem => ({ label, run: vi.fn(), ...extra });
const score = (q: string, s: string) => fuzzyMatch(q, s)?.score ?? null;
const labels = (es: readonly MenuEntry[]) => es.map((e) => (e === 'sep' ? 'sep' : 'heading' in e ? `# ${e.heading}` : e.label));

/** Open, Diff, rule, Open in \u25b8 (VS Code, Explorer), a greyed row, Delete. */
function menu(): MenuEntry[] {
  return [
    row('Open'),
    row('Diff'),
    'sep',
    submenu('Open in', [row('VS Code', { full: 'Open in VS Code' }), row('Explorer')]),
    row('Merge\u2026', { disabled: true, title: 'Draft pull request: publish it first' }),
    { heading: 'away' },
    row('Delete\u2026', { danger: true }),
  ];
}

describe('menu rows', () => {
  it('walks the live rows round the ends, skipping rules, headings and greyed rows', () => {
    const m = menu();
    expect(step(m, -1, 1)).toBe(0);
    expect(step(m, -1, -1)).toBe(6);
    expect(step(m, 1, 1)).toBe(3);
    expect(step(m, 3, 1)).toBe(6);
    expect(step(m, 6, 1)).toBe(0);
  });

  it('flattens submenus for the palette and type-ahead, with their path and whole words', () => {
    const leaves = leafItems(menu());
    expect(leaves.map((l) => l.label)).toEqual(['Open', 'Diff', 'Open in \u203a VS Code', 'Open in \u203a Explorer', 'Merge\u2026', 'Delete\u2026']);
    expect(leaves[2].full).toBe('Open in VS Code');
    expect(leaves[3].full).toBe('Open in \u203a Explorer');
  });
});

describe('type-ahead', () => {
  it('narrows to the rows that match, a submenu row by its path', () => {
    const hits = narrow(menu(), 'code', score);
    expect(labels(hits)).toEqual(['Open in \u203a VS Code']);
    expect(labels(narrow(menu(), 'expl', score))).toEqual(['Open in \u203a Explorer']);
  });

  it('keeps the menu as it is with no query, and the pinned row whatever the query', () => {
    expect(labels(narrow(menu(), '  ', score))).toEqual(labels(menu()));
    const withKeys = [...menu(), row('All keys\u2026', { pinned: true })];
    expect(labels(narrow(withKeys, 'diff', score))).toEqual(['Diff', 'All keys\u2026']);
  });

  it('types letters into the query; j, k and Space walk and run until a query starts', () => {
    expect(typedChar({ key: 'o' }, '')).toBe('o');
    expect(typedChar({ key: 'j' }, '')).toBeNull();
    expect(typedChar({ key: 'k' }, '')).toBeNull();
    expect(typedChar({ key: ' ' }, '')).toBeNull();
    expect(typedChar({ key: 'j' }, 'o')).toBe('j');
    expect(typedChar({ key: ' ' }, 'open')).toBe(' ');
    expect(typedChar({ key: 'a', ctrlKey: true }, '')).toBeNull();
    expect(typedChar({ key: 'ArrowDown' }, '')).toBeNull();
    expect(navKeyOf('j', true)).toBe('down');
    expect(navKeyOf('j', false)).toBeNull();
  });
});

describe('the keys', () => {
  it('jumps with Home and End', () => {
    const m = menu();
    expect(navKey({ ...NAV0, at: 3 }, 'home', m).nav.at).toBe(0);
    expect(navKey(NAV0, 'end', m).nav.at).toBe(6);
  });

  it('opens a submenu with Right, Enter or Space, on its first row, and never runs the row', () => {
    const m = menu();
    for (const k of ['right', 'enter', 'space'] as const) {
      const out = navKey({ ...NAV0, at: 3 }, k, m);
      expect(out.run).toBeUndefined();
      expect(out.nav).toEqual({ at: 3, open: 3, subAt: 0, inSub: true });
    }
  });

  it('walks inside the submenu and runs its row', () => {
    const m = menu();
    let nav = navKey({ ...NAV0, at: 3 }, 'right', m).nav;
    nav = navKey(nav, 'down', m).nav;
    expect(nav.subAt).toBe(1);
    const out = navKey(nav, 'enter', m);
    expect(out.run?.label).toBe('Explorer');
  });

  it('closes one level with Left or Esc, then the menu with Esc', () => {
    const m = menu();
    const open = navKey({ ...NAV0, at: 3 }, 'right', m).nav;
    for (const k of ['left', 'escape'] as const) {
      const out = navKey(open, k, m);
      expect(out.close).toBeUndefined();
      expect(out.nav).toEqual({ ...NAV0, at: 3 });
    }
    expect(navKey({ ...NAV0, at: 3 }, 'escape', m).close).toBe(true);
    expect(navKey({ ...NAV0, at: 3 }, 'tab', m).close).toBe(true);
  });

  it('closes a submenu the pointer opened when the keys walk on', () => {
    const m = menu();
    const hovered = { at: 3, open: 3, subAt: 0, inSub: false };
    expect(navKey(hovered, 'down', m).nav).toEqual({ ...NAV0, at: 6 });
    // Right takes the keys into it.
    expect(navKey(hovered, 'right', m).nav.inSub).toBe(true);
  });

  it('runs a row with Enter, not a greyed one', () => {
    const m = menu();
    expect(navKey({ ...NAV0, at: 1 }, 'enter', m).run?.label).toBe('Diff');
    expect(navKey({ ...NAV0, at: 4 }, 'enter', m).run).toBeUndefined();
  });
});

describe('where a submenu goes', () => {
  const view = { width: 1000, height: 700 };
  const size = { width: 200, height: 120 };

  it('opens to the right of the menu, level with its row', () => {
    const p = placeSub({ left: 100, top: 300, right: 320, bottom: 324 }, { left: 100, top: 200, right: 320, bottom: 500 }, size, view);
    expect(p).toEqual({ x: 319, y: 295, left: false });
  });

  it('flips left at the window edge and up at the bottom', () => {
    const p = placeSub({ left: 700, top: 640, right: 920, bottom: 664 }, { left: 700, top: 400, right: 920, bottom: 690 }, size, view);
    expect(p.left).toBe(true);
    expect(p.x).toBe(501);
    expect(p.y + size.height).toBeLessThanOrEqual(view.height - 4);
    expect(p.y).toBe(664 + 5 - 120);
  });

  it('keeps the pointer\u2019s diagonal move into an open submenu', () => {
    const sub = { left: 320, top: 280, right: 520, bottom: 400 };
    // From a row's left side, down and right towards the submenu.
    expect(aimsAt({ x: 150, y: 300 }, { x: 200, y: 310 }, sub, false)).toBe(true);
    // Straight down the menu, away from it.
    expect(aimsAt({ x: 150, y: 300 }, { x: 150, y: 420 }, sub, false)).toBe(false);
    expect(inTriangle({ x: 1, y: 1 }, { x: 0, y: 0 }, { x: 4, y: 0 }, { x: 0, y: 4 })).toBe(true);
    expect(inTriangle({ x: 5, y: 5 }, { x: 0, y: 0 }, { x: 4, y: 0 }, { x: 0, y: 4 })).toBe(false);
  });
});
