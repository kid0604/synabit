<script setup lang="ts">
/**
 * One page of a PDF at a time, big enough to work on, with the page's own
 * layer on top (the slot): where Sign and fill places things and Redact draws
 * its boxes. The slot gets the page's size on screen and its shape, so what is
 * put on it can be kept as shares of the page.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronLeft, ChevronRight } from 'lucide-vue-next';
import { pagePicture } from '../render';

const props = defineProps<{ path: string; pageCount: number; page: number; marked?: Set<number> }>();
const emit = defineEmits<{
  (e: 'update:page', page: number): void;
  /** The shown page's width over its height, and its size in points, once drawn. */
  (e: 'shape', shape: { aspect: number; points: { width: number; height: number } }): void;
}>();
const { t } = useI18n();

const MAX_W = 720;
const MAX_H = 600;

const picture = ref<{ url: string; width: number; height: number; points: { width: number; height: number } } | null>(null);
const failed = ref(false);
let drawn = 0;
watch(
  () => [props.path, props.page] as const,
  async ([path, page]) => {
    const mine = ++drawn;
    failed.value = false;
    try {
      const made = await pagePicture(path, page, 1400);
      if (mine !== drawn) {
        URL.revokeObjectURL(made.url);
        return;
      }
      if (picture.value) URL.revokeObjectURL(picture.value.url);
      picture.value = made;
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

const shown = computed(() => {
  const p = picture.value;
  if (!p) return null;
  const s = Math.min(MAX_W / p.width, MAX_H / p.height);
  return { width: Math.round(p.width * s), height: Math.round(p.height * s), aspect: p.points.width / p.points.height, points: p.points };
});

watch(shown, (now) => {
  if (now) emit('shape', { aspect: now.aspect, points: now.points });
});

function go(to: number) {
  emit('update:page', Math.min(props.pageCount, Math.max(1, to)));
}
</script>

<template>
  <div class="flex flex-col items-start gap-2.5">
    <div class="flex items-center gap-1.5 text-sm text-text-secondary dark:text-text-secondary-dark">
      <button type="button" class="w-8 h-8 grid place-items-center rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark disabled:opacity-40" :disabled="page <= 1" :aria-label="t('utilities.board.previous')" @click="go(page - 1)">
        <ChevronLeft class="w-4 h-4" aria-hidden="true" />
      </button>
      <label class="flex items-center gap-1.5 tabular-nums">
        <span class="sr-only">{{ t('utilities.board.page') }}</span>
        <input
          type="number"
          min="1"
          :max="pageCount"
          :value="page"
          class="utility-field !w-16 !h-8 text-center"
          @change="go(Number(($event.target as HTMLInputElement).value) || 1)"
        />
        <span>/ {{ pageCount }}</span>
      </label>
      <button type="button" class="w-8 h-8 grid place-items-center rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark disabled:opacity-40" :disabled="page >= pageCount" :aria-label="t('utilities.board.next')" @click="go(page + 1)">
        <ChevronRight class="w-4 h-4" aria-hidden="true" />
      </button>
      <span v-if="marked?.size" class="ml-2 text-xs text-muted dark:text-muted-dark">
        {{ t('utilities.board.marked_pages', { pages: [...marked].sort((a, b) => a - b).join(', ') }) }}
      </span>
    </div>
    <p v-if="failed" class="text-sm text-danger m-0">{{ t('utilities.frame.not_drawn') }}</p>
    <div
      v-else-if="picture && shown"
      class="relative shadow-sm border border-border dark:border-border-dark select-none"
      :style="{ width: `${shown.width}px`, height: `${shown.height}px` }"
    >
      <img :src="picture.url" alt="" class="block w-full h-full" draggable="false" />
      <slot :width="shown.width" :height="shown.height" :aspect="shown.aspect" :points="shown.points" />
    </div>
    <div v-else class="w-[480px] h-[620px] max-w-full bg-surface-hover dark:bg-surface-hover-dark animate-pulse" aria-hidden="true" />
  </div>
</template>
