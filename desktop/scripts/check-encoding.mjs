// Keep the frontend's sources plain ASCII. Glyphs go in as escapes
// (`&minus;` in markup, `…` in script), so no editor or shell that
// reads a file in the wrong code page can turn them into mojibake.
// Runs before every build; exits 1 and names the offending lines.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const EXT = new Set(['.svelte', '.ts', '.css', '.html', '.mjs']);
const bad = [];

function walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else if (EXT.has(path.extname(e.name))) check(p);
  }
}

function check(file) {
  const bytes = fs.readFileSync(file);
  if (bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) bad.push(`${file}: UTF-8 BOM`);
  bytes
    .toString('utf8')
    .split('\n')
    .forEach((line, i) => {
      if (/[^\x00-\x7f]/.test(line)) bad.push(`${path.relative(ROOT, file)}:${i + 1}: ${line.trim()}`);
    });
}

walk(path.join(ROOT, 'src'));
check(path.join(ROOT, 'index.html'));
if (bad.length) {
  console.error('non-ASCII in frontend sources (use an escape instead):\n' + bad.join('\n'));
  process.exit(1);
}
