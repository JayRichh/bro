// The code spells out only when ten keys in a row do, whatever came before
// and in either case; and keys aimed at a text field are not the code.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { KONAMI, konami, typingTarget } from './konami';

/** Feeds `keys`, returning the answer to each. */
function feed(f: (key: string) => boolean, keys: readonly string[]): boolean[] {
  return keys.map((k) => f(k));
}

/** True only on the last key. */
const onlyLast = (n: number) => Array.from({ length: n }, (_, i) => i === n - 1);

describe('konami', () => {
  it('fires on the last key of the plain sequence', () => {
    expect(feed(konami(), KONAMI)).toEqual(onlyLast(10));
  });

  it('a stray key in the middle spoils it', () => {
    const keys = [...KONAMI.slice(0, 5), 'x', ...KONAMI.slice(5)];
    expect(feed(konami(), keys).some(Boolean)).toBe(false);
    // A key from the code in the wrong place does too.
    const swapped = [...KONAMI];
    [swapped[4], swapped[5]] = [swapped[5], swapped[4]];
    expect(feed(konami(), swapped).some(Boolean)).toBe(false);
  });

  it('a repeated prefix still completes', () => {
    const keys = ['ArrowUp', ...KONAMI];
    expect(feed(konami(), keys)).toEqual(onlyLast(11));
    const noise = ['ArrowUp', 'ArrowUp', ...KONAMI];
    expect(feed(konami(), noise)).toEqual(onlyLast(12));
  });

  it('anything typed before it is forgotten', () => {
    const keys = ['h', 'e', 'l', 'l', 'o', 'Enter', ...KONAMI];
    expect(feed(konami(), keys)).toEqual(onlyLast(16));
  });

  it('counts upper-case B and A', () => {
    const keys = [...KONAMI.slice(0, 8), 'B', 'A'];
    expect(feed(konami(), keys)).toEqual(onlyLast(10));
  });

  it('does not mind a held Shift', () => {
    const keys = [...KONAMI.slice(0, 8), 'Shift', 'B', 'A'];
    expect(feed(konami(), keys)).toEqual(onlyLast(11));
  });

  it('matches key names exactly', () => {
    // Only single letters are lower-cased: "ARROWUP" is not "ArrowUp".
    expect(feed(konami(), KONAMI.map((k) => (k.length > 1 ? k.toUpperCase() : k))).some(Boolean)).toBe(false);
  });

  it('a fresh match needs all ten keys again', () => {
    const f = konami();
    expect(feed(f, KONAMI)).toEqual(onlyLast(10));
    // Nine of the ten are not enough.
    expect(feed(f, KONAMI.slice(1)).some(Boolean)).toBe(false);
    // The whole code again is.
    expect(feed(f, KONAMI)).toEqual(onlyLast(10));
    expect(feed(f, KONAMI)).toEqual(onlyLast(10));
  });

  it('feeders are independent', () => {
    const a = konami();
    const b = konami();
    feed(a, KONAMI.slice(0, 9));
    expect(b('a')).toBe(false);
    expect(a('a')).toBe(true);
  });
});

/** Just enough of an Element for `typingTarget`: it matches the simple
 *  selectors it is given, as `closest` would on itself or an ancestor. */
class FakeElement {
  readonly is: string[];
  constructor(is: string[]) {
    this.is = is;
  }
  closest(selector: string): FakeElement | null {
    const parts = selector.split(',').map((s) => s.trim());
    return this.is.some((m) => parts.some((p) => p.startsWith(m))) ? this : null;
  }
}

describe('typingTarget', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('is false without a DOM or a target', () => {
    expect(typingTarget(null)).toBe(false);
    expect(typingTarget({} as EventTarget)).toBe(false);
  });

  it('is true inside inputs, terminals and editors, false elsewhere', () => {
    vi.stubGlobal('Element', FakeElement);
    const el = (...is: string[]) => new FakeElement(is) as unknown as EventTarget;
    for (const sel of ['input', 'textarea', 'select', '[contenteditable]', '.xterm', '.monaco-editor']) {
      expect(typingTarget(el(sel)), sel).toBe(true);
    }
    expect(typingTarget(el('div'))).toBe(false);
    expect(typingTarget(el('button', '.board'))).toBe(false);
    expect(typingTarget(null)).toBe(false);
    expect(typingTarget({} as EventTarget)).toBe(false);
  });
});
