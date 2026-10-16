// The critter menu reads the critter: what it is and what it's doing
// decide each row's words and whether it can be picked. And the pixel
// font sets text the way the crate's font.rs does.
import { describe, expect, it, vi } from 'vitest';

vi.mock('../api', () => ({ api: { settingsSet: vi.fn(async () => {}) } }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));

const { critterRows } = await import('./state.svelte');
const { defaultInput, World } = await import('./world');
const { textHeight, textPixels, textWidth } = await import('./font');

type Row = { label?: string; disabled?: boolean; title?: string; heading?: string };
const noop = { pet() {}, feed() {}, name() {}, shoo() {} };

function critter(species: string) {
  const w = new World(3);
  w.spawning = false;
  const id = w.summon(species, defaultInput())!;
  const c = w.get(id)!;
  c.entered = true;
  return c;
}
const rows = (c: ReturnType<typeof critter>) => critterRows(c, noop) as Row[];
const row = (rs: Row[], label: string) => rs.find((r) => r.label?.startsWith(label));

describe('the critter menu', () => {
  it('names the critter and offers every verb while it stands about', () => {
    const c = critter('kiwi');
    const rs = rows(c);
    expect(rs[0].heading).toBe(`${c.name}, kiwi`);
    for (const l of ['Pet', 'Feed', 'Name', 'Shoo', 'Hide critters until tomorrow', 'Critter log', 'Critter settings']) {
      expect(row(rs, l), l).toBeTruthy();
      expect(row(rs, l)?.disabled, l).toBeFalsy();
    }
  });

  it('greys out what needs it standing while it is held or in the air', () => {
    const c = critter('kiwi');
    for (const mode of ['held', 'air'] as const) {
      c.mode = mode;
      const rs = rows(c);
      for (const l of ['Pet', 'Feed', 'Shoo']) {
        expect(row(rs, l)?.disabled, `${l} ${mode}`).toBe(true);
        expect(row(rs, l)?.title, `${l} ${mode}`).toBeTruthy();
      }
      expect(row(rs, 'Name')?.disabled).toBeFalsy();
    }
  });

  it('pops a balloon, never feeds it, and takes the genie lamp when offered', () => {
    const b = rows(critter('balloon'));
    expect(row(b, 'Pop')).toBeTruthy();
    expect(row(b, 'Feed')?.disabled).toBe(true);
    const g = critter('genie');
    expect(row(rows(g), 'Pet')).toBeTruthy();
    g.offering = true;
    expect(row(rows(g), 'Take the lamp')).toBeTruthy();
  });

  it('lets one on its way out go', () => {
    const c = critter('pig');
    c.exiting = true;
    const rs = rows(c);
    expect(row(rs, 'Feed')?.disabled).toBe(true);
    expect(row(rs, 'Shoo')?.disabled).toBe(true);
    expect(row(rs, 'Pet')?.disabled).toBeFalsy();
  });
});

describe('the pixel font', () => {
  it('sets macrons above the cap, as font.rs does', () => {
    expect(textHeight('kia ora')).toBe(5);
    expect(textHeight('P\u016bkeko')).toBe(7);
    expect(textWidth('A')).toBe(3);
    expect(textWidth('AA')).toBe(7);
    const px = textPixels('\u016b');
    const has = (x: number, y: number) => px.some(([a, b]) => a === x && b === y);
    expect(has(0, 0) && has(2, 0)).toBe(true);
    expect(px.some(([, y]) => y === 1), 'a clear row under the bar').toBe(false);
  });
});
