import { describe, expect, it, vi } from 'vitest';

const seen: string[] = [];
vi.mock('./api', () => ({ api: { markSeen: async (id: string) => void seen.push(id) } }));
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));

const { plate } = await import('./update');
const { app } = await import('./store.svelte');
const { openNotified } = await import('./alerts');
const { nav } = await import('./shell/history.svelte');

describe('the version plate', () => {
  const status = (latest: string | null) => ({ version: '0.46.0', latest, url: 'u', command: 'kete upgrade' });

  it('names this build, and the newer release when one is out', () => {
    expect(plate(status(null))).toEqual({ version: 'kete v0.46.0', update: null });
    expect(plate(status('0.47.0'))).toEqual({ version: 'kete v0.46.0', update: 'v0.47.0' });
    expect(plate(null)).toEqual({ version: '', update: null });
  });
});

describe('a clicked notification', () => {
  it('opens its session, and nothing for one deleted since', () => {
    app.model = { ...app.model, agents: [{ id: 'a1', worktree_id: 'w' } as never], worktrees: [] };
    openNotified('gone');
    expect(app.session).toBeNull();
    openNotified('a1');
    expect(app.session).toEqual({ Agent: 'a1' });
    expect(nav.current.view).toBe('session');
    expect(seen).toEqual(['a1']);
  });
});
