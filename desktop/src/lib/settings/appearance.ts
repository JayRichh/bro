// Settings > Desktop > Appearance's actions, which the palette offers too:
// import a VS Code theme (an installed one, with swatches, or a file),
// take VS Code's fonts, put Kete's own look back, forget an imported theme,
// and pick a font family from the installed ones. Each writes through the
// Rust side (src-tauri look.rs) and adopts the preferences it answers.

import { api, type DesktopPrefs, type FontFamily, type SettingRow } from '../api';
import { app, toast } from '../store.svelte';
import { ask, type PickItem } from '../ui/ui.svelte';
import { DEFAULT_SIZES, knownTheme, themeLabel } from '../look';
import { look, setLookPrefs } from '../look.svelte';

/** Wear what a write answered: the look's preferences and the theme. */
export function adopt(p: DesktopPrefs) {
  setLookPrefs(p);
  if (knownTheme(p.theme, look.prefs.themes)) app.themePref = p.theme;
}

/** Is the window wearing Kete's own look (so Reset has nothing to do)? */
export function lookIsKetes(): boolean {
  const { fonts, sizes } = look.prefs;
  return (
    app.themePref === 'system' &&
    !fonts.ui &&
    !fonts.mono &&
    !fonts.term &&
    sizes.ui === DEFAULT_SIZES.ui &&
    sizes.term === DEFAULT_SIZES.term &&
    sizes.editor === DEFAULT_SIZES.editor &&
    sizes.line === DEFAULT_SIZES.line
  );
}

const FROM_FILE = '\u0000file';

/** The import list: every installed VS Code theme with its swatches (the
 *  user's first, VS Code's own marked), and From a file beside Cancel.
 *  True when a theme was imported and is worn. */
export async function importVsCodeTheme(): Promise<boolean> {
  const [install, installed] = await Promise.all([
    api.lookVscodeInstall().catch(() => null),
    api.lookInstalledThemes().catch(() => []),
  ]);
  const items: PickItem[] = installed.map((t) => ({
    id: t.name,
    label: t.label,
    hint: t.builtin ? 'built in' : t.light ? 'light' : 'dark',
    swatch: t.swatch,
  }));
  const note = install
    ? `The themes ${install} has installed, mapped onto Kete's colours (text that would not read is lifted). Kept under Theme to pick again.`
    : "VS Code wasn't found here: pick a theme's .json file instead.";
  const id = await ask.pick({ title: 'Import a VS Code theme', note, items, extra: { id: FROM_FILE, label: 'From a file\u2026' } });
  if (id === null) return false;
  try {
    const prefs = id === FROM_FILE ? await api.lookImportFile() : await api.lookImportInstalled(id);
    if (!prefs) return false;
    adopt(prefs);
    toast(`Wearing ${themeLabel(prefs.theme, look.prefs.themes)}: it stays under Theme to pick again.`);
    return true;
  } catch (e) {
    toast(`Could not import the theme: ${e}`);
    return false;
  }
}

/** VS Code's editor and terminal fonts and sizes, taken. */
export async function useVsCodeFonts(): Promise<boolean> {
  try {
    const r = await api.lookUseVscodeFonts();
    adopt(r.prefs);
    toast(r.said);
    return true;
  } catch (e) {
    toast(`Could not take VS Code's fonts: ${e}`);
    return false;
  }
}

/** Kete's own look back, after a confirm that says what changes. */
export async function resetAppearance(): Promise<boolean> {
  const r = await ask.confirm({
    title: 'Reset appearance',
    message: "Put this window's look back to Kete's own?",
    detail: [
      'Theme: Workshop, following the system',
      'Fonts: Geist, Geist Mono, JetBrains Mono',
      'Sizes: interface 14, terminal 13 (line height 1.12), diff 12.5',
      'Imported themes stay, to pick again',
    ],
    confirm: 'Reset appearance',
  });
  if (!r.ok) return false;
  try {
    adopt(await api.lookReset());
    toast("Kete's own look is back.");
    return true;
  } catch (e) {
    toast(`Could not reset the appearance: ${e}`);
    return false;
  }
}

/** Forget the imported theme `id`, after a confirm. */
export async function removeImported(id: string): Promise<boolean> {
  const label = look.prefs.themes[id]?.label ?? id;
  const r = await ask.confirm({
    title: 'Remove imported theme',
    message: `Remove ${label} from the themes? Import it again any time.`,
    confirm: 'Remove',
    danger: true,
  });
  if (!r.ok) return false;
  try {
    adopt(await api.lookRemoveTheme(id));
    return true;
  } catch (e) {
    toast(`Could not remove the theme: ${e}`);
    return false;
  }
}

/** The families the window bundles, always there whatever is installed. */
const BUNDLED: FontFamily[] = [
  { name: 'Geist', mono: false },
  { name: 'Geist Mono', mono: true },
  { name: 'JetBrains Mono', mono: true },
];
let installed: FontFamily[] | null = null;
const KETES = '\u0000kete';

/** The rows a font picker lists: Kete's own first, then every family,
 *  the fixed-pitch ones first for a code or terminal font. */
export function fontItems(families: FontFamily[], placeholder: string, mono: boolean): PickItem[] {
  const all = [...families];
  for (const b of BUNDLED) if (!all.some((f) => f.name === b.name)) all.push(b);
  all.sort((a, b) => (mono ? Number(b.mono) - Number(a.mono) : 0) || a.name.localeCompare(b.name));
  return [
    { id: KETES, label: `Kete's own (${placeholder})`, font: placeholder },
    ...all.map((f) => ({ id: f.name, label: f.name, hint: f.mono ? 'mono' : undefined, font: f.name })),
  ];
}

/** Pick a family for a font row: from the installed ones, or typed where
 *  they can't be listed. The family picked; '' for Kete's own; null when
 *  the picker was put away. */
export async function pickFont(row: SettingRow): Promise<string | null> {
  if (row.control.type !== 'font') return null;
  const { placeholder, mono } = row.control;
  installed ??= await api.systemFonts().catch(() => []);
  if (!installed.length) {
    const r = await ask.prompt({
      title: row.label,
      note: `A font family installed on this machine; empty for Kete's own, ${placeholder}.`,
      fields: [{ key: 'family', label: 'Family', value: String(row.value), placeholder }],
    });
    return r ? r.family.trim() : null;
  }
  const id = await ask.pick({
    title: row.label,
    items: fontItems(installed, placeholder, mono),
    selected: String(row.value) || KETES,
  });
  if (id === null) return null;
  return id === KETES ? '' : id;
}
