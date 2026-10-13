// The terminal and the diff editor are drawn by libraries that take colours
// as JS values, not CSS. They read the same tokens the chrome uses, from the
// CSS variables theme.css defines, so theme.css stays the one place a
// colour or font is decided. Call again after `data-theme` flips.

import type { ITheme } from '@xterm/xterm';

let probe: CanvasRenderingContext2D | null = null;
const BLACK = 'rgb(0 0 0)';
const WHITE = 'rgb(255 255 255)';

/** A theme token as `#rrggbb` or `#rrggbbaa`, whatever syntax theme.css used. */
export function token(name: string): string {
  const raw = getComputedStyle(document.documentElement).getPropertyValue(`--${name}`).trim();
  if (!raw) return BLACK;
  probe ??= Object.assign(document.createElement('canvas'), { width: 1, height: 1 }).getContext('2d', {
    willReadFrequently: true,
  });
  if (!probe) return raw;
  // Paint the colour over black and over white: the two results give its
  // alpha and its straight RGB, whatever syntax theme.css wrote it in.
  const over = (base: string) => {
    probe!.fillStyle = base;
    probe!.fillRect(0, 0, 1, 1);
    probe!.fillStyle = BLACK;
    probe!.fillStyle = raw;
    probe!.fillRect(0, 0, 1, 1);
    return probe!.getImageData(0, 0, 1, 1).data;
  };
  const k = over(BLACK);
  const w = over(WHITE);
  const alpha = 1 - (w[0] - k[0]) / 255;
  const ch = (i: number) => (alpha > 0 ? Math.round(k[i] / alpha) : 0);
  const hex = (n: number) => Math.max(0, Math.min(255, n)).toString(16).padStart(2, '0');
  const a = Math.round(alpha * 255);
  return `#${hex(ch(0))}${hex(ch(1))}${hex(ch(2))}${a >= 255 ? '' : hex(a)}`;
}

/** The terminal font stack (--font-term), for the libraries that take one. */
export function termFamily(): string {
  return getComputedStyle(document.documentElement).getPropertyValue('--font-term').trim();
}

export function xtermTheme(): ITheme {
  const t = (n: string) => token(`color-term-${n}`);
  return {
    background: t('bg'),
    foreground: t('fg'),
    cursor: t('cursor'),
    cursorAccent: t('bg'),
    selectionBackground: t('selection'),
    black: t('black'),
    red: t('red'),
    green: t('green'),
    yellow: t('yellow'),
    blue: t('blue'),
    magenta: t('magenta'),
    cyan: t('cyan'),
    white: t('white'),
    brightBlack: t('bright-black'),
    brightRed: t('bright-red'),
    brightGreen: t('bright-green'),
    brightYellow: t('bright-yellow'),
    brightBlue: t('bright-blue'),
    brightMagenta: t('bright-magenta'),
    brightCyan: t('bright-cyan'),
    brightWhite: t('bright-white'),
  };
}

/** (Re)define the `kete` Monaco theme from the current tokens. */
export function defineMonacoTheme(
  monaco: typeof import('monaco-esm/editor/editor.api.js'),
  dark: boolean,
) {
  const c = (n: string) => token(`color-${n}`);
  const fgOf = (n: string) => token(`color-${n}`).slice(1, 7);
  monaco.editor.defineTheme('kete', {
    base: dark ? 'vs-dark' : 'vs',
    inherit: true,
    rules: [
      { token: 'comment', foreground: fgOf('syn-comment'), fontStyle: 'italic' },
      { token: 'keyword', foreground: fgOf('syn-keyword') },
      { token: 'string', foreground: fgOf('syn-string') },
      { token: 'number', foreground: fgOf('syn-number') },
      { token: 'type', foreground: fgOf('syn-type') },
      { token: 'type.identifier', foreground: fgOf('syn-type') },
      { token: 'identifier', foreground: fgOf('ink') },
      { token: 'delimiter', foreground: fgOf('ink') },
      { token: 'tag', foreground: fgOf('syn-tag') },
      { token: 'attribute.name', foreground: fgOf('syn-attr') },
      { token: 'attribute.value', foreground: fgOf('syn-string') },
      { token: 'variable', foreground: fgOf('syn-tag') },
      { token: 'predefined', foreground: fgOf('syn-function') },
      { token: 'regexp', foreground: fgOf('syn-regexp') },
    ],
    colors: {
      'editor.background': c('bg'),
      'editor.foreground': c('ink'),
      'editorLineNumber.foreground': c('editor-gutter'),
      'editorLineNumber.activeForeground': c('ink'),
      'editor.selectionBackground': c('term-selection'),
      'editor.lineHighlightBackground': c('editor-line'),
      'editorGutter.background': c('bg'),
      'diffEditor.insertedTextBackground': c('diff-insert'),
      'diffEditor.removedTextBackground': c('diff-remove'),
      'diffEditor.insertedLineBackground': c('diff-insert-line'),
      'diffEditor.removedLineBackground': c('diff-remove-line'),
      'diffEditorGutter.insertedLineBackground': c('diff-insert'),
      'diffEditorGutter.removedLineBackground': c('diff-remove'),
      'diffEditor.diagonalFill': c('rule'),
      'scrollbarSlider.background': c('rule'),
      'scrollbarSlider.hoverBackground': c('faint'),
      'editorOverviewRuler.border': c('none'),
      focusBorder: c('none'),
    },
  });
  monaco.editor.setTheme('kete');
}
