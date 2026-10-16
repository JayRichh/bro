import { describe, expect, it } from 'vitest';
import type { Palette } from './api';
import {
  DEFAULT_SIZES,
  editorFont,
  fontStack,
  importedId,
  knownTheme,
  lookPrefs,
  nextTheme,
  paintTokens,
  paletteOf,
  presetOf,
  termFont,
  themeLabel,
  themeLight,
  zoomOf,
  type StyleSink,
} from './look';

const paper: Palette = { label: 'Paper', light: true, tokens: { 'color-bg': '#fdfdf8', 'color-ink': '#222222' } };
const night: Palette = { label: 'Atom One Dark', light: false, tokens: { 'color-bg': '#282c34', 'diff-add-word': '#00809b33' } };
const themes = { paper, 'atom-one-dark': night };

/** A style that records what is painted on it. */
function sink(): StyleSink & { props: Map<string, string> } {
  const props = new Map<string, string>();
  return {
    props,
    setProperty: (n, v) => void props.set(n, v),
    removeProperty: (n) => {
      const v = props.get(n) ?? '';
      props.delete(n);
      return v;
    },
  };
}

describe('the theme a value paints', () => {
  it('reads light or dark off Workshop, One, VS Code and imports', () => {
    expect(themeLight('system', true, themes, null)).toBe(false);
    expect(themeLight('system', false, themes, null)).toBe(true);
    expect(themeLight('light', true, themes, null)).toBe(true);
    expect(themeLight('one-dark', false, themes, null)).toBe(false);
    expect(themeLight('one-light', true, themes, null)).toBe(true);
    expect(themeLight('vscode', false, themes, null)).toBe(false);
    expect(themeLight('vscode', true, themes, paper)).toBe(true);
    expect(themeLight('import:paper', true, themes, null)).toBe(true);
    expect(themeLight('import:atom-one-dark', false, themes, null)).toBe(false);
    expect(themeLight('import:gone', true, themes, null), 'a forgotten import follows the OS').toBe(false);
  });

  it('paints One from theme.css, and One Dark for VS Code until it is read', () => {
    expect(presetOf('one-dark', null)).toBe('one-dark');
    expect(presetOf('one-light', null)).toBe('one-light');
    expect(presetOf('vscode', null)).toBe('one-dark');
    expect(presetOf('vscode', night)).toBeNull();
    expect(presetOf('dark', null)).toBeNull();
    expect(presetOf('import:paper', null)).toBeNull();
  });

  it('paints an inline palette only for VS Code and imports', () => {
    expect(paletteOf('vscode', themes, night)).toBe(night);
    expect(paletteOf('vscode', themes, null)).toBeNull();
    expect(paletteOf('import:paper', themes, night)).toBe(paper);
    expect(paletteOf('import:gone', themes, null)).toBeNull();
    expect(paletteOf('system', themes, night)).toBeNull();
    expect(importedId('import:paper')).toBe('paper');
    expect(importedId('one-dark')).toBeNull();
  });

  it('knows which values it can paint', () => {
    for (const t of ['system', 'light', 'dark', 'one-dark', 'one-light', 'vscode', 'import:paper']) expect(knownTheme(t, themes), t).toBe(true);
    for (const t of ['purple', 'import:gone', '']) expect(knownTheme(t, themes), t).toBe(false);
  });

  it('labels themes as the Theme row does, imports by name', () => {
    expect(themeLabel('system', themes)).toBe('Workshop, follow system');
    expect(themeLabel('vscode', themes)).toBe('VS Code (live)');
    expect(themeLabel('import:atom-one-dark', themes)).toBe('Atom One Dark');
    expect(themeLabel('import:gone', themes)).toBe('import:gone');
  });

  it("cycles Workshop's three, and steps back onto Workshop from any other", () => {
    expect(nextTheme('system')).toBe('light');
    expect(nextTheme('light')).toBe('dark');
    expect(nextTheme('dark')).toBe('system');
    expect(nextTheme('one-dark')).toBe('system');
    expect(nextTheme('import:paper')).toBe('system');
  });
});

describe('painting a palette', () => {
  it('sets every colour, then clears what the next one does not set', () => {
    const s = sink();
    let painted = paintTokens(s, night.tokens, []);
    expect(s.props.get('--color-bg')).toBe('#282c34');
    expect(s.props.get('--diff-add-word')).toBe('#00809b33');
    painted = paintTokens(s, paper.tokens, painted);
    expect(s.props.get('--color-bg')).toBe('#fdfdf8');
    expect(s.props.has('--diff-add-word'), 'cleared').toBe(false);
    painted = paintTokens(s, null, painted);
    expect(s.props.size, 'back to theme.css').toBe(0);
    expect(painted).toEqual([]);
  });

  it('writes colours only: a palette is data from a file', () => {
    const s = sink();
    const painted = paintTokens(s, { 'color-bg': 'red; background: url(x)', 'Color-Ink': '#000000', 'color-ink': '#123', 'color-rule': '#a1b2c3' }, []);
    expect(painted).toEqual(['color-rule']);
    expect([...s.props.keys()]).toEqual(['--color-rule']);
  });
});

describe('fonts and sizes', () => {
  it("quotes a family ahead of Kete's own stack, and is nothing when empty", () => {
    expect(fontStack('JetBrains Mono', "'Geist Mono', monospace")).toBe(`"JetBrains Mono", 'Geist Mono', monospace`);
    expect(fontStack('  ', 'x')).toBeNull();
    expect(fontStack('a"; } b', '')).toBe('"a  b"');
  });

  it('reads the preferences, defaults for what is unset or off', () => {
    const p = lookPrefs({ theme: 'system', diff_mode: 'side', home: 'overview', font_term: ' JetBrains Mono ', term_size: '15', ui_size: 'big' });
    expect(p.fonts).toEqual({ ui: '', mono: '', term: 'JetBrains Mono' });
    expect(p.sizes).toEqual({ ...DEFAULT_SIZES, term: 15 });
    expect(lookPrefs(null).sizes).toEqual(DEFAULT_SIZES);
  });

  it('scales the window by the interface size and keeps the terminal and diff sizes what they say', () => {
    expect(zoomOf(DEFAULT_SIZES)).toBe(1);
    expect(termFont(DEFAULT_SIZES)).toEqual({ fontSize: 13, lineHeight: 1.12 });
    expect(editorFont(DEFAULT_SIZES)).toEqual({ fontSize: 12.5, lineHeight: 19 });
    const big = { ...DEFAULT_SIZES, ui: 16, term: 16 };
    expect(zoomOf(big)).toBeCloseTo(16 / 14);
    // 14 CSS px at a zoom of 16/14 is 16 on screen.
    expect(termFont(big).fontSize).toBe(14);
    expect(editorFont({ ...DEFAULT_SIZES, editor: 15 })).toEqual({ fontSize: 15, lineHeight: 23 });
  });
});
