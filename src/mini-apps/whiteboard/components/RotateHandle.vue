<script setup lang="ts">
import { computed, ref } from 'vue';
import { RotateCw } from 'lucide-vue-next';
import { useVueFlow } from '@vue-flow/core';
import { normalizeAngle } from '../imageAssets';

/**
 * The turn handle under a selected item, and the angle while it turns.
 *
 * Any item that can be turned carries one: drag it round the item's middle,
 * hold Shift for steps of 15°, double-click it to stand the item back up.
 * The whole node element turns — outline, handles and all — which the canvas
 * draws from the node's `data.rotation` (see `applySize`).
 */
const props = defineProps<{ nodeId: string; rotation?: number; label: string }>();
const emit = defineEmits<{ (e: 'rotate', degrees: number, final: boolean): void }>();

const { updateNodeInternals } = useVueFlow();

const angle = computed(() => normalizeAngle(props.rotation ?? 0));
const isRotating = ref(false);

let centre: { x: number; y: number } | null = null;
let startPointer = 0;
let startAngle = 0;
let frame = 0;
let pending: number | null = null;

const pointerAngle = (e: PointerEvent) =>
  centre ? (Math.atan2(e.clientY - centre.y, e.clientX - centre.x) * 180) / Math.PI : 0;

function onStart(e: PointerEvent) {
  // Inside the node: without this the canvas reads the press as a drag of it.
  e.stopPropagation();
  e.preventDefault();
  try {
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  } catch {
    // A pointer the browser no longer tracks; the turn works without capture.
  }
  // Measured once: a turn is about this point, so this point does not move.
  const node = (e.currentTarget as Element).closest('.vue-flow__node');
  const rect = node?.getBoundingClientRect();
  centre = rect ? { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 } : null;
  startPointer = pointerAngle(e);
  startAngle = angle.value;
  isRotating.value = true;
}

function onMove(e: PointerEvent) {
  if (!isRotating.value) return;
  e.stopPropagation();
  const turned = startAngle + (pointerAngle(e) - startPointer);
  const settled = Math.round(normalizeAngle(e.shiftKey ? Math.round(turned / 15) * 15 : turned));
  if (settled === angle.value) return;
  // At most once a frame: the canvas redraws the node for each write.
  pending = settled;
  if (!frame) {
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (pending !== null) emit('rotate', pending, false);
      pending = null;
    });
  }
}

function onEnd(e: PointerEvent) {
  if (!isRotating.value) return;
  e.stopPropagation();
  isRotating.value = false;
  centre = null;
  if (frame) cancelAnimationFrame(frame);
  frame = 0;
  emit('rotate', pending ?? angle.value, true);
  pending = null;
  // Connection points are measured from the drawn element, and only when
  // the canvas is told to look again.
  updateNodeInternals([props.nodeId]);
}

function reset(e: MouseEvent) {
  e.stopPropagation();
  if (angle.value !== 0) {
    emit('rotate', 0, true);
    updateNodeInternals([props.nodeId]);
  }
}
</script>

<template>
  <button
    class="wb-rotate nodrag nopan"
    type="button"
    :title="label"
    :aria-label="label"
    @pointerdown="onStart"
    @pointermove="onMove"
    @pointerup="onEnd"
    @pointercancel="onEnd"
    @dblclick="reset"
  >
    <RotateCw class="w-3 h-3" />
  </button>
  <!-- Turned back by the same angle, so the number stays the right way up. -->
  <div v-if="isRotating" class="wb-rotate__angle" :style="{ transform: `translateX(-50%) rotate(${-angle}deg)` }">
    {{ angle }}°
  </div>
</template>

<style scoped>
.wb-rotate {
  position: absolute;
  left: 50%;
  bottom: -30px;
  transform: translateX(-50%);
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 999px;
  border: 1px solid var(--wb-selection, var(--color-accent));
  background: var(--color-surface, #fff);
  color: var(--wb-selection, var(--color-accent));
  cursor: grab;
  touch-action: none;
  pointer-events: auto;
  z-index: 30;
}
.dark .wb-rotate {
  background: var(--color-surface-dark, #1e1e1e);
}
.wb-rotate:active {
  cursor: grabbing;
}
.wb-rotate__angle {
  position: absolute;
  left: 50%;
  top: -28px;
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--color-accent);
  color: #fff;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  pointer-events: none;
  white-space: nowrap;
  z-index: 30;
}
</style>
