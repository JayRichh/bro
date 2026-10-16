import { describe, expect, it, vi } from 'vitest';

vi.mock('../api', () => ({ api: {} }));
vi.mock('../store.svelte', () => ({ app: { themePref: 'system' }, toast: () => {} }));

import { fontItems } from './appearance';

describe('the font picker', () => {
  const families = [
    { name: 'Segoe UI', mono: false },
    { name: 'Consolas', mono: true },
    { name: 'Arial', mono: false },
    { name: 'JetBrains Mono', mono: true },
  ];

  it("lists Kete's own first, then every family by name, the bundled ones too", () => {
    const items = fontItems(families, 'Geist', false);
    expect(items[0]).toMatchObject({ label: "Kete's own (Geist)", font: 'Geist' });
    expect(items.slice(1).map((i) => i.label)).toEqual(['Arial', 'Consolas', 'Geist', 'Geist Mono', 'JetBrains Mono', 'Segoe UI']);
    expect(items.find((i) => i.label === 'Consolas')?.hint).toBe('mono');
  });

  it('puts the fixed-pitch families first for a code or terminal font', () => {
    const items = fontItems(families, 'JetBrains Mono', true);
    expect(items.slice(1).map((i) => i.label)).toEqual(['Consolas', 'Geist Mono', 'JetBrains Mono', 'Arial', 'Geist', 'Segoe UI']);
    expect(items.filter((i) => i.label === 'JetBrains Mono')).toHaveLength(1);
  });
});
