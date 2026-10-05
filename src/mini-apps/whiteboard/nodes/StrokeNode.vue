<script setup lang="ts">
import { computed } from 'vue';
import { paint } from '../ink';
import { strokeSize } from '../inkLayer';

const props = defineProps<{
  data: {
    svgPath: string;
    color: string;
    size: number;
    opacity?: number;
    width?: number;
    height?: number;
    points?: number[][];
  };
  selected: boolean;
}>();

/**
 * The box the stroke covers: the canvas measures, culls and crops an export
 * by it. Most strokes are drawn by the ink layer (see `inkLayer.ts`); this is
 * a stroke while it is selected, or stacked somewhere of its own.
 */
const box = computed(() => strokeSize(props.data));
</script>

<template>
  <div class="wb-stroke-node" :style="{ width: box.width + 'px', height: box.height + 'px' }">
    <svg class="overflow-visible" :width="box.width" :height="box.height">
      <path
        :d="data.svgPath"
        :style="{ fill: paint(data.color) }"
        :opacity="data.opacity ?? 0.85"
        stroke="none"
      />
      <rect
        v-if="selected"
        x="0" y="0" :width="box.width" :height="box.height"
        fill="none"
        style="stroke: var(--wb-selection, var(--color-accent))"
        stroke-width="1"
        stroke-dasharray="4 3"
      />
    </svg>
  </div>
</template>

<style scoped>
/* Only the ink takes the pointer. The box around a loop of ink is mostly
   empty, and whatever is under that emptiness must stay clickable. */
.wb-stroke-node {
  pointer-events: none;
}
.wb-stroke-node path {
  pointer-events: visiblePainted;
  cursor: grab;
}
</style>
