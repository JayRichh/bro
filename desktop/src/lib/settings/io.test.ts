import { beforeEach, describe, expect, it, vi } from 'vitest';

const calls: string[] = [];
let preview: { path: string; lines: string[]; changes: boolean } | null = null;
vi.mock('../api', () => ({
  api: {
    settingsImportPreview: async () => preview,
    settingsImport: async (path: string) => {
      calls.push(`import ${path}`);
      return 'config: 1 key changed';
    },
    settingsExport: async () => 'settings written to /d/nebula-settings.json (2 files)',
    launchables: async () => ({}),
    runCommands: async () => ({}),
    accounts: async () => [],
  },
}));

const { app } = await import('../store.svelte');
const { ui } = await import('../ui/ui.svelte');
const io = await import('./io');

beforeEach(() => {
  calls.length = 0;
  ui.dialog = null;
  app.toast = null;
});

async function answer(ok: boolean) {
  for (let i = 0; i < 50 && !ui.dialog; i++) await Promise.resolve();
  const d = ui.dialog;
  if (d?.kind !== 'confirm') throw new Error('no confirm up');
  d.resolve({ ok, checked: false });
  ui.dialog = null;
}

describe('settings export and import', () => {
  it('says what an import changes and writes only on yes', async () => {
    preview = { path: '/b.json', lines: ['config: 1 key changed', 'config.json: theme'], changes: true };
    const no = io.importSettings();
    for (let i = 0; i < 50 && !ui.dialog; i++) await Promise.resolve();
    expect(ui.dialog?.kind === 'confirm' && ui.dialog.detail).toEqual(['/b.json', 'config.json: theme']);
    await answer(false);
    expect(await no).toBe(false);
    expect(calls).toEqual([]);

    const yes = io.importSettings();
    await answer(true);
    expect(await yes).toBe(true);
    expect(calls).toEqual(['import /b.json']);
  });

  it('a file that changes nothing asks nothing', async () => {
    preview = { path: '/same.json', lines: ['nothing changed'], changes: false };
    expect(await io.importSettings()).toBe(false);
    expect(ui.dialog).toBeNull();
    expect(app.toast).toContain('changes nothing');
  });

  it('an export says where it went', async () => {
    await io.exportSettings();
    expect(app.toast).toBe('Settings written to /d/nebula-settings.json (2 files)');
  });
});
