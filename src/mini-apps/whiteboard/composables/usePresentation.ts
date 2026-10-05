import { computed, onBeforeUnmount, ref, watch, type Ref } from 'vue';
import { slidesOf, type Slide } from '../presentation';
import type { Box } from '../inkLayer';

/**
 * Showing a board one frame at a time.
 *
 * The canvas stays the canvas — nothing is copied or rendered again — it is
 * only stopped from being edited and moved to each frame in turn. A board with
 * no frames is shown whole, as one slide. Where the camera was is put back
 * on the way out.
 */
export function usePresentation(ctx: {
  nodes: () => any[];
  boardTitle: () => string;
  /** The box around everything, for a board with no frames. */
  boardBox: () => Box | null;
  fitBounds: (box: Box, options: { padding: number; duration: number }) => unknown;
  getViewport: () => { x: number; y: number; zoom: number };
  setViewport: (v: { x: number; y: number; zoom: number }) => unknown;
  /** The canvas's size: a slide is fitted again when it changes. */
  size: Ref<{ width: number; height: number }>;
  /** Clear the selection, so no handles or menus show on a slide. */
  deselect: () => void;
}) {
  const active = ref(false);
  const index = ref(0);
  const laser = ref(false);
  let cameraBefore: { x: number; y: number; zoom: number } | null = null;
  let focusBefore: HTMLElement | null = null;

  const slides = computed<Slide[]>(() => {
    const frames = slidesOf(ctx.nodes());
    if (frames.length) return frames;
    const box = ctx.boardBox();
    return box ? [{ id: null, title: ctx.boardTitle(), box }] : [];
  });
  const current = computed<Slide | null>(() => slides.value[index.value] ?? null);

  const still = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;

  function show(duration = still() ? 0 : 450) {
    const slide = current.value;
    if (slide) void ctx.fitBounds(slide.box, { padding: 0.04, duration });
  }

  function go(i: number) {
    const last = slides.value.length - 1;
    const next = Math.max(0, Math.min(last, i));
    if (next === index.value) return;
    index.value = next;
    show();
  }

  function start(fromFrameId?: string) {
    if (!slides.value.length) return;
    cameraBefore = ctx.getViewport();
    focusBefore = document.activeElement as HTMLElement | null;
    ctx.deselect();
    const at = fromFrameId ? slides.value.findIndex((s) => s.id === fromFrameId) : 0;
    index.value = Math.max(0, at);
    laser.value = false;
    active.value = true;
    // The canvas grows to the whole window first; fit once it has.
    requestAnimationFrame(() => {
      show(0);
      // Into the bar, so a screen reader is told where it is and Tab reaches
      // the controls without a walk through a page that is no longer shown.
      document.querySelector<HTMLElement>('.wb-present-bar button:not([disabled])')?.focus();
    });
  }

  function stop() {
    if (!active.value) return;
    active.value = false;
    laser.value = false;
    if (cameraBefore) {
      const back = cameraBefore;
      cameraBefore = null;
      requestAnimationFrame(() => void ctx.setViewport(back));
    }
    // Back to what started it. The title bar is taken away while presenting,
    // so when that is where it was, it is the title bar's Present button anew.
    const was = focusBefore;
    focusBefore = null;
    requestAnimationFrame(() => {
      const to = was?.isConnected ? was : (was?.hasAttribute('data-wb-present') ? document.querySelector<HTMLElement>('[data-wb-present]') : null);
      to?.focus();
    });
  }

  /**
   * Keys while presenting. Taken before anything else on the page sees them:
   * the board's own keys — delete, nudge, undo — must not act on a board
   * that is being shown.
   */
  function onKey(e: KeyboardEvent) {
    if (!active.value) return;
    // A button in the bar has focus: Enter and Space press it, as they would.
    const onButton = (e.target as HTMLElement | null)?.closest?.('button');
    if (onButton && (e.key === 'Enter' || e.key === ' ')) return;
    const handled = (() => {
      switch (e.key) {
        case 'ArrowRight': case 'ArrowDown': case 'PageDown': case ' ': case 'Enter':
          go(index.value + 1); return true;
        case 'ArrowLeft': case 'ArrowUp': case 'PageUp': case 'Backspace':
          go(index.value - 1); return true;
        case 'Home': go(0); return true;
        case 'End': go(slides.value.length - 1); return true;
        case 'Escape': stop(); return true;
        case 'l': case 'L': laser.value = !laser.value; return true;
        default: return false;
      }
    })();
    e.stopPropagation();
    // Tab moves between the bar's buttons, as anywhere else.
    if (handled || (!e.metaKey && e.key !== 'Tab')) e.preventDefault();
  }
  window.addEventListener('keydown', onKey, true);
  onBeforeUnmount(() => window.removeEventListener('keydown', onKey, true));

  // A window resized while presenting, or the canvas taking the whole window.
  watch(() => [ctx.size.value.width, ctx.size.value.height], () => { if (active.value) show(0); });
  // The board changed under the presentation — a frame deleted, moved or
  // resized elsewhere: keep to a slide that exists, and follow it.
  watch(() => slides.value.length, (n) => { if (active.value && index.value >= n) index.value = Math.max(0, n - 1); });
  watch(
    () => { const b = current.value?.box; return b ? `${current.value?.id}:${b.x},${b.y},${b.width},${b.height}` : ''; },
    (now, was) => { if (active.value && was && now !== was) show(); },
  );

  return {
    active, index, laser, slides, current,
    start, stop, go,
    next: () => go(index.value + 1),
    prev: () => go(index.value - 1),
  };
}
