// The window's one authored motion: the needs-you bar pulling back to the
// row's left edge when the session no longer needs you. 120 ms, no bounce;
// instant when the OS asks for reduced motion.

import { cubicOut } from 'svelte/easing';

export const RETRACT_MS = 120;

function reduced(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export function retract(_node: Element) {
  return {
    duration: reduced() ? 0 : RETRACT_MS,
    easing: cubicOut,
    css: (t: number) => `transform: scaleX(${t})`,
  };
}
