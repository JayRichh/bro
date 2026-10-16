// The palettes theme.css writes by hand hold the contrast rules the
// repair holds a VS Code theme to (nebula-config look.rs): body text at
// WCAG AA (4.5:1) on the canvas and the surface, the faint tier, the
// status colours and the syntax at 3:1 on the canvas.

import { describe, expect, it } from 'vitest';

// Read as written, from disk: through Vite, `?raw` hands back the CSS
// after Tailwind has compiled its `@theme` block away. (Named by a
// variable so the type check, which has no Node types, leaves it be.)
const fsModule = 'node:fs';
const { readFileSync } = (await import(/* @vite-ignore */ fsModule)) as { readFileSync: (path: URL, encoding: 'utf8') => string };
const css = readFileSync(new URL('../theme.css', import.meta.url), 'utf8');

/** The `--color-*` hex tokens of the block that opens with `selector`. */
function block(selector: string): Record<string, string> {
  const at = css.indexOf(selector);
  if (at < 0) throw new Error(`no block ${selector}`);
  const body = css.slice(css.indexOf('{', at) + 1, css.indexOf('}', at));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/--color-([a-z-]+):\s*(#[0-9a-fA-F]{6});/g)) out[m[1]] = m[2];
  return out;
}

function luminance(hex: string): number {
  const ch = (i: number) => {
    const v = parseInt(hex.slice(i, i + 2), 16) / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * ch(1) + 0.7152 * ch(3) + 0.0722 * ch(5);
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const PALETTES: [string, string][] = [
  ['Workshop light', '@theme static {'],
  ['Workshop dark', "html[data-theme='dark'] {"],
  ['One Dark', "html[data-palette='one-dark'][data-theme] {"],
  ['One Light', "html[data-palette='one-light'][data-theme] {"],
];

describe.each(PALETTES)('%s', (_name, selector) => {
  const t = block(selector);
  const on = (token: string, ground: string, min: number) =>
    expect(contrast(t[token], t[ground]), `${token} ${t[token]} on ${ground} ${t[ground]}`).toBeGreaterThanOrEqual(min);

  it('reads its body text at AA on the canvas and the surface', () => {
    for (const token of ['ink', 'muted']) for (const ground of ['bg', 'surface']) on(token, ground, 4.5);
    on('ember-ink', 'bg', 4.5);
    on('term-fg', 'term-bg', 4.5);
  });

  it('reads its faint tier, statuses and syntax at 3:1 on the canvas and the surface', () => {
    for (const token of ['faint', 'done', 'fail', 'merged', 'unread', 'git-modified', 'git-added', 'git-deleted', 'git-renamed', 'git-conflict'])
      for (const ground of ['bg', 'surface']) on(token, ground, 3);
    for (const token of ['syn-comment', 'syn-keyword', 'syn-string', 'syn-number', 'syn-type', 'syn-tag', 'syn-attr', 'syn-function', 'syn-regexp']) on(token, 'bg', 3);
  });

  it('writes on an ember fill in a colour that reads', () => {
    on('on-ember', 'ember', 4.5);
  });
});
