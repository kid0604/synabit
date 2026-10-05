<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue';

/**
 * A laser pointer over the board while it is presented: a red dot where the
 * pointer is, and a short trail behind it that fades, so a circle drawn
 * around something is seen as a circle. Nothing is drawn on the board.
 *
 * With reduced motion asked for, the dot alone.
 */
const LIFE_MS = 600;

interface Mark { x: number; y: number; t: number }

const dot = ref<{ x: number; y: number } | null>(null);
const segments = ref<{ d: string; opacity: number; width: number }[]>([]);
const marks: Mark[] = [];
const still = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
let frame = 0;

function draw() {
  frame = 0;
  const now = performance.now();
  while (marks.length && now - marks[0].t > LIFE_MS) marks.shift();
  const out: { d: string; opacity: number; width: number }[] = [];
  for (let i = 1; i < marks.length; i++) {
    const life = 1 - (now - marks[i].t) / LIFE_MS;
    out.push({
      d: `M${marks[i - 1].x} ${marks[i - 1].y}L${marks[i].x} ${marks[i].y}`,
      opacity: life * 0.8,
      width: 2 + life * 4,
    });
  }
  segments.value = out;
  if (marks.length) frame = requestAnimationFrame(draw);
}

function onMove(e: PointerEvent) {
  const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const at = { x: e.clientX - box.left, y: e.clientY - box.top };
  dot.value = at;
  if (still) return;
  marks.push({ ...at, t: performance.now() });
  if (!frame) frame = requestAnimationFrame(draw);
}

function onLeave() {
  dot.value = null;
}

onBeforeUnmount(() => {
  if (frame) cancelAnimationFrame(frame);
});
</script>

<template>
  <div class="wb-laser" @pointermove="onMove" @pointerleave="onLeave">
    <svg class="wb-laser__ink" aria-hidden="true">
      <path
        v-for="(s, i) in segments"
        :key="i"
        :d="s.d"
        :stroke-width="s.width"
        :stroke-opacity="s.opacity"
      />
      <circle v-if="dot" :cx="dot.x" :cy="dot.y" r="6" />
    </svg>
  </div>
</template>

<style scoped>
.wb-laser {
  position: absolute;
  inset: 0;
  cursor: none;
}
.wb-laser__ink {
  width: 100%;
  height: 100%;
  overflow: visible;
  pointer-events: none;
}
.wb-laser__ink path {
  fill: none;
  stroke: #ef4444;
  stroke-linecap: round;
}
.wb-laser__ink circle {
  fill: #ef4444;
  filter: drop-shadow(0 0 6px rgb(239 68 68 / 0.8));
}
</style>
