import { beforeEach, describe, expect, it } from 'vitest';
import { closeFiles, sheet, showFiles, stepTab } from './files.svelte';
import { layerOpen } from '../ui/ui.svelte';

const opened = (n: number) => ({
  root: '/w/demo',
  files: Array.from({ length: n }, (_, i) => ({ path: `/w/demo/f${i}.md`, label: `f${i}.md` })),
});

beforeEach(() => closeFiles());

describe('the opened-files sheet', () => {
  it('raises on the first of the new files, replacing the last set', () => {
    showFiles(opened(3));
    sheet.at = 2;
    showFiles(opened(2));
    expect(sheet.open).toBe(true);
    expect(sheet.at).toBe(0);
    expect(sheet.tabs.map((t) => t.label)).toEqual(['f0.md', 'f1.md']);
  });

  it('ignores an empty set', () => {
    showFiles(opened(0));
    expect(sheet.open).toBe(false);
  });

  it('steps through the tabs, wrapping both ways', () => {
    showFiles(opened(3));
    stepTab(-1);
    expect(sheet.at).toBe(2);
    stepTab(1);
    expect(sheet.at).toBe(0);
  });

  it('holds the window keys while it is up', () => {
    expect(layerOpen()).toBe(false);
    showFiles(opened(1));
    expect(layerOpen()).toBe(true);
    closeFiles();
    expect(layerOpen()).toBe(false);
  });
});
