// Where a dialog puts the focus when it opens: the first text field, else
// the element marked data-autofocus (the primary button). Never a checkbox
// or radio: Enter or Space on a focused "Force" box would tick a
// destructive option the user never chose.

export const FOCUS_ORDER = ['input:not([type="checkbox"]):not([type="radio"]), textarea', '[data-autofocus]'];

export function firstFocus(root: { querySelector(sel: string): Element | null }): HTMLElement | null {
  for (const sel of FOCUS_ORDER) {
    const el = root.querySelector(sel);
    if (el) return el as HTMLElement;
  }
  return null;
}

// Where the focus goes when the last layer (a menu, a dialog, the palette,
// the help) closes: back to what had it before, the row, card or button
// that opened it, so the row keys keep working after Esc. Not when that
// is gone from the page, nor when something else took the focus since.

/** A layer: its focus never counts as "where the user was". */
export const LAYER_SEL = '[aria-modal="true"], [role="menu"]';

type Focusable = { isConnected: boolean; focus(): void };

export function focusBack<T extends Focusable>(last: T | null, active: unknown, body: unknown): T | null {
  if (!last || !last.isConnected) return null;
  if (active && active !== body) return null;
  return last;
}
