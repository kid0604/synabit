<script setup lang="ts" generic="T extends { id: string; x: number; y: number; w: number; h: number }">
/**
 * Boxes over a page shown at `width` × `height` pixels: drag one to move it,
 * its corner to size it, click to pick it. Picked, the arrow keys move it (⇧
 * for bigger steps), ⌥ + arrows size it, Delete takes it off.
 *
 * With `draw`, dragging where there is no box draws a new one — how Redact is
 * used. Boxes are shares of the page (`marks.ts`), so nothing here depends on
 * how large the page is drawn.
 */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { clampBox, type Box } from '../marks';

const props = defineProps<{
  items: T[];
  selected: string | null;
  width: number;
  height: number;
  /** Whether a box keeps its shape when sized (a signature, a tick). */
  keepShape?: (item: T) => boolean;
  /** Drag on the page to draw a new box. */
  draw?: boolean;
  /** What each box is called, for a screen reader. */
  label: (item: T) => string;
}>();

const emit = defineEmits<{
  (e: 'select', id: string | null): void;
  (e: 'change', id: string, box: Box): void;
  (e: 'remove', id: string): void;
  (e: 'create', box: Box): void;
}>();

defineSlots<{ default(props: { item: T; selected: boolean }): unknown }>();

const { t } = useI18n();

type Grip = 'move' | 'size';
const drag = ref<{ id: string; grip: Grip; x: number; y: number; from: Box } | null>(null);
const drawing = ref<{ x0: number; y0: number; box: Box } | null>(null);

function start(item: T, grip: Grip, e: PointerEvent) {
  e.preventDefault();
  e.stopPropagation();
  emit('select', item.id);
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  drag.value = { id: item.id, grip, x: e.clientX, y: e.clientY, from: { x: item.x, y: item.y, w: item.w, h: item.h } };
}

function move(e: PointerEvent) {
  const d = drag.value;
  if (!d) return;
  const dx = (e.clientX - d.x) / props.width;
  const dy = (e.clientY - d.y) / props.height;
  const f = d.from;
  if (d.grip === 'move') {
    emit('change', d.id, clampBox({ ...f, x: f.x + dx, y: f.y + dy }));
    return;
  }
  const item = props.items.find((i) => i.id === d.id);
  const w = f.w + dx;
  let h = f.h + dy;
  if (item && props.keepShape?.(item)) h = (w * f.h) / f.w;
  emit('change', d.id, clampBox({ ...f, w: Math.min(w, 1 - f.x), h: Math.min(h, 1 - f.y) }));
}

function end() {
  drag.value = null;
}

function pointAt(e: PointerEvent) {
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
  return { x: Math.min(1, Math.max(0, (e.clientX - r.left) / props.width)), y: Math.min(1, Math.max(0, (e.clientY - r.top) / props.height)) };
}

function startDrawing(e: PointerEvent) {
  emit('select', null);
  if (!props.draw) return;
  e.preventDefault();
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  const p = pointAt(e);
  drawing.value = { x0: p.x, y0: p.y, box: { x: p.x, y: p.y, w: 0, h: 0 } };
}

function moveDrawing(e: PointerEvent) {
  const d = drawing.value;
  if (!d) return;
  const p = pointAt(e);
  d.box = { x: Math.min(d.x0, p.x), y: Math.min(d.y0, p.y), w: Math.abs(p.x - d.x0), h: Math.abs(p.y - d.y0) };
}

function endDrawing() {
  const d = drawing.value;
  drawing.value = null;
  // A click, not a drag: no box.
  if (d && d.box.w * props.width > 4 && d.box.h * props.height > 4) emit('create', d.box);
}

function keys(item: T, e: KeyboardEvent) {
  if (e.key === 'Delete' || e.key === 'Backspace') {
    e.preventDefault();
    emit('remove', item.id);
    return;
  }
  if (e.key === 'Escape') {
    emit('select', null);
    return;
  }
  const step = (e.shiftKey ? 10 : 1) / props.width;
  const stepY = (e.shiftKey ? 10 : 1) / props.height;
  const by: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -stepY], ArrowDown: [0, stepY] };
  const m = by[e.key];
  if (!m) return;
  e.preventDefault();
  const b = { x: item.x, y: item.y, w: item.w, h: item.h };
  if (e.altKey) {
    let w = b.w + m[0];
    let h = b.h + m[1];
    if (props.keepShape?.(item)) h = m[0] ? (w * b.h) / b.w : h;
    if (props.keepShape?.(item) && m[1]) w = (h * b.w) / b.h;
    emit('change', item.id, clampBox({ ...b, w, h }));
  } else {
    emit('change', item.id, clampBox({ ...b, x: b.x + m[0], y: b.y + m[1] }));
  }
}
</script>

<template>
  <div
    class="absolute inset-0 touch-none"
    :class="draw ? 'cursor-crosshair' : ''"
    @pointerdown.self="startDrawing"
    @pointermove="moveDrawing"
    @pointerup="endDrawing"
    @pointercancel="drawing = null"
  >
    <div
      v-for="item in items"
      :key="item.id"
      role="button"
      tabindex="0"
      :aria-label="label(item)"
      :aria-pressed="selected === item.id"
      aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown Alt+ArrowRight Alt+ArrowDown Delete"
      class="absolute cursor-move outline-none"
      :class="selected === item.id ? 'ring-2 ring-accent' : 'hover:ring-1 hover:ring-accent/60 focus-visible:ring-2 focus-visible:ring-accent'"
      :style="{ left: `${item.x * 100}%`, top: `${item.y * 100}%`, width: `${item.w * 100}%`, height: `${item.h * 100}%` }"
      @pointerdown="start(item, 'move', $event)"
      @pointermove="move"
      @pointerup="end"
      @pointercancel="end"
      @keydown="keys(item, $event)"
      @focus="emit('select', item.id)"
    >
      <slot :item="item" :selected="selected === item.id" />
      <span
        v-if="selected === item.id"
        class="absolute -right-1.5 -bottom-1.5 w-3 h-3 bg-white border border-accent rounded-sm cursor-nwse-resize"
        :aria-label="t('utilities.board.resize')"
        @pointerdown.stop="start(item, 'size', $event)"
        @pointermove="move"
        @pointerup="end"
        @pointercancel="end"
      />
    </div>
    <div
      v-if="drawing"
      class="absolute bg-black/70 border border-accent pointer-events-none"
      :style="{ left: `${drawing.box.x * 100}%`, top: `${drawing.box.y * 100}%`, width: `${drawing.box.w * 100}%`, height: `${drawing.box.h * 100}%` }"
    />
  </div>
</template>
