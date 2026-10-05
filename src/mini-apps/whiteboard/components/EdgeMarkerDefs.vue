<script setup lang="ts">
import { computed } from 'vue';
import { markerId, markerKind, type MarkerPart } from '../edgeMarkers';

/**
 * The line ends in use on the board, one `<marker>` per kind and colour.
 *
 * Rendered inside the canvas element, so an exported picture carries them:
 * the export copies the canvas, and a line pointing at a marker outside it
 * would come out with bare ends.
 */
const props = defineProps<{ uses: { kind: string; color: string }[] }>();

const markers = computed(() => {
  const seen = new Map<string, { id: string; color: string; parts: MarkerPart[] }>();
  for (const { kind, color } of props.uses) {
    const def = markerKind(kind);
    const id = markerId(kind, color);
    if (!def || seen.has(id)) continue;
    seen.set(id, { id, color, parts: def.parts });
  }
  return [...seen.values()];
});

/** A part's paint, as a style object: bound by Vue, never written as markup. */
function paintOf(part: MarkerPart, color: string) {
  if (part.paint === 'solid') return { fill: color, stroke: color };
  if (part.paint === 'hollow') return { fill: 'var(--wb-paper, #fff)', stroke: color };
  return { fill: 'none', stroke: color };
}
</script>

<template>
  <svg class="wb-marker-defs" aria-hidden="true" width="0" height="0">
    <defs>
      <marker
        v-for="m in markers"
        :id="m.id"
        :key="m.id"
        viewBox="0 0 20 20"
        refX="20"
        refY="10"
        markerWidth="18"
        markerHeight="18"
        markerUnits="userSpaceOnUse"
        orient="auto-start-reverse"
      >
        <template v-for="(part, i) in m.parts" :key="i">
          <circle v-if="part.cx !== undefined" :cx="part.cx" cy="10" r="4" stroke-width="1.5" :style="paintOf(part, m.color)" />
          <path
            v-else
            :d="part.d"
            :stroke-width="part.paint === 'solid' ? 1 : 1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            :style="paintOf(part, m.color)"
          />
        </template>
      </marker>
    </defs>
  </svg>
</template>

<style scoped>
.wb-marker-defs {
  position: absolute;
  width: 0;
  height: 0;
  overflow: hidden;
  pointer-events: none;
}
</style>
