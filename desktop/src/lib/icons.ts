// File icons from Material Icon Theme (MIT), resolved the way VS Code
// resolves them: exact file name first, then the longest matching
// extension, then the generic file icon. The table and SVGs are generated
// by scripts/material-icons.mjs and served from disk.

import map from './material-map.json';

interface Table {
  file: string;
  folder: string;
  folderExpanded: string;
  fileNames: Record<string, string>;
  fileExtensions: Record<string, string>;
  light: { fileNames: Record<string, string>; fileExtensions: Record<string, string> };
}
const t = map as Table;

export function fileIcon(path: string, dark: boolean): string {
  const base = path.split(/[\\/]/).pop()!.toLowerCase();
  const names = dark ? t.fileNames : { ...t.fileNames, ...t.light.fileNames };
  const exts = dark ? t.fileExtensions : { ...t.fileExtensions, ...t.light.fileExtensions };
  let icon = names[base];
  if (!icon) {
    const parts = base.split('.');
    for (let i = 1; i < parts.length && !icon; i++) icon = exts[parts.slice(i).join('.')];
  }
  return `/material-icons/${icon ?? t.file}.svg`;
}

/** A folder's icon, shut or open (the theme's generic one). */
export function folderIcon(open: boolean): string {
  return `/material-icons/${open ? t.folderExpanded : t.folder}.svg`;
}

const LANG: Record<string, string> = {
  ts: 'typescript', tsx: 'typescript', mts: 'typescript', cts: 'typescript',
  js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
  rs: 'rust', py: 'python', go: 'go', java: 'java', kt: 'kotlin', swift: 'swift',
  c: 'cpp', h: 'cpp', cc: 'cpp', cpp: 'cpp', hpp: 'cpp', cs: 'csharp',
  json: 'json', jsonc: 'json', md: 'markdown', mdx: 'mdx', toml: 'ini', ini: 'ini',
  yml: 'yaml', yaml: 'yaml', html: 'html', htm: 'html', svelte: 'html', vue: 'html',
  css: 'css', scss: 'scss', less: 'less', sql: 'sql', sh: 'shell', bash: 'shell',
  ps1: 'powershell', psm1: 'powershell', bat: 'bat', cmd: 'bat', rb: 'ruby', php: 'php',
  lua: 'lua', xml: 'xml', svg: 'xml', dockerfile: 'dockerfile', graphql: 'graphql',
};

export function languageOf(path: string): string {
  const base = path.split(/[\\/]/).pop()!.toLowerCase();
  if (base === 'dockerfile') return 'dockerfile';
  if (base === 'makefile') return 'shell';
  return LANG[base.split('.').pop() ?? ''] ?? 'plaintext';
}
