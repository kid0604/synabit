<script setup lang="ts">
import { ref, computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import { NodeResizer } from '@vue-flow/node-resizer';
import { SHAPES_MAP } from '../shapes';

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
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
}>();

const isEditing = ref(false);
const editText = ref('');

const strokeWidth = computed(() => props.data.borderWidth || 2);

const shapeOpacity = computed(() => (props.data.opacity ?? 100) / 100);
const labelFontSize = computed(() => `${props.data.fontSize || 13}px`);
const strokeDasharray = computed(() => {
  const d = props.data.dashStyle;
  if (d === 'dashed') return '8 4';
  if (d === 'dotted') return '2 4';
  return 'none';
});
const fillColor = computed(() => {
  if (props.data.fillColor) {
    // User-set fill: apply with ~80% opacity so inner shapes remain visible
    const hex = props.data.fillColor.replace('#', '');
    // If it's a 6-char hex, append alpha; if already has alpha (8-char), use as-is
    if (hex.length === 6) return props.data.fillColor + 'CC';
    return props.data.fillColor;
  }
  return 'none';
});

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
 * Compute handle offsets by sampling the shape's path to find where
 * the shape boundary actually is along each cardinal direction.
 * Returns percentage offsets from the bounding box edge.
 */
const handleOffsets = computed(() => {
  const path = shapeDef.value.path;
  const coords: [number, number][] = [];
  const numRegex = /-?\d+(?:\.\d+)?/g;
  const tokens = path.match(numRegex);
  if (tokens) {
    for (let i = 0; i < tokens.length - 1; i += 2) {
      coords.push([parseFloat(tokens[i]), parseFloat(tokens[i + 1])]);
    }
  }

  if (coords.length < 3) return { top: '50%', right: '50%', bottom: '50%', left: '50%' };

  let topY = Infinity;
  let botY = -Infinity;
  let leftX = Infinity;
  let rightX = -Infinity;

  for (let i = 0; i < coords.length; i++) {
    const [x1, y1] = coords[i];
    const [x2, y2] = coords[(i + 1) % coords.length];

    // Intersection with X = 50 (vertical centerline)
    if ((x1 <= 50 && x2 >= 50) || (x2 <= 50 && x1 >= 50)) {
      if (x1 === x2) {
        topY = Math.min(topY, y1, y2);
        botY = Math.max(botY, y1, y2);
      } else {
        const m = (y2 - y1) / (x2 - x1);
        const yInt = y1 + m * (50 - x1);
        topY = Math.min(topY, yInt);
        botY = Math.max(botY, yInt);
      }
    }

    // Intersection with Y = 50 (horizontal centerline)
    if ((y1 <= 50 && y2 >= 50) || (y2 <= 50 && y1 >= 50)) {
      if (y1 === y2) {
        leftX = Math.min(leftX, x1, x2);
        rightX = Math.max(rightX, x1, x2);
      } else {
        const invM = (x2 - x1) / (y2 - y1);
        const xInt = x1 + invM * (50 - y1);
        leftX = Math.min(leftX, xInt);
        rightX = Math.max(rightX, xInt);
      }
    }
  }

  if (topY === Infinity) topY = 0;
  if (botY === -Infinity) botY = 100;
  if (leftX === Infinity) leftX = 0;
  if (rightX === -Infinity) rightX = 100;

  // Transform path coordinates (2-98) to div percentage (0-100%)
  return {
    top: `${Math.max(0, (topY - 2) / 96 * 100)}%`,
    bottom: `${Math.max(0, (98 - botY) / 96 * 100)}%`,
    left: `${Math.max(0, (leftX - 2) / 96 * 100)}%`,
    right: `${Math.max(0, (98 - rightX) / 96 * 100)}%`,
  };
});

function startEdit() {
  isEditing.value = true;
  editText.value = props.data.label;
}

function finishEdit() {
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
  <div class="wb-shape-node" @dblclick.stop="startEdit">
    <NodeResizer
      :is-visible="!!selected"
      :min-width="40"
      :min-height="40"
      color="var(--color-accent, #7c3aed)"
      @resize-end="onResizeEnd"
    />

    <!-- SVG Shape — all shapes rendered through same SVG pipeline for consistent stroke -->
    <svg viewBox="2 2 96 96" preserveAspectRatio="none" class="wb-shape-svg" style="overflow: visible;">
      <!-- Rounded Rect: use native <rect> with compensated rx/ry for circular corners -->
      <rect
        v-if="data.shapeType === 'roundedRect'"
        x="2" y="2" width="96" height="96"
        :rx="roundedRectRx"
        :ry="roundedRectRy"
        :fill="fillColor"
        :stroke="data.color"
        :stroke-width="strokeWidth"
        :stroke-dasharray="strokeDasharray"
        vector-effect="non-scaling-stroke"
        :opacity="shapeOpacity"
      />
      <!-- All other shapes: render via path -->
      <path
        v-else
        :d="shapeDef.path"
        :fill="fillColor"
        :stroke="data.color"
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
        :stroke="data.color"
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
        stroke="var(--color-accent, #7c3aed)"
        stroke-width="1.5"
        vector-effect="non-scaling-stroke"
        stroke-dasharray="6 4"
      />
    </svg>

    <!-- Label -->
    <div class="wb-shape-label-container">
      <input
        v-if="isEditing"
        v-model="editText"
        @blur="finishEdit"
        @keydown.enter="finishEdit"
        @keydown.escape="isEditing = false"
        class="wb-shape-input"
        autofocus
      />
      <span v-else class="wb-shape-label text-text dark:text-text-dark" :style="{ fontSize: labelFontSize }">
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
  background: var(--color-accent, #7c3aed) !important;
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
