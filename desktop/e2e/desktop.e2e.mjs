// The desktop's flows, one test each, on the real binaries: the built
// window over a real daemon, a scripted fake agent as Claude, temp repos
// with a local bare repo as `origin`. `npm run e2e` (Windows).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { Desktop, FEATURE, Key, git, ghDirs } from './harness.mjs';

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

// ---- critters ----

/** The arrow keys, as W3C key codes. */
const ARROW = { up: '', down: '', left: '', right: '' };

/** A mouse gesture as W3C pointer actions: synthetic input inside the
 *  WebView, never the OS pointer. `keep` leaves the button down for the
 *  next call (a drag checked halfway). */
async function mouse(d, steps, keep = false) {
  await d.wd.call('POST', '/actions', { actions: [{ type: 'pointer', id: 'mouse', parameters: { pointerType: 'mouse' }, actions: steps }] });
  if (!keep) await d.wd.call('DELETE', '/actions');
}
const moveTo = (p, dx = 0, dy = 0) => ({ type: 'pointerMove', duration: 0, origin: 'viewport', x: Math.round(p.x + dx), y: Math.round(p.y + dy) });
const press = (button) => ({ type: 'pointerDown', button });
const lift = (button) => ({ type: 'pointerUp', button });

flow('critters: the verb turns them on, a click pets, a right-click opens the critter menu, a drag picks one up, Konami brings them back', {}, async (d) => {
  await d.el('[aria-label="Home view"]');
  const on = () => {
    try {
      return d.config().critters === true;
    } catch {
      return false;
    }
  };
  assert.equal(on(), false, 'off until found');
  // The `critters` verb, from the palette.
  await d.press('k', Key.Control);
  await d.type('[aria-label="Command palette"] input', 'critters');
  await d.click('[role="listbox"] [role="option"]', 'Turn critters on');
  await d.waitFor('critters on in config.json', on);
  await d.waitForText('Critters are on');
  // The debug build's hook: the clock stopped, the world stepped by hand.
  const k = (body, ...args) => d.wd.exec(`const k = window.__kete_critters; ${body}`, ...args);
  await d.waitFor('the critters test hook', () => k('return !!k;'));
  await k('k.freeze(true); k.clear();');
  const id = await k(`return k.spawn('kiwi');`);
  assert.ok(id, 'a kiwi summoned');
  const kiwi = `return k.list().find((c) => c.id === arguments[0]) ?? null;`;
  await d.waitFor('the kiwi walked on', async () => {
    await k('for (let i = 0; i < 4; i++) k.step(0.5);');
    const c = await k(kiwi, id);
    return c && c.entered && c.mode === 'brain';
  });
  if (process.env.KETE_E2E_SHOTS) {
    await k(`k.spawn('cow'); for (let i = 0; i < 6; i++) k.step(0.5);`);
    fs.writeFileSync(path.join(process.env.KETE_E2E_SHOTS, 'critters.png'), Buffer.from(await d.wd.screenshot(), 'base64'));
  }

  // A click pets it (a heart), and the quick prompt keeps the focus.
  await d.wd.exec(`document.querySelector('form[aria-label="Start a session"] textarea').focus();`);
  const focused = `return document.activeElement?.getAttribute('aria-label') ?? null;`;
  assert.equal(await d.wd.exec(focused), 'Prompt');
  const p = await k('return k.point(arguments[0]);', id);
  assert.ok(p, 'a drawn pixel to click');
  await mouse(d, [moveTo(p), press(0), lift(0)]);
  await d.waitFor('a heart over the kiwi', () => k(`return k.particles().includes('icon:heart');`));
  assert.equal(await d.wd.exec(focused), 'Prompt', 'a click on a critter takes no focus');

  // A right-click opens the critter menu, every row in it.
  await mouse(d, [moveTo(await k('return k.point(arguments[0]);', id)), press(2), lift(2)]);
  await d.el('[role="menu"] [role="menuitem"]', 'Feed');
  const rows = await d.wd.exec(`return [...document.querySelectorAll('[role="menu"] [role="menuitem"]')].map((e) => e.innerText.trim());`);
  for (const r of ['Pet', 'Feed', 'Name', 'Shoo', 'Hide critters until tomorrow', 'Critter log', 'Critter settings']) {
    assert.ok(rows.some((x) => x.startsWith(r)), `the menu has ${r}: ${rows.join(', ')}`);
  }
  await d.press(Key.Escape);
  await d.waitFor('the menu closed', async () => !(await d.wd.exec(`return !!document.querySelector('[role="menu"]');`)));

  // The visit and the pet reach the collection log.
  const log = () => {
    try {
      return JSON.parse(fs.readFileSync(path.join(d.dataDir, 'critters.json'), 'utf8'));
    } catch {
      return null;
    }
  };
  // (The world hands its notes over as it ticks: step it along.)
  await d.waitFor('critters.json counts the kiwi', async () => {
    await k('k.step(0.05);');
    const l = log();
    return l?.species?.kiwi?.seen >= 1 && l.species.kiwi.pets >= 1;
  });

  // A drag picks it up; letting go drops it, and it lands on its feet.
  const q = await k('return k.point(arguments[0]);', id);
  await mouse(d, [moveTo(q), press(0), moveTo(q, 6, -6), moveTo(q, 20, -30), moveTo(q, 40, -60)], true);
  await d.waitFor('the kiwi held', async () => (await k(kiwi, id))?.mode === 'held');
  await mouse(d, [lift(0)]);
  assert.notEqual((await k(kiwi, id))?.mode, 'held', 'let go');
  await d.waitFor('the kiwi landed', async () => {
    await k('k.step(0.25);');
    return (await k(kiwi, id))?.mode === 'brain';
  });
  assert.equal(await d.wd.exec(focused), 'Prompt', 'a drag takes no focus either');

  // Nobody on screen: no animation frames, one timer to the next visitor.
  await k('k.clear(); k.ignoreFocus(true); k.freeze(false);');
  const before = await k('return k.stats();');
  await new Promise((r) => setTimeout(r, 1500));
  const after = await k('return k.stats();');
  assert.equal(after.active, false);
  assert.equal(after.frames, before.frames, 'no frames while nobody is on screen');
  assert.ok(after.wake > 0, 'a timer for the next visitor');
  assert.equal(after.boxes, 0, 'no hit boxes');

  // Off with the verb; the Konami code, outside a text field, turns them on.
  await d.press('k', Key.Control);
  await d.type('[aria-label="Command palette"] input', 'critters');
  await d.click('[role="listbox"] [role="option"]', 'Turn critters off');
  await d.waitFor('critters off in config.json', () => !on());
  await d.wd.exec('document.activeElement?.blur();');
  for (const key of [ARROW.up, ARROW.up, ARROW.down, ARROW.down, ARROW.left, ARROW.right, ARROW.left, ARROW.right, 'b', 'a']) await d.press(key);
  await d.waitFor('critters on again', on);
});
