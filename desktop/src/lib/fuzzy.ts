// The terminal app's fuzzy matcher (crates/nebula-fuzzy), ported so the
// command palette ranks the way `/` does: a greedy leftmost subsequence,
// case-insensitive, scoring consecutive runs and matches that start a
// path segment or word. Whitespace splits the query into terms that must
// all match, in any order.

export interface FuzzyMatch {
  score: number;
  /** Ascending, de-duplicated indices of the matched characters. */
  positions: number[];
}

const CONSECUTIVE_BONUS = 8;
const BOUNDARY_BONUS = 6;
const BOUNDARY = new Set(['/', '\\', '_', '-', '.', ' ']);

function isBoundary(prev: string | undefined): boolean {
  return prev === undefined || BOUNDARY.has(prev);
}

function greedyFrom(term: string[], cand: string[], start: number, positions?: number[]): number | null {
  let score = 0;
  let qi = 0;
  let prev = false;
  for (let i = start; i < cand.length; i++) {
    if (cand[i] === term[qi]) {
      score += 1;
      if (prev) score += CONSECUTIVE_BONUS;
      if (isBoundary(i > 0 ? cand[i - 1] : undefined)) score += BOUNDARY_BONUS;
      positions?.push(i);
      prev = true;
      qi += 1;
      if (qi === term.length) return score;
    } else {
      prev = false;
    }
  }
  return null;
}

function matchTerm(term: string[], cand: string[]): FuzzyMatch | null {
  if (!term.length) return { score: 0, positions: [] };
  let best: [number, number] | null = null;
  for (let start = 0; start < cand.length; start++) {
    if (cand[start] !== term[0]) continue;
    // A pass that fails here fails from every later start too.
    const score = greedyFrom(term, cand, start);
    if (score === null) break;
    if (!best || score > best[0]) best = [score, start];
  }
  if (!best) return null;
  const positions: number[] = [];
  greedyFrom(term, cand, best[1], positions);
  return { score: best[0], positions };
}

/** `query` against `candidate`, or null when some term never matches. */
export function fuzzyMatch(query: string, candidate: string): FuzzyMatch | null {
  const terms = query
    .split(/\s+/)
    .filter(Boolean)
    .map((t) => [...t.toLowerCase()]);
  if (!terms.length) return { score: 0, positions: [] };
  const cand = [...candidate.toLowerCase()];
  let score = 0;
  let positions: number[] = [];
  for (const t of terms) {
    const m = matchTerm(t, cand);
    if (!m) return null;
    score += m.score;
    positions = positions.concat(m.positions);
  }
  if (terms.length > 1) positions = [...new Set(positions)].sort((a, b) => a - b);
  return { score, positions };
}

/**
 * Rank `items` by `query` over `text(item)`: best score first, ties by the
 * caller's `order` (lower first), then by input order. An empty query
 * keeps every item, sorted by `order` alone.
 */
export function rank<T>(
  query: string,
  items: T[],
  text: (t: T) => string,
  order: (t: T) => number = () => 0,
): { item: T; positions: number[] }[] {
  const indexed = items.map((item, i) => ({ item, i }));
  if (!query.trim()) {
    return indexed.sort((a, b) => order(a.item) - order(b.item) || a.i - b.i).map(({ item }) => ({ item, positions: [] }));
  }
  const hits: { item: T; i: number; score: number; positions: number[] }[] = [];
  for (const { item, i } of indexed) {
    const m = fuzzyMatch(query, text(item));
    if (m) hits.push({ item, i, score: m.score, positions: m.positions });
  }
  hits.sort((a, b) => b.score - a.score || order(a.item) - order(b.item) || a.i - b.i);
  return hits.map(({ item, positions }) => ({ item, positions }));
}
