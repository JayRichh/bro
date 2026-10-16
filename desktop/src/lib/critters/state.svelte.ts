// The critters' switches and commands: the shared `critters` setting
// (config.json, which the terminal app reads too), the ways to find them
// (the verb, the Konami code, the kete mark), the collection log's writes,
// and what the critter menu, the palette and Settings run. Critters.svelte
// owns the engine and attaches it here.

import { api } from '../api';
import { openSettings, toast } from '../store.svelte';
import { keyLabel, keysOf } from '../shortcuts';
import { ask, openMenu, type MenuEntry } from '../ui/ui.svelte';
import { FLAGS, SPECIES } from './cast.gen';
import type { Engine, Size } from './engine';
import { critterApi, names, today, type CritterLog, type CrittersPrefs, type LogNote } from './log';
import { logSheet, openCritterLog } from './sheet.svelte';
import { speciesOf, type Critter, type Note } from './world';

export const critters = $state({
  prefs: { on: false, size: 'medium', often: 'normal', animations: true, debug: false } as CrittersPrefs,
  /** The prefs have been read once. */
  loaded: false,
  /** The local date "Hide critters until tomorrow" was chosen (the log's
   *  `hidden_on`, which the terminal app shares). */
  hiddenOn: null as string | null,
});

let engine: Engine | null = null;
/** Work waiting for the engine to start (a summon asked for while turning on). */
let queued: ((e: Engine) => void)[] = [];

export function attachEngine(e: Engine | null) {
  engine = e;
}

/** Run `f` on the engine now if it's going, else once it starts. */
function withEngine(f: (e: Engine) => void) {
  if (engine && critters.prefs.on && critters.prefs.animations) {
    f(engine);
    engine.poke();
  } else queued.push(f);
}

/** The engine just started: what was asked for meanwhile. */
export function flushQueued(e: Engine) {
  const q = queued;
  queued = [];
  for (const f of q) f(e);
  if (q.length) e.poke();
}

export function hiddenNow(): boolean {
  return critters.hiddenOn === today();
}

export const sizeOf = (s: string): Size => (s === 'small' || s === 'large' ? s : 'medium');

/** Settings and the log, read fresh (at startup and whenever the window
 *  comes back, so a change in the terminal app shows here). */
export async function loadCritters() {
  try {
    critters.prefs = await critterApi.prefs();
  } catch {
    // An older backend: the critters stay off.
  }
  critters.loaded = true;
  try {
    applyLog(await critterApi.log());
  } catch {}
}

function applyLog(log: CritterLog) {
  critters.hiddenOn = log.hidden_on ?? null;
  if (logSheet.open) logSheet.log = log;
  if (engine) engine.world.names = names(log);
}

// ---------- the log ----------

let pending: LogNote[] = [];
let flushTimer: ReturnType<typeof setTimeout> | undefined;

/** The world's notes, folded into critters.json a moment later in one
 *  read-apply-write (both apps write the file). */
export function logNotes(notes: Note[]) {
  pending.push(...notes);
  clearTimeout(flushTimer);
  flushTimer = setTimeout(flushNotes, 800);
}

async function flushNotes(hide?: boolean) {
  const notes = pending;
  pending = [];
  try {
    applyLog(await critterApi.update(notes, hide));
  } catch {
    // An easter egg's tally: a failed write is not worth a toast.
  }
}

// ---------- on and off ----------

const paletteKey = () => keyLabel(keysOf('palette')[0] ?? 'ctrl+k');
const offHint = () => `Turn them off with Critters in the palette (${paletteKey()}) or in Settings, Appearance.`;

async function setOn(on: boolean): Promise<boolean> {
  try {
    await api.settingsSet({ scope: 'global', key: 'critters' }, on);
    critters.prefs = { ...critters.prefs, on };
    return true;
  } catch (e) {
    toast(`Could not save the setting: ${e}`);
    return false;
  }
}

/** Turns the critters on (every way in sets the same shared setting), and
 *  brings them back if they were hidden for the day. */
async function turnOn(): Promise<boolean> {
  if (hiddenNow()) await bringBack();
  if (critters.prefs.on) return true;
  if (!(await setOn(true))) return false;
  toast(
    critters.prefs.animations
      ? `Critters are on: little visitors along the bottom of the window. ${offHint()}`
      : 'Critters are on, but Animations is off (Settings, General), so none will come until it is on.',
  );
  return true;
}

/** The `critters` verb: on or off. */
export async function toggleCritters() {
  if (!critters.prefs.on) return void (await turnOn());
  if (await setOn(false)) toast(`Critters are off. Critters in the palette (${paletteKey()}) or Settings, Appearance brings them back.`);
}

/** Found by the Konami code, or by five clicks on the kete mark (`from`:
 *  the mark's box, where a kiwi climbs out of the basket). Already on,
 *  either still brings a visitor. */
export async function discover(via: 'konami' | 'mark', from?: DOMRect) {
  if (!(await turnOn())) return;
  withEngine((e) => {
    if (via === 'mark' && from) e.summonAt('kiwi', from.left + from.width / 2, from.top + from.height * 0.6);
    else e.summon(pickVisitor(e));
  });
}

/** Someone who could come now, by the spawn weights (the clock's too). */
function pickVisitor(e: Engine): string {
  const w = e.world.spawnWeights(new Date().getHours(), []);
  const total = w.reduce((a, b) => a + b, 0);
  let r = Math.random() * total;
  for (let i = 0; i < w.length; i++) {
    r -= w[i];
    if (w[i] > 0 && r <= 0) return SPECIES[i].id;
  }
  return 'kiwi';
}

/** Hide critters until tomorrow: everyone leaves, nobody comes until the
 *  date changes (in both apps: the day is in the shared log). */
export async function hideUntilTomorrow() {
  critters.hiddenOn = today();
  engine?.world.scatter();
  engine?.poke();
  clearTimeout(flushTimer);
  await flushNotes(true);
  toast(`The critters are hiding until tomorrow. Critters in the palette (${paletteKey()}) brings them back now.`);
}

/** Back today after all. */
export async function bringBack() {
  critters.hiddenOn = null;
  clearTimeout(flushTimer);
  await flushNotes(false);
  if (engine && !engine.world.spawning) {
    engine.world.resume();
    engine.poke();
  }
}

// ---------- the critter menu ----------

const isBalloon = (c: Critter) => (speciesOf(c).flags & FLAGS.inanimate) !== 0;

/** Why a critter can't take a verb that needs it standing, or null. */
function busy(c: Critter): string | null {
  if (c.mode === 'held') return "You're holding it: let go first";
  if (c.mode === 'air') return 'It is still in the air';
  return null;
}

/** The critter menu's rows for `c`, by its state. */
export function critterRows(c: Critter, run: { pet(): void; feed(): void; name(): void; shoo(): void }): MenuEntry[] {
  const s = speciesOf(c);
  const why = busy(c);
  const feedWhy = why ?? (isBalloon(c) ? `A ${s.name.toLowerCase()} doesn't eat` : c.exiting ? 'It is on its way out' : null);
  const shooWhy = why ?? (c.exiting ? 'It is already on its way out' : null);
  const petLabel = s.id === 'balloon' ? 'Pop' : s.special === 'lamp' && c.offering ? 'Take the lamp' : 'Pet';
  const item = (label: string, f: () => void, no: string | null, title?: string) => ({
    label,
    run: f,
    ...(no ? { disabled: true, title: no } : title ? { title } : {}),
  });
  return [
    { heading: `${c.name}, ${s.name.toLowerCase()}` },
    item(petLabel, run.pet, why, 'Or click it'),
    item('Feed', run.feed, feedWhy, 'Or double-click it'),
    item('Name\u2026', run.name, null, `Every ${s.name.toLowerCase()} that visits keeps the name`),
    item('Shoo', run.shoo, shooWhy),
    'sep',
    { label: 'Hide critters until tomorrow', run: () => void hideUntilTomorrow() },
    { label: 'Critter log\u2026', run: () => void openCritterLog() },
    { label: 'Critter settings\u2026', run: () => openSettings('appearance') },
  ];
}

/** The critter menu for critter `id` (the router's `critter:` target). */
export function critterMenu(id: number): MenuEntry[] {
  const e = engine;
  const c = e?.world.get(id);
  if (!e || !c) return [];
  const act = (f: () => void) => () => {
    f();
    e.poke();
  };
  return critterRows(c, {
    pet: act(() => e.world.pet(id)),
    feed: act(() => e.world.feed(id)),
    shoo: act(() => e.world.shoo(id)),
    name: () => void nameCritter(id),
  });
}

/** Right-click on a critter. */
export function critterMenuAt(id: number, x: number, y: number) {
  openMenu(critterMenu(id), { x, y }, 'Critter');
}

async function nameCritter(id: number) {
  const c = engine?.world.get(id);
  if (!c) return;
  const kind = speciesOf(c).name.toLowerCase();
  const v = await ask.prompt({
    title: `Name this ${kind}`,
    note: `Every ${kind} that visits answers to it. Sixteen letters at most.`,
    fields: [{ key: 'name', label: 'Name', value: c.name }],
    confirm: 'Name it',
  });
  if (!v || !engine) return;
  if (engine.world.rename(id, v.name)) engine.poke();
}
