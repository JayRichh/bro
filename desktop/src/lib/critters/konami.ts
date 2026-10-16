// The old cheat code, for the critters' secret: a feeder of key presses that
// says when the last ten spell it out, and the check that keeps it from
// listening while you type somewhere. Pure (no listeners), so the node
// tests can run it.

export const KONAMI = ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'b', 'a'] as const;

/** Held on their own while another key is typed: not part of the code. */
const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'CapsLock']);

/** Feed it each `KeyboardEvent.key`; it answers true on the key that
 *  completes the code. It keeps the last ten keys, so a stray repeat at the
 *  start (up up up down down ...) doesn't spoil it, and starts over after a
 *  match. Letters count in either case. */
export function konami(): (key: string) => boolean {
  let last: string[] = [];
  return (key) => {
    if (MODIFIERS.has(key)) return false;
    last.push(key.length === 1 ? key.toLowerCase() : key);
    if (last.length > KONAMI.length) last.shift();
    if (last.length === KONAMI.length && last.every((k, i) => k === KONAMI[i])) {
      last = [];
      return true;
    }
    return false;
  };
}

/** Keys aimed at a text field, a terminal or the editor aren't the code. */
export function typingTarget(t: EventTarget | null): boolean {
  if (typeof Element === 'undefined' || !(t instanceof Element)) return false;
  return t.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"]), .xterm, .monaco-editor') !== null;
}
