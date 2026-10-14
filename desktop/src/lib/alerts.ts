// The done / needs-you notification's click: the Rust side brings the
// window forward (src-tauri/src/alerts.rs) and names the session, which
// opens here like any other pick.

import { listen } from '@tauri-apps/api/event';
import { app, openSession } from './store.svelte';

/** Open the session a clicked notification names, if it is still there. */
export function openNotified(agent: string) {
  if (app.model.agents.some((a) => a.id === agent)) openSession({ Agent: agent });
}

export function listenAlerts() {
  return listen<string>('kete://open-session', (e) => openNotified(e.payload));
}
