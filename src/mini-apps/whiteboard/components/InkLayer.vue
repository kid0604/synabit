<script setup lang="ts">
import { computed, inject, onBeforeUnmount } from 'vue';
import { paint } from '../ink';
import { EXPORT_INK } from '../composables/useClipboardExport';
import { contains, inkBox, overlaps, viewBox } from '../inkLayer';
import type { Box } from '../inkLayer';

/**
 * The board's loose ink, as one picture.
 *
 * Placed inside the canvas's own moving layer, so it pans and zooms with
 * everything else and is in the picture when the board is exported. Only the
 * strokes in view are drawn — or all of them, for an export. See `inkLayer.ts`
 * for which strokes are here and which are items on the canvas.
 */
const props = defineProps<{
  strokes: any[];
  viewport: { x: number; y: number; zoom: number };
  size: { width: number; height: number };
  /** Draw every stroke, in view or not: the board is being exported. */
  everything: boolean;
  /** When only part of the board is exported, the items in the picture. */
  only?: Set<string> | null;
  /** Whether a press on ink selects it (the select tool) or passes through. */
  interactive: boolean;
  /** A marquee being dragged, in board coordinates: what it will take is marked. */
  marquee: Box | null;
  /** Around selected ink kept here (a large selection), in board coordinates. */
  selectionBox?: Box | null;
}>();

const emit = defineEmits<{
  (e: 'press', id: string, event: PointerEvent): void;
  (e: 'menu', id: string, event: MouseEvent): void;
}>();

const shown = computed(() => {
  if (props.only) return props.strokes.filter((s) => props.only!.has(s.id));
  if (props.everything || !props.size.width) return props.strokes;
  const view = viewBox(props.viewport, props.size);
  return props.strokes.filter((s) => overlaps(view, inkBox(s)));
});

/**
 * The export the board is part of. While it takes its picture the selection
 * box is left out (the canvas's own selection is cleared for it, but loose ink
 * is not the canvas's), and it asks here what loose ink to select again after.
 */
const exportLink = inject(EXPORT_INK, null);
const capturing = computed(() => !!exportLink?.capturing.value);
exportLink?.selectedInk(() => props.strokes.filter((s) => s.selected).map((s) => s.id));
onBeforeUnmount(() => exportLink?.selectedInk(null));

const marked = computed(() => {
  if (!props.marquee || capturing.value) return [];
  const area = props.marquee;
  return shown.value.map(inkBox).filter((b) => contains(area, b));
});
</script>

<template>
  <svg class="wb-ink" :class="{ 'wb-ink--live': interactive }" width="1" height="1" aria-hidden="true">
    <path
      v-for="s in shown"
      :key="s.id"
      class="wb-ink__path nopan"
      :class="{ 'wb-ink__path--locked': s.data?.locked }"
      :data-id="s.id"
      :d="s.data.svgPath"
      :transform="`translate(${s.position.x} ${s.position.y})`"
      :style="{ fill: paint(s.data.color) }"
      :opacity="s.data.opacity ?? 0.85"
      @pointerdown="(e: PointerEvent) => emit('press', s.id, e)"
      @contextmenu.prevent.stop="(e: MouseEvent) => emit('menu', s.id, e)"
    />
    <rect
      v-if="selectionBox && !capturing"
      class="wb-ink__selection"
      :x="selectionBox.x - 4 / (viewport.zoom || 1)"
      :y="selectionBox.y - 4 / (viewport.zoom || 1)"
      :width="selectionBox.width + 8 / (viewport.zoom || 1)"
      :height="selectionBox.height + 8 / (viewport.zoom || 1)"
      :stroke-width="1.5 / (viewport.zoom || 1)"
    />
    <rect
      v-for="(b, i) in marked"
      :key="i"
      class="wb-ink__marked"
      :x="b.x" :y="b.y" :width="b.width" :height="b.height"
      :stroke-width="1 / (viewport.zoom || 1)"
    />
  </svg>
</template>

<style scoped>
/* Stacked with the items the canvas stacks at zero — text, mind maps, notes —
   and after them in the document, so above them; frames are below, shapes,
   stickies and pictures above. That is where a stroke was drawn when every
   stroke was an item. */
.wb-ink {
  position: absolute;
  left: 0;
  top: 0;
  z-index: 0;
  overflow: visible;
  pointer-events: none;
}
.wb-ink--live .wb-ink__path {
  pointer-events: visiblePainted;
  cursor: grab;
}
.wb-ink__selection {
  fill: none;
  stroke: var(--wb-selection, var(--color-accent));
}
.wb-ink__marked {
  fill: none;
  stroke: var(--wb-selection, var(--color-accent));
  stroke-dasharray: 4 3;
}
</style>
