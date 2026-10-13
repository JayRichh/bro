#!/usr/bin/env node
// Re-apply the Bro rebrand on top of an upstream nebula tree.
//
//   node scripts/rebrand.mjs          (or: sh scripts/rebrand.sh)
//
// Idempotent: a second run changes nothing. Every edit is either applied,
// already applied, or reported as MISSING (upstream moved the text), and a
// MISSING edit makes the run exit 1 so drift after a rebase is loud.
//
// What it does, in order:
//   1. Copies the whole files under scripts/rebrand/overlay/ over the tree
//      (README, installers, CI workflows).
//   2. Applies the exact edits in EDITS (code changes, URLs, the binary name,
//      the splash, the LICENSE line, the Makefile).
//   3. Rewrites the product name in Rust string literals across crates/, and
//      in the `///` docs of the CLI (clap turns those into --help).
//   4. Rewrites the product name in the user docs under docs/.
//   5. Runs rustfmt over the Rust files it rewrote.
//
// It never renames crates, modules, identifiers, env vars (NEBULA_*) or
// comments. A source line carrying the marker `rebrand:keep` is left alone.
// Files it writes are written with LF line endings, and only when their
// content actually changes.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OVERLAY = path.join(ROOT, 'scripts', 'rebrand', 'overlay');
const KEEP = 'rebrand:keep';

const changed = new Set();
const missing = [];

const abs = (rel) => path.join(ROOT, ...rel.split('/'));
const rel = (p) => path.relative(ROOT, p).split(path.sep).join('/');
const readLF = (p) => fs.readFileSync(p, 'utf8').replace(/\r\n/g, '\n');

function writeIfChanged(p, next) {
  const cur = fs.existsSync(p) ? readLF(p) : null;
  if (cur === next) return;
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, next);
  changed.add(rel(p));
}

function walk(dir, keep, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name === 'target' || e.name === '.git' || e.name === 'node_modules') continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, keep, out);
    else if (keep(p)) out.push(p);
  }
  return out;
}

// ---------------------------------------------------------------- 1. overlay

for (const src of walk(OVERLAY, () => true)) {
  const dest = path.join(ROOT, path.relative(OVERLAY, src));
  writeIfChanged(dest, readLF(src));
}

// ------------------------------------------------------------ 2. exact edits

const EDITS = [
  // The binary.
  {
    file: 'crates/nebula/Cargo.toml',
    from: '[[bin]]\nname = "nebula"',
    to: '[[bin]]\nname = "bro"',
  },
  // Usage lines say `bro`, not `bro.exe` from argv[0].
  {
    file: 'crates/nebula/src/cli.rs',
    from: '    name = "nebula",\n    version,\n',
    to: '    name = "bro",\n    bin_name = "bro",\n    version,\n',
  },
  // Release and install URLs.
  {
    file: 'crates/nebula/src/upgrade.rs',
    from: 'https://raw.githubusercontent.com/AgentSystemLabs/nebula/main/install.sh',
    to: 'https://raw.githubusercontent.com/JayRichh/bro/main/install.sh',
  },
  {
    file: 'crates/nebula-tui/src/update_check.rs',
    from: '"https://github.com/AgentSystemLabs/nebula/releases/latest"',
    to: '"https://github.com/JayRichh/bro/releases/latest"',
  },
  // Unix data/config and runtime dirs. Windows already uses %LOCALAPPDATA%\bro.
  {
    file: 'crates/nebula-core/src/paths.rs',
    from: 'Path::new(FALLBACK_RUNTIME_ROOT).join(format!("nebula-{uid}"))',
    to: 'Path::new(FALLBACK_RUNTIME_ROOT).join(format!("bro-{uid}"))',
  },
  {
    file: 'crates/nebula-core/src/paths.rs',
    from: '.unwrap_or_else(|| env::home_dir().unwrap_or_default().join(".nebula"))',
    to: '.unwrap_or_else(|| env::home_dir().unwrap_or_default().join(".bro"))',
  },
  // `.bro.json` first, `.nebula.json` as the fallback.
  {
    file: 'crates/nebula-core/src/project_file.rs',
    from: 'pub const FILE_NAME: &str = ".nebula.json";\n',
    to:
      'pub const FILE_NAME: &str = ".bro.json";\n\n' +
      '/// The name upstream nebula reads, still honoured in a checkout that has\n' +
      '/// no `.bro.json`.\n' +
      `pub const LEGACY_FILE_NAME: &str = ".nebula.json"; // ${KEEP}\n`,
  },
  {
    file: 'crates/nebula-core/src/project_file.rs',
    from:
      '/// Parse one checkout\'s file. Ok(None) when the checkout has none.\n' +
      'pub fn read(dir: &Path) -> Result<Option<ProjectFile>, String> {\n' +
      '    let path = dir.join(FILE_NAME);\n',
    to:
      '/// Parse one checkout\'s file: `.bro.json`, else `.nebula.json`. Ok(None)\n' +
      '/// when the checkout has neither.\n' +
      'pub fn read(dir: &Path) -> Result<Option<ProjectFile>, String> {\n' +
      '    match read_named(dir, FILE_NAME)? {\n' +
      '        Some(file) => Ok(Some(file)),\n' +
      '        None => read_named(dir, LEGACY_FILE_NAME),\n' +
      '    }\n' +
      '}\n\n' +
      'fn read_named(dir: &Path, name: &str) -> Result<Option<ProjectFile>, String> {\n' +
      '    let path = dir.join(name);\n',
  },
  {
    file: 'crates/nebula-core/src/project_file.rs',
    from:
      '        assert!(err.contains("invalid type"), "{err}");\n' +
      '    }\n' +
      '}\n',
    to:
      '        assert!(err.contains("invalid type"), "{err}");\n' +
      '    }\n\n' +
      '    #[test]\n' +
      '    fn the_bro_file_wins_and_the_nebula_file_still_reads() {\n' +
      '        let legacy = TempDir::new();\n' +
      '        std::fs::write(legacy.0.join(LEGACY_FILE_NAME), r#"{"run": "npm run dev"}"#).unwrap();\n' +
      '        assert_eq!(\n' +
      '            command(&legacy.0, &legacy.0, ProjectCommand::Run).unwrap(),\n' +
      '            "npm run dev"\n' +
      '        );\n\n' +
      '        let both = TempDir::new().with_file(r#"{"run": "bun dev"}"#);\n' +
      '        std::fs::write(both.0.join(LEGACY_FILE_NAME), r#"{"run": "npm run dev"}"#).unwrap();\n' +
      '        assert_eq!(\n' +
      '            command(&both.0, &both.0, ProjectCommand::Run).unwrap(),\n' +
      '            "bun dev"\n' +
      '        );\n' +
      '    }\n' +
      '}\n',
  },
  // Prompts injected by an older nebula stay recognised in history.
  {
    file: 'crates/nebula-daemon/src/prompt_history.rs',
    from: '    "[nebula]",\n    "[Cross-session idle notice]",\n',
    to: `    "[bro]",\n    "[nebula]", // ${KEEP}\n    "[Cross-session idle notice]",\n`,
  },
  // Splash: the wordmark, the welcome line, the tagline.
  {
    file: 'crates/nebula-tui/src/splash.rs',
    from:
      '/// 5-row block bitmaps for N E B U L A.\n' +
      'const LETTERS: &[&[&str; 5]] = &[\n' +
      '    &["#...#", "##..#", "#.#.#", "#..##", "#...#"],\n' +
      '    &["####", "#...", "###.", "#...", "####"],\n' +
      '    &["###.", "#..#", "###.", "#..#", "###."],\n' +
      '    &["#..#", "#..#", "#..#", "#..#", ".##."],\n' +
      '    &["#...", "#...", "#...", "#...", "####"],\n' +
      '    &[".##.", "#..#", "####", "#..#", "#..#"],\n' +
      '];\n',
    to:
      '/// 5-row block bitmaps for B R O.\n' +
      'const LETTERS: &[&[&str; 5]] = &[\n' +
      '    &["###.", "#..#", "###.", "#..#", "###."],\n' +
      '    &["###.", "#..#", "###.", "#.#.", "#..#"],\n' +
      '    &[".##.", "#..#", "#..#", "#..#", ".##."],\n' +
      '];\n',
  },
  {
    file: 'crates/nebula-tui/src/ui/launcher_view.rs',
    from: 'crate::splash::wordmark_word("nebula", t)',
    to: 'crate::splash::wordmark_word("bro", t)',
  },
  // Test data: the issue title above it says `bro open` once renamed.
  {
    file: 'crates/nebula-tui/src/branch_name.rs',
    from: '"issue-7-crash-nebula-open-on-a-pdf"',
    to: '"issue-7-crash-bro-open-on-a-pdf"',
  },
  // The license keeps the upstream notice and gains the fork's line.
  {
    file: 'LICENSE',
    from: 'Copyright (c) 2026 Cody Seibert\n',
    to: 'Copyright (c) 2026 Cody Seibert\nCopyright (c) 2026 JayRichh (Bro fork)\n',
  },
  // Makefile: the binary it builds, installs and looks for.
  { file: 'Makefile', from: 'RELEASE_BIN := target/release/nebula\n', to: 'RELEASE_BIN := target/release/bro\n' },
  { file: 'Makefile', from: 'DEBUG_BIN   := target/debug/nebula\n', to: 'DEBUG_BIN   := target/debug/bro\n' },
  {
    file: 'Makefile',
    from:
      '\tcp $(RELEASE_BIN) $(PREFIX)/nebula.new\n' +
      '\tmv $(PREFIX)/nebula.new $(PREFIX)/nebula\n' +
      '\t@$(PREFIX)/nebula --version\n' +
      '\t@$(PREFIX)/nebula _stale-daemon-note\n',
    to:
      '\tcp $(RELEASE_BIN) $(PREFIX)/bro.new\n' +
      '\tmv $(PREFIX)/bro.new $(PREFIX)/bro\n' +
      '\t@$(PREFIX)/bro --version\n' +
      '\t@$(PREFIX)/bro _stale-daemon-note\n',
  },
  { file: 'Makefile', from: "grep -q 'nebula daemon'", to: "grep -q 'bro daemon'", all: true },
  {
    file: 'Makefile',
    from: '# The real dir is where `directories::ProjectDirs::from("dev","nebula","nebula")`',
    to: '# The real dir is where `directories::ProjectDirs::from("dev","bro","bro")`',
  },
  {
    file: 'Makefile',
    from: 'Darwin) real="$$HOME/Library/Application Support/dev.nebula.nebula";;',
    to: 'Darwin) real="$$HOME/Library/Application Support/dev.bro.bro";;',
  },
  {
    file: 'Makefile',
    from: '*)      real="$${XDG_DATA_HOME:-$$HOME/.local/share}/nebula";;',
    to: '*)      real="$${XDG_DATA_HOME:-$$HOME/.local/share}/bro";;',
  },
  { file: 'scripts/shot/shot.sh', from: 'BIN="$REPO/target/debug/nebula"', to: 'BIN="$REPO/target/debug/bro"' },
  // Docs: where the unix dirs moved.
  { file: 'docs/configuration.md', from: 'dev.nebula.nebula', to: 'dev.bro.bro', all: true },
  { file: 'docs/configuration.md', from: '~/.local/share/nebula/', to: '~/.local/share/bro/', all: true },
  { file: 'docs/how-it-works.md', from: '~/.local/share/nebula/', to: '~/.local/share/bro/', all: true },
  { file: 'docs/configuration.md', from: '~/.local/state/nebula/', to: '~/.local/state/bro/', all: true },
  {
    file: 'docs/configuration.md',
    from: '`$XDG_RUNTIME_DIR/nebula`, else `/tmp/nebula-<uid>`',
    to: '`$XDG_RUNTIME_DIR/bro`, else `/tmp/bro-<uid>`',
  },
  {
    file: 'docs/how-it-works.md',
    from: '(`$XDG_RUNTIME_DIR/nebula/`\n  or `/tmp/nebula-<uid>/`',
    to: '(`$XDG_RUNTIME_DIR/bro/`\n  or `/tmp/bro-<uid>/`',
  },
];

for (const e of EDITS) {
  const p = abs(e.file);
  if (!fs.existsSync(p)) {
    missing.push(`${e.file}: file not found`);
    continue;
  }
  let s = readLF(p);
  // `to` first: some edits (the LICENSE line) keep `from` inside `to`.
  if (s.includes(e.to)) continue;
  if (s.includes(e.from)) {
    s = e.all ? s.split(e.from).join(e.to) : s.replace(e.from, () => e.to);
    writeIfChanged(p, s);
  } else {
    missing.push(`${e.file}: neither the upstream text nor the rebrand found:\n    ${JSON.stringify(e.from.slice(0, 120))}`);
  }
}

// -------------------------------------------------- 3. Rust string literals

// "nebula" as the product name in prose: not part of a path, a crate name,
// an identifier, a file name or an email. `\n` / `\t` escapes right before
// it still count as a boundary.
const PROSE =
  /(?:(?<=\\[nrt])|(?<![A-Za-z0-9_\-/.$~@\\]))(nebula|Nebula)(?![A-Za-z0-9_\-/@]|\.[A-Za-z0-9])/g;
const PROJECT_FILE = /(?<![A-Za-z0-9_\-/])\.nebula\.json/g;
const TAGLINE = [
  ['"your agents keep running, even when you leave"', '"Sweet as agent management."'],
  ['"your agents keep running"', '"Sweet as agent management"'],
];

// Files where a bare "nebula" literal is the binary's own name (clap's name,
// PATH lookups, fallback exe names, test stubs, the unix dir names, the
// splash). Everywhere else a bare "nebula" is test data (a project called
// nebula) and stays.
const BIN_NAME_FILES = [
  /^crates\/nebula\/src\//,
  /^crates\/nebula\/tests\//,
  /^crates\/nebula-core\/src\/paths\.rs$/,
  /^crates\/nebula-daemon\/src\/hooks\/installer\.rs$/,
  /^crates\/nebula-tui\/src\/splash\.rs$/,
  /^crates\/nebula-tui\/src\/ui\.rs$/,
];

function prose(text) {
  return text
    .replace(PROJECT_FILE, '.bro.json')
    .replace(PROSE, (m) => (m === 'Nebula' ? 'Bro' : 'bro'));
}

function rewriteLiteral(lit, file) {
  let t = lit.replace(/CARGO_BIN_EXE_nebula\b/g, 'CARGO_BIN_EXE_bro');
  if (t === '"nebula"') return BIN_NAME_FILES.some((r) => r.test(file)) ? '"bro"' : t;
  for (const [a, b] of TAGLINE) if (t === a) t = b;
  return prose(t);
}

// A small Rust lexer: enough to tell string literals and comments apart.
function lex(src) {
  const toks = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === '/' && src[i + 1] === '/') {
      let j = src.indexOf('\n', i);
      if (j < 0) j = n;
      const outer = src[i + 2] === '/' && src[i + 3] !== '/';
      toks.push({ k: outer ? 'doc' : 'cmt', s: i, e: j });
      i = j;
      continue;
    }
    if (c === '/' && src[i + 1] === '*') {
      let depth = 1;
      let j = i + 2;
      while (j < n && depth) {
        if (src[j] === '/' && src[j + 1] === '*') { depth++; j += 2; }
        else if (src[j] === '*' && src[j + 1] === '/') { depth--; j += 2; }
        else j++;
      }
      i = j;
      continue;
    }
    if ((c === 'r' || (c === 'b' && src[i + 1] === 'r')) && !/[A-Za-z0-9_]/.test(src[i - 1] || '')) {
      let j = i + (c === 'b' ? 2 : 1);
      let h = 0;
      while (src[j] === '#') { h++; j++; }
      if (src[j] === '"') {
        const close = '"' + '#'.repeat(h);
        const k = src.indexOf(close, j + 1);
        toks.push({ k: 'str', s: i, e: k + close.length });
        i = k + close.length;
        continue;
      }
    }
    if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') {
        if (src[j] === '\\') j++;
        j++;
      }
      toks.push({ k: 'str', s: i, e: j + 1 });
      i = j + 1;
      continue;
    }
    if (c === "'") {
      // A char literal ('x', '\n', '\''), else a lifetime.
      if (src[i + 1] === '\\') { i = src.indexOf("'", i + 3) + 1; continue; }
      if (src[i + 2] === "'") { i += 3; continue; }
      i++;
      continue;
    }
    i++;
  }
  return toks;
}

function lineAt(src, at) {
  const s = src.lastIndexOf('\n', at) + 1;
  let e = src.indexOf('\n', at);
  if (e < 0) e = src.length;
  return src.slice(s, e);
}

const CLI_DOCS = 'crates/nebula/src/cli.rs';

for (const p of walk(abs('crates'), (f) => f.endsWith('.rs'))) {
  const file = rel(p);
  const src = readLF(p);
  let out = '';
  let last = 0;
  for (const t of lex(src)) {
    if (t.k === 'cmt') continue;
    if (t.k === 'doc' && file !== CLI_DOCS) continue;
    if (lineAt(src, t.s).includes(KEEP)) continue;
    const text = src.slice(t.s, t.e);
    const next = t.k === 'doc' ? prose(text) : rewriteLiteral(text, file);
    if (next === text) continue;
    out += src.slice(last, t.s) + next;
    last = t.e;
  }
  out += src.slice(last);
  writeIfChanged(p, out);
}

// ---------------------------------------------------------- 4. user docs

// The pages README links to. PORT.md is Bro's own and already says bro.
const DOCS = [
  'docs/commands.md',
  'docs/configuration.md',
  'docs/how-it-works.md',
  'docs/keys.md',
  'docs/orchestrator.md',
  'docs/sessions.md',
];
for (const d of DOCS) {
  const p = abs(d);
  if (!fs.existsSync(p)) {
    missing.push(`${d}: file not found`);
    continue;
  }
  const src = readLF(p);
  const next = src
    .split('\n')
    .map((line) => (line.includes(KEEP) ? line : prose(line)))
    .join('\n');
  writeIfChanged(p, next);
}

// ------------------------------------------------------------ 5. rustfmt

// A renamed literal can change how rustfmt wraps a line, and CI runs
// `cargo fmt --check`. Format only the Rust files this run rewrote, through
// stdin so rustfmt doesn't wander into their child modules. A second run
// rewrites nothing, so it formats nothing.
const rewrittenRs = [...changed].filter((f) => f.endsWith('.rs'));
if (rewrittenRs.length) {
  const cargoBin = path.join(process.env.USERPROFILE || process.env.HOME || '', '.cargo', 'bin');
  const candidates = ['rustfmt', path.join(cargoBin, 'rustfmt')];
  let formatted = false;
  for (const exe of candidates) {
    const probe = spawnSync(exe, ['--version'], { encoding: 'utf8' });
    if (probe.error || probe.status !== 0) continue;
    for (const f of rewrittenRs) {
      const p = abs(f);
      const r = spawnSync(exe, ['--edition', '2021', '--emit', 'stdout'], {
        input: readLF(p),
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
      });
      if (r.status !== 0) {
        missing.push(`${f}: rustfmt failed: ${r.stderr.trim()}`);
        continue;
      }
      writeIfChanged(p, r.stdout.replace(/\r\n/g, '\n'));
    }
    formatted = true;
    break;
  }
  if (!formatted) console.warn('rebrand: rustfmt not found; run `cargo fmt` on the files above');
}

// ------------------------------------------------------------------ report

for (const f of [...changed].sort()) console.log(`rebrand: wrote ${f}`);
if (missing.length) {
  console.error('\nrebrand: MISSING (upstream changed under these edits):');
  for (const m of missing) console.error(`  ${m}`);
  process.exit(1);
}
console.log(changed.size ? `rebrand: ${changed.size} file(s) updated` : 'rebrand: already applied, nothing to do');
