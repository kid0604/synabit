<script setup lang="ts">
import { computed, ref } from 'vue';
import { unionBox, viewBox, type Box } from '../inkLayer';

/**
 * The whole board, small, with the part on screen marked. Click or drag on
 * it to move there.
 *
 * The items are drawn once, in the board's own coordinates, as one path — a
 * drawing of thousands of things is one element. Moving the camera changes
 * only what the picture frames (its `viewBox`) and the marker; the items'
 * path is the same string and is not rebuilt.
 */
const props = defineProps<{
  boxes: Box[];
  viewport: { x: number; y: number; zoom: number };
  size: { width: number; height: number };
}>();
const emit = defineEmits<{ (e: 'go', centre: { x: number; y: number }): void }>();

const W = 200;
const H = 130;

const view = computed(() => viewBox(props.viewport, props.size, 0));
const items = computed(() =>
  props.boxes.map((b) => `M${Math.round(b.x)} ${Math.round(b.y)}h${Math.round(b.width) || 1}v${Math.round(b.height) || 1}h${-(Math.round(b.width) || 1)}z`).join(''),
);
const itemsBox = computed(() => unionBox(props.boxes));
const frame = computed(() => {
  const all = unionBox([...(itemsBox.value ? [itemsBox.value] : []), view.value]) ?? view.value;
  const pad = Math.max(all.width, all.height) * 0.05;
  return `${all.x - pad} ${all.y - pad} ${all.width + pad * 2} ${all.height + pad * 2}`;
});

const svg = ref<SVGSVGElement | null>(null);
let dragging = false;
function goTo(e: PointerEvent) {
  const m = svg.value?.getScreenCTM();
  if (!m) return;
  const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
  emit('go', { x: p.x, y: p.y });
}
function down(e: PointerEvent) {
  dragging = true;
  (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
  goTo(e);
}
function move(e: PointerEvent) {
  if (dragging) goTo(e);
}
function up() {
  dragging = false;
}
</script>

<template>
  <svg
    ref="svg"
    class="wb-minimap"
    :width="W"
    :height="H"
    :viewBox="frame"
    role="img"
    :aria-label="$t('whiteboard.minimap')"
    @pointerdown.stop="down"
    @pointermove="move"
    @pointerup="up"
    @pointercancel="up"
  >
    <path class="wb-minimap__items" :d="items" />
    <rect
      class="wb-minimap__view"
      :x="view.x" :y="view.y" :width="view.width" :height="view.height"
      vector-effect="non-scaling-stroke"
    />
  </svg>
</template>

<style scoped>
.wb-minimap {
  position: absolute;
  right: 56px;
  bottom: 12px;
  z-index: 5;
  border-radius: 10px;
  border: 1px solid var(--color-border, #e6e6e6);
  background: var(--color-surface, #fff);
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.08);
  cursor: pointer;
  touch-action: none;
}
:global(.dark .wb-minimap) {
  border-color: var(--color-border-dark, #2c2c2c);
  background: var(--color-surface-dark, #1e1e1e);
}
.wb-minimap__items {
  fill: rgb(113 113 122 / 0.35);
}
.wb-minimap__view {
  fill: color-mix(in oklab, var(--color-accent) 12%, transparent);
  stroke: var(--color-accent);
  stroke-width: 1.5;
}
@media (max-width: 767px) {
  .wb-minimap { display: none; }
}
</style>
