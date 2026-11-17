<script setup lang="ts">
/**
 * The first page of the first PDF, as the crop tool will change it: with a
 * box to drag (by hand), with the box found around what is on it (margins
 * trimmed), or laid on the paper it will be put on (new page size).
 *
 * The page is drawn once as a picture; the box lives in that picture's pixels
 * (`options.pdfcrop.rect`, `frame`), and the run turns it into shares of the
 * page, which is what Rust takes.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import CropBox from './CropBox.vue';
import { contentBox, pagePicture } from '../render';
import { PAPERS } from '../tools';
import { useWorkbench } from '../useWorkbench';

const { t } = useI18n();
const wb = useWorkbench();
const c = wb.options.pdfcrop;

const picture = ref<{ url: string; width: number; height: number } | null>(null);
const failed = ref(false);
const path = computed(() => wb.firstPdf.value?.input.path ?? null);

let drawn = 0;
watch(
  path,
  async (now) => {
    const mine = ++drawn;
    if (picture.value) URL.revokeObjectURL(picture.value.url);
    picture.value = null;
    failed.value = false;
    if (!now) return;
    try {
      const made = await pagePicture(now, 1, 900);
      if (mine !== drawn) {
        URL.revokeObjectURL(made.url);
        return;
      }
      picture.value = made;
      const frameChanged = c.frame?.width !== made.width || c.frame?.height !== made.height;
      c.frame = { width: made.width, height: made.height };
      // A box to start from, a little in from the edges, so there is one to see.
      if (!c.rect || frameChanged) {
        const [mx, my] = [made.width * 0.06, made.height * 0.06];
        c.rect = { x: mx, y: my, width: made.width - 2 * mx, height: made.height - 2 * my };
      }
    } catch {
      if (mine === drawn) failed.value = true;
    }
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  drawn++;
  if (picture.value) URL.revokeObjectURL(picture.value.url);
});

/** Margins trimmed: the box found on page 1, worked out again as the margin changes. */
const found = ref<[number, number, number, number] | null>(null);
let finding = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
watch(
  [path, () => c.mode, () => c.margin],
  ([p, mode, margin]) => {
    clearTimeout(timer);
    if (!p || mode !== 'auto' || !Number.isFinite(margin)) return;
    const mine = ++finding;
    timer = setTimeout(async () => {
      const box = await contentBox(p, 1, margin).catch(() => null);
      if (mine === finding) found.value = box;
    }, 150);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearTimeout(timer));

const MAX_W = 640;
const MAX_H = 460;
const shown = computed(() => {
  const p = picture.value;
  if (!p) return null;
  const s = Math.min(MAX_W / p.width, MAX_H / p.height, 1);
  return { width: p.width * s, height: p.height * s };
});

/** The paper, scaled to the screen, and the page as it lands on it. */
const paper = computed(() => {
  const p = picture.value;
  if (!p) return null;
  const [short, long] = PAPERS[c.paper];
  const landscape = c.orientation === 'landscape' || (c.orientation === 'auto' && p.width > p.height);
  const [pw, ph] = landscape ? [long, short] : [short, long];
  const s = Math.min(MAX_W / pw, MAX_H / ph);
  return { width: pw * s, height: ph * s };
});
</script>

<template>
  <div class="flex flex-col items-start gap-2">
    <p v-if="failed" class="text-sm text-danger m-0">{{ t('utilities.frame.not_drawn') }}</p>
    <template v-else-if="picture && shown">
      <CropBox v-if="c.mode === 'manual'" :src="picture.url" :width="picture.width" :height="picture.height" target="pdfcrop" />
      <div v-else-if="c.mode === 'auto'" class="relative rounded-sm overflow-hidden" :style="{ width: `${shown.width}px`, height: `${shown.height}px` }">
        <img :src="picture.url" alt="" class="block w-full h-full" draggable="false" />
        <div
          v-if="found"
          class="absolute border-2 border-dashed border-accent pointer-events-none"
          :style="{
            left: `${found[0] * 100}%`, top: `${found[1] * 100}%`,
            width: `${(found[2] - found[0]) * 100}%`, height: `${(found[3] - found[1]) * 100}%`,
            boxShadow: '0 0 0 4000px rgb(0 0 0 / 0.35)',
          }"
        />
      </div>
      <div
        v-else-if="paper"
        class="relative overflow-hidden bg-surface dark:bg-surface-dark border border-border dark:border-border-dark shadow-sm"
        :style="{ width: `${paper.width}px`, height: `${paper.height}px` }"
      >
        <!-- Absolute: in a grid or flex box a percentage height would follow the picture, not the paper. -->
        <img :src="picture.url" alt="" class="absolute inset-0 w-full h-full object-contain" draggable="false" />
      </div>
      <p class="text-xs text-muted dark:text-muted-dark m-0">{{ t(`utilities.frame.caption_${c.mode}`) }}</p>
    </template>
    <div v-else class="w-[320px] h-[420px] rounded-sm bg-surface-hover dark:bg-surface-hover-dark animate-pulse" aria-hidden="true" />
  </div>
</template>
