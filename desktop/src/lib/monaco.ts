// Monaco (MIT), bundled from disk: the editor API, the diff editor and a
// handful of reading features, the Monarch grammars, and the one editor
// worker the diff computation runs in. No CDN, no language services.
//
// The worker comes in through Vite's `?worker` import, so it is emitted as
// a file of this app and loaded from 'self': the CSP (`worker-src 'self'
// blob:`, `script-src 'self'`) needs no loosening for it.

import * as monaco from 'monaco-esm/editor/editor.api.js';
import 'monaco-esm/features/diffEditor/register.js';
import 'monaco-esm/features/codicon/register.js';
import 'monaco-esm/features/find/register.js';
import 'monaco-esm/features/folding/register.js';
import 'monaco-esm/features/bracketMatching/register.js';
import 'monaco-esm/features/wordHighlighter/register.js';
import 'monaco-esm/features/clipboard/register.js';
import 'monaco-esm/features/hover/register.js';
import 'monaco-esm/languages/definitions/register.all.js';
import EditorWorker from 'monaco-esm/editor/editor.worker.js?worker';

(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
  getWorker: () => new EditorWorker(),
};

/** What every read-only editor in the window shares: the diff editor and
 *  replacement mode. `font` is the Diff and file size the look asks for
 *  (look.svelte.ts editorOptions); the window's own is 12.5 on 19. */
export function readingOptions(
  fontFamily: string,
  font: { fontSize: number; lineHeight: number } = { fontSize: 12.5, lineHeight: 19 },
): monaco.editor.IStandaloneEditorConstructionOptions {
  return {
    theme: 'kete',
    readOnly: true,
    domReadOnly: true,
    automaticLayout: true,
    fontFamily,
    fontSize: font.fontSize,
    lineHeight: font.lineHeight,
    fontLigatures: false,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    padding: { top: 6 },
    stickyScroll: { enabled: false },
    renderLineHighlight: 'none',
    scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
  };
}

export { monaco };
