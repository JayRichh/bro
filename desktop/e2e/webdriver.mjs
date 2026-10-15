// A W3C WebDriver client, small: the handful of endpoints the suite uses,
// over fetch, against tauri-driver (which hands them to msedgedriver and
// so to the app's WebView2). No dependency beyond Node.

export const ELEMENT = 'element-6066-11e4-a52e-4f735466cecf';

/** W3C key codes for the keys the tests press. */
export const Key = {
  Alt: '\uE00A',
  Control: '\uE009',
  Shift: '\uE008',
  Enter: '\uE007',
  Escape: '\uE00C',
  F2: '\uE032',
};

export class WebDriver {
  constructor(base) {
    this.base = base.replace(/\/$/, '');
    this.session = null;
  }

  async call(method, path, body) {
    const url = this.session ? `${this.base}/session/${this.session}${path}` : `${this.base}${path}`;
    const res = await fetch(url, {
      method,
      headers: { 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const json = await res.json().catch(() => ({}));
    if (!res.ok || json.value?.error) {
      const v = json.value ?? {};
      throw new Error(`webdriver ${method} ${path}: ${v.error ?? res.status} ${v.message ?? ''}`.trim());
    }
    return json.value;
  }

  /** Open the app: tauri-driver starts the binary as the "browser". */
  async start(application, args = []) {
    const value = await this.call('POST', '/session', {
      capabilities: {
        alwaysMatch: { browserName: 'wry', 'tauri:options': { application, args } },
      },
    });
    this.session = value.sessionId;
    return value;
  }

  async quit() {
    if (!this.session) return;
    try {
      await this.call('DELETE', '');
    } finally {
      this.session = null;
    }
  }

  /** Run `body` in the page: `arguments` are `args`; a returned promise is
   *  awaited (W3C execute/sync). */
  exec(body, ...args) {
    return this.call('POST', '/execute/sync', { script: body, args });
  }

  async find(css) {
    const v = await this.call('POST', '/element', { using: 'css selector', value: css });
    return v[ELEMENT];
  }

  click(el) {
    return this.call('POST', `/element/${el}/click`, {});
  }

  /** Type into an element (or, with no element, to the focused one). */
  type(el, text) {
    return this.call('POST', `/element/${el}/value`, { text });
  }

  /** Press a chord: modifiers held, then `key`, then all released. */
  async press(key, ...mods) {
    const down = [...mods, key].map((value) => ({ type: 'keyDown', value }));
    const up = [key, ...mods.reverse()].map((value) => ({ type: 'keyUp', value }));
    await this.call('POST', '/actions', { actions: [{ type: 'key', id: 'kb', actions: [...down, ...up] }] });
    await this.call('DELETE', '/actions');
  }

  screenshot() {
    return this.call('GET', '/screenshot');
  }
}
