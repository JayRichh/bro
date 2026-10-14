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
