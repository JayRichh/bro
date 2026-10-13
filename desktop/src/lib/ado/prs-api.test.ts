import { describe, expect, it } from 'vitest';
import { composeItem, parseItem, prItem } from './prs-api';

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
