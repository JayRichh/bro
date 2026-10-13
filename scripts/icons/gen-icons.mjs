#!/usr/bin/env node
// Generate crates/nebula-tui/src/icons_table.rs: the FILE ICONS table, a
// compact cut of Material Icon Theme's associations (file names, file
// extensions, folder names) mapped to Nerd Font glyphs, each colored with
// the fill of the material icon's own SVG.
//
//   node scripts/icons/gen-icons.mjs <material-icon-theme dir> <glyphnames.json>
//
// <material-icon-theme dir> is an installed copy of the VS Code extension
// (`~/.vscode/extensions/pkief.material-icon-theme-<version>`, MIT, by
// Philipp Kief) — its `dist/material-icons.json` and `icons/*.svg`.
// <glyphnames.json> is nerd-fonts' own glyph list
// (https://github.com/ryanoasis/nerd-fonts/blob/master/glyphnames.json),
// which turns the glyph names below into codepoints, so no codepoint is
// typed by hand.
//
// Only the material icons named in GLYPHS make the table: those are the
// ones a Nerd Font has a fitting glyph for. Anything else falls back to the
// generic file or folder icon at run time.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [materialDir, glyphFile] = process.argv.slice(2);
if (!materialDir || !glyphFile) {
  console.error('usage: gen-icons.mjs <material-icon-theme dir> <glyphnames.json>');
  process.exit(2);
}
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'crates', 'nebula-tui', 'src', 'icons_table.rs');

const material = JSON.parse(fs.readFileSync(path.join(materialDir, 'dist', 'material-icons.json'), 'utf8'));
const glyphs = JSON.parse(fs.readFileSync(glyphFile, 'utf8'));
// Its MIT notice, word for word (wrapped), for the generated file's header.
const licenseFile = ['LICENSE.md', 'LICENSE']
  .map((f) => path.join(materialDir, f))
  .find((f) => fs.existsSync(f));
const wrap = (line, width = 72) => {
  const out = [];
  let cur = '';
  for (const word of line.split(' ')) {
    if (cur && cur.length + 1 + word.length > width) (out.push(cur), (cur = word));
    else cur = cur ? `${cur} ${word}` : word;
  }
  return [...out, cur];
};
const license =
  fs
    .readFileSync(licenseFile, 'utf8')
    .trimEnd()
    .split(/\r?\n/)
    .flatMap((l) => (l ? wrap(l).map((w) => `//! > ${w}`) : ['//! >']))
    .join('\n') + '\n';
const version = JSON.parse(fs.readFileSync(path.join(materialDir, 'package.json'), 'utf8')).version;

// material icon name -> [nerd glyph name, ascii tag]
const GLYPHS = {
  // the fallbacks
  file: ['seti-default', '  '],
  folder: ['md-folder', ''],
  'folder-open': ['md-folder_open', ''],
  // languages
  rust: ['seti-rust', 'rs'],
  typescript: ['seti-typescript', 'ts'],
  react_ts: ['seti-react', 'tx'],
  react: ['seti-react', 'jx'],
  javascript: ['seti-javascript', 'js'],
  python: ['seti-python', 'py'],
  go: ['seti-go', 'go'],
  go_gopher: ['seti-go', 'go'],
  'go-mod': ['seti-go', 'go'],
  c: ['seti-c', 'c '],
  h: ['seti-c', 'h '],
  cpp: ['seti-cpp', 'c+'],
  hpp: ['seti-cpp', 'h+'],
  csharp: ['seti-c_sharp', 'c#'],
  java: ['seti-java', 'jv'],
  kotlin: ['seti-kotlin', 'kt'],
  scala: ['seti-scala', 'sc'],
  swift: ['seti-swift', 'sw'],
  ruby: ['seti-ruby', 'rb'],
  php: ['seti-php', 'ph'],
  lua: ['seti-lua', 'lu'],
  dart: ['seti-dart', 'dt'],
  zig: ['seti-zig', 'zg'],
  haskell: ['seti-haskell', 'hs'],
  elixir: ['seti-elixir', 'ex'],
  elm: ['seti-elm', 'el'],
  clojure: ['seti-clojure', 'cl'],
  julia: ['seti-julia', 'jl'],
  r: ['seti-r', 'r '],
  perl: ['seti-perl', 'pl'],
  ocaml: ['seti-ocaml', 'ml'],
  nim: ['seti-nim', 'nm'],
  crystal: ['seti-crystal', 'cr'],
  vue: ['seti-vue', 'vu'],
  svelte: ['seti-svelte', 'sv'],
  html: ['seti-html', '<>'],
  css: ['seti-css', 'cs'],
  sass: ['seti-sass', 'sa'],
  less: ['seti-less', 'le'],
  console: ['seti-shell', '$ '],
  powershell: ['seti-powershell', 'ps'],
  database: ['seti-db', 'db'],
  graphql: ['seti-graphql', 'gq'],
  terraform: ['seti-terraform', 'tf'],
  nix: ['md-nix', 'nx'],
  webassembly: ['seti-wasm', 'wa'],
  tex: ['seti-tex', 'tx'],
  // data and docs
  json: ['seti-json', '{}'],
  yaml: ['seti-yml', 'ym'],
  xml: ['seti-xml', '<>'],
  settings: ['seti-config', 'cf'],
  tune: ['seti-config', 'cf'],
  markdown: ['seti-markdown', 'md'],
  readme: ['seti-info', 'i '],
  document: ['md-file_document', 'tx'],
  todo: ['seti-todo', 'td'],
  table: ['seti-csv', 'cv'],
  pdf: ['seti-pdf', 'pd'],
  word: ['seti-word', 'dc'],
  excel: ['seti-xls', 'xl'],
  powerpoint: ['md-file_powerpoint', 'pp'],
  lock: ['md-lock', 'lk'],
  key: ['md-key', 'ky'],
  certificate: ['md-certificate', 'ce'],
  // media
  image: ['seti-image', 'im'],
  svg: ['seti-svg', 'sg'],
  font: ['seti-font', 'ft'],
  video: ['seti-video', 'vd'],
  audio: ['seti-audio', 'au'],
  favicon: ['seti-favicon', 'fv'],
  zip: ['md-zip_box', 'zp'],
  // tooling
  git: ['seti-git', 'gt'],
  docker: ['seti-docker', 'dk'],
  makefile: ['seti-makefile', 'mk'],
  nodejs: ['md-nodejs', 'nj'],
  npm: ['seti-npm', 'np'],
  yarn: ['seti-yarn', 'yn'],
  editorconfig: ['seti-editorconfig', 'ec'],
  tsconfig: ['seti-tsconfig', 'tc'],
  eslint: ['seti-eslint', 'es'],
  webpack: ['seti-webpack', 'wp'],
  babel: ['seti-babel', 'bb'],
  gradle: ['seti-gradle', 'gr'],
  maven: ['seti-maven', 'mv'],
  'test-ts': ['md-flask', 'te'],
  'test-js': ['md-flask', 'te'],
  'test-jsx': ['md-flask', 'te'],
  vscode: ['md-microsoft_visual_studio_code', 'vs'],
  // folders: one folder glyph in the folder's own color, a few with a
  // glyph of their own
  'folder-git': ['seti-git', ''],
  'folder-github': ['md-github', ''],
  'folder-node': ['md-nodejs', ''],
  'folder-vscode': ['md-microsoft_visual_studio_code', ''],
  'folder-config': ['md-folder_cog', ''],
  'folder-docs': ['md-folder_text', ''],
  'folder-images': ['md-folder_image', ''],
  'folder-test': ['md-folder', ''],
  'folder-src': ['md-folder', ''],
  'folder-dist': ['md-folder', ''],
  'folder-scripts': ['md-folder', ''],
  'folder-target': ['md-folder', ''],
  'folder-lib': ['md-folder', ''],
  'folder-public': ['md-folder', ''],
  'folder-resource': ['md-folder', ''],
  'folder-components': ['md-folder', ''],
  'folder-assets': ['md-folder', ''],
  'folder-include': ['md-folder', ''],
  'folder-app': ['md-folder', ''],
  'folder-server': ['md-folder', ''],
  'folder-client': ['md-folder', ''],
  'folder-database': ['md-folder', ''],
  'folder-docker': ['md-folder', ''],
  'folder-ci': ['md-folder', ''],
  'folder-temp': ['md-folder', ''],
  'folder-log': ['md-folder', ''],
  'folder-download': ['md-folder_download', ''],
  'folder-secure': ['md-folder_lock', ''],
};

// Languages VS Code knows by id rather than by extension, as Material Icon
// Theme maps them through `languageIds`: the extensions that carry them.
const LANGUAGE_EXTENSIONS = {
  typescript: ['ts', 'mts', 'cts'],
  javascript: ['js', 'mjs', 'cjs'],
  yaml: ['yaml', 'yml'],
  html: ['html', 'htm'],
  php: ['php'],
  xml: ['xml', 'xsd', 'xsl'],
  shellscript: ['sh', 'bash', 'zsh', 'fish'],
  bat: ['bat', 'cmd'],
  dockerfile: [],
  makefile: ['mk'],
  rust: ['rs'],
  python: ['py', 'pyi'],
};

// A few the theme leaves to `languageIds` or does not name, which a Bro
// user meets constantly.
const EXTRA_NAMES = {
  'cargo.toml': 'rust',
  'cargo.lock': 'lock',
  'rust-toolchain.toml': 'rust',
  'go.mod': 'go-mod',
  'go.sum': 'go-mod',
  '.env': 'tune',
  '.env.local': 'tune',
  'claude.md': 'readme',
  'agents.md': 'readme',
};
const EXTRA_FOLDERS = { crates: 'folder-lib', 'worktrees': 'folder' };

function glyph(name) {
  const g = glyphs[name];
  if (!g) throw new Error(`no Nerd Font glyph named ${name}`);
  return parseInt(g.code, 16);
}

/** The first hex fill in the icon's SVG; folders and files fall back to grays. */
function fill(icon) {
  const def = material.iconDefinitions[icon];
  if (!def) throw new Error(`material has no icon ${icon}`);
  const svg = fs.readFileSync(path.resolve(materialDir, 'dist', def.iconPath), 'utf8');
  const m = svg.match(/fill="#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})"/);
  if (!m) return [0x90, 0xa4, 0xae];
  let hex = m[1];
  if (hex.length === 3) hex = [...hex].map((c) => c + c).join('');
  return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
}

// The canvas the colors have to read on: One Dark's `#282C34`, the theme
// Bro wears beside a VS Code in Atom One Dark. Material's fills are drawn
// for VS Code's own sidebar; a few of its slate grays (`folder-github`,
// `folder-scripts`) all but vanish on it. Each fill is checked against the
// canvas by WCAG contrast and, below FLOOR, lifted toward white just far
// enough — the hue stays the material one, only lighter. (The light
// themes darken the pale ones at run time, `icons::color`.)
const CANVAS = [0x28, 0x2c, 0x34];
const FLOOR = 3.0;
const lin = (v) => {
  const c = v / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};
const lum = ([r, g, b]) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
const contrast = (a, b) => {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const adjusted = new Map();
function onCanvas(icon, rgb) {
  let out = rgb;
  for (let t = 0.05; contrast(out, CANVAS) < FLOOR && t <= 1; t += 0.05) {
    out = rgb.map((v) => Math.round(v + (255 - v) * t));
  }
  if (out !== rgb) adjusted.set(icon, [rgb, out]);
  return out;
}

const usable = (icon) => Object.prototype.hasOwnProperty.call(GLYPHS, icon);
const icons = new Map();
const use = (icon) => {
  if (!icons.has(icon)) {
    const [g, tag] = GLYPHS[icon];
    icons.set(icon, { glyph: glyph(g), color: onCanvas(icon, fill(icon)), tag });
  }
};
const pairs = (obj, keep = () => true) =>
  Object.entries(obj)
    .filter(([k, icon]) => usable(icon) && keep(k))
    .map(([k, icon]) => (use(icon), [k.toLowerCase(), icon]));

const exts = new Map(pairs(material.fileExtensions));
for (const [lang, list] of Object.entries(LANGUAGE_EXTENSIONS)) {
  const icon = material.languageIds[lang];
  if (!icon || !usable(icon)) continue;
  use(icon);
  for (const e of list) if (!exts.has(e)) exts.set(e, icon);
}
const names = new Map(pairs(material.fileNames));
for (const [n, icon] of Object.entries(EXTRA_NAMES)) {
  if (!names.has(n)) (use(icon), names.set(n, icon));
}
const folders = new Map(pairs(material.folderNames));
for (const [n, icon] of Object.entries(EXTRA_FOLDERS)) {
  if (!folders.has(n)) (use(icon), folders.set(n, icon));
}
for (const icon of ['file', 'folder', 'folder-open']) use(icon);

const sorted = (m) => [...m.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
const esc = (s) => s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
const iconNames = [...icons.keys()].sort();
const index = new Map(iconNames.map((n, i) => [n, i]));
let out = `//! GENERATED by \`scripts/icons/gen-icons.mjs\` from Material Icon Theme
//! ${version} (\`pkief.material-icon-theme\`, MIT, © Philipp Kief and
//! contributors — https://github.com/material-extensions/vscode-material-icon-theme)
//! and nerd-fonts' \`glyphnames.json\` (MIT). Do not edit by hand: change the
//! script and run it again. Colors are each material icon's own SVG fill,
//! lifted toward white where it would all but vanish on One Dark's \`#282C34\`.
//!
//! Material Icon Theme's license, which covers the associations below:
//!
${license}
/// One icon: its Nerd Font glyph, its color, and the two-letter tag the
/// \`ascii\` style draws instead.
pub struct IconDef {
    pub name: &'static str,
    pub glyph: char,
    pub rgb: (u8, u8, u8),
    pub tag: &'static str,
}

/// Every icon the tables below point at, by name, sorted.
pub static ICONS: &[IconDef] = &[
`;
for (const n of iconNames) {
  const { glyph: g, color: [r, gg, b], tag } = icons.get(n);
  out += `    IconDef { name: "${n}", glyph: '\\u{${g.toString(16)}}', rgb: (${r}, ${gg}, ${b}), tag: "${esc(tag)}" },\n`;
}
out += '];\n\n';
const table = (doc, name, m) => {
  out += `/// ${doc}, lowercased and sorted, to an index into [\`ICONS\`].\npub static ${name}: &[(&str, u16)] = &[\n`;
  for (const [k, icon] of sorted(m)) out += `    ("${esc(k)}", ${index.get(icon)}),\n`;
  out += '];\n\n';
};
table('File names', 'FILE_NAMES', names);
table('File extensions (without the dot; compound ones like `d.ts` too)', 'FILE_EXTENSIONS', exts);
table('Folder names', 'FOLDER_NAMES', folders);
out += `/// The generic file, the folder and the open folder.
pub const FILE: u16 = ${index.get('file')};
pub const FOLDER: u16 = ${index.get('folder')};
pub const FOLDER_OPEN: u16 = ${index.get('folder-open')};
`;
fs.writeFileSync(OUT, out);
for (const [icon, [a, b]] of adjusted) console.log(`  ${icon}: rgb(${a}) -> rgb(${b}) on #282C34`);
console.log(`wrote ${path.relative(ROOT, OUT)}: ${icons.size} icons, ${names.size} names, ${exts.size} extensions, ${folders.size} folders`);
