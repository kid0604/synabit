<script setup lang="ts">
import { ref, watch } from 'vue';
import getStroke from 'perfect-freehand';
import { Eraser } from 'lucide-vue-next';

interface StrokeData {
  points: number[][];
  color: string;
  size: number;
}

const props = defineProps<{
  page: number;
  width: number;
  height: number;
  color: string;
  size: number;
  active: boolean;
  initialStrokes?: StrokeData[];
}>();

const emit = defineEmits<{
  (e: 'save', strokes: StrokeData[]): void;
}>();

const svgRef = ref<SVGSVGElement | null>(null);
const strokes = ref<StrokeData[]>(props.initialStrokes || []);
const currentPoints = ref<number[][]>([]);
const isDrawing = ref(false);
const isErasing = ref(false);

watch(() => props.initialStrokes, (newStrokes) => {
  if (newStrokes) {
    strokes.value = [...newStrokes];
  }
}, { deep: true });

watch(() => props.active, (active) => {
  if (!active) {
    isErasing.value = false;
    isDrawing.value = false;
    currentPoints.value = [];
  }
});

const getStrokePath = (points: number[][], size: number): string => {
  if (points.length < 2) return '';
  const outlinePoints = getStroke(points, {
    size,
    thinning: 0.5,
    smoothing: 0.5,
    streamline: 0.5,
  });
  if (outlinePoints.length < 2) return '';
  const d = outlinePoints.reduce(
    (acc, [x, y], i, arr) => {
      if (i === 0) return `M ${x},${y}`;
      const [cx, cy] = arr[i - 1];
      const mx = (cx + x) / 2;
      const my = (cy + y) / 2;
      return `${acc} Q ${cx},${cy} ${mx},${my}`;
    },
    ''
  );
  return `${d} Z`;
};

const handlePointerDown = (e: PointerEvent) => {
  if (!props.active || isErasing.value) return;
  isDrawing.value = true;
  const rect = svgRef.value!.getBoundingClientRect();
  currentPoints.value = [[e.clientX - rect.left, e.clientY - rect.top, e.pressure]];
  (e.target as Element).setPointerCapture(e.pointerId);
};

const handlePointerMove = (e: PointerEvent) => {
  if (!props.active || !isDrawing.value) return;
  const rect = svgRef.value!.getBoundingClientRect();
  currentPoints.value = [...currentPoints.value, [e.clientX - rect.left, e.clientY - rect.top, e.pressure]];
};

const handlePointerUp = () => {
  if (!props.active || !isDrawing.value) return;
  isDrawing.value = false;
  if (currentPoints.value.length > 1) {
    strokes.value.push({
      points: currentPoints.value,
      color: props.color,
      size: props.size,
    });
    emit('save', strokes.value);
  }
  currentPoints.value = [];
};

// Eraser: click on a stroke path to remove it
const handleEraseClick = (index: number) => {
  if (!props.active || !isErasing.value) return;
  strokes.value.splice(index, 1);
  emit('save', strokes.value);
};

const clearAll = () => {
  if (!props.active) return;
  strokes.value = [];
  emit('save', []);
};
</script>

<template>
  <div class="absolute inset-0 z-20" :class="[active ? (isErasing ? 'cursor-crosshair' : 'cursor-crosshair') : 'pointer-events-none']">
    <!-- Eraser toggle button -->
    <div v-if="active" class="absolute top-2 left-2 z-30 flex gap-1 pointer-events-auto">
      <button @click="isErasing = !isErasing" :class="isErasing ? 'bg-red-100 dark:bg-red-500/20 text-red-500' : 'bg-white/80 dark:bg-black/40 text-gray-500'"
        class="p-1.5 rounded-md shadow-sm text-xs transition-colors cursor-pointer" title="Eraser">
        <Eraser class="w-3.5 h-3.5" />
      </button>
      <button v-if="strokes.length > 0" @click="clearAll"
        class="px-2 py-1 rounded-md bg-white/80 dark:bg-black/40 shadow-sm text-[10px] font-medium text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors cursor-pointer">
        Clear
      </button>
    </div>

    <svg ref="svgRef"
      :viewBox="`0 0 ${width} ${height}`"
      :width="width" :height="height"
      class="absolute inset-0"
      :class="[active ? 'pointer-events-auto' : 'pointer-events-none']"
      @pointerdown="handlePointerDown"
      @pointermove="handlePointerMove"
      @pointerup="handlePointerUp"
      @pointerleave="handlePointerUp"
      style="touch-action: none;"
    >
      <!-- Saved strokes -->
      <path
        v-for="(stroke, i) in strokes"
        :key="i"
        :d="getStrokePath(stroke.points, stroke.size)"
        :fill="stroke.color"
        :opacity="active && isErasing ? 0.5 : 1"
        :class="[active && isErasing ? 'cursor-pointer hover:opacity-25 pointer-events-auto' : 'pointer-events-none']"
        @click.stop="handleEraseClick(i)"
      />
      <!-- Current stroke (while drawing) -->
      <path
        v-if="currentPoints.length > 1"
        :d="getStrokePath(currentPoints, props.size)"
        :fill="props.color"
      />
    </svg>
  </div>
</template>
