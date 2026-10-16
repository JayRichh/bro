// The critter collection log, window side: the types of <data>/critters.json
// (crates/kete-critters/src/log.rs, which the terminal app shares), a pure
// port of its `record`, and typed wrappers over the Tauri commands in
// src-tauri/src/critters.rs. The file is the truth and the Rust side reads
// it fresh on every change; the copy held here only paints the sheet.

import { invoke } from '@tauri-apps/api/core';

export interface LogEntry {
  seen: number;
  pets: number;
  fed: number;
  /** The name the user gave this species' visitors. */
  name?: string;
  /** Local date of the first visit, `YYYY-MM-DD`. */
  first_seen?: string;
}

export interface CritterLog {
  version: number;
  /** By species id. */
  species: Record<string, LogEntry>;
  /** Genie lamps taken. */
  lamps: number;
  /** The local date "Hide critters until tomorrow" was chosen on. */
  hidden_on?: string;
}

/** What the world noted (`World.drainNotes`), as the Rust side takes it. */
export type LogNote =
  | { kind: 'seen' | 'petted' | 'fed'; species: string }
  | { kind: 'named'; species: string; name: string }
  | { kind: 'lamp' };

/** The Critters settings the overlay reads at startup. */
export interface CrittersPrefs {
  on: boolean;
  /** `small`, `medium` or `large`. */
  size: string;
  /** `rare`, `normal` or `busy`. */
  often: string;
  /** Off means no critters at all. */
  animations: boolean;
  /** A debug build (the e2e test hook). */
  debug?: boolean;
}

export const EMPTY_LOG: CritterLog = { version: 1, species: {}, lamps: 0 };

/** The local date as `YYYY-MM-DD`. Built from the local parts: toISOString
 *  is UTC, and a New Zealand evening (UTC+13) would read as yesterday. */
export function today(d = new Date()): string {
  const y = String(d.getFullYear()).padStart(4, '0');
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

/** Visitors are hidden on `day`. */
export function hiddenToday(log: CritterLog, day = today()): boolean {
  return log.hidden_on === day;
}

/** The given names by species id, for `World.names`. */
export function names(log: CritterLog): Map<string, string> {
  const out = new Map<string, string>();
  for (const [id, e] of Object.entries(log.species)) {
    if (e.name !== undefined) out.set(id, e.name);
  }
  return out;
}

/** Folds a note in, as `CritterLog::record` does; `day` is the local date.
 *  Returns a new log and leaves `log` alone. */
export function record(log: CritterLog, note: LogNote, day: string): CritterLog {
  if (note.kind === 'lamp') return { ...log, lamps: log.lamps + 1 };
  const e: LogEntry = { ...(log.species[note.species] ?? { seen: 0, pets: 0, fed: 0 }) };
  switch (note.kind) {
    case 'seen':
      e.seen += 1;
      e.first_seen ??= day;
      break;
    case 'petted':
      e.pets += 1;
      break;
    case 'fed':
      e.fed += 1;
      break;
    case 'named':
      e.name = note.name;
      break;
  }
  return { ...log, species: { ...log.species, [note.species]: e } };
}

export const critterApi = {
  /** The log as the file has it now. */
  log: () => invoke<CritterLog>('critters_log'),
  /** Record `notes` and optionally hide visitors for today (`hide`: true
   *  hides, false brings them back); resolves to the log as written. */
  update: (notes: LogNote[], hide?: boolean) =>
    invoke<CritterLog>('critters_log_update', { notes, hide: hide ?? null, today: today() }),
  /** The Critters settings. */
  prefs: () => invoke<CrittersPrefs>('critters_prefs'),
};
