// The overlay's clock, on fake timers and a stub page: nothing runs while
// the critters are off, between visits there is one timer and no frames,
// and a wake after a long gap still brings the visitor.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Engine } from './engine';
import { steps } from './world';

const on = () => {};
function stubPage() {
  const ctx = { clearRect: on, fillRect: on, drawImage: on, createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4) }), putImageData: on, globalAlpha: 1, fillStyle: '', imageSmoothingEnabled: false, canvas: { width: 1, height: 1 } };
  const canvas = { width: 1, height: 1, style: {} as Record<string, string>, getContext: () => ctx };
  ctx.canvas = canvas;
  const listeners = { addEventListener: vi.fn(), removeEventListener: vi.fn() };
  vi.stubGlobal('window', { innerWidth: 960, innerHeight: 600, devicePixelRatio: 1, ...listeners });
  vi.stubGlobal('document', {
    visibilityState: 'visible',
    hasFocus: () => true,
    querySelectorAll: () => [],
    elementFromPoint: () => null,
    documentElement: { addEventListener: on, removeEventListener: on },
    // A frame bitmap (a canvas) or a hit box (a div): one stub does for both.
    createElement: () => ({ ...canvas, style: {}, dataset: {}, setAttribute: on, addEventListener: on, remove: on }),
    ...listeners,
  });
  vi.stubGlobal('OffscreenCanvas', undefined);
  // Node has no frames: a frame is a 16 ms timer on the fake clock.
  vi.stubGlobal('requestAnimationFrame', (f: (t: number) => void) => setTimeout(() => f(performance.now()), 16));
  vi.stubGlobal('cancelAnimationFrame', (id: ReturnType<typeof setTimeout>) => clearTimeout(id));
  const layer = { append: on };
  return { canvas: canvas as unknown as HTMLCanvasElement, layer: layer as unknown as HTMLElement, win: listeners };
}

describe('the overlay clock', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance', 'Date'] });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('does nothing at all until started', () => {
    const { canvas, layer, win } = stubPage();
    const e = new Engine(canvas, layer, { notes: on, menu: on });
    vi.advanceTimersByTime(60_000);
    expect(e.stats.ticks).toBe(0);
    expect(win.addEventListener).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('waits on one timer between visits, with no frames, even across a long gap', () => {
    const { canvas, layer } = stubPage();
    const e = new Engine(canvas, layer, { notes: on, menu: on });
    e.world.often = 'rare';
    e.start();
    // The first visitor is a few seconds off: one timer, no frame loop.
    expect(vi.getTimerCount()).toBe(1);
    e.world.spawnIn = steps(200);
    e.poke();
    vi.advanceTimersByTime(199_000);
    expect(e.stats.frames).toBe(0);
    expect(e.world.active()).toBe(false);
    vi.advanceTimersByTime(2_000);
    expect(e.stats.wakes).toBe(1);
    expect(e.world.active(), 'the visitor came').toBe(true);
    // Now it animates.
    vi.advanceTimersByTime(100);
    expect(e.stats.frames).toBeGreaterThan(0);
  });

  it('stops everything when turned off', () => {
    const { canvas, layer } = stubPage();
    const e = new Engine(canvas, layer, { notes: on, menu: on });
    e.start();
    e.summon('kiwi');
    e.poke();
    vi.advanceTimersByTime(500);
    expect(e.world.active()).toBe(true);
    e.stop();
    expect(e.world.active()).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });
});
