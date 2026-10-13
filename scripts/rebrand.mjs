#!/usr/bin/env node
// Re-apply the Kete rebrand on top of an upstream nebula tree.
//
//   node scripts/rebrand.mjs          (or: sh scripts/rebrand.sh)
//
// Idempotent: a second run changes nothing. Every edit is either applied,
// already applied, or reported as MISSING (upstream moved the text), and a
// MISSING edit makes the run exit 1 so drift after a rebase is loud.
//
// What it does, in order:
//   1. Copies the whole files under scripts/rebrand/overlay/ over the tree
//      (README, installers, CI workflows, the `bro` alias binary).
//   2. Applies the exact edits in EDITS (code changes, URLs, the binary
//      names, the splash, the LICENSE line, the Makefile).
//   3. Rewrites the product name in Rust string literals across crates/, and
//      in the `///` docs of the CLI (clap turns those into --help). The same
//      pass takes the em dashes out of that text (see `undash`).
//   4. Rewrites the product name in the user docs under docs/, and takes
//      the em dashes out of them too.
//   5. Runs rustfmt over the Rust files it rewrote.
//
// It never renames crates, modules, identifiers, env vars (NEBULA_*) or
// comments. A source line carrying the marker `rebrand:keep` is left alone.
// Files it writes are written with LF line endings, and only when their
// content actually changes.
//
// Kete was called Bro before. The fork's own code (not upstream's) carries
// the `bro` fallbacks: the alias binary, the old data dirs, `.bro.json`.

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

// No `to` below may hold an em dash: step 3 takes those out after step 2
// has run, and a second run looks for `to` verbatim.
const EDITS = [
  // The binaries: `kete`, and `bro` (the old name) from the same program.
  // src/bro.rs comes from the overlay.
  {
    file: 'crates/nebula/Cargo.toml',
    from: '[[bin]]\nname = "nebula"\npath = "src/main.rs"\n',
    to:
      '[[bin]]\nname = "kete"\npath = "src/main.rs"\n\n' +
      '# The old name, still installed: the same program (src/bro.rs includes\n' +
      '# src/main.rs), and usage lines say `bro` when it is run as `bro`.\n' +
      '[[bin]]\nname = "bro"\npath = "src/bro.rs"\n',
  },
  // Usage lines name the binary you ran, `kete` or `bro`, never `kete.exe`.
  {
    file: 'crates/nebula/src/cli.rs',
    from: 'use clap::{Parser, Subcommand};\n',
    to:
      'use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};\n\n' +
      '/// Parse the command line under the name it was run as: `kete`, or `bro`\n' +
      '/// (the old name, installed beside it), so usage lines say what you typed.\n' +
      'pub(crate) fn parse_as_invoked() -> Cli {\n' +
      '    let name = invoked_name(std::env::args_os().next().as_deref());\n' +
      '    let matches = Cli::command().bin_name(name).get_matches();\n' +
      '    Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())\n' +
      '}\n\n' +
      '/// `bro` when argv[0] is a `bro` (the alias, a copy or a symlink), else\n' +
      '/// `kete`.\n' +
      'pub(crate) fn invoked_name(argv0: Option<&std::ffi::OsStr>) -> &\'static str {\n' +
      '    let stem = argv0\n' +
      '        .map(std::path::Path::new)\n' +
      '        .and_then(std::path::Path::file_stem);\n' +
      '    match stem.and_then(|s| s.to_str()) {\n' +
      `        Some(s) if s.eq_ignore_ascii_case("bro") => "bro", // ${KEEP}\n` +
      '        _ => "kete",\n' +
      '    }\n' +
      '}\n',
  },
  {
    file: 'crates/nebula/src/main.rs',
    from: 'use clap::Parser;\nuse cli::{Cli, Command,',
    to: 'use cli::{Command,',
  },
  {
    file: 'crates/nebula/src/main.rs',
    from: '    let cli = Cli::parse();\n',
    to: '    let cli = cli::parse_as_invoked();\n',
  },
  // What the root help says it is.
  {
    file: 'crates/nebula/src/cli.rs',
    from:
      '    about = "Terminal multiplexer for Claude Code agents",\n' +
      '    long_about = "Terminal multiplexer for Claude Code agents.\\n\\n\\\n',
    to:
      '    about = "Run your coding agents side by side, in one place",\n' +
      '    long_about = "Run your coding agents side by side, in one place.\\n\\n\\\n',
  },
  {
    file: 'crates/nebula/tests/help_cli.rs',
    from: '        "Terminal multiplexer for Claude Code agents",\n',
    to: '        "Run your coding agents side by side, in one place",\n',
  },
  // Release and install URLs.
  {
    file: 'crates/nebula/src/upgrade.rs',
    from: 'https://raw.githubusercontent.com/AgentSystemLabs/nebula/main/install.sh',
    to: 'https://raw.githubusercontent.com/JayRichh/kete/main/install.sh',
  },
  {
    file: 'crates/nebula-tui/src/update_check.rs',
    from: '"https://github.com/AgentSystemLabs/nebula/releases/latest"',
    to: '"https://github.com/JayRichh/kete/releases/latest"',
  },
  // Unix data/config and runtime dirs. Windows uses %LOCALAPPDATA%\kete.
  {
    file: 'crates/nebula-core/src/paths.rs',
    from: 'Path::new(FALLBACK_RUNTIME_ROOT).join(format!("nebula-{uid}"))',
    to: 'Path::new(FALLBACK_RUNTIME_ROOT).join(format!("kete-{uid}"))',
  },
  {
    file: 'crates/nebula-core/src/paths.rs',
    from: '.unwrap_or_else(|| env::home_dir().unwrap_or_default().join(".nebula"))',
    to: '.unwrap_or_else(|| env::home_dir().unwrap_or_default().join(".kete"))',
  },
  // `.kete.json` first, then `.bro.json` (the old name), then `.nebula.json`.
  {
    file: 'crates/nebula-core/src/project_file.rs',
    from: 'pub const FILE_NAME: &str = ".nebula.json";\n',
    to:
      'pub const FILE_NAME: &str = ".kete.json";\n\n' +
      '/// Older names still honoured, in order, in a checkout that has no\n' +
      '/// `.kete.json`: Bro\'s, then upstream nebula\'s.\n' +
      `pub const LEGACY_FILE_NAMES: &[&str] = &[".bro.json", ".nebula.json"]; // ${KEEP}\n`,
  },
  {
    file: 'crates/nebula-core/src/project_file.rs',
    from:
      '/// Parse one checkout\'s file. Ok(None) when the checkout has none.\n' +
      'pub fn read(dir: &Path) -> Result<Option<ProjectFile>, String> {\n' +
      '    let path = dir.join(FILE_NAME);\n',
    to:
      '/// Parse one checkout\'s file: `.kete.json`, else `.bro.json`, else\n' +
      '/// `.nebula.json`. Ok(None) when the checkout has none of them.\n' +
      'pub fn read(dir: &Path) -> Result<Option<ProjectFile>, String> {\n' +
      '    for name in std::iter::once(FILE_NAME).chain(LEGACY_FILE_NAMES.iter().copied()) {\n' +
      '        if let Some(file) = read_named(dir, name)? {\n' +
      '            return Ok(Some(file));\n' +
      '        }\n' +
      '    }\n' +
      '    Ok(None)\n' +
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
      '    fn the_kete_file_wins_and_the_older_names_still_read() {\n' +
      '        let [bro, nebula] = [LEGACY_FILE_NAMES[0], LEGACY_FILE_NAMES[1]];\n' +
      '        let legacy = TempDir::new();\n' +
      '        std::fs::write(legacy.0.join(nebula), r#"{"run": "npm run dev"}"#).unwrap();\n' +
      '        assert_eq!(\n' +
      '            command(&legacy.0, &legacy.0, ProjectCommand::Run).unwrap(),\n' +
      '            "npm run dev"\n' +
      '        );\n' +
      '        std::fs::write(legacy.0.join(bro), r#"{"run": "pnpm dev"}"#).unwrap();\n' +
      '        assert_eq!(\n' +
      '            command(&legacy.0, &legacy.0, ProjectCommand::Run).unwrap(),\n' +
      '            "pnpm dev"\n' +
      '        );\n\n' +
      '        let all = TempDir::new().with_file(r#"{"run": "bun dev"}"#);\n' +
      '        std::fs::write(all.0.join(bro), r#"{"run": "pnpm dev"}"#).unwrap();\n' +
      '        std::fs::write(all.0.join(nebula), r#"{"run": "npm run dev"}"#).unwrap();\n' +
      '        assert_eq!(\n' +
      '            command(&all.0, &all.0, ProjectCommand::Run).unwrap(),\n' +
      '            "bun dev"\n' +
      '        );\n' +
      '    }\n' +
      '}\n',
  },
  // Prompts injected by Bro or an older nebula stay recognised in history.
  {
    file: 'crates/nebula-daemon/src/prompt_history.rs',
    from: '    "[nebula]",\n    "[Cross-session idle notice]",\n',
    to: `    "[kete]",\n    "[bro]",\n    "[nebula]", // ${KEEP}\n    "[Cross-session idle notice]",\n`,
  },
  // Claude Code may run the commands under either name.
  {
    file: 'crates/nebula-daemon/src/hooks/installer.rs',
    from: '    "Bash(nebula ask:*)",\n];\n',
    to:
      '    "Bash(kete ask:*)",\n' +
      '    "Bash(bro rename:*)",\n' +
      '    "Bash(bro worktree:*)",\n' +
      '    "Bash(bro spawn:*)",\n' +
      '    "Bash(bro open:*)",\n' +
      '    "Bash(bro sessions:*)",\n' +
      '    "Bash(bro read:*)",\n' +
      '    "Bash(bro ask:*)",\n' +
      '];\n',
  },
  // Splash: the wordmark, the tagline.
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
      '/// 5-row block bitmaps for K E T E.\n' +
      'const LETTERS: &[&[&str; 5]] = &[\n' +
      '    &["#..#", "#.#.", "##..", "#.#.", "#..#"],\n' +
      '    &["####", "#...", "###.", "#...", "####"],\n' +
      '    &["#####", "..#..", "..#..", "..#..", "..#.."],\n' +
      '    &["####", "#...", "###.", "#...", "####"],\n' +
      '];\n',
  },
  {
    file: 'crates/nebula-tui/src/ui/launcher_view.rs',
    from: 'crate::splash::wordmark_word("nebula", t)',
    to: 'crate::splash::wordmark_word("kete", t)',
  },
  // Test data: the issue title above it says `kete open` once renamed.
  {
    file: 'crates/nebula-tui/src/branch_name.rs',
    from: '"issue-7-crash-nebula-open-on-a-pdf"',
    to: '"issue-7-crash-kete-open-on-a-pdf"',
  },
  // The license keeps the upstream notice and gains the fork's line.
  {
    file: 'LICENSE',
    from: 'Copyright (c) 2026 Cody Seibert\n',
    to: 'Copyright (c) 2026 Cody Seibert\nCopyright (c) 2026 JayRichh (Kete fork)\n',
  },
  // Makefile: the binary it builds, installs and looks for. `bro` goes in
  // beside `kete` as a symlink.
  { file: 'Makefile', from: 'RELEASE_BIN := target/release/nebula\n', to: 'RELEASE_BIN := target/release/kete\n' },
  { file: 'Makefile', from: 'DEBUG_BIN   := target/debug/nebula\n', to: 'DEBUG_BIN   := target/debug/kete\n' },
  {
    file: 'Makefile',
    from:
      '\tcp $(RELEASE_BIN) $(PREFIX)/nebula.new\n' +
      '\tmv $(PREFIX)/nebula.new $(PREFIX)/nebula\n' +
      '\t@$(PREFIX)/nebula --version\n' +
      '\t@$(PREFIX)/nebula _stale-daemon-note\n',
    to:
      '\tcp $(RELEASE_BIN) $(PREFIX)/kete.new\n' +
      '\tmv $(PREFIX)/kete.new $(PREFIX)/kete\n' +
      '\tln -sf kete $(PREFIX)/bro\n' +
      '\t@$(PREFIX)/kete --version\n' +
      '\t@$(PREFIX)/kete _stale-daemon-note\n',
  },
  { file: 'Makefile', from: '\t$(PREFIX)/nebula kill\n', to: '\t$(PREFIX)/kete kill\n' },
  { file: 'Makefile', from: "grep -q 'nebula daemon'", to: "grep -q 'kete daemon'", all: true },
  {
    file: 'Makefile',
    from: '# The real dir is where `directories::ProjectDirs::from("dev","nebula","nebula")`',
    to: '# The real dir is where `directories::ProjectDirs::from("dev","kete","kete")`',
  },
  {
    file: 'Makefile',
    from: 'Darwin) real="$$HOME/Library/Application Support/dev.nebula.nebula";;',
    to: 'Darwin) real="$$HOME/Library/Application Support/dev.kete.kete";;',
  },
  {
    file: 'Makefile',
    from: '*)      real="$${XDG_DATA_HOME:-$$HOME/.local/share}/nebula";;',
    to: '*)      real="$${XDG_DATA_HOME:-$$HOME/.local/share}/kete";;',
  },
  { file: 'scripts/shot/shot.sh', from: 'BIN="$REPO/target/debug/nebula"', to: 'BIN="$REPO/target/debug/kete"' },
  // Docs: where the unix dirs moved.
  { file: 'docs/configuration.md', from: 'dev.nebula.nebula', to: 'dev.kete.kete', all: true },
  { file: 'docs/configuration.md', from: '~/.local/share/nebula/', to: '~/.local/share/kete/', all: true },
  { file: 'docs/how-it-works.md', from: '~/.local/share/nebula/', to: '~/.local/share/kete/', all: true },
  { file: 'docs/configuration.md', from: '~/.local/state/nebula/', to: '~/.local/state/kete/', all: true },
  {
    file: 'docs/configuration.md',
    from: '`$XDG_RUNTIME_DIR/nebula`, else `/tmp/nebula-<uid>`',
    to: '`$XDG_RUNTIME_DIR/kete`, else `/tmp/kete-<uid>`',
  },
  {
    file: 'docs/how-it-works.md',
    from: '(`$XDG_RUNTIME_DIR/nebula/`\n  or `/tmp/nebula-<uid>/`',
    to: '(`$XDG_RUNTIME_DIR/kete/`\n  or `/tmp/kete-<uid>/`',
  },
  { file: 'docs/keys.md', from: 'nebula-side bookkeeping', to: 'kete-side bookkeeping', all: true },
  {
    file: 'docs/how-it-works.md',
    from: '(configuration.md#the-project-file-nebulajson)',
    to: '(configuration.md#the-project-file-ketejson)',
    all: true,
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

// ------------------------------------------------------- the dash transform

// Kete's copy has no em dashes (and no spaced en dashes standing in for
// one). `undash` rewrites a piece of prose: a pair of dashes around an
// aside becomes parentheses, a lone dash a comma when the words after it
// carry the sentence on, else a colon, and a dash standing alone for
// "nothing" (a table cell) a hyphen. Text with no dash comes back as is,
// so a second run changes nothing.

const COMMA_WORDS = new Set(
  (
    'and but or so nor yet then though although while which who whose unless except even not see ' +
    'until since because as if where when whatever whichever without with before after instead ' +
    'otherwise plus rather including usually only also never always just still once same say ' +
    'e.g. i.e. for to in on at by from into onto over under unlike like each every its it this that ' +
    'these those the a an all both either neither nothing none no'
  ).split(' '),
);

// A run of whitespace as it reads: in a Rust literal `\` + newline +
// indentation is nothing at all, and a newline in Markdown is a space.
const WS = String.raw`(?:[ \t\n]|\\\n[ \t]*)*`;
const DASH = new RegExp(`(${WS})(?:—|(?<=[ \\t\\n])–(?=[ \\t\\n]))(${WS})`, 'g');
const CONTINUATION = /\\\n[ \t]*/g;
const rendersSpace = (ws) => /[ \t\n]/.test(ws.replace(CONTINUATION, ''));
const squeeze = (ws) => ws.replace(/[ \t]+/g, '');
// A pair of dashes never spans a sentence, a paragraph or a table cell.
const BOUNDARY = /[.;!?](?=\s|$)|\n\s*\n|\|/;
const balanced = (s) => {
  let depth = 0;
  for (const c of s) {
    if (c === '(') depth++;
    else if (c === ')' && --depth < 0) return false;
  }
  return depth === 0;
};
// Nothing worded before the dash: the text, a literal, a cell or an aside
// opens there.
const opensAt = (before) => /(?:^|^r?#*"|[([|]|\n\s*\n)$/.test(before.replace(/[\s]*$/, '').replace(/(?:\\\n[ \t]*)+$/, ''));
// Nothing worded after it: the text, a literal or a cell ends there.
const closesAt = (after) => /^(?:"#*$|"|\||\)|$)/.test(after.replace(/^(?:\s|\\\n[ \t]*)*/, ''));

function undash(text) {
  text = text.replace(/(\|[ \t]*)—(?=[ \t]*\|)/g, '$1-');
  if (/^"[ ]*—[ ]*"$/.test(text)) return text.replace('—', '-');
  const hits = [...text.matchAll(DASH)].map((m) => ({ at: m.index, end: m.index + m[0].length, l: m[1], r: m[2] }));
  if (!hits.length) return text;
  let out = '';
  let last = 0;
  for (let i = 0; i < hits.length; i++) {
    const h = hits[i];
    out += text.slice(last, h.at);
    last = h.end;
    const after = text.slice(h.end);
    if (closesAt(after)) {
      out += squeeze(h.l) + h.r;
      continue;
    }
    if (opensAt(text.slice(0, h.at))) {
      out += h.l + squeeze(h.r);
      continue;
    }
    const n = hits[i + 1];
    const inner = n && text.slice(h.end, n.at);
    if (n && !BOUNDARY.test(inner) && balanced(inner)) {
      const rest = text.slice(n.end);
      const tight = closesAt(rest) || rendersSpace(n.r) || /^[,.;:!?]/.test(rest);
      out += (rendersSpace(h.l) ? h.l : h.l + ' ') + '(' + squeeze(h.r) + inner;
      out += ')' + squeeze(n.l) + (tight ? n.r : n.r + ' ');
      last = n.end;
      i++;
      continue;
    }
    const word = ((after.replace(/^(?:\\\n[ \t]*)+/, '').match(/^[`*_"“(]*([A-Za-z.]+)/) || [])[1] || '').toLowerCase();
    const comma = COMMA_WORDS.has(word) || COMMA_WORDS.has(word.replace(/\.$/, ''));
    const tail = squeeze(h.l) + h.r;
    out += (comma ? ',' : ':') + (rendersSpace(tail) ? tail : tail + ' ');
  }
  return out + text.slice(last);
}

// `undash` over lines that read as one text (a `///` block, a Markdown
// file), handed back as the same number of lines. A line that gained a
// leading space from a dash moved off its end loses it again.
function undashLines(lines) {
  const next = undash(lines.join('\n')).split('\n');
  if (next.length !== lines.length) throw new Error('undash changed the line count');
  return next.map((l, i) => (/^\s/.test(lines[i]) ? l : l.replace(/^[ \t]+/, '')));
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
    .replace(PROJECT_FILE, '.kete.json')
    .replace(PROSE, (m) => (m === 'Nebula' ? 'Kete' : 'kete'));
}

function rewriteLiteral(lit, file) {
  let t = lit.replace(/CARGO_BIN_EXE_nebula\b/g, 'CARGO_BIN_EXE_kete');
  if (t === '"nebula"') return BIN_NAME_FILES.some((r) => r.test(file)) ? '"kete"' : t;
  for (const [a, b] of TAGLINE) if (t === a) t = b;
  return undash(prose(t));
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

// The CLI's `///` docs as blocks: a run of consecutive `///` lines is one
// text to the dash transform, since an aside often spans two of them.
function undashDocBlocks(src) {
  const lines = src.split('\n');
  const DOC = /^(\s*\/\/\/ ?)(.*)$/;
  for (let i = 0; i < lines.length; ) {
    if (!DOC.test(lines[i]) || lines[i].includes(KEEP)) { i++; continue; }
    let j = i;
    while (j < lines.length && DOC.test(lines[j]) && !lines[j].includes(KEEP)) j++;
    const parts = lines.slice(i, j).map((l) => l.match(DOC));
    const next = undashLines(parts.map((m) => m[2]));
    for (let k = i; k < j; k++) lines[k] = (parts[k - i][1] + next[k - i]).replace(/\s+$/, '');
    i = j;
  }
  return lines.join('\n');
}

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
  if (file === CLI_DOCS) out = undashDocBlocks(out);
  writeIfChanged(p, out);
}

// ---------------------------------------------------------- 4. user docs

// The pages README links to that come from upstream. Kete's own pages
// (PORT.md, ports.md, profiles.md) are written without em dashes.
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
  const lines = readLF(p)
    .split('\n')
    .map((line) => (line.includes(KEEP) ? line : prose(line)));
  // The prose between fences is undashed. Fenced code stays as it is, but
  // for its `# ` comments, which read as prose (a run of them as one text).
  const COMMENT = /^(.*?(?:^|\s)#\s)(.*)$/;
  const out = [];
  let run = [];
  let fenced = false;
  const flush = () => {
    if (!run.length) return;
    if (fenced) {
      const parts = run.map((l) => l.match(COMMENT));
      out.push(...undashLines(parts.map((m) => m[2])).map((t, i) => parts[i][1] + t));
    } else {
      out.push(...undashLines(run));
    }
    run = [];
  };
  for (const line of lines) {
    if (/^\s*```/.test(line)) {
      flush();
      fenced = !fenced;
      out.push(line);
    } else if (line.includes(KEEP) || (fenced && !COMMENT.test(line))) {
      flush();
      out.push(line);
    } else {
      run.push(line);
    }
  }
  flush();
  writeIfChanged(p, out.join('\n'));
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
