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
