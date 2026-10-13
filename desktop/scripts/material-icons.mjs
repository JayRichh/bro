// Build the file-icon table from the material-icon-theme package (MIT):
// a compact JSON map (file name / extension -> icon) for the app, and the
// SVGs it names copied into public/ so Vite serves them from disk. Nothing
// is fetched at runtime.

import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PKG = path.dirname(require.resolve('material-icon-theme/package.json'));
const manifest = JSON.parse(
  fs.readFileSync(path.join(PKG, 'dist', 'material-icons.json'), 'utf8'),
);

const OUT_SVG = path.join(ROOT, 'public', 'material-icons');
const OUT_MAP = path.join(ROOT, 'src', 'lib', 'material-map.json');

const used = new Set([manifest.file]);
const pick = (table = {}) => {
  for (const icon of Object.values(table)) used.add(icon);
  return table;
};

const map = {
  file: manifest.file,
  fileNames: pick(manifest.fileNames),
  fileExtensions: pick(manifest.fileExtensions),
  light: {
    fileNames: pick(manifest.light?.fileNames),
    fileExtensions: pick(manifest.light?.fileExtensions),
  },
};

fs.mkdirSync(OUT_SVG, { recursive: true });
let copied = 0;
for (const name of used) {
  const def = manifest.iconDefinitions[name];
  if (!def) continue;
  const src = path.resolve(PKG, 'dist', def.iconPath);
  const dest = path.join(OUT_SVG, `${name}.svg`);
  if (!fs.existsSync(src)) continue;
  if (!fs.existsSync(dest) || fs.statSync(dest).mtimeMs < fs.statSync(src).mtimeMs) {
    fs.copyFileSync(src, dest);
  }
  copied++;
}
fs.writeFileSync(OUT_MAP, JSON.stringify(map));
console.log(`material icons: ${copied} file icons, map at ${path.relative(ROOT, OUT_MAP)}`);
