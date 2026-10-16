// The Critter log sheet's state: whether it is open, and the log it shows,
// read from the file each time it opens (the terminal app writes the same
// file, so a copy kept from earlier would be stale).

import { critterApi, EMPTY_LOG, type CritterLog } from './log';

export const logSheet = $state({
  open: false,
  log: EMPTY_LOG as CritterLog,
  /** The log has been read since the sheet opened: until then the body
   *  stays empty, so every species doesn't flash as unseen. */
  loaded: false,
});

/** Open the sheet and read the log. A log that can't be read shows as empty. */
export async function openCritterLog() {
  logSheet.open = true;
  logSheet.loaded = false;
  try {
    logSheet.log = await critterApi.log();
  } catch {
    logSheet.log = EMPTY_LOG;
  }
  logSheet.loaded = true;
}

export function closeCritterLog() {
  logSheet.open = false;
}
