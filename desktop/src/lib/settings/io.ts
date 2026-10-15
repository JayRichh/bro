// Settings export and import (the registry's export_settings and
// import_settings): the system's save and open dialogs, then the `kete
// config export / import` the CLI and the terminal app run (src-tauri
// settings_io.rs). An import is read with nothing written first, and
// only the confirm's yes writes it.

import { api, type ImportPreview } from '../api';
import { refreshLaunchables, refreshRunCommands, toast } from '../store.svelte';
import { ask } from '../ui/ui.svelte';

/** Save dialog, then the export; says where it went. */
export async function exportSettings(): Promise<void> {
  try {
    const said = await api.settingsExport();
    if (said) toast(said.replace(/^settings/, 'Settings'));
  } catch (e) {
    toast(`Could not export the settings: ${e}`);
  }
}

/** The confirm an import puts up: what changes, from the dry run. */
export function importConfirm(p: ImportPreview) {
  const [summary, ...rest] = p.lines;
  return {
    title: 'Import settings',
    message: `${summary ?? 'Nothing changed'}. Nothing is written until you import.`,
    detail: [p.path, ...rest],
    confirm: 'Import',
  };
}

/** Open dialog, the dry run, the confirm, then the import. True when
 *  something was written, so a caller showing the settings reads them
 *  again. */
export async function importSettings(): Promise<boolean> {
  let p: ImportPreview | null;
  try {
    p = await api.settingsImportPreview();
  } catch (e) {
    toast(`Could not read the settings file: ${e}`);
    return false;
  }
  if (!p) return false;
  if (!p.changes) {
    toast(`${p.path} changes nothing here.`);
    return false;
  }
  const r = await ask.confirm(importConfirm(p));
  if (!r.ok) return false;
  try {
    const said = await api.settingsImport(p.path);
    toast(said ? `Imported: ${said}` : 'Settings imported.');
    refreshLaunchables();
    refreshRunCommands();
    return true;
  } catch (e) {
    toast(`Could not import the settings: ${e}`);
    return false;
  }
}
