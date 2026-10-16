// The desktop's flows, one test each, on the real binaries: the built
// window over a real daemon, a scripted fake agent as Claude, temp repos
// with a local bare repo as `origin`. `npm run e2e` (Windows).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { Desktop, FEATURE, Key, clipboardText, git, ghDirs, setClipboard } from './harness.mjs';

/** One window per test, closed (driver, daemon, temp root) however it ends. */
function flow(name, options, body) {
  test(name, { timeout: 180_000 }, async () => {
    const d = await Desktop.open(options);
    try {
      await body(d);
    } finally {
      await d.close();
    }
  });
}

/** Board home, the lanes up. */
async function toBoard(d) {
  await d.click('[aria-label="Home view"] button', 'Board');
  // An empty board is the empty state; the toggle says which home is up.
  await d.el('[aria-label="Home view"] [aria-checked="true"]', 'Board');
}

/** Back to the board from an open session (the window follows a launch
 *  into its session; one watched as it finishes is seen, so Done). */
async function leaveSession(d) {
  await d.click('nav[aria-label="Where you are"] ol button', 'Board');
  await d.waitFor('the board the place you are', async () =>
    (await d.wd.exec(`return document.querySelector('nav[aria-label="Where you are"] [aria-current="page"]')?.innerText;`)) === 'Board',
  );
}

/** Until no menu is up. */
function menuGone(d) {
  return d.waitFor('the menu closed', () => d.wd.exec(`return !document.querySelector('[role="menu"]');`));
}

/** The router's record of the last right-click (`kind`, `prevented`). */
function lastRightClick(d) {
  return d.wd.exec('return { ...window.__keteContextMenu };');
}

/** Reply from the board: the card's Follow-up, typed and sent. */
async function followUp(d, id, text) {
  // The card's tools show on hover or focus.
  await d.wd.exec('document.querySelector(arguments[0]).focus();', `${card(id)} > button`);
  await d.click(`${card(id)} [data-card-tools] button[aria-label^="Follow-up to"]`);
  await d.type('[role="dialog"] textarea', text);
  await d.click('[role="dialog"] button', 'Send');
}

/** A FEATURE session launched, left for the board, its question answered
 *  with the card's Follow-up, and its card in Review. Resolves to the
 *  session's id. */
async function toReview(d) {
  const id = await d.launch('build the feature');
  await leaveSession(d);
  await d.waitForLane(id, 'needs');
  await d.waitForAgent('step ask');
  await followUp(d, id, 'yes ship it');
  await d.waitForAgent('reply "yes ship it"');
  await d.waitForLane(id, 'unseen');
  // The turn's end reads its worktree at once: the commit counts well
  // before the half-minute poll would.
  await d.waitFor(
    'the Review card with its commit',
    () => d.wd.exec(`const c = document.querySelector(arguments[0]); return !!c?.closest('section[aria-label="Review"]') && c.innerText.includes('1 commit');`, card(id)),
    10_000,
  );
  return id;
}

const card = (id) => `[data-target="agent:${id}"]`;
const lane = (title) => `section[aria-label="${title}"]`;

flow('opens to the overview with a repo', {}, async (d) => {
  const repo = d.makeRepo('life-proj');
  await d.waitForText('Add a repo');
  await d.addRepo(repo);
  // The overview is home, the repo on it and in the rail.
  const on = await d.wd.exec(`return document.querySelector('[aria-label="Home view"] [aria-checked="true"]')?.innerText;`);
  assert.equal(on, 'Overview');
  const s = await d.invoke('get_state');
  assert.deepEqual(s.projects.map((p) => p.name), ['life-proj']);
});

flow('a session runs, needs you, takes a reply in its pane and lands in Review with its commit', { script: `gate:asked;ask;gate:replied;${FEATURE.slice('ask;'.length)}` }, async (d) => {
  const repo = d.makeRepo('run-proj');
  await d.addRepo(repo);
  await toBoard(d);
  // Started, the window opens its session.
  const id = await d.launch('build the feature');
  await d.el('.xterm-helper-textarea');
  await d.waitForLane(id, 'running');
  d.openGate('asked');
  await d.waitForLane(id, 'needs');
  // The pane draws on a canvas: the agent's log says it is at its question.
  await d.waitForAgent('step ask');
  await d.replyInPane('yes ship it');
  await d.waitForAgent('reply "yes ship it"');
  // Left before the turn ends, so its card is unread when it does.
  await leaveSession(d);
  d.openGate('replied');
  await d.waitForLane(id, 'unseen');
  // Counted from a read at the turn's end, not the half-minute poll.
  await d.waitFor(
    'the Review card with 1 commit',
    () => d.wd.exec(`const c = document.querySelector(arguments[0]); return !!c?.closest('section[aria-label="Review"]') && c.innerText.includes('1 commit');`, card(id)),
    10_000,
  );
  // The work is on the session's own worktree, committed whole.
  const [wt] = d.linkedWorktrees(repo);
  assert.equal(git(wt, 'log', '--format=%s', 'main..HEAD').trim(), 'add the feature');
  assert.equal(git(wt, 'status', '--porcelain').trim(), '');
});

flow('dragging a Review card onto Ship opens the ship sheet with the install-gh fix', { script: FEATURE, stripPath: ghDirs() }, async (d) => {
  const repo = d.makeRepo('ship-proj', { github: true });
  await d.addRepo(repo);
  await toBoard(d);
  const id = await toReview(d);
  // Ship takes it: the read at the turn's end found the commit to push.
  await d.waitFor('Ship open to the card', async () => (await d.invoke('get_state')).cards[id]?.drops.pr?.act === 'open_pr', 10_000);
  await d.settle();
  const hint = await d.drag(card(id), lane('Ship'));
  assert.equal(hint, 'Drop to open a pull request');
  await d.waitForText('Open a pull request');
  // The composer starts from the commit; with no gh the sheet says so and
  // carries the fix.
  await d.waitFor('the title from the commit', () =>
    d.wd.exec(`return document.querySelector('[aria-labelledby="ship-title"] input')?.value === 'add the feature';`),
  );
  await d.waitForText('The GitHub CLI (gh) is not installed');
  await d.el('[aria-labelledby="ship-title"] [role="alert"] button', 'How to install gh');
});

flow('dragging a Done card onto Archived offers the worktree delete', { script: 'stop;echo' }, async (d) => {
  const repo = d.makeRepo('drag-proj');
  await d.addRepo(repo);
  await toBoard(d);
  const id = await d.launch('tidy the docs');
  await d.waitForLane(id, 'done');
  const [wt] = d.linkedWorktrees(repo);
  assert.ok(wt && fs.existsSync(wt), 'the session has a worktree of its own');
  await d.drag(card(id), '[role="region"][aria-label="Archive"]');
  // Archived after the usual confirm, then the emptied worktree offered.
  await d.click('[role="dialog"] button, [role="alertdialog"] button', 'Archive');
  await d.waitForText('Delete its worktree too?');
  await d.click('[role="dialog"] button, [role="alertdialog"] button', 'Delete worktree');
  await d.waitFor('the worktree gone from disk', () => !fs.existsSync(wt));
  assert.ok((await d.agents()).find((a) => a.id === id)?.archived, 'archived');
});

flow('a Review card opens the review split: the diff against base with its commit, Alt+A archives', { script: FEATURE }, async (d) => {
  const repo = d.makeRepo('review-proj');
  await d.addRepo(repo);
  await toBoard(d);
  const id = await toReview(d);
  await d.click(`${card(id)} > button`);
  await d.waitForText('1 commit since main');
  await d.waitForText('add the feature');
  await d.waitForText('feature.txt');
  await d.waitForText('made by the fake agent');
  // The diff's own menu: Copy path:line copies the file on disk at the line
  // under the pointer.
  await d.contextClick('.editor.modified .view-line');
  const rows = await d.menu('Viewer');
  assert.deepEqual(rows.map((r) => r.label), ['Copy', 'Select all', 'Copy path:line']);
  assert.equal((await lastRightClick(d)).kind, 'viewer');
  const st = await d.invoke('get_state');
  const wt = st.worktrees.find((w) => w.id === st.agents.find((a) => a.id === id).worktree_id).path;
  const saved = clipboardText();
  try {
    await d.click('[role="menu"] [role="menuitem"]', 'Copy path:line');
    await d.waitFor('the path and line on the clipboard', () => clipboardText().endsWith('feature.txt:1'));
    assert.ok(clipboardText().startsWith(wt), 'the path is under the worktree');
  } finally {
    if (saved) setClipboard(saved);
  }
  await d.press('a', Key.Alt);
  await d.waitForText('Archive session');
  await d.click('[role="dialog"] button, [role="alertdialog"] button', 'Archive');
  await d.waitFor('the session archived', async () => (await d.agents()).find((a) => a.id === id)?.archived);
});

flow('the scope chip narrows the board and counts what is outside it', { script: FEATURE, scripts: { tidy: 'stop;echo' } }, async (d) => {
  const repo = d.makeRepo('scope-proj');
  await d.addRepo(repo);
  await toBoard(d);
  const review = await toReview(d);
  const done = await d.launch('tidy the docs');
  await d.waitForLane(done, 'done');
  // The chip, narrowed to the Done session's worktree.
  const st = await d.invoke('get_state');
  const branch = st.worktrees.find((w) => w.id === st.agents.find((a) => a.id === done).worktree_id).branch;
  await d.click('[aria-label="Scope"] button', 'all');
  await d.click('[role="menu"] [role="menuitem"]', 'scope-proj');
  await d.click('[aria-label="Change the scope"]');
  await d.click('[role="menu"] [role="menuitem"]', branch);
  await d.waitForText('1 more outside scope');
  assert.equal(await d.laneOf(review), null, 'the Review card is outside the scope');
  await d.waitFor('the chip naming the worktree', () =>
    d.wd.exec(`return document.querySelector('[aria-label="Scope"]').innerText.includes(arguments[0]);`, branch),
  );
  // Its x clears it.
  await d.click('[aria-label="Clear the scope"]');
  await d.waitForGone('outside scope');
  await d.waitForLane(review, 'unseen');
});

flow('Settings, Hotkeys: a rebound key saves to config.json and works after a reload', {}, async (d) => {
  await d.click('button', 'Settings');
  await d.click('button, [role="tab"]', 'Hotkeys');
  // Keyboard shortcuts (F1) gets F2 too.
  const addKey = `for (const li of document.querySelectorAll('li'))
      if (/^Keyboard shortcuts/.test(li.innerText)) return [...li.querySelectorAll('button')].find((b) => b.innerText.includes('Add key'))?.click() ?? true;
    return false;`;
  await d.waitFor('the Keyboard shortcuts row', () => d.wd.exec(addKey));
  await d.waitForText('Press a key');
  await d.press(Key.F2);
  await d.waitFor('desktop.help saved', () => (d.config().keybindings?.['desktop.help'] ?? []).includes('f2'));
  // A fresh page: the window reads its keys from config.json again.
  await d.wd.exec('window.__kete_e2e_before_reload = true; location.reload();');
  await d.waitFor('the page reloaded', async () => !(await d.wd.exec('return window.__kete_e2e_before_reload === true;')) && (await d.invoke('get_state')).connected);
  const help = `return !!document.querySelector('[role="dialog"][aria-labelledby="help-title"]');`;
  assert.equal(await d.wd.exec(help), false);
  await d.el('[aria-label="Home view"]');
  await d.press(Key.F2);
  await d.waitFor('the shortcuts overlay on F2', () => d.wd.exec(help));
});

flow('? on a Review card lists Create pull request', { script: FEATURE }, async (d) => {
  const repo = d.makeRepo('verbs-proj');
  await d.addRepo(repo);
  await toBoard(d);
  const id = await toReview(d);
  await d.wd.exec(`document.querySelector(arguments[0] + ' > button').focus();`, card(id));
  await d.press('?');
  await d.waitForText('Create pull request');
  await d.press(Key.Escape);
  await d.waitForGone('Create pull request');
});

flow("right-click a field opens Kete's Edit menu, never WebView2's; Paste fills it; Alt+Left goes back", { env: { KETE_BROWSER_KEYS: 'off' } }, async (d) => {
  const repo = d.makeRepo('menu-proj');
  // A field's menu in a dialog shows over the dialog.
  await d.click('button', 'Add a repo');
  await d.contextClick('input[aria-label="Repo folder"]');
  await d.menu('Edit');
  const onTop = await d.wd.exec(`const r = document.querySelector('[role="menu"]').getBoundingClientRect();
    return !!document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2)?.closest('[role="menu"]');`);
  assert.equal(onTop, true, 'the menu is drawn over the dialog');
  await d.press(Key.Escape);
  await menuGone(d);
  await d.press(Key.Escape);
  await d.waitForGone('Repo folder');
  await d.addRepo(repo);
  const prompt = 'form[aria-label="Start a session"] textarea[aria-label="Prompt"]';
  const inField = (js) => d.wd.exec(`const f = document.querySelector(arguments[0]); ${js}`, prompt);
  await d.type(prompt, 'hello menus');
  await inField('f.focus(); f.select();');
  // On the text itself, so the right-click keeps the selection.
  const width = await inField('return f.getBoundingClientRect().width;');
  await d.contextClick(prompt, undefined, { x: 40 - width / 2 });
  const rows = await d.menu('Edit');
  assert.deepEqual(rows.map((r) => r.label), ['Undo', 'Cut', 'Copy', 'Paste', 'Select all']);
  for (const verb of ['Cut', 'Copy']) assert.equal(rows.find((r) => r.label === verb).disabled, false, `${verb} works on the selection`);
  const seen = await lastRightClick(d);
  assert.equal(seen.prevented, true, "WebView2's own menu was stopped");
  assert.equal(seen.kind, 'text');
  // The menu left the focus in the field.
  assert.equal(await inField('return document.activeElement === f;'), true);
  await d.press(Key.Escape);
  await menuGone(d);
  assert.equal(await inField('return f.value;'), 'hello menus');
  // Paste: the clipboard's text, as typed.
  const saved = clipboardText();
  try {
    setClipboard('pasted by the menu');
    await inField(`f.value = ''; f.dispatchEvent(new Event('input', { bubbles: true }));`);
    await d.contextClick(prompt);
    await d.menu('Edit');
    await d.click('[role="menu"] [role="menuitem"]', 'Paste');
    await d.waitFor('the pasted text in the field', async () => (await inField('return f.value;')) === 'pasted by the menu');
  } finally {
    // A clipboard that held no text (an image, files) is not put back.
    if (saved) setClipboard(saved);
  }
  // Alt+Left and Alt+Right are Kete's back and forward: with the browser
  // keys off, WebView2 no longer takes them.
  const place = () => d.wd.exec(`return document.querySelector('nav[aria-label="Where you are"] [aria-current="page"]')?.innerText ?? null;`);
  const blur = () => d.wd.exec('document.activeElement?.blur();');
  await d.click('button', 'Settings');
  await d.waitForText('Hotkeys');
  await d.waitFor('Settings the place you are', async () => (await place()) === 'Settings');
  await blur();
  await d.press(Key.ArrowLeft, Key.Alt);
  await d.waitFor('back out of Settings', async () => {
    const p = await place();
    return !!p && p !== 'Settings';
  });
  await d.el('[aria-label="Home view"]');
  await blur();
  await d.press(Key.ArrowRight, Key.Alt);
  await d.waitFor('forward to Settings', async () => (await place()) === 'Settings');
  await d.waitForText('Hotkeys');
  // WebDriver's keys reach the page without passing WebView2's browser-key
  // layer (F5 and Ctrl+P do nothing here even with the keys on), so F5,
  // Ctrl+P and the like are checked with real keystrokes, not here.
});

flow("right-click a terminal, a board card and Shift+F10 open Kete's menus, one at a time", { script: 'stop;echo' }, async (d) => {
  const repo = d.makeRepo('menu-board-proj');
  await d.addRepo(repo);
  await toBoard(d);
  // Started, the window opens its session.
  const id = await d.launch('tidy the docs');
  await d.el('.xterm-helper-textarea');
  await d.contextClick('.xterm-screen');
  const rows = await d.menu('Terminal');
  assert.deepEqual(rows.slice(0, 4).map((r) => r.label), ['Copy', 'Paste', 'Select all', 'Clear']);
  assert.ok(rows.some((r) => r.label === 'Copy folder path'), "the session's folder is offered");
  assert.equal((await lastRightClick(d)).kind, 'terminal');
  // The menu left the focus in the terminal.
  assert.equal(await d.wd.exec(`return document.activeElement?.classList.contains('xterm-helper-textarea') === true;`), true);
  await d.press(Key.Escape);
  await menuGone(d);
  // Paste reaches the agent as Ctrl+V sends it: bracketed, once the agent
  // has asked for that (its echo step).
  await d.waitForAgent('step echo');
  const echoed = () => {
    try {
      return fs.readFileSync(path.join(d.root, 'echo.log'), 'utf8');
    } catch {
      return '';
    }
  };
  const saved = clipboardText();
  try {
    setClipboard('menu paste');
    await d.contextClick('.xterm-screen');
    const pasteRows = await d.menu('Terminal');
    assert.equal(pasteRows.find((r) => r.label === 'Paste').disabled, false, 'Paste is on for a live session');
    await d.click('[role="menu"] [role="menuitem"]', 'Paste');
    await d.waitFor('the paste in the agent', () => echoed().includes('\x1b[200~menu paste\x1b[201~'));
  } finally {
    // A clipboard that held no text (an image, files) is not put back.
    if (saved) setClipboard(saved);
  }
  // The card: the menu its Menu key opens, named for the session.
  await leaveSession(d);
  await d.waitForLane(id, 'done');
  const { name } = (await d.agents()).find((a) => a.id === id);
  await d.contextClick(`${card(id)} > button`);
  await d.menu(name);
  assert.equal((await lastRightClick(d)).kind, 'row');
  await d.press(Key.Escape);
  await menuGone(d);
  // Shift+F10 on the focused card: one menu, not the key's and the router's.
  await d.wd.exec(`document.querySelector(arguments[0] + ' > button').focus();`, card(id));
  await d.press(Key.F10, Key.Shift);
  // The row key's menu (labelled Actions; the right-click's is named for the
  // row), with the card's verbs.
  await d.menu('Actions');
  await d.el('[role="menu"] [role="menuitem"]', 'Archive');
  await new Promise((r) => setTimeout(r, 400));
  assert.equal(await d.wd.exec(`return document.querySelectorAll('[role="menu"]').length;`), 1);
  await d.press(Key.Escape);
  await menuGone(d);
  // The session's worktree band on the overview: the band's own menu (Open
  // folder, Copy path), over its title's data-target too.
  await d.click('[aria-label="Home view"] button', 'Overview');
  const st = await d.invoke('get_state');
  const branch = st.worktrees.find((w) => w.id === st.agents.find((a) => a.id === id).worktree_id).branch;
  await d.contextClick(`[data-target^="worktree:"]`, branch);
  const band = await d.menu(branch);
  for (const row of ['Open folder', 'Copy path']) assert.ok(band.some((r) => r.label === row), `the band's ${row}`);
  assert.equal((await lastRightClick(d)).kind, 'own');
  await d.press(Key.Escape);
  await menuGone(d);
});

// The webview's settings held back (KETE_QUIET_DELAY_MS, debug builds) until
// the app's page shows: it is restarted under them and comes up again, not
// blank.
flow('the window comes up when the webview settings land late', { env: { KETE_QUIET_DELAY_MS: '1500' } }, async (d) => {
  const said = () => {
    try {
      return fs.readFileSync(d.driverLog, 'utf8');
    } catch {
      return '';
    }
  };
  await d.waitFor('the late settings restarting the page', () => said().includes('kete-desktop: webview settings set, restarting'));
  await d.waitFor('the app page back', () =>
    d.wd.exec(`return location.host === 'tauri.localhost' && !!window.__keteContextMenu && document.body.innerText.includes('Add a repo');`),
  );
});

// Settings, Desktop, Appearance in the narrowest window at the largest
// interface size (the window zoomed to 16/14, so 640 px holds 560 of the
// page's): One Dark worn from its theme.css block, the terminal font row
// naming its family, the actions and every row inside the page with no
// sideways scroll. Then Reset appearance, confirmed, puts Kete's own look
// back and leaves the imported themes alone.
flow(
  'Settings, Appearance fits a 640px window at the largest interface size and resets',
  { config: { desktop: { theme: 'one-dark', ui_size: '16', font_term: 'Consolas' } } },
  async (d) => {
    await d.wd.call('POST', '/window/rect', { width: 640, height: 760 });
    // The rail is the compact one at this width: Settings is its gear.
    await d.click('button[aria-label="Settings"]');
    await d.waitForText('Interface font');
    assert.equal(await d.wd.exec('return document.documentElement.dataset.palette ?? null;'), 'one-dark');
    assert.equal(await d.wd.exec('return document.documentElement.dataset.theme;'), 'dark');
    await d.el('button', 'Consolas');
    const fits = `const page = document.documentElement;
      const panel = document.querySelector('[role="tabpanel"]');
      const out = [];
      if (page.scrollWidth > page.clientWidth + 1) out.push('page ' + page.scrollWidth + ' > ' + page.clientWidth);
      if (panel && panel.scrollWidth > panel.clientWidth + 1) out.push('panel ' + panel.scrollWidth + ' > ' + panel.clientWidth);
      for (const b of document.querySelectorAll('[role="tabpanel"] button')) {
        const r = b.getBoundingClientRect();
        if (r.width && r.right > innerWidth + 1) out.push((b.innerText || b.getAttribute('aria-label')) + ' ends at ' + Math.round(r.right) + ' of ' + innerWidth);
      }
      return out;`;
    assert.deepEqual(await d.wd.exec(fits), []);
    assert.ok((await d.wd.exec('return innerWidth;')) < 600, 'the interface size zooms the page');

    await d.click('button', 'Reset appearance to Kete defaults');
    await d.click('[role="dialog"] button', 'Reset appearance');
    await d.waitFor('Workshop back', async () => (await d.wd.exec('return document.documentElement.dataset.palette ?? null;')) === null);
    await d.waitFor('the look reset in config.json', () => {
      const desk = d.config().desktop ?? {};
      return desk.theme === 'system' && desk.font_term === undefined && desk.ui_size === undefined;
    });
    await d.waitFor('the zoom back to 1', async () => (await d.wd.exec('return innerWidth;')) > 600);
  },
);
