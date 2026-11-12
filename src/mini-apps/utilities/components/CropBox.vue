<script setup lang="ts">
/**
 * One picture with a box to cut it to. Drag inside the box to move it, a
 * corner to size it; with a ratio chosen, the corners keep it. Arrow keys
 * move the box and ⌥ + arrows size it (⇧ for bigger steps), so it works
 * without a mouse.
 *
 * The box lives in the picture's own pixels (`options.crop.rect`); this only
 * draws it scaled to fit the screen.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { fileUrl, type Region } from '../render';
import { useWorkbench } from '../useWorkbench';

const props = defineProps<{ path: string; width: number; height: number }>();
const { t } = useI18n();
const wb = useWorkbench();

const MAX_W = 640;
const MAX_H = 460;
const scale = computed(() => Math.min(MAX_W / props.width, MAX_H / props.height, 1));
const shown = computed(() => ({ width: props.width * scale.value, height: props.height * scale.value }));

const rect = computed(() => wb.options.crop.rect);
const box = computed(() => {
  const r = rect.value;
  if (!r) return null;
  const s = scale.value;
  return { left: r.x * s, top: r.y * s, width: r.width * s, height: r.height * s };
});

const ratio = computed(() => {
  const [a, b] = wb.options.crop.aspect.split(':').map(Number);
  return a && b ? a / b : null;
});

function clampRect(r: Region): Region {
  const width = Math.min(Math.max(8, r.width), props.width);
  const height = Math.min(Math.max(8, r.height), props.height);
  return {
    x: Math.min(Math.max(0, r.x), props.width - width),
    y: Math.min(Math.max(0, r.y), props.height - height),
    width,
    height,
  };
}

type Grip = 'move' | 'nw' | 'ne' | 'sw' | 'se';
const drag = ref<{ grip: Grip; startX: number; startY: number; from: Region } | null>(null);

function start(grip: Grip, e: PointerEvent) {
  if (!rect.value) return;
  e.preventDefault();
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  drag.value = { grip, startX: e.clientX, startY: e.clientY, from: { ...rect.value } };
}

function move(e: PointerEvent) {
  const d = drag.value;
  if (!d) return;
  const dx = (e.clientX - d.startX) / scale.value;
  const dy = (e.clientY - d.startY) / scale.value;
  const f = d.from;
  if (d.grip === 'move') {
    wb.options.crop.rect = clampRect({ ...f, x: f.x + dx, y: f.y + dy });
    return;
  }
  const left = d.grip === 'nw' || d.grip === 'sw';
  const top = d.grip === 'nw' || d.grip === 'ne';
  let width = left ? f.width - dx : f.width + dx;
  let height = top ? f.height - dy : f.height + dy;
  if (ratio.value) {
    // The larger change leads; the other follows the ratio.
    if (Math.abs(dx) * (f.height / f.width) > Math.abs(dy)) height = width / ratio.value;
    else width = height * ratio.value;
  }
  width = Math.min(Math.max(8, width), left ? f.x + f.width : props.width - f.x);
  height = Math.min(Math.max(8, height), top ? f.y + f.height : props.height - f.y);
  if (ratio.value) {
    width = Math.min(width, height * ratio.value);
    height = width / ratio.value;
  }
  wb.options.crop.rect = {
    x: left ? f.x + f.width - width : f.x,
    y: top ? f.y + f.height - height : f.y,
    width,
    height,
  };
}

function end() {
  drag.value = null;
}

function nudge(e: KeyboardEvent) {
  const r = rect.value;
  if (!r) return;
  const step = (e.shiftKey ? 10 : 1) / scale.value;
  const moves: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
  const m = moves[e.key];
  if (!m) return;
  e.preventDefault();
  if (!e.altKey) {
    wb.options.crop.rect = clampRect({ ...r, x: r.x + m[0], y: r.y + m[1] });
    return;
  }
  // ⌥ sizes it instead, from the bottom right corner, keeping any ratio.
  let width = Math.min(Math.max(8, r.width + m[0]), props.width - r.x);
  let height = Math.min(Math.max(8, r.height + m[1]), props.height - r.y);
  if (ratio.value) {
    if (m[0]) height = width / ratio.value;
    else width = height * ratio.value;
    if (r.x + width > props.width || r.y + height > props.height) return;
  }
  wb.options.crop.rect = { ...r, width, height };
}

const size = computed(() => (rect.value ? `${Math.round(rect.value.width)} × ${Math.round(rect.value.height)}` : ''));
</script>

<template>
  <div class="flex flex-col items-start gap-2">
    <div class="relative select-none touch-none overflow-hidden" :style="{ width: `${shown.width}px`, height: `${shown.height}px` }">
      <img :src="fileUrl(path)" alt="" class="block w-full h-full rounded-sm" draggable="false" />
      <template v-if="box">
        <div
          role="slider"
          tabindex="0"
          :aria-label="t('utilities.crop.box')"
          :aria-valuetext="size"
          aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown Alt+ArrowLeft Alt+ArrowRight Alt+ArrowUp Alt+ArrowDown"
          class="absolute border-2 border-white cursor-move focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          :style="{
            left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px`,
            /* What is cut away, dimmed: a shadow far larger than the picture, clipped by it. */
            boxShadow: '0 0 0 1px rgb(0 0 0 / 0.4), 0 0 0 4000px rgb(0 0 0 / 0.45)',
          }"
          @pointerdown="start('move', $event)"
          @pointermove="move"
          @pointerup="end"
          @pointercancel="end"
          @keydown="nudge"
        >
          <span
            v-for="g in (['nw', 'ne', 'sw', 'se'] as const)"
            :key="g"
            class="absolute w-3.5 h-3.5 bg-white border border-black/40 rounded-sm"
            :class="{
              '-left-2 -top-2 cursor-nwse-resize': g === 'nw',
              '-right-2 -top-2 cursor-nesw-resize': g === 'ne',
              '-left-2 -bottom-2 cursor-nesw-resize': g === 'sw',
              '-right-2 -bottom-2 cursor-nwse-resize': g === 'se',
            }"
            @pointerdown.stop="start(g, $event)"
            @pointermove="move"
            @pointerup="end"
            @pointercancel="end"
          />
        </div>
      </template>
    </div>
    <p class="text-sm text-text-secondary dark:text-text-secondary-dark tabular-nums m-0">{{ t('utilities.crop.size', { size }) }}</p>
    <p class="text-xs text-muted dark:text-muted-dark m-0">{{ t('utilities.crop.keys') }}</p>
  </div>
</template>
