<script setup lang="ts">
import { ref, computed } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { ZoomIn, ZoomOut, RotateCw, Maximize2 } from 'lucide-vue-next';

const props = defineProps<{
  filePath: string;
  vaultPath: string;
}>();

const zoom = ref(1);
const rotation = ref(0);
const isDragging = ref(false);
const offset = ref({ x: 0, y: 0 });
const dragStart = ref({ x: 0, y: 0 });

const imageSrc = computed(() => convertFileSrc(props.filePath));

const imageStyle = computed(() => ({
  transform: `scale(${zoom.value}) rotate(${rotation.value}deg) translate(${offset.value.x / zoom.value}px, ${offset.value.y / zoom.value}px)`,
  transition: isDragging.value ? 'none' : 'transform 0.2s ease',
  cursor: zoom.value > 1 ? (isDragging.value ? 'grabbing' : 'grab') : 'default',
}));

const zoomIn = () => { zoom.value = Math.min(zoom.value + 0.25, 5); };
const zoomOut = () => { zoom.value = Math.max(zoom.value - 0.25, 0.25); };
const rotate = () => { rotation.value = (rotation.value + 90) % 360; };
const resetView = () => { zoom.value = 1; rotation.value = 0; offset.value = { x: 0, y: 0 }; };

const onMouseDown = (e: MouseEvent) => {
  if (zoom.value <= 1) return;
  isDragging.value = true;
  dragStart.value = { x: e.clientX - offset.value.x, y: e.clientY - offset.value.y };
};
const onMouseMove = (e: MouseEvent) => {
  if (!isDragging.value) return;
  offset.value = { x: e.clientX - dragStart.value.x, y: e.clientY - dragStart.value.y };
};
const onMouseUp = () => { isDragging.value = false; };
const onWheel = (e: WheelEvent) => {
  e.preventDefault();
  if (e.deltaY < 0) zoomIn(); else zoomOut();
};

defineExpose({ zoomIn, zoomOut, rotate, resetView, zoom });
</script>

<template>
  <div class="relative flex-1 flex flex-col overflow-hidden bg-[#f0f0f0] dark:bg-[#1a1a1a]">
    <!-- Toolbar -->
    <div class="flex items-center justify-center gap-2 px-4 py-2 bg-white/80 dark:bg-[#222]/80 backdrop-blur border-b border-gray-200/50 dark:border-white/5">
      <button @click="zoomOut" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><ZoomOut class="w-4 h-4" /></button>
      <span class="text-xs font-mono text-gray-500 w-12 text-center">{{ Math.round(zoom * 100) }}%</span>
      <button @click="zoomIn" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><ZoomIn class="w-4 h-4" /></button>
      <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-1" />
      <button @click="rotate" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><RotateCw class="w-4 h-4" /></button>
      <button @click="resetView" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><Maximize2 class="w-4 h-4" /></button>
    </div>
    <!-- Image -->
    <div
      class="flex-1 flex items-center justify-center overflow-hidden select-none"
      @mousedown="onMouseDown"
      @mousemove="onMouseMove"
      @mouseup="onMouseUp"
      @mouseleave="onMouseUp"
      @wheel.passive="onWheel"
    >
      <img
        :src="imageSrc"
        :style="imageStyle"
        class="max-w-full max-h-full object-contain"
        draggable="false"
        @load=""
      />
    </div>
  </div>
</template>
