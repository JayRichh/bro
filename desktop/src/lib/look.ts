// The window's look, as data: which palette a theme value paints, whether
// it is light, the font stacks and sizes the preferences ask for. Pure, so
// it is tested without a window; look.svelte.ts holds the state and puts
// it on <html>, the terminals and the diff editor.
//
// A theme value is Workshop's `system`, `light` or `dark` (theme.css's own
// blocks), `one-dark` / `one-light` (theme.css's `data-palette` blocks),
// `vscode` (VS Code's current theme, followed live) or `import:<id>` (a
// theme imported from VS Code). The last two arrive as palettes from the
// Rust side (nebula-config look.rs) and are painted as inline variables,
// which beat every block in theme.css.

import type { DesktopPrefs, Palette } from './api';

/** Workshop's three: what the cycle (Ctrl+Shift+L, the rail's tile) steps through. */
export const WORKSHOP = ['system', 'light', 'dark'] as const;
export const IMPORT_PREFIX = 'import:';
/** The interface size the window is drawn at, unscaled. */
export const BASE_UI = 14;
/** The diff editor's line height over its font size (19 at 12.5). */
const EDITOR_LINE = 19 / 12.5;

export interface LookSizes {
  ui: number;
  term: number;
  editor: number;
  line: number;
}
export const DEFAULT_SIZES: LookSizes = { ui: 14, term: 13, editor: 12.5, line: 1.12 };

export interface LookFonts {
  ui: string;
  mono: string;
  term: string;
}

export interface LookPrefs {
  fonts: LookFonts;
  sizes: LookSizes;
  themes: Record<string, Palette>;
}

const num = (word: string | undefined, fallback: number) => {
  const n = Number(word);
  return word && Number.isFinite(n) && n > 0 ? n : fallback;
};

/** The look's part of the window's preferences, defaults filled in. */
export function lookPrefs(p: DesktopPrefs | null | undefined): LookPrefs {
  return {
    fonts: { ui: p?.font_ui?.trim() ?? '', mono: p?.font_mono?.trim() ?? '', term: p?.font_term?.trim() ?? '' },
    sizes: {
      ui: num(p?.ui_size, DEFAULT_SIZES.ui),
      term: num(p?.term_size, DEFAULT_SIZES.term),
      editor: num(p?.editor_size, DEFAULT_SIZES.editor),
      line: num(p?.term_line, DEFAULT_SIZES.line),
    },
    themes: p?.themes ?? {},
  };
}

/** The imported theme's id a theme value names, if it is one. */
export function importedId(theme: string): string | null {
  return theme.startsWith(IMPORT_PREFIX) ? theme.slice(IMPORT_PREFIX.length) : null;
}

/** Is `theme` one the window can paint, given the imported ones? */
export function knownTheme(theme: string, themes: Record<string, Palette>): boolean {
  const id = importedId(theme);
  return id !== null ? id in themes : ['system', 'light', 'dark', 'one-dark', 'one-light', 'vscode'].includes(theme);
}

/** The palette painted inline: VS Code's live one, or an imported one. */
export function paletteOf(theme: string, themes: Record<string, Palette>, live: Palette | null): Palette | null {
  if (theme === 'vscode') return live;
  const id = importedId(theme);
  return id !== null ? (themes[id] ?? null) : null;
}

/** The theme.css palette block to wear: One Dark or One Light, and One
 *  Dark for `vscode` while VS Code's theme can't be read (as the TUI). */
export function presetOf(theme: string, live: Palette | null): 'one-dark' | 'one-light' | null {
  if (theme === 'one-dark' || theme === 'one-light') return theme;
  if (theme === 'vscode' && !live) return 'one-dark';
  return null;
}

/** Does `theme` paint a light window? */
export function themeLight(theme: string, osDark: boolean, themes: Record<string, Palette>, live: Palette | null): boolean {
  switch (theme) {
    case 'light':
    case 'one-light':
      return true;
    case 'dark':
    case 'one-dark':
      return false;
    case 'vscode':
      return live?.light ?? false;
    case 'system':
      return !osDark;
  }
  const imported = paletteOf(theme, themes, live);
  return imported ? imported.light : !osDark;
}

/** What the Theme row and the rail call a theme value. */
export function themeLabel(theme: string, themes: Record<string, Palette>): string {
  const fixed: Record<string, string> = {
    system: 'Workshop, follow system',
    light: 'Workshop light',
    dark: 'Workshop dark',
    'one-dark': 'One Dark',
    'one-light': 'One Light',
    vscode: 'VS Code (live)',
  };
  if (theme in fixed) return fixed[theme];
  const id = importedId(theme);
  return (id !== null && themes[id]?.label) || theme;
}

/** The cycle's next theme: Workshop system, light, dark, round again; any
 *  other theme steps back onto Workshop's first. */
export function nextTheme(theme: string): string {
  const at = (WORKSHOP as readonly string[]).indexOf(theme);
  return WORKSHOP[(at + 1) % WORKSHOP.length];
}

/** A family name, quoted for a CSS font-family list, ahead of `fallback`
 *  (Kete's own stack, so a glyph the family lacks still draws). null for
 *  an empty family: Kete's own stack alone. */
export function fontStack(family: string, fallback: string): string | null {
  const name = family.replace(/["'\\;{}<>]/g, '').trim();
  if (!name) return null;
  return fallback ? `"${name}", ${fallback}` : `"${name}"`;
}

/** The window's zoom: the interface size over the size it is drawn at. */
export function zoomOf(sizes: LookSizes): number {
  return sizes.ui / BASE_UI;
}

/** The terminal's font options. The window's zoom scales CSS pixels, so
 *  the size is taken back down by it: 13 reads as 13 at any interface size. */
export function termFont(sizes: LookSizes): { fontSize: number; lineHeight: number } {
  return { fontSize: round2(sizes.term / zoomOf(sizes)), lineHeight: sizes.line };
}

/** The diff and file viewers' font size and line height, the same way. */
export function editorFont(sizes: LookSizes): { fontSize: number; lineHeight: number } {
  const fontSize = round2(sizes.editor / zoomOf(sizes));
  return { fontSize, lineHeight: Math.round(fontSize * EDITOR_LINE) };
}

const round2 = (n: number) => Math.round(n * 100) / 100;

/** Something an inline palette is written onto: <html>'s style. */
export interface StyleSink {
  setProperty(name: string, value: string): void;
  removeProperty(name: string): string;
}

/** Paint `tokens` as inline custom properties on `style`, clearing the
 *  ones `before` painted that `tokens` does not set (all of them when it
 *  is null: back to theme.css). Returns what is painted now. Only colour
 *  values are written: a palette is data from a file. */
export function paintTokens(style: StyleSink, tokens: Record<string, string> | null, before: readonly string[]): string[] {
  const now: string[] = [];
  for (const [name, value] of Object.entries(tokens ?? {})) {
    if (!/^[a-z0-9-]+$/.test(name) || !/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(value)) continue;
    style.setProperty(`--${name}`, value);
    now.push(name);
  }
  for (const name of before) if (!now.includes(name)) style.removeProperty(`--${name}`);
  return now;
}
