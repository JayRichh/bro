import { expect, test, vi } from 'vitest';
import { createNav } from './history.svelte';

test('back and forward restore places and per-view state', () => {
  const n = createNav();
  n.go({ view: 'tasks', repo: 'api' });
  n.ui('tasks', 'api').filter = 'gst';
  n.go({ view: 'branches', repo: 'api' });
  n.back();
  expect(n.current.view).toBe('tasks');
  expect(n.ui('tasks', 'api').filter).toBe('gst');
  n.forward();
  expect(n.current.view).toBe('branches');
});

test('go after back drops the forward stack', () => {
  const n = createNav();
  n.go({ view: 'tasks', repo: 'api' });
  n.go({ view: 'branches', repo: 'api' });
  n.back();
  n.go({ view: 'prs', repo: 'api' });
  expect(n.canForward).toBe(false);
  n.forward();
  expect(n.current.view).toBe('prs');
  n.back();
  expect(n.current.view).toBe('tasks');
  n.back();
  expect(n.current.view).toBe('overview');
  expect(n.canBack).toBe(false);
  n.back();
  expect(n.current.view).toBe('overview');
});

test('going to the place already shown adds no history', () => {
  const n = createNav();
  n.go({ view: 'tasks', repo: 'api' });
  n.go({ view: 'tasks', repo: 'api' });
  n.back();
  expect(n.current.view).toBe('overview');
});

test('per-view state is kept apart by view and repo', () => {
  const n = createNav();
  n.ui('tasks', 'api').filter = 'gst';
  expect(n.ui('tasks', 'web').filter).toBeUndefined();
  expect(n.ui('branches', 'api').filter).toBeUndefined();
});

test('a change to view state is saved once, after a pause', () => {
  vi.useFakeTimers();
  const save = vi.fn();
  const n = createNav({ save, debounceMs: 300 });
  const s = n.ui('tasks', 'api');
  s.filter = 'g';
  s.filter = 'gs';
  s.filter = 'gst';
  expect(save).not.toHaveBeenCalled();
  vi.advanceTimersByTime(300);
  expect(save).toHaveBeenCalledTimes(1);
  expect(save).toHaveBeenCalledWith('tasks', 'api', { filter: 'gst' });
  vi.useRealTimers();
});

test('seeded state shows through without being saved back', () => {
  vi.useFakeTimers();
  const save = vi.fn();
  const n = createNav({ save, debounceMs: 10 });
  n.seed('api', { tasks: { filter: 'old' } });
  expect(n.ui('tasks', 'api').filter).toBe('old');
  vi.advanceTimersByTime(50);
  expect(save).not.toHaveBeenCalled();
  vi.useRealTimers();
});
