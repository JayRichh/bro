import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./api', () => ({ api: {} }));

const { app } = await import('./store.svelte');
const { nav } = await import('./shell/history.svelte');
const { runShortcut, toggleMaximize } = await import('./commands');

describe('the full pane', () => {
  beforeEach(() => {
    app.paneMax = false;
    nav.go({ view: 'overview' });
  });

  it('does nothing without a split', () => {
    expect(toggleMaximize()).toBe(false);
    expect(runShortcut('maximize')).toBe(false);
    expect(app.paneMax).toBe(false);
  });

  it('toggles with a split up, by the key or the button', () => {
    nav.go({ view: 'diff', item: 'w1' });
    expect(runShortcut('maximize_z')).toBe(true);
    expect(app.paneMax).toBe(true);
    expect(runShortcut('maximize_z')).toBe(true);
    expect(app.paneMax).toBe(false);
    expect(runShortcut('maximize')).toBe(true);
    expect(app.paneMax).toBe(true);
    expect(toggleMaximize()).toBe(true);
    expect(app.paneMax).toBe(false);
  });

  it('comes back down on the first Esc and closes the split on the next', () => {
    nav.go({ view: 'overview' });
    nav.go({ view: 'session', item: 'a:x' });
    app.paneMax = true;
    expect(runShortcut('close_split')).toBe(true);
    expect(app.paneMax).toBe(false);
    expect(nav.current.view).toBe('session');
    expect(runShortcut('close_split')).toBe(true);
    expect(nav.current.view).toBe('overview');
  });
});

describe('the shared keys', () => {
  beforeEach(() => {
    app.paneMax = false;
    nav.go({ view: 'overview' });
  });

  it('f and / open the palette, unless the view filters itself', async () => {
    const { ui } = await import('./ui/ui.svelte');
    ui.palette = false;
    expect(runShortcut('find')).toBe(true);
    expect(ui.palette).toBe(true);
    ui.palette = false;
    nav.go({ view: 'diff', item: 'w1' });
    expect(runShortcut('find')).toBe(false);
    nav.go({ view: 'branches', repo: 'p1' });
    expect(runShortcut('find')).toBe(false);
    expect(runShortcut('refresh')).toBe(false);
    expect(ui.palette).toBe(false);
  });

  it('a digit opens that repo on the rail, in the rail order', () => {
    const was = app.model.projects;
    app.model.projects = [
      { id: 'p1', name: 'api', repo_path: '/api', sort_order: 1 },
      { id: 'p2', name: 'web', repo_path: '/web', sort_order: 0 },
    ] as never;
    expect(runShortcut('repo_1')).toBe(true);
    expect(nav.current.repo).toBe('p2');
    expect(runShortcut('repo_2')).toBe(true);
    expect(nav.current.repo).toBe('p1');
    expect(runShortcut('repo_3')).toBe(false);
    app.model.projects = was;
  });
});
