<script setup lang="ts">
/**
 * The first page with what will be drawn on it: a watermark or a page
 * number, placed by the same sums the Rust side uses (`markCentres`,
 * `turnedBox`), so moving a slider here shows what the file will get.
 */
import { computed, ref, watch, watchEffect } from 'vue';
import PdfThumb from './PdfThumb.vue';
import { imageMark, loadImage, shownPageSize, textMark } from '../render';
import { markCentres, numberText, turnedBox } from '../tools';
import { useWorkbench } from '../useWorkbench';

const props = defineProps<{ path: string; mode: 'watermark' | 'numbers'; pageCount: number }>();

const WIDTH = 380;
const wb = useWorkbench();
const page = ref<{ width: number; height: number } | null>(null);
watchEffect(async () => {
  page.value = await shownPageSize(props.path).catch(() => null);
});
const scale = computed(() => (page.value ? WIDTH / page.value.width : 1));
const height = computed(() => (page.value ? page.value.height * scale.value : WIDTH * 1.41));

// The watermark picture, redrawn when what it shows changes.
const mark = ref<{ url: string; width: number; height: number } | null>(null);
let serial = 0;
watch(
  () => [wb.options.watermark.mode, wb.options.watermark.text, wb.options.watermark.color, wb.options.watermark.opacity, wb.options.watermark.logo?.path],
  async () => {
    if (props.mode !== 'watermark') return;
    const ask = ++serial;
    const w = wb.options.watermark;
    let picture = null;
    try {
      if (w.mode === 'text') picture = textMark(w.text, w.color, w.opacity);
      else if (w.logo) picture = imageMark(await loadImage(w.logo.path), w.opacity);
    } catch {
      // A logo that will not decode shows nothing, not the last one.
      picture = null;
    }
    if (ask === serial) mark.value = picture;
  },
  { immediate: true },
);

/** Where each copy of the watermark goes, in CSS pixels from the top left. */
const placed = computed(() => {
  const w = wb.options.watermark;
  // This is page 1, which "all but the first page" leaves alone.
  if (!page.value || !mark.value || w.exceptFirst) return [];
  const pw = page.value.width;
  const ph = page.value.height;
  const sw = w.size * pw;
  const sh = (sw * mark.value.height) / mark.value.width;
  const box = turnedBox(sw, sh, w.rotate);
  return markCentres(w, [pw, ph], box).map(([cx, cy]) => ({
    left: (cx - sw / 2) * scale.value,
    top: (ph - cy - sh / 2) * scale.value,
    width: sw * scale.value,
    height: sh * scale.value,
  }));
});

/** The page number as page 1 will show it, where it goes. */
const number = computed(() => {
  if (!page.value) return null;
  const n = wb.options.numbers;
  if (n.skipFirst || !Number.isInteger(n.start)) return null;
  const total = n.start + props.pageCount - 1;
  const margin = Math.max(18, 0.045 * Math.min(page.value.width, page.value.height)) * scale.value;
  const style: Record<string, string> = {
    fontSize: `${n.size * scale.value}px`,
    fontFamily: 'Helvetica, Arial, sans-serif',
    color: '#404040',
    position: 'absolute',
    whiteSpace: 'nowrap',
  };
  const col = n.position % 3;
  if (col === 0) style.left = `${margin}px`;
  else if (col === 2) style.right = `${margin}px`;
  else {
    style.left = '50%';
    style.transform = 'translateX(-50%)';
  }
  if (n.position < 3) style.top = `${margin}px`;
  else style.bottom = `${margin}px`;
  return { text: numberText(n.format, n.start, total), style };
});
</script>

<template>
  <div class="relative shadow-sm" :style="{ width: `${WIDTH}px`, height: `${height}px` }">
    <PdfThumb :path="path" :width="WIDTH" />
    <div class="absolute inset-0 overflow-hidden pointer-events-none" aria-hidden="true">
      <template v-if="mode === 'watermark' && mark">
        <img
          v-for="(p, i) in placed"
          :key="i"
          :src="mark.url"
          alt=""
          class="absolute max-w-none"
          :class="wb.options.watermark.under ? 'mix-blend-multiply' : ''"
          :style="{ left: `${p.left}px`, top: `${p.top}px`, width: `${p.width}px`, height: `${p.height}px`, transform: `rotate(${wb.options.watermark.rotate}deg)` }"
        />
      </template>
      <span v-else-if="mode === 'numbers' && number" :style="number.style">{{ number.text }}</span>
    </div>
  </div>
</template>
