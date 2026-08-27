<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref } from 'vue';
import { Handle, Position, useVueFlow } from '@vue-flow/core';
import { ImageOff, RotateCw } from 'lucide-vue-next';
import { assetUrl, normalizeAngle } from '../imageAssets';
import { resizeRotatedBox } from '../boxGeometry';
import type { Box, Corner } from '../boxGeometry';
import { useEventBus } from '../../../composables/useEventBus';

const props = defineProps<{
  id: string;
  selected?: boolean;
  /** Where the canvas has placed this node, in board coordinates. */
  position: { x: number; y: number };
  data: {
    /** Where the picture lives inside the vault, e.g. `assets/a1b2c3.png`. */
    assetPath: string;
    alt?: string;
    width?: number;
    height?: number;
    /** Clockwise, in degrees. Absent means upright. */
    rotation?: number;
  };
}>();

const emit = defineEmits<{
  /**
   * A new position *and* size together: resizing a turned box moves both.
   *
   * `final` marks the end of the gesture. Everything before it is the canvas
   * being redrawn as the pointer moves; the last one is the change itself,
   * and the only one the board is written for.
   */
  (e: 'update:box', box: Box, final: boolean): void;
  (e: 'update:rotation', degrees: number, final: boolean): void;
}>();

/**
 * Let through at most one update per frame.
 *
 * A pointer reports far more often than the screen redraws — a trackpad can
 * be twice the refresh rate or more — and each update here reaches the canvas
 * and redraws this node. Sending all of them does not make the turn smoother;
 * it makes the canvas do the same work several times for one frame, and what
 * the user sees is the picture shivering and smearing. Only the last position
 * in a frame is worth anything, so only the last one is sent.
 */
let frame: number | null = null;
let pending: (() => void) | null = null;

function onNextFrame(work: () => void) {
  pending = work;
  if (frame !== null) return;
  frame = requestAnimationFrame(() => {
    frame = null;
    const run = pending;
    pending = null;
    run?.();
  });
}

/** Run whatever is waiting, now. For the end of a gesture. */
function flushFrame() {
  if (frame !== null) cancelAnimationFrame(frame);
  frame = null;
  const run = pending;
  pending = null;
  run?.();
}

/** Throw away whatever is waiting. Only for going away entirely. */
function dropFrame() {
  if (frame !== null) cancelAnimationFrame(frame);
  frame = null;
  pending = null;
}

onBeforeUnmount(dropFrame);

const { viewport, updateNodeInternals } = useVueFlow({ id: 'whiteboard-flow' });

// The board is opened for one vault at a time; the canvas hands its path
// down rather than every picture asking for it.
const vaultPath = inject<{ value: string }>('whiteboardVaultPath', { value: '' });

// A picture whose bytes have not arrived yet. Attachments come over the wire
// separately from the board that names them, so on a second device a board
// can be readable before its pictures are — and the failed load is
// remembered, which would leave a placeholder standing until the board was
// reopened. Try again when a sync brings something new.
const broken = ref(false);
useEventBus().on('vault:sync-completed', () => {
  if (broken.value) broken.value = false;
});

const src = computed(() => {
  if (!props.data.assetPath || !vaultPath.value) return '';
  return assetUrl(vaultPath.value, props.data.assetPath);
});

// ── Resizing ────────────────────────────────────────────────
//
// Not the library's resizer. That one reads the pointer in the world's axes
// and writes straight to width and height, which is right up until the
// picture is turned: then the handle that looks like the bottom-right corner
// widens the picture along whichever way the world happens to be wide, and
// the corner opposite it wanders off. `boxGeometry` does the turn arithmetic;
// this end of it is the pointer.

const CORNERS: Corner[] = ['nw', 'ne', 'se', 'sw'];

let resizeStart: { pointer: { x: number; y: number }; box: Box; corner: Corner } | null = null;

function currentBox(): Box {
  return {
    x: props.position.x,
    y: props.position.y,
    width: props.data.width || 320,
    height: props.data.height || 240,
  };
}

function onResizeStart(event: PointerEvent, corner: Corner) {
  event.stopPropagation();
  event.preventDefault();
  (event.target as Element).setPointerCapture(event.pointerId);
  resizeStart = { pointer: { x: event.clientX, y: event.clientY }, box: currentBox(), corner };
}

function onResizeMove(event: PointerEvent) {
  if (!resizeStart) return;
  event.stopPropagation();

  // Screen pixels into board units. Only the zoom matters for a difference;
  // the pan cancels out.
  const zoom = viewport.value.zoom || 1;
  const delta = {
    x: (event.clientX - resizeStart.pointer.x) / zoom,
    y: (event.clientY - resizeStart.pointer.y) / zoom,
  };

  const next = resizeRotatedBox(
    resizeStart.box,
    normalizeAngle(props.data.rotation ?? 0),
    resizeStart.corner,
    delta,
    // A picture keeps its shape unless the user says otherwise.
    { keepAspect: !event.altKey, minSize: 40 }
  );

  onNextFrame(() =>
    emit(
      'update:box',
      { x: next.x, y: next.y, width: Math.round(next.width), height: Math.round(next.height) },
      false
    )
  );
}

function onResizeEnd(event: PointerEvent) {
  if (!resizeStart) return;
  event.stopPropagation();
  resizeStart = null;
  flushFrame();
  // The board is written once, here, with wherever the drag ended up.
  emit('update:box', currentBox(), true);
}

// ── Turning the picture ─────────────────────────────────────
//
// The picture turns inside its box; the box itself stays square to the world.
// That is a deliberate simplification and it shows in two places: a turned
// picture overhangs its box, and clicking or resizing still works on the
// upright box underneath. Turning the box instead is not available — the
// canvas owns the node element's `transform` and rewrites it on every pan.

const rootRef = ref<HTMLElement | null>(null);
/** True while the turn handle is being dragged, for the angle readout. */
const isRotating = ref(false);

const angle = computed(() => normalizeAngle(props.data.rotation ?? 0));

let startPointerAngle = 0;
let startAngle = 0;

/**
 * The middle of the picture on screen, measured once when the drag starts.
 *
 * Measuring it per movement is what made turning stutter while dragging a
 * picture across the board stayed smooth. `getBoundingClientRect` cannot
 * answer from memory: it makes the browser work out the up-to-date layout of
 * everything with a change outstanding — and there is a change outstanding on
 * every frame of a turn, on a canvas holding every node in view. Dragging
 * never asks, which is exactly why dragging never stuttered.
 *
 * Once is enough: a turn is about this point, so this point does not move.
 */
let turnCentre: { x: number; y: number } | null = null;

/** Where the pointer is, as an angle around the middle of the picture. */
function pointerAngle(event: PointerEvent): number {
  if (!turnCentre) return 0;
  return (
    (Math.atan2(event.clientY - turnCentre.y, event.clientX - turnCentre.x) * 180) / Math.PI
  );
}

function onRotateStart(event: PointerEvent) {
  // The handle sits inside the node, and the canvas would otherwise read the
  // press as the start of a drag across the board.
  event.stopPropagation();
  event.preventDefault();
  (event.target as Element).setPointerCapture(event.pointerId);

  const rect = rootRef.value?.getBoundingClientRect();
  turnCentre = rect ? { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 } : null;

  startPointerAngle = pointerAngle(event);
  startAngle = angle.value;
  isRotating.value = true;
}

/**
 * Turned as the pointer moves, rather than once at the end.
 *
 * The whole node is what turns now, and the canvas draws that from the
 * node's own data — so there is nowhere local to preview it. Written on
 * every move, then: the board coalesces a stream of edits to one item into a
 * single step to undo, and saving is on a timer regardless.
 */
function onRotateMove(event: PointerEvent) {
  if (!isRotating.value) return;
  event.stopPropagation();
  const turned = startAngle + (pointerAngle(event) - startPointerAngle);
  // Held shift, the angle steps in twelfths of a right angle, which is what
  // gets a picture back to straight — or to a deliberate 15°.
  const settled = Math.round(normalizeAngle(event.shiftKey ? Math.round(turned / 15) * 15 : turned));
  if (settled !== angle.value) {
    onNextFrame(() => emit('update:rotation', settled, false));
  }
}

function onRotateEnd(event: PointerEvent) {
  if (!isRotating.value) return;
  event.stopPropagation();
  isRotating.value = false;
  turnCentre = null;
  flushFrame();
  emit('update:rotation', angle.value, true);
  // Where the connection points are is measured from the drawn element, and
  // only when the canvas is told to look again. Without this an arrow drawn
  // to a picture stays attached to where its edge used to be.
  updateNodeInternals([props.id]);
}

/** Back to upright. The handle is the only way in, so it is the way out. */
function resetRotation(event: MouseEvent) {
  event.stopPropagation();
  isRotating.value = false;
  turnCentre = null;
  dropFrame();
  if (angle.value !== 0) {
    emit('update:rotation', 0, true);
    updateNodeInternals([props.id]);
  }
}
</script>

<template>
  <div
    ref="rootRef"
    class="wb-image-node"
    :class="{ 'is-selected': selected }"
  >
    <Handle type="target" :position="Position.Top" />
    <Handle type="target" :position="Position.Left" />
    <Handle type="source" :position="Position.Bottom" />
    <Handle type="source" :position="Position.Right" />

    <img
      v-if="src && !broken"
      :src="src"
      :alt="data.alt || ''"
      :data-asset="data.assetPath"
      class="wb-image-node__img"
      draggable="false"
      @error="broken = true"
    />

    <!-- The file is gone, or arrived from another device before its bytes did -->
    <div v-else class="wb-image-node__missing">
      <ImageOff class="w-5 h-5 mb-1 opacity-50" />
      <span class="truncate max-w-full">{{ data.alt || data.assetPath }}</span>
    </div>

    <!-- Corner handles. `nodrag` is what stops the canvas dragging the node. -->
    <div
      v-for="corner in CORNERS"
      v-show="selected"
      :key="corner"
      :class="['wb-image-node__grip', `wb-image-node__grip--${corner}`, 'nodrag', 'nopan']"
      @pointerdown="onResizeStart($event, corner)"
      @pointermove="onResizeMove"
      @pointerup="onResizeEnd"
      @pointercancel="onResizeEnd"
    />

    <!-- Turn handle. -->
    <button
      v-if="selected"
      class="wb-image-node__rotate nodrag nopan"
      type="button"
      :title="$t('whiteboard.rotate_image')"
      @pointerdown="onRotateStart"
      @pointermove="onRotateMove"
      @pointerup="onRotateEnd"
      @pointercancel="onRotateEnd"
      @dblclick="resetRotation"
    >
      <RotateCw class="w-3 h-3" />
    </button>

    <!-- Turned back by the same angle, so the number stays the right way up
         while the picture behind it is upside down. -->
    <div
      v-if="isRotating"
      class="wb-image-node__angle"
      :style="{ transform: `translateX(-50%) rotate(${-angle}deg)` }"
    >{{ angle }}°</div>
  </div>
</template>

<style scoped>
.wb-image-node {
  position: relative;
  width: 100%;
  height: 100%;
  background: transparent;
  /* Never clipped: a turned picture reaches past its box, and so do the turn
     handle below it and the angle readout above. The rounded corners belong
     to the picture itself instead. */
  overflow: visible;
}
.wb-image-node.is-selected {
  /* A shadow rather than an outline. An outline on an element that is being
     turned is painted outside the box the browser works out it has to
     repaint, which leaves the old ring behind on the canvas as the picture
     moves; a shadow is part of the element's own paint. */
  box-shadow: 0 0 0 2px var(--color-accent, #7c3aed);
}
.wb-image-node__img {
  width: 100%;
  height: 100%;
  border-radius: 6px;
  object-fit: contain;
  display: block;
  -webkit-user-drag: none;
  user-select: none;
}
.wb-image-node__missing {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 8px;
  border: 1px dashed rgba(127, 127, 127, 0.5);
  border-radius: 6px;
  font-size: 10px;
  text-align: center;
  color: rgb(113, 113, 122);
}
.wb-image-node__grip {
  position: absolute;
  width: 10px;
  height: 10px;
  border: 1px solid var(--color-accent, #7c3aed);
  border-radius: 2px;
  background: var(--color-surface, #fff);
  touch-action: none;
  z-index: 10;
}
.wb-image-node__grip--nw { top: -5px; left: -5px; cursor: nwse-resize; }
.wb-image-node__grip--ne { top: -5px; right: -5px; cursor: nesw-resize; }
.wb-image-node__grip--se { bottom: -5px; right: -5px; cursor: nwse-resize; }
.wb-image-node__grip--sw { bottom: -5px; left: -5px; cursor: nesw-resize; }
.wb-image-node__rotate {
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
  border: 1px solid var(--color-accent, #7c3aed);
  background: var(--color-surface, #fff);
  color: var(--color-accent, #7c3aed);
  cursor: grab;
  touch-action: none;
  z-index: 10;
}
.wb-image-node__rotate:active {
  cursor: grabbing;
}
.wb-image-node__angle {
  position: absolute;
  left: 50%;
  top: -28px;
  transform: translateX(-50%);
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--color-accent, #7c3aed);
  color: #fff;
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  pointer-events: none;
  white-space: nowrap;
  z-index: 10;
}
</style>
