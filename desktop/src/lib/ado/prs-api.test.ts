import { describe, expect, it } from 'vitest';
import { composeItem, parseItem, policyMark, prItem, prRange } from './prs-api';

describe('prRange', () => {
  it('pins the review diff to the last merge commits', () => {
    const t = 'a'.repeat(40);
    const s = 'b'.repeat(40);
    expect(prRange({ id: 12, targetCommit: t, sourceCommit: s })).toEqual({ base: t, tip: s, pr: 12 });
  });

  it('falls back to the merge ref parents, never a branch name', () => {
    expect(prRange({ id: 12, targetCommit: null, sourceCommit: 'b'.repeat(40) })).toEqual({
      base: 'refs/pull/12/merge^1',
      tip: 'refs/pull/12/merge^2',
      pr: 12,
    });
  });
});

describe('policyMark', () => {
  it('names unreadable policies in words', () => {
    expect(policyMark('unavailable', false)).toEqual({ square: 'border border-fail', word: 'unavailable' });
  });
});

describe('parseItem', () => {
  it('reads the list, a PR and the composer', () => {
    expect(parseItem(undefined)).toEqual({ kind: 'list' });
    expect(parseItem(prItem(42))).toEqual({ kind: 'pr', id: 42 });
    expect(parseItem('42')).toEqual({ kind: 'pr', id: 42 });
    expect(parseItem('compose:jari/gst-rounding')).toEqual({ kind: 'compose', branch: 'jari/gst-rounding', task: null });
    expect(parseItem('compose:jari/gst-rounding:task:4211')).toEqual({
      kind: 'compose',
      branch: 'jari/gst-rounding',
      task: 4211,
    });
  });

  it('round-trips through composeItem', () => {
    expect(parseItem(composeItem('ga/fix', 7))).toEqual({ kind: 'compose', branch: 'ga/fix', task: 7 });
    expect(parseItem(composeItem('ga/fix'))).toEqual({ kind: 'compose', branch: 'ga/fix', task: null });
  });

  it('falls back to the list for nonsense', () => {
    expect(parseItem('pr:abc')).toEqual({ kind: 'list' });
    expect(parseItem('0')).toEqual({ kind: 'list' });
  });
});
