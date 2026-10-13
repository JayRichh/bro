import { describe, expect, it } from 'vitest';
import { blockLine, replacementModel } from './replacement';

describe('replacementModel', () => {
  it('marks nothing when the texts are the same', () => {
    const m = replacementModel('a\nb\n', 'a\nb\n');
    expect(m).toEqual({ text: 'a\nb\n', lines: [], spans: [], removed: [], blocks: [] });
  });

  it('returns the new text unchanged', () => {
    const m = replacementModel('one\r\ntwo\r\n', 'one\r\n2\r\n');
    expect(m.text).toBe('one\r\n2\r\n');
  });

  it('marks inserted lines whole, with no spans', () => {
    const m = replacementModel('a\nc\n', 'a\nb1\nb2\nc\n');
    expect(m.lines).toEqual([2, 3]);
    expect(m.spans).toEqual([]);
    expect(m.removed).toEqual([]);
    expect(m.blocks).toEqual([{ start: 2, end: 3, old: [] }]);
  });

  it('anchors a removal at the top above line 1', () => {
    const m = replacementModel('x\ny\na\nb', 'a\nb');
    expect(m.lines).toEqual([]);
    expect(m.removed).toEqual([{ afterLine: 0, lines: ['x', 'y'] }]);
    expect(blockLine(m.blocks[0], 2)).toBe(1);
  });

  it('anchors a removal in the middle after the line before it', () => {
    const m = replacementModel('a\ngone\nb\n', 'a\nb\n');
    expect(m.removed).toEqual([{ afterLine: 1, lines: ['gone'] }]);
    expect(m.blocks).toEqual([{ start: 2, end: 1, old: ['gone'] }]);
  });

  it('anchors a removal at the end after the last line', () => {
    const m = replacementModel('a\nb\nc', 'a\nb');
    expect(m.removed).toEqual([{ afterLine: 2, lines: ['c'] }]);
    expect(blockLine(m.blocks[0], 2)).toBe(2);
  });

  it('marks the changed words inside a rewritten line', () => {
    const m = replacementModel('let total = a + b;\n', 'let total = a * c;\n');
    expect(m.lines).toEqual([1]);
    // `*` at column 15, `c` at column 17.
    expect(m.spans).toEqual([
      { line: 1, startColumn: 15, endColumn: 16 },
      { line: 1, startColumn: 17, endColumn: 18 },
    ]);
    expect(m.blocks[0].old).toEqual(['let total = a + b;']);
  });

  it('merges neighbouring changed words into one span', () => {
    const m = replacementModel('call(alpha)', 'call(beta, gamma)');
    expect(m.spans).toEqual([{ line: 1, startColumn: 6, endColumn: 17 }]);
  });

  it('marks a line with nothing in common whole, without spans', () => {
    const m = replacementModel('foo\n', 'bar baz\n');
    expect(m.lines).toEqual([1]);
    expect(m.spans).toEqual([]);
  });

  it('keeps old lines beyond the rewritten ones as a removal', () => {
    const m = replacementModel('a\nold1\nold2\nold3\nz', 'a\nnew1\nz');
    expect(m.lines).toEqual([2]);
    expect(m.removed).toEqual([{ afterLine: 2, lines: ['old2', 'old3'] }]);
  });

  it('reads CRLF and LF lines alike', () => {
    const m = replacementModel('a\r\nb\r\nc\r\n', 'a\nB\nc\n');
    expect(m.lines).toEqual([2]);
    expect(m.removed).toEqual([]);
  });

  it('sees a trailing newline added as a change on the last line', () => {
    const m = replacementModel('a\nb', 'a\nb\n');
    expect(m.lines).toEqual([3]);
    expect(m.removed).toEqual([]);
  });

  it('marks every line of a new file', () => {
    const m = replacementModel('', 'x\ny');
    expect(m.lines).toEqual([1, 2]);
    expect(m.removed).toEqual([]);
  });

  it('marks a file emptied as one removal above line 1', () => {
    const m = replacementModel('x\ny', '');
    expect(m.lines).toEqual([]);
    expect(m.removed).toEqual([{ afterLine: 0, lines: ['x', 'y'] }]);
  });

  it('finds separate blocks in order', () => {
    const m = replacementModel('1\n2\n3\n4\n5\n6\n', '1\nII\n3\n4\n5\nVI\n');
    expect(m.blocks.map((b) => b.start)).toEqual([2, 6]);
  });

  it('finds a small edit in a large file', () => {
    const big = Array.from({ length: 60000 }, (_, i) => `line ${i}`);
    const next = [...big];
    next[30000] = 'changed';
    const m = replacementModel(big.join('\n'), next.join('\n'));
    expect(m.blocks).toHaveLength(1);
    expect(m.lines).toEqual([30001]);
  });

  it('falls back to one block when the texts share almost nothing', () => {
    const a = Array.from({ length: 5000 }, (_, i) => `a${i}`).join('\n');
    const b = Array.from({ length: 5000 }, (_, i) => `b${i}`).join('\n');
    const m = replacementModel(a, b);
    expect(m.blocks).toHaveLength(1);
    expect(m.lines).toHaveLength(5000);
  });
});
