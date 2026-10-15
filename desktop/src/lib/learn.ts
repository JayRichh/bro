// LEARN IN PLACE, the window's pure half: when a first-use tip may show,
// its words with the user's own keys, an empty state's copy, and the
// help's task groups. The copy is nebula-config's (learn.rs, through
// actions.gen.ts), shared with the terminal app; the seen-state is the
// config's `tips_seen`, so a tip seen in one app is not shown in the other.

import { EMPTY, TIPS, type ActionId } from './actions.gen';

export interface TipState {
  /** Settings, General, Show tips. */
  showTips: boolean;
  /** Tips already shown, in either app. */
  seen: readonly string[];
  /** The tip up now. */
  tip: string | null;
  /** The What's new page is up. */
  whatsNew: boolean;
  /** The state has been read from the config: until then nothing shows. */
  loaded: boolean;
}

/** Whether tip `id` may show now: tips on, not seen, and nothing else up
 *  (one tip at a time; none over What's new). A tip that waits is not
 *  marked seen, so it shows the next time its concept comes round. */
export function tipDue(s: TipState, id: string): boolean {
  return s.loaded && s.showTips && !s.tip && !s.whatsNew && !s.seen.includes(id) && TIPS.some((t) => t.id === id);
}

/** Show tip `id` if it is due: returns the state with it up and seen
 *  (the caller writes the seen-state), or null when it waits. */
export function offer(s: TipState, id: string): TipState | null {
  if (!tipDue(s, id)) return null;
  return { ...s, tip: id, seen: [...s.seen, id] };
}

/** `text` with each `{verb}` / `{verb|words}` filled: the verb's key as
 *  the user has it bound, else the words (else the id). */
export function fill(text: string, keyOf: (id: string) => string | null): string {
  return text.replace(/\{([^}|]+)(?:\|([^}]*))?\}/g, (_, id: string, words?: string) => keyOf(id) ?? words ?? id);
}

/** A tip's line, keys filled. Empty for an unknown id. */
export function tipText(id: string, keyOf: (id: string) => string | null): string {
  const t = TIPS.find((x) => x.id === id);
  return t ? fill(t.text, keyOf) : '';
}

/** An empty state's line and the verb that fills it. */
export function emptyCopy(id: string): { text: string; verb: ActionId | null } {
  const e = EMPTY.find((x) => x.id === id);
  return e ? { text: e.text, verb: e.verb } : { text: '', verb: null };
}

/** The board lane's empty-state id, by the window's lane id. */
export const LANE_EMPTY: Record<string, string> = {
  running: 'lane_running',
  needs: 'lane_needs',
  unseen: 'lane_review',
  pr: 'lane_ship',
  done: 'lane_done',
};
