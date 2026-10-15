// Where the suite's binaries are: written by prepare.mjs, read by the
// harness.

import fs from 'node:fs';
import path from 'node:path';

export const E2E = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));
/** Downloaded and installed tools, and paths.json: ignored by git. */
export const TOOLS = path.join(E2E, '.tools');
export const PATHS_FILE = path.join(TOOLS, 'paths.json');

let cached = null;

/** `{ kete, agent, desktop, tauriDriver, edgeDriver }`. */
export function paths() {
  if (cached) return cached;
  if (!fs.existsSync(PATHS_FILE)) throw new Error('run `npm run e2e` (or `node e2e/prepare.mjs`) first: no e2e/.tools/paths.json');
  cached = JSON.parse(fs.readFileSync(PATHS_FILE, 'utf8'));
  for (const [name, p] of Object.entries(cached)) {
    if (!fs.existsSync(p)) throw new Error(`e2e ${name} is missing: ${p} (run node e2e/prepare.mjs)`);
  }
  return cached;
}
