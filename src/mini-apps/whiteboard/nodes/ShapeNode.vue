<script setup lang="ts">
import { ref, computed, nextTick } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import { NodeResizer } from '@vue-flow/node-resizer';
import { SHAPES_MAP } from '../shapes';
import { isInk, paint, labelOn } from '../ink';
import { cleanGlyph, GLYPH_SHAPE } from '../glyph';
import { outlineInsets } from '../pathGeometry';
import RotateHandle from '../components/RotateHandle.vue';
import { useOnlySelected } from '../composables/useOnlySelected';

const props = defineProps<{
  id: string;
  selected?: boolean;
  data: {
    shapeType: string;
    label: string;
    color: string;
    fillColor?: string;
    width?: number;
    height?: number;
    borderWidth?: number;
    dashStyle?: string;   // 'solid' | 'dashed' | 'dotted'
    opacity?: number;     // 0-100
    fontSize?: number;
    rotation?: number;
    /** The drawing, when this is an icon (`shapeType: 'glyph'`). */
    glyph?: unknown;
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'rotate', degrees: number, final: boolean): void;
}>();

const isEditing = ref(false);
const alone = useOnlySelected(() => props.selected);
const editText = ref('');
const inputRef = ref<HTMLInputElement | null>(null);

const strokeWidth = computed(() => props.data.borderWidth || 2);

const shapeOpacity = computed(() => (props.data.opacity ?? 100) / 100);
const labelFontSize = computed(() => `${props.data.fontSize || 13}px`);
const strokeDasharray = computed(() => {
  const d = props.data.dashStyle;
  if (d === 'dashed') return '8 4';
  if (d === 'dotted') return '2 4';
  return 'none';
});
const fillColor = computed(() => paint(props.data.fillColor) || 'none');
// A user-set fill sits at ~80% so what is inside the shape still shows
// through. A fill that already carries its own alpha (8-digit hex) keeps it.
const fillOpacity = computed(() => {
  const fill = props.data.fillColor;
  if (!fill) return 1;
  return isInk(fill) || fill.replace('#', '').length === 6 ? 0.8 : 1;
});
const strokeColor = computed(() => paint(props.data.color));
/** An icon: its drawing, checked, or null for every other shape. */
const glyph = computed(() => (props.data.shapeType === GLYPH_SHAPE ? cleanGlyph(props.data.glyph) : null));
/** A picture of a thing — a figure, an icon — has its words under it, on the board. */
const labelBelow = computed(() => !!glyph.value || !!shapeDef.value.labelBelow);
/** Words that stay readable on the fill (or the board): see `labelOn`. */
const labelColor = computed(() => (labelBelow.value ? labelOn(null) : labelOn(props.data.fillColor)));

const shapeDef = computed(() => SHAPES_MAP[props.data.shapeType] || SHAPES_MAP['rectangle']);

// Compensate rx/ry for non-uniform SVG scaling so corners stay circular
const CORNER_PX = 12; // desired visual corner radius in pixels
const roundedRectRx = computed(() => {
  const w = props.data.width || shapeDef.value.defaultWidth || 160;
  return Math.min(CORNER_PX * 100 / w, 49);
});
const roundedRectRy = computed(() => {
  const h = props.data.height || shapeDef.value.defaultHeight || 80;
  return Math.min(CORNER_PX * 100 / h, 49);
});

/**
 * Where the connection handles sit: where the outline crosses the middle of
 * its box (see `outlineInsets`). Read from the path as a path — curves,
 * subpaths and open lines as they are drawn. Pairing every number in the path
 * into a point, as before, put a handle on the wrong side of a shape or in
 * the air beside it on one shape in eight.
 */
const handleOffsets = computed(() => {
  const inset = outlineInsets(shapeDef.value.path);
  return { top: `${inset.top}%`, right: `${inset.right}%`, bottom: `${inset.bottom}%`, left: `${inset.left}%` };
});

/** Focused by hand: `autofocus` only works for the first editor a page opens. */
function startEdit() {
  if ((props.data as any).locked) return;
  isEditing.value = true;
  editText.value = props.data.label;
  nextTick(() => {
    inputRef.value?.focus();
    inputRef.value?.select();
  });
}

function finishEdit() {
  // Enter, then the blur as the input goes: one edit, one write.
  if (!isEditing.value) return;
  isEditing.value = false;
  emit('update:data', { ...props.data, label: editText.value });
}

function onResizeEnd(event: any) {
  emit('update:data', {
    ...props.data,
    width: Math.round(event.params.width),
    height: Math.round(event.params.height),
  });
}
</script>

<template>
  <div class="wb-shape-node" :style="data.rotation ? { transform: `rotate(${data.rotation}deg)` } : undefined" @dblclick.stop="startEdit">
    <RotateHandle
      v-if="alone && !(data as any).locked && !isEditing"
      :node-id="id"
      :rotation="data.rotation"
      :label="$t('whiteboard.rotate')"
      @rotate="(deg: number, final: boolean) => emit('rotate', deg, final)"
    />
    <NodeResizer
      :is-visible="!!selected && !(data as any).locked"
      :min-width="40"
      :min-height="40"
      color="var(--wb-selection, var(--color-accent))"
      @resize-end="onResizeEnd"
    />

    <!-- An icon: its own drawing, on a tile of the fill colour if it has one. -->
    <svg
      v-if="glyph"
      :viewBox="glyph.viewBox.join(' ')"
      class="wb-shape-svg wb-glyph"
      fill="none"
      :style="{ stroke: strokeColor, opacity: shapeOpacity }"
      :stroke-width="strokeWidth"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <rect
        :x="glyph.viewBox[0] - glyph.viewBox[2] * 0.12" :y="glyph.viewBox[1] - glyph.viewBox[3] * 0.12"
        :width="glyph.viewBox[2] * 1.24" :height="glyph.viewBox[3] * 1.24" :rx="glyph.viewBox[2] * 0.2"
        class="wb-glyph-hit"
        :style="{ fill: data.fillColor ? fillColor : 'transparent', stroke: 'none' }"
        :fill-opacity="fillOpacity"
      />
      <component :is="part[0]" v-for="(part, i) in glyph.parts" :key="i" v-bind="part[1]" />
    </svg>
    <!-- SVG Shape — all shapes rendered through same SVG pipeline for consistent stroke -->
    <svg v-else viewBox="2 2 96 96" preserveAspectRatio="none" class="wb-shape-svg" style="overflow: visible;">
      <!-- Rounded Rect: use native <rect> with compensated rx/ry for circular corners -->
      <rect
        v-if="data.shapeType === 'roundedRect'"
        x="2" y="2" width="96" height="96"
        :rx="roundedRectRx"
        :ry="roundedRectRy"
        :style="{ fill: fillColor, stroke: strokeColor }"
        :fill-opacity="fillOpacity"
        :stroke-width="strokeWidth"
        :stroke-dasharray="strokeDasharray"
        vector-effect="non-scaling-stroke"
        :opacity="shapeOpacity"
      />
      <!-- All other shapes: render via path -->
      <path
        v-else
        :d="shapeDef.path"
        :style="{ fill: fillColor, stroke: strokeColor }"
        :fill-opacity="fillOpacity"
        :stroke-width="strokeWidth"
        :stroke-dasharray="strokeDasharray"
        vector-effect="non-scaling-stroke"
        stroke-linejoin="round"
        fill-rule="evenodd"
        :opacity="shapeOpacity"
      />
      <!-- Decoration paths (fold lines, inner lines, etc.) -->
      <path
        v-for="(deco, i) in (shapeDef.deco || [])"
        :key="i"
        :d="deco"
        fill="none"
        :style="{ stroke: strokeColor }"
        :stroke-width="strokeWidth"
        :stroke-dasharray="strokeDasharray"
        vector-effect="non-scaling-stroke"
        stroke-linejoin="round"
        :opacity="shapeOpacity"
      />
      <!-- Selection Border (non-scaling, perfectly snug) -->
      <rect
        v-if="selected"
        x="2" y="2" width="96" height="96"
        fill="none"
        style="stroke: var(--wb-selection, var(--color-accent))"
        stroke-width="1.5"
        vector-effect="non-scaling-stroke"
        stroke-dasharray="6 4"
      />
    </svg>

    <!-- Label -->
    <div
      class="wb-shape-label-container"
      :class="{ 'wb-shape-label-container--below': labelBelow }"
      :style="!labelBelow && shapeDef.labelBox ? { inset: shapeDef.labelBox.map((v) => `${v}%`).join(' ') } : undefined"
    >
      <input
        v-if="isEditing"
        ref="inputRef"
        v-model="editText"
        @blur="finishEdit"
        @keydown.enter="finishEdit"
        @keydown.escape="isEditing = false"
        class="wb-shape-input"
      />
      <span v-else class="wb-shape-label" :style="{ fontSize: labelFontSize, color: labelColor }">
        {{ data.label || '' }}
      </span>
    </div>

    <!-- Connection Handles with dynamic offsets -->
    <Handle id="top" type="source" :position="Position.Top" class="wb-handle" :connectable="true"
      :style="{ top: handleOffsets.top }" />
    <Handle id="right" type="source" :position="Position.Right" class="wb-handle" :connectable="true"
      :style="{ right: handleOffsets.right }" />
    <Handle id="bottom" type="source" :position="Position.Bottom" class="wb-handle" :connectable="true"
      :style="{ bottom: handleOffsets.bottom }" />
    <Handle id="left" type="source" :position="Position.Left" class="wb-handle" :connectable="true"
      :style="{ left: handleOffsets.left }" />
  </div>
</template>

<style scoped>
.wb-shape-node {
  position: relative;
  width: 100%;
  height: 100%;
  cursor: grab;
}
.wb-shape-svg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}
/* An icon is picked up anywhere in its square: it has no inside to see through. */
.wb-glyph { overflow: visible; }
.wb-glyph .wb-glyph-hit { pointer-events: all; cursor: grab; }
.wb-glyph :is(path, circle, ellipse, rect, line, polyline, polygon):not(.wb-glyph-hit) { pointer-events: none; }
/* Only the stroke/border captures clicks — fill area is click-through */
.wb-shape-svg path {
  pointer-events: visibleStroke;
  cursor: grab;
}
.wb-shape-label-container {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
  padding: 0 8px;
  pointer-events: auto;
}
.wb-shape-label-container--below {
  inset: 100% -48px auto;
  align-items: flex-start;
  padding-top: 4px;
  pointer-events: none;
}
.wb-shape-label-container--below .wb-shape-input {
  pointer-events: auto;
}
.wb-shape-label {
  font-size: 13px;
  font-weight: 500;
  text-align: center;
  word-break: break-word;
  pointer-events: none;
  opacity: 0.85;
}
.wb-shape-input {
  width: 90%;
  text-align: center;
  font-size: 13px;
  font-weight: 500;
  background: transparent;
  border: none;
  outline: none;
  color: inherit;
  pointer-events: auto;
}
.wb-handle {
  width: 10px !important;
  height: 10px !important;
  background: var(--color-accent) !important;
  border: 2px solid white !important;
  border-radius: 50% !important;
  opacity: 0;
  transition: opacity 0.15s;
  z-index: 20 !important;
  pointer-events: auto !important;
}
.wb-shape-node:hover .wb-handle {
  opacity: 1;
}
:global(.vue-flow__node.selected) .wb-handle {
  opacity: 1;
}
</style>
