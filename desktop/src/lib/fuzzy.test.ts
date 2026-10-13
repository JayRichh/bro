import { describe, expect, it } from 'vitest';
import { fuzzyMatch, rank } from './fuzzy';

// The same cases crates/nebula-fuzzy pins, so the palette ranks as `/` does.
describe('fuzzyMatch', () => {
  it('matches a subsequence and keeps the leftmost of equal starts', () => {
    expect(fuzzyMatch('srv', 'src/server.rs')?.positions).toEqual([0, 1, 7]);
  });
  it('prefers the consecutive run over a scattered greedy pass', () => {
    expect(fuzzyMatch('serv', 'src/server.rs')?.positions).toEqual([4, 5, 6, 7]);
    expect(fuzzyMatch('serv', 'src/server.rs')!.score).toBeGreaterThan(fuzzyMatch('serv', 's_e_r_v.rs')!.score);
  });
  it('fails on a missing char and ignores case', () => {
    expect(fuzzyMatch('xyz', 'src/server.rs')).toBeNull();
    expect(fuzzyMatch('abc', 'ab')).toBeNull();
    expect(fuzzyMatch('READ', 'readme.md')).not.toBeNull();
  });
  it('scores a segment start above a mid-word match', () => {
    expect(fuzzyMatch('ui', 'src/ui.rs')!.score).toBeGreaterThan(fuzzyMatch('ui', 'build.rs')!.score);
  });
  it('matches space-separated terms independently, in any order, all required', () => {
    expect(fuzzyMatch('neb #10', 'nebula/#10 Credit Codex and Cursor in the README')?.positions).toEqual([0, 1, 2, 7, 8, 9]);
    expect(fuzzyMatch('#10 neb', 'nebula/#10 Credit Codex')).not.toBeNull();
    expect(fuzzyMatch('neb #11', 'nebula/#10 Credit Codex')).toBeNull();
    expect(fuzzyMatch('ne neb', 'nebula/main')?.positions).toEqual([0, 1, 2]);
  });
  it('matches everything on an empty or blank query', () => {
    expect(fuzzyMatch('', 'x')).toEqual({ score: 0, positions: [] });
    expect(fuzzyMatch('   ', 'x')).toEqual({ score: 0, positions: [] });
  });
});

describe('rank', () => {
  const rows = ['ledger-api/main', 'ledger-api/gst-rounding', 'storefront-web/main'];
  it('keeps an empty query in caller order', () => {
    const out = rank('', rows, (r) => r, (r) => (r.startsWith('store') ? 0 : 1));
    expect(out.map((o) => o.item)).toEqual(['storefront-web/main', 'ledger-api/main', 'ledger-api/gst-rounding']);
  });
  it('puts the best score first and drops misses', () => {
    const out = rank('gst', rows, (r) => r);
    expect(out.map((o) => o.item)).toEqual(['ledger-api/gst-rounding']);
    const both = rank('main', rows, (r) => r, (r) => (r.startsWith('store') ? 0 : 1));
    expect(both[0].item).toBe('storefront-web/main');
  });
});
