import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./api', () => ({ api: { settingsSet: async () => {}, markSeen: async () => {} } }));

const { app, closeDiff, openDiff, openSession, setHome } = await import('./store.svelte');
const { nav } = await import('./shell/history.svelte');

describe('home and the diff', () => {
  beforeEach(() => {
    app.home = 'overview';
    nav.go({ view: 'overview' });
  });

  it('a diff opened from the board closes back to the board', () => {
    setHome('board');
    expect(nav.current.view).toBe('board');
    openDiff('w1');
    expect(nav.current.view).toBe('diff');
    closeDiff();
    expect(nav.current.view).toBe('board');
  });

  it('Overview | Board keeps an open session or diff in the split', () => {
    openSession({ Agent: 'a1' });
    setHome('board');
    expect(nav.current.view).toBe('session');
    expect(app.home).toBe('board');
    openDiff('w1');
    setHome('overview');
    expect(nav.current.view).toBe('diff');
  });

  it('closing the diff lands on the home picked while it was up', () => {
    openDiff('w1');
    setHome('board');
    closeDiff();
    expect(nav.current.view).toBe('board');
  });
});
