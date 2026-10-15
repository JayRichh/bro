import { describe, expect, it } from 'vitest';
import type { Model } from './api';
import { againstLabel, againstTitle, canStage, groupWorktrees } from './diff-base';

const base = { sha: '0123456789abcdef', short: '0123456', branch: 'main' };

describe('against', () => {
  it('names the base and its merge-base, or HEAD', () => {
    expect(againstLabel('base', base)).toBe('vs main (merge-base 0123456)');
    expect(againstLabel('head', base)).toBe('vs HEAD');
    expect(againstLabel('base', null)).toBe('vs HEAD');
  });

  it('says what the toggle does, and why there is none', () => {
    expect(againstTitle('base', base)).toMatch(/HEAD instead/);
    expect(againstTitle('head', base)).toMatch(/main instead.*0123456/);
    expect(againstTitle('head', null)).toMatch(/no base branch/);
  });

  it('stages only in a working diff against HEAD', () => {
    expect(canStage(false, 'head')).toBe(true);
    expect(canStage(false, 'base')).toBe(false);
    expect(canStage(true, 'head')).toBe(false);
  });
});

describe('groupWorktrees', () => {
  const model = {
    projects: [
      { id: 'p2', name: 'web', repo_path: '/w', sort_order: 1 },
      { id: 'p1', name: 'api', repo_path: '/a', sort_order: 0 },
      { id: 'p3', name: 'empty', repo_path: '/e', sort_order: 2 },
    ],
    worktrees: [
      { id: 'w1', project_id: 'p2', path: '/w/x', branch: 'feat', is_main: false, sort_order: 0 },
      { id: 'w2', project_id: 'p2', path: '/w', branch: 'main', is_main: true, sort_order: 0 },
      { id: 'w3', project_id: 'p1', path: '/a', branch: 'main', is_main: true, sort_order: 0 },
    ],
  } as unknown as Model;

  it('groups by repo in rail order, main checkout first, empty repos left out', () => {
    const g = groupWorktrees(model);
    expect(g.map((x) => x.name)).toEqual(['api', 'web']);
    expect(g[1].worktrees.map((w) => w.branch)).toEqual(['main', 'feat']);
  });
});
