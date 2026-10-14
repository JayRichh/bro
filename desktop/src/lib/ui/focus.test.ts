import { describe, expect, it } from 'vitest';
import { firstFocus } from './focus';

// A stand-in for the dialog box: elements in document order, matched the
// way the two selectors in FOCUS_ORDER read.
function box(els: { id: string; tag: string; type?: string; autofocus?: boolean }[]) {
  return {
    querySelector(sel: string) {
      const hit = els.find((e) =>
        sel.startsWith('[data-autofocus]')
          ? !!e.autofocus
          : e.tag === 'textarea' || (e.tag === 'input' && e.type !== 'checkbox' && e.type !== 'radio'),
      );
      return (hit ?? null) as unknown as Element | null;
    },
  };
}

describe('firstFocus', () => {
  it('takes the primary button over a Force checkbox', () => {
    const el = firstFocus(box([{ id: 'force', tag: 'input', type: 'checkbox' }, { id: 'delete', tag: 'button', autofocus: true }]));
    expect((el as unknown as { id: string }).id).toBe('delete');
  });
  it('takes a text field first', () => {
    const el = firstFocus(box([{ id: 'name', tag: 'input' }, { id: 'ok', tag: 'button', autofocus: true }]));
    expect((el as unknown as { id: string }).id).toBe('name');
  });
});
