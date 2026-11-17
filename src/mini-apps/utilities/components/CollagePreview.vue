<script setup lang="ts">
/** The pictures put together, as the result will be (smaller, for speed), in the order of the list. */
import { onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { collageCanvas } from '../canvas';
import { useWorkbench } from '../useWorkbench';

const { t } = useI18n();
const wb = useWorkbench();
const c = wb.options.collage;
const shown = ref<string | null>(null);

let serial = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => [wb.usable.value.map((i) => i.input.id).join(), c.layout, c.gap],
  () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const paths = wb.usable.value.filter((i) => i.width).map((i) => i.input.path);
      if (paths.length < 2) {
        shown.value = null;
        return;
      }
      const mine = ++serial;
      try {
        const canvas = await collageCanvas(paths, c.layout, c.gap, 900);
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
  <figure class="m-0 flex flex-col gap-2 max-w-[520px]">
    <img v-if="shown" :src="shown" :alt="t('utilities.collage.preview')" class="block max-w-full max-h-[520px] object-contain self-start border border-border dark:border-border-dark shadow-sm" />
    <div v-else class="w-[360px] h-[420px] bg-surface-hover dark:bg-surface-hover-dark animate-pulse" aria-hidden="true" />
    <figcaption class="text-xs text-muted dark:text-muted-dark">{{ t('utilities.collage.caption') }}</figcaption>
  </figure>
</template>
