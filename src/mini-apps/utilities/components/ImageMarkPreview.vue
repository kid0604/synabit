<script setup lang="ts">
/**
 * The first picture with its watermark, drawn as the result will be (smaller,
 * for speed): the same place, size, turn and opacity the run uses.
 */
import { onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { markedCanvas } from '../canvas';
import { imageMark, loadImage, textMark } from '../render';
import { useWorkbench, type Item } from '../useWorkbench';

const props = defineProps<{ item: Item }>();
const { t } = useI18n();
const wb = useWorkbench();
const w = wb.options.watermark;
const shown = ref<string | null>(null);

let serial = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => [props.item.input.id, w.mode, w.text, w.color, w.opacity, w.size, w.rotate, w.position, w.tile, w.logo?.id],
  () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const mine = ++serial;
      try {
        const img = await loadImage(props.item.input.path);
        const mark = w.mode === 'text' ? textMark(w.text || t('utilities.imgmark.sample'), w.color, w.opacity) : w.logo ? imageMark(await loadImage(w.logo.path), w.opacity) : null;
        if (!mark) return;
        const canvas = await markedCanvas(img, mark, w, 900);
        const url = canvas.toDataURL('image/jpeg', 0.85);
        canvas.width = canvas.height = 0;
        if (mine === serial) shown.value = url;
      } catch {
        if (mine === serial) shown.value = null;
      }
    }, 150);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <figure class="m-0 flex flex-col gap-2 max-w-[560px]">
    <img v-if="shown" :src="shown" :alt="t('utilities.imgmark.preview')" class="block max-w-full max-h-[480px] rounded-sm border border-border dark:border-border-dark" />
    <div v-else class="w-[420px] h-[320px] bg-surface-hover dark:bg-surface-hover-dark animate-pulse rounded-sm" aria-hidden="true" />
    <figcaption class="text-xs text-muted dark:text-muted-dark">{{ t('utilities.imgmark.caption', { name: item.input.name }) }}</figcaption>
  </figure>
</template>
