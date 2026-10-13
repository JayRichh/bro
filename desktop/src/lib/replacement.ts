// Replacement mode: the new text alone, with what changed marked on it.
// Pure (no Monaco, no DOM) so it runs under vitest. Coordinates are
// Monaco's: lines and columns from 1, `endColumn` exclusive.

/** A changed run of characters inside one line of the new text. */
export interface Span {
  line: number;
  startColumn: number;
  endColumn: number;
}

/** Old lines that are gone, shown after `afterLine` (0: above line 1). */
export interface Removed {
  afterLine: number;
  lines: string[];
}

/** One change: new lines `start..end` (inclusive; empty when end < start) replace `old`. */
export interface Block {
  start: number;
  end: number;
  old: string[];
}

export interface ReplacementModel {
  /** The new text, unchanged. */
  text: string;
  /** New-text lines that were added or rewritten. */
  lines: number[];
  /** Changed spans inside rewritten lines. */
  spans: Span[];
  /** Where old lines were removed with nothing in their place. */
  removed: Removed[];
  /** Every change, in order; the stops for next and previous change. */
  blocks: Block[];
}

/** Past this many line edits the middle is one changed block, not diffed. */
const MAX_LINE_EDITS = 2000;
/** Lines longer than this are marked whole, not diffed by word. */
const MAX_WORD_LINE = 2000;

const splitLines = (s: string) => (s === '' ? [] : s.split(/\r?\n/));

type Op = { kind: 'same' | 'del' | 'ins'; a: number; b: number };

/**
 * Myers' O(ND) diff of `a` and `b` (already interned to numbers), as a list
 * of same / del / ins steps. Null when more than `maxD` edits are needed.
 */
function myers(a: number[], b: number[], maxD: number): Op[] | null {
  const n = a.length;
  const m = b.length;
  const max = Math.min(n + m, maxD);
  const off = max + 1;
  const v = new Int32Array(2 * max + 3);
  const trace: Int32Array[] = [];
  let found = -1;
  for (let d = 0; d <= max; d++) {
    // Only the diagonals step d can read: memory stays O(D^2), not O(D(N+M)).
    trace.push(v.slice(off - d - 1, off + d + 2));
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[off + k - 1] < v[off + k + 1]) ? v[off + k + 1] : v[off + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) (x++, y++);
      v[off + k] = x;
      if (x >= n && y >= m) {
        found = d;
        break;
      }
    }
    if (found >= 0) break;
  }
  if (found < 0) return null;
  // Walk back through the saved frontiers.
  const ops: Op[] = [];
  let x = n;
  let y = m;
  for (let d = found; d > 0; d--) {
    const t = trace[d];
    const at = (k: number) => t[k + d + 1];
    const k = x - y;
    const prevK = k === -d || (k !== d && at(k - 1) < at(k + 1)) ? k + 1 : k - 1;
    const px = at(prevK);
    const py = px - prevK;
    while (x > px && y > py) ops.push({ kind: 'same', a: --x, b: --y });
    if (x === px) ops.push({ kind: 'ins', a: x, b: --y });
    else ops.push({ kind: 'del', a: --x, b: y });
  }
  while (x > 0 && y > 0) ops.push({ kind: 'same', a: --x, b: --y });
  return ops.reverse();
}

function interner() {
  const ids = new Map<string, number>();
  return (s: string) => {
    let id = ids.get(s);
    if (id === undefined) ids.set(s, (id = ids.size));
    return id;
  };
}

/** Line blocks of old [a0, a1) replaced by new [b0, b1), zero-based. */
type Hunk = { a0: number; a1: number; b0: number; b1: number };

function lineHunks(oldLines: string[], newLines: string[]): Hunk[] {
  // Trim the common ends first: big files with small edits stay linear.
  let pre = 0;
  const maxPre = Math.min(oldLines.length, newLines.length);
  while (pre < maxPre && oldLines[pre] === newLines[pre]) pre++;
  let suf = 0;
  while (
    suf < maxPre - pre &&
    oldLines[oldLines.length - 1 - suf] === newLines[newLines.length - 1 - suf]
  )
    suf++;
  const a1 = oldLines.length - suf;
  const b1 = newLines.length - suf;
  if (pre === a1 && pre === b1) return [];
  const id = interner();
  const a = oldLines.slice(pre, a1).map(id);
  const b = newLines.slice(pre, b1).map(id);
  const ops = myers(a, b, MAX_LINE_EDITS);
  if (!ops) return [{ a0: pre, a1, b0: pre, b1 }];
  const hunks: Hunk[] = [];
  let cur: Hunk | null = null;
  for (const op of ops) {
    if (op.kind === 'same') {
      cur = null;
      continue;
    }
    if (!cur) {
      cur = { a0: op.a + pre, a1: op.a + pre, b0: op.b + pre, b1: op.b + pre };
      hunks.push(cur);
    }
    if (op.kind === 'del') cur.a1 = op.a + pre + 1;
    else cur.b1 = op.b + pre + 1;
  }
  return hunks;
}

const TOKEN = /\w+|\s+|[^\w\s]/g;

/** Columns of `next` that are not in `prev`, by word, merged into runs. */
function wordSpans(prev: string, next: string, line: number): Span[] | null {
  if (prev.length > MAX_WORD_LINE || next.length > MAX_WORD_LINE) return null;
  const pt = prev.match(TOKEN) ?? [];
  const nt = next.match(TOKEN) ?? [];
  const id = interner();
  const ops = myers(pt.map(id), nt.map(id), 400);
  if (!ops) return null;
  // Nothing in common but blanks: the whole line is new, no spans.
  if (!ops.some((o) => o.kind === 'same' && nt[o.b].trim() !== '')) return null;
  const starts: number[] = [];
  let col = 0;
  for (const t of nt) (starts.push(col), (col += t.length));
  const spans: Span[] = [];
  for (const o of ops) {
    if (o.kind !== 'ins') continue;
    const s = starts[o.b] + 1;
    const e = s + nt[o.b].length;
    const last = spans[spans.length - 1];
    if (last && last.endColumn === s) last.endColumn = e;
    else spans.push({ line, startColumn: s, endColumn: e });
  }
  return spans;
}

/**
 * What replacement mode draws: `newText` itself, the lines that are new or
 * rewritten, the changed words inside rewritten lines, and markers where
 * old lines were removed outright.
 */
export function replacementModel(oldText: string, newText: string): ReplacementModel {
  const oldLines = splitLines(oldText);
  const newLines = splitLines(newText);
  const model: ReplacementModel = { text: newText, lines: [], spans: [], removed: [], blocks: [] };
  for (const h of lineHunks(oldLines, newLines)) {
    const old = oldLines.slice(h.a0, h.a1);
    const added = h.b1 - h.b0;
    model.blocks.push({ start: h.b0 + 1, end: h.b1, old });
    for (let i = 0; i < added; i++) {
      const line = h.b0 + i + 1;
      model.lines.push(line);
      if (i < old.length) {
        const spans = wordSpans(old[i], newLines[h.b0 + i], line);
        if (spans) model.spans.push(...spans);
      }
    }
    // Old lines with no new line to stand in: removed after the block.
    if (old.length > added) model.removed.push({ afterLine: h.b1, lines: old.slice(added) });
  }
  return model;
}

/** The first line a change sits on, for revealing it (a removal: the line after it). */
export function blockLine(b: Block, lineCount: number): number {
  return Math.max(1, Math.min(lineCount, b.start));
}
