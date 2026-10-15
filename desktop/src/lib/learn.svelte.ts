// LEARN IN PLACE, the window's state: the tip up now and the What's new
// page, read from the shared config at boot (src-tauri learn.rs) and
// written back as tips show. learn.ts holds the rules.

import { api } from './api';
import { offer, type TipState } from './learn';

export const learn: TipState = $state({
  showTips: true,
  seen: [],
  tip: null,
  whatsNew: false,
  loaded: false,
});

/** Read the tips' state; open What's new when it is due (src-tauri has
 *  already recorded it as shown). */
export async function loadLearn() {
  const s = await api.learnState().catch(() => null);
  if (!s) return;
  learn.showTips = s.showTips;
  learn.seen = s.seen;
  learn.loaded = true;
  if (s.whatsNew) learn.whatsNew = true;
}

/** The concept `id` is on screen: its tip shows, once, if it is due. */
export function offerTip(id: string) {
  const next = offer(learn, id);
  if (!next) return;
  learn.tip = next.tip;
  learn.seen = next.seen;
  api.learnSeen(id).catch(() => {});
}

/** "Got it": the tip goes (it is already seen). */
export function gotIt() {
  learn.tip = null;
}

/** "Don't show tips": this one goes, and no other shows. */
export function tipsOff() {
  learn.tip = null;
  learn.showTips = false;
  api.learnTips(false).catch(() => {});
}

/** Settings' Show tips row changed. */
export function setShowTips(on: boolean) {
  learn.showTips = on;
  if (!on) learn.tip = null;
}
