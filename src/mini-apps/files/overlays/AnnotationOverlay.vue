<script setup lang="ts">
import { computed } from 'vue';
import type { PdfAnnotation } from './composables/usePdfAnnotations';

const props = defineProps<{
  annotations: PdfAnnotation[];
  page: number;
  scale: number;
}>();

const emit = defineEmits<{
  (e: 'click-annotation', annotation: PdfAnnotation): void;
}>();

const pageAnnotations = computed(() =>
  props.annotations.filter(a => a.page === props.page)
);

const colorMap: Record<string, string> = {
  yellow: 'rgba(255, 235, 59, 0.35)',
  green: 'rgba(76, 175, 80, 0.30)',
  blue: 'rgba(33, 150, 243, 0.30)',
  pink: 'rgba(233, 30, 99, 0.25)',
};

const hoverColorMap: Record<string, string> = {
  yellow: 'rgba(255, 235, 59, 0.55)',
  green: 'rgba(76, 175, 80, 0.50)',
  blue: 'rgba(33, 150, 243, 0.50)',
  pink: 'rgba(233, 30, 99, 0.45)',
};
</script>

<template>
  <div class="annotation-overlay absolute inset-0 pointer-events-none z-10">
    <template v-for="ann in pageAnnotations" :key="ann.id">
      <div
        v-for="(rect, idx) in ann.rects"
        :key="`${ann.id}-${idx}`"
        class="annotation-highlight pointer-events-auto cursor-pointer transition-colors duration-150"
        :style="{
          position: 'absolute',
          left: `${rect.x * scale * 100 / scale}%`,
          top: `${rect.y * scale * 100 / scale}%`,
          width: `${rect.w * scale * 100 / scale}%`,
          height: `${rect.h * scale * 100 / scale}%`,
          backgroundColor: colorMap[ann.color] || colorMap.yellow,
          borderRadius: '2px',
          mixBlendMode: 'multiply',
        }"
        :title="ann.text"
        @click.stop="emit('click-annotation', ann)"
        @mouseenter="($event.target as HTMLElement).style.backgroundColor = hoverColorMap[ann.color] || hoverColorMap.yellow"
        @mouseleave="($event.target as HTMLElement).style.backgroundColor = colorMap[ann.color] || colorMap.yellow"
      >
        <!-- Note indicator dot -->
        <div
          v-if="ann.content && idx === 0"
          class="absolute -top-1 -right-1 w-2.5 h-2.5 rounded-full border border-white shadow-sm"
          :style="{ backgroundColor: ann.color === 'yellow' ? '#f59e0b' : ann.color === 'green' ? '#22c55e' : ann.color === 'blue' ? '#3b82f6' : '#ec4899' }"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.annotation-highlight:hover {
  outline: 1px solid rgba(0,0,0,0.15);
}
</style>
