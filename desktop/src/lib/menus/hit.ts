// What a right-click landed on, read from the event's path (innermost
// first, as `composedPath()` gives it). Pure: the router (router.ts) asks,
// the tests build paths of plain objects.
//
// In order, the first that holds:
//   1. A menu is up, or the click is inside one: `menu` (nothing opens; the
//      Menu key's late event after Shift+F10 or the key itself lands here,
//      so a keyboard open is never doubled).
//   2. A text field (input, textarea, contenteditable): `text`. Not the
//      hidden input the terminal and Monaco type through (their own menus
//      cover it); a field inside one (Monaco's find box) is a field.
//   3. Inside the terminal (`.xterm`) or a Monaco viewer (`.monaco-editor`):
//      `terminal` / `viewer`, whatever wraps them.
//   4. Selected page text under the pointer (a `select-text` error line, a
//      review's history): `selection`.
//   5. The nearest of an element with a menu of its own (`ownMenu`) and a
//      `[data-target]`, the innermost winning: `own` or `row`. An own menu
//      covers a row whose box is wider than its `[data-target]` (a worktree
//      band's ahead/behind and kebab sit beside its title), and a surface
//      whose menu needs what only it knows (a diff file row's staging); a
//      target inside it (the band's PR chip, `pr:`, and ports, `port:`)
//      keeps its own menu. A `row` is built by the target's kind (the part
//      before the colon), so a new kind needs no change here. Never put an
//      own menu on a container of other rows: the background is `none`'s.
//   6. Anything else: `none` (the background menu, menus/background.ts).

/** As much of an element as the resolution reads. */
export interface MenuNode {
  tagName?: string;
  type?: string;
  disabled?: boolean;
  isContentEditable?: boolean;
  classList?: { contains(c: string): boolean };
  getAttribute?(name: string): string | null;
}

export type Hit =
  | { kind: 'menu' }
  | { kind: 'terminal'; el: MenuNode }
  | { kind: 'viewer'; el: MenuNode }
  | { kind: 'text'; el: MenuNode }
  | { kind: 'selection'; el: MenuNode }
  /** `focus`: the innermost focusable element on the way to the row (the
   *  card's button, the row itself), which takes the focus so the row keys
   *  and the focus return after Esc act on what was clicked. */
  | { kind: 'own'; el: MenuNode; focus: MenuNode | null }
  | { kind: 'row'; el: MenuNode; target: string; focus: MenuNode | null }
  | { kind: 'none'; el: MenuNode | null };

export type HitKind = Hit['kind'];

export interface HitContext {
  /** A menu is up. */
  menuOpen?: boolean;
  /** The page's selection covers this node. */
  selected?: (n: MenuNode) => boolean;
  /** This node has a menu of its own. */
  own?: (n: MenuNode) => boolean;
}

const TEXT_INPUTS = new Set(['', 'text', 'search', 'url', 'tel', 'email', 'password', 'number']);
/** The inputs xterm and Monaco type through: theirs, not a field. */
const SURFACE_INPUTS = ['xterm-helper-textarea', 'inputarea', 'ime-text-area', 'native-edit-context'];

function isElement(n: unknown): n is MenuNode {
  return !!n && typeof n === 'object' && typeof (n as MenuNode).tagName === 'string';
}

const tag = (n: MenuNode) => (n.tagName ?? '').toUpperCase();
const has = (n: MenuNode, cls: string) => !!n.classList?.contains(cls);
const attr = (n: MenuNode, name: string) => n.getAttribute?.(name) ?? null;

/** A field the user types in (or could, read-only): a text-like input, a
 *  textarea or contenteditable. Not a disabled one: it can't be selected. */
export function isTextField(n: MenuNode): boolean {
  if (n.disabled || SURFACE_INPUTS.some((c) => has(n, c))) return false;
  const t = tag(n);
  if (t === 'TEXTAREA') return true;
  if (t === 'INPUT') return TEXT_INPUTS.has((n.type ?? '').toLowerCase());
  return !!n.isContentEditable;
}

function isFocusable(n: MenuNode): boolean {
  const t = tag(n);
  if (n.disabled) return false;
  if (t === 'BUTTON' || t === 'SELECT' || t === 'TEXTAREA' || t === 'INPUT' || t === 'SUMMARY') return true;
  if (t === 'A' && attr(n, 'href') !== null) return true;
  return attr(n, 'tabindex') !== null;
}

/** What takes the focus for a row: the row when it can (a tree row), else
 *  the innermost focusable element in it on the path (a card's button). */
function focusWithin(els: MenuNode[], row: MenuNode): MenuNode | null {
  if (isFocusable(row)) return row;
  for (const n of els) {
    if (n === row) break;
    if (isFocusable(n)) return n;
  }
  return null;
}

export function resolveHit(path: readonly unknown[], ctx: HitContext = {}): Hit {
  const els = path.filter(isElement);
  if (ctx.menuOpen || els.some((n) => attr(n, 'role') === 'menu')) return { kind: 'menu' };
  const field = els.find(isTextField);
  if (field) return { kind: 'text', el: field };
  const term = els.find((n) => has(n, 'xterm'));
  if (term) return { kind: 'terminal', el: term };
  const viewer = els.find((n) => has(n, 'monaco-editor'));
  if (viewer) return { kind: 'viewer', el: viewer };
  const at = els[0] ?? null;
  if (at && ctx.selected?.(at)) return { kind: 'selection', el: at };
  // The innermost of the two; on one element, its own menu.
  const n = els.find((x) => !!ctx.own?.(x) || !!attr(x, 'data-target'));
  if (n && ctx.own?.(n)) return { kind: 'own', el: n, focus: focusWithin(els, n) };
  if (n) return { kind: 'row', el: n, target: attr(n, 'data-target')!, focus: focusWithin(els, n) };
  return { kind: 'none', el: at };
}

/** A `data-target`'s kind: `agent` of `agent:7`. */
export function targetKind(target: string): string {
  const i = target.indexOf(':');
  return i < 0 ? target : target.slice(0, i);
}

/** Where the menu opens: the pointer, or for a keyboard open that carries
 *  no point (the Menu key on some layouts), under the element. */
export function menuPoint(
  e: { clientX: number; clientY: number },
  el: { getBoundingClientRect?(): { left: number; bottom: number } } | null,
): { x: number; y: number } {
  if ((e.clientX || e.clientY) || !el?.getBoundingClientRect) return { x: e.clientX, y: e.clientY };
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.bottom };
}
