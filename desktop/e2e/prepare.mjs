// Gets everything the desktop end-to-end suite runs, then writes
// e2e/.tools/paths.json:
// - msedgedriver matching the installed WebView2 runtime, from Microsoft's
//   own download host;
// - tauri-driver, pinned, installed with cargo into e2e/.tools;
// - `kete` and the fake agent (the TUI suite's e2e_windows binary), built
//   from this checkout's root workspace;
// - kete-desktop.exe, a debug build with the frontend embedded (no dev
//   server, no installer).
// Windows only: tauri-driver drives WebView2 there through msedgedriver.
//
// `node e2e/prepare.mjs [--skip-build]`

import { execFileSync, execSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { E2E, TOOLS, PATHS_FILE } from './paths.mjs';

const TAURI_DRIVER = '2.1.0';
const DESKTOP = path.resolve(E2E, '..');
const ROOT = path.resolve(DESKTOP, '..');
const skipBuild = process.argv.includes('--skip-build');

if (process.platform !== 'win32') {
  console.error('the desktop e2e suite runs on Windows (tauri-driver over WebView2)');
  process.exit(1);
}
fs.mkdirSync(TOOLS, { recursive: true });

/** The WebView2 runtime's version: the machine-wide install, else the
 *  user's. */
function webview2Version() {
  const key = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  for (const hive of [
    `HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\${key}`,
    `HKLM\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\${key}`,
    `HKCU\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\${key}`,
  ]) {
    const out = spawnSync('reg', ['query', hive, '/v', 'pv'], { encoding: 'utf8' });
    const m = out.status === 0 && out.stdout.match(/pv\s+REG_SZ\s+([\d.]+)/);
    if (m && m[1] !== '0.0.0.0') return m[1];
  }
  // No EdgeUpdate key (some images): the newest runtime folder on disk.
  const dir = path.join(process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)', 'Microsoft', 'EdgeWebView', 'Application');
  const versions = fs.existsSync(dir) ? fs.readdirSync(dir).filter((v) => /^\d+\.\d+\.\d+\.\d+$/.test(v)) : [];
  const cmp = (a, b) => {
    const [x, y] = [a, b].map((v) => v.split('.').map(Number));
    return x.reduce((d, n, i) => d || n - y[i], 0);
  };
  if (versions.length) return versions.sort(cmp).at(-1);
  throw new Error('no WebView2 runtime found in the registry or under Program Files');
}

async function edgeDriver() {
  const version = webview2Version();
  const dir = path.join(TOOLS, `msedgedriver-${version}`);
  const exe = path.join(dir, 'msedgedriver.exe');
  if (fs.existsSync(exe)) return exe;
  fs.mkdirSync(dir, { recursive: true });
  const url = `https://msedgedriver.microsoft.com/${version}/edgedriver_win64.zip`;
  console.log(`fetching ${url}`);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  const zip = path.join(dir, 'edgedriver_win64.zip');
  fs.writeFileSync(zip, Buffer.from(await res.arrayBuffer()));
  execFileSync('powershell', ['-NoProfile', '-Command', `Expand-Archive -LiteralPath '${zip}' -DestinationPath '${dir}' -Force`], { stdio: 'inherit' });
  fs.rmSync(zip);
  // Microsoft signs it: refuse anything that isn't.
  const sig = execFileSync(
    'powershell',
    ['-NoProfile', '-Command', `$s = Get-AuthenticodeSignature -LiteralPath '${exe}'; "$($s.Status)|$($s.SignerCertificate.Subject)"`],
    { encoding: 'utf8' },
  ).trim();
  if (!sig.startsWith('Valid|CN=Microsoft Corporation,')) {
    fs.rmSync(dir, { recursive: true, force: true });
    throw new Error(`msedgedriver signature: ${sig}`);
  }
  return exe;
}

function tauriDriver() {
  const root = path.join(TOOLS, `tauri-driver-${TAURI_DRIVER}`);
  const exe = path.join(root, 'bin', 'tauri-driver.exe');
  if (!fs.existsSync(exe)) {
    execFileSync('cargo', ['install', 'tauri-driver', '--locked', '--version', TAURI_DRIVER, '--root', root], { stdio: 'inherit' });
  }
  return exe;
}

/** `kete` and the e2e_windows binary, from cargo's own report of them. */
function keteAndAgent() {
  const out = execFileSync(
    'cargo',
    ['test', '-p', 'nebula', '--locked', '--test', 'e2e_windows', '--no-run', '--message-format=json-render-diagnostics'],
    { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'], maxBuffer: 256 * 1024 * 1024 },
  );
  let kete = null;
  let agent = null;
  for (const line of out.split('\n')) {
    if (!line.startsWith('{')) continue;
    const m = JSON.parse(line);
    if (m.reason !== 'compiler-artifact' || !m.executable) continue;
    if (m.target.kind.includes('bin') && m.target.name === 'kete') kete = m.executable;
    if (m.target.kind.includes('test') && m.target.name === 'e2e_windows') agent = m.executable;
  }
  if (!kete || !agent) throw new Error('cargo did not report the kete and e2e_windows binaries');
  return { kete, agent };
}

function desktop() {
  const npx = process.platform === 'win32' ? 'npx.cmd' : 'npx';
  if (!skipBuild) {
    execSync(`${npx} tauri build --debug --no-bundle`, { cwd: DESKTOP, stdio: 'inherit', shell: true });
  }
  return path.join(DESKTOP, 'target', 'debug', 'kete-desktop.exe');
}

const edge = await edgeDriver();
const driver = tauriDriver();
const bins = skipBuild && fs.existsSync(PATHS_FILE) ? JSON.parse(fs.readFileSync(PATHS_FILE, 'utf8')) : keteAndAgent();
const out = { kete: bins.kete, agent: bins.agent, desktop: desktop(), tauriDriver: driver, edgeDriver: edge };
fs.writeFileSync(PATHS_FILE, JSON.stringify(out, null, 2));
console.log(out);
