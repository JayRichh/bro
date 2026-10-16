// The window's look, live: the preferences it was painted from, VS Code's
// theme while `vscode` is worn, and `paint`, which puts it all on <html>.
// The terminals and the diff editor read the result through theme.ts and
// re-read it when `look.seq` moves (App.svelte bumps it after a paint).

import { api, type DesktopPrefs, type Palette } from './api';
import { editorFont, fontStack, lookPrefs, paintTokens, paletteOf, presetOf, termFont, themeLight, type LookPrefs } from './look';

export const look = $state({
  prefs: lookPrefs(null) as LookPrefs,
  /** VS Code's current theme, while `vscode` is worn; null until read, or
   *  when it can't be (One Dark stands in). */
  live: null as Palette | null,
  /** Why VS Code's theme isn't worn, for the Appearance group to say. */
  liveProblem: null as string | null,
  /** The settings.json stamp `live` was read at. */
  liveStamp: null as number | null,
  /** Bumped after every paint: colours or fonts may have moved. */
  seq: 0,
});

/** Take the look's preferences from a fresh read of the window's own. */
export function setLookPrefs(p: DesktopPrefs | null | undefined) {
  look.prefs = lookPrefs(p);
}

/** Change one font or size word, as its Settings row just saved it. */
export function setLookWord(key: string, value: string) {
  const fonts = { ...look.prefs.fonts };
  const sizes = { ...look.prefs.sizes };
  const n = Number(value);
  if (key === 'font_ui') fonts.ui = value;
  else if (key === 'font_mono') fonts.mono = value;
  else if (key === 'font_term') fonts.term = value;
  else if (key === 'ui_size' && n > 0) sizes.ui = n;
  else if (key === 'term_size' && n > 0) sizes.term = n;
  else if (key === 'editor_size' && n > 0) sizes.editor = n;
  else if (key === 'term_line' && n > 0) sizes.line = n;
  else return;
  look.prefs = { ...look.prefs, fonts, sizes };
}

/** Kete's own font stacks, as theme.css has them, read once before any
 *  family is laid over them. */
const kete: Partial<Record<'ui' | 'mono' | 'term', string>> = {};
function keteStack(kind: 'ui' | 'mono' | 'term'): string {
  const known = kete[kind];
  if (known !== undefined) return known;
  const root = document.documentElement;
  const name = `--font-${kind}`;
  const held = root.style.getPropertyValue(name);
  if (held) root.style.removeProperty(name);
  const stack = getComputedStyle(root).getPropertyValue(name).trim();
  if (held) root.style.setProperty(name, held);
  kete[kind] = stack;
  return stack;
}

/** The tokens the last paint wrote inline, for the next to clear. */
let painted: string[] = [];

/** Put `theme` on <html>: light or dark, One's block or an inline palette,
 *  the font families. Answers whether the window is now dark. */
export function paint(theme: string, osDark: boolean): boolean {
  const root = document.documentElement;
  const { themes, fonts } = look.prefs;
  const live = theme === 'vscode' ? look.live : null;
  const light = themeLight(theme, osDark, themes, live);
  root.dataset.theme = light ? 'light' : 'dark';
  const preset = presetOf(theme, live);
  if (preset) root.dataset.palette = preset;
  else delete root.dataset.palette;
  painted = paintTokens(root.style, paletteOf(theme, themes, live)?.tokens ?? null, painted);
  for (const kind of ['ui', 'mono', 'term'] as const) {
    const stack = fontStack(fonts[kind], keteStack(kind));
    if (stack) root.style.setProperty(`--font-${kind}`, stack);
    else root.style.removeProperty(`--font-${kind}`);
  }
  return !light;
}

/** Read VS Code's theme again: `force` reads it whatever the stamp says
 *  (the OS flipped light or dark, which VS Code's auto-detect follows). */
export async function refreshLive(osLight: boolean, force = false) {
  try {
    const r = await api.lookVscode(osLight, force ? null : look.liveStamp);
    if (!r.changed && !force) return;
    look.live = r.palette;
    look.liveProblem = r.problem;
    look.liveStamp = r.stamp;
  } catch (e) {
    look.liveProblem = String(e);
  }
}

/** The terminals' font options now. */
export function termOptions() {
  return termFont(look.prefs.sizes);
}

/** The diff and file viewers' font size and line height now. */
export function editorOptions() {
  return editorFont(look.prefs.sizes);
}
