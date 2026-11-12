<script setup lang="ts">
/**
 * One page of a PDF, drawn small, and only once it scrolls into view: a
 * 500-page file in the page grid draws the twenty or so on screen, not all of
 * them at once.
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { drawPage } from '../render';
import { logger } from '../../../utils/logger';

const props = withDefaults(defineProps<{ path: string; page?: number; width?: number; rotate?: number }>(), {
  page: 1,
  width: 140,
  rotate: 0,
});

const canvas = ref<HTMLCanvasElement | null>(null);
const box = ref<HTMLElement | null>(null);
const drawn = ref(false);
const failed = ref(false);
let seen = false;
let observer: IntersectionObserver | null = null;

/**
 * Draw, one at a time: pdf.js refuses a second render on a canvas still being
 * drawn, so a turn pressed twice quickly would fail. A change that comes in
 * meanwhile is drawn once the current one is done.
 */
let drawing = false;
let again = false;
async function draw() {
  if (!canvas.value || !seen) return;
  if (drawing) {
    again = true;
    return;
  }
  drawing = true;
  try {
    do {
      again = false;
      try {
        await drawPage(props.path, props.page, canvas.value, props.width, props.rotate);
        drawn.value = true;
        failed.value = false;
      } catch (e) {
        logger.warn('[utilities] could not draw a page', e);
        failed.value = true;
      }
    } while (again && canvas.value);
  } finally {
    drawing = false;
  }
}

onMounted(() => {
  observer = new IntersectionObserver((entries) => {
    if (entries.some((e) => e.isIntersecting)) {
      seen = true;
      observer?.disconnect();
      void draw();
    }
  }, { rootMargin: '200px' });
  if (box.value) observer.observe(box.value);
});

onBeforeUnmount(() => observer?.disconnect());

watch(() => [props.path, props.page, props.rotate, props.width], () => void draw());
</script>

<template>
  <div
    ref="box"
    class="relative bg-white rounded-sm border border-border dark:border-border-dark overflow-hidden flex items-center justify-center"
    :style="{ width: `${width}px`, minHeight: drawn ? undefined : `${Math.round(width * 1.3)}px` }"
  >
    <canvas ref="canvas" class="block" :class="drawn ? '' : 'invisible absolute'" aria-hidden="true" />
    <span v-if="failed" class="text-xs text-muted px-2 text-center">{{ $t('utilities.files.no_preview') }}</span>
  </div>
</template>
