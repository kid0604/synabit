<script setup lang="ts">
/**
 * One picture before and after its background goes: drag the bar to compare.
 * The subject is found when the picture is shown, and the same mask is used
 * for the result, so what is seen here is what is saved. Arrow keys move the
 * bar for anyone not using a mouse.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronLeft, ChevronRight, GripVertical, Loader2 } from 'lucide-vue-next';
import { cutOut, fileUrl, loadImage } from '../render';
import { useWorkbench, type Item } from '../useWorkbench';
import { errorText } from '../../../shared/errorText';

const props = defineProps<{ items: Item[] }>();
const { t } = useI18n();
const wb = useWorkbench();

const index = ref(0);
const item = computed(() => props.items[Math.min(index.value, props.items.length - 1)]);
const after = ref<string | null>(null);
const state = ref<'finding' | 'ready' | 'none' | 'failed'>('finding');
const reason = ref('');
const split = ref(50);

let serial = 0;
watch(
  () => [item.value?.input.id, wb.options.removebg.backdrop, wb.options.removebg.trim],
  async () => {
    const it = item.value;
    if (!it) return;
    const ask = ++serial;
    state.value = 'finding';
    try {
      const [img, mask] = await Promise.all([loadImage(it.input.path), wb.maskFor(it)]);
      if (ask !== serial) return;
      // The preview is never trimmed: trimming would move the picture under the bar.
      const canvas = cutOut(img, mask, wb.options.removebg.backdrop, false);
      const url = await new Promise<string>((resolve) => canvas.toBlob((b) => resolve(b ? URL.createObjectURL(b) : ''), 'image/png'));
      canvas.width = canvas.height = 0;
      if (ask !== serial) return URL.revokeObjectURL(url);
      if (after.value) URL.revokeObjectURL(after.value);
      after.value = url;
      state.value = 'ready';
    } catch (e) {
      if (ask !== serial) return;
      // The last picture's cut-out is not this one's: showing it would say
      // this picture had a subject.
      if (after.value) URL.revokeObjectURL(after.value);
      after.value = null;
      const text = errorText(e);
      state.value = /no-subject/.test(text) ? 'none' : 'failed';
      reason.value = text;
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  if (after.value) URL.revokeObjectURL(after.value);
});

const box = ref<HTMLElement | null>(null);
let dragging = false;
function moveTo(e: PointerEvent) {
  const r = box.value?.getBoundingClientRect();
  if (!r) return;
  split.value = Math.max(0, Math.min(100, ((e.clientX - r.left) / r.width) * 100));
}
function down(e: PointerEvent) {
  dragging = true;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  moveTo(e);
}
function key(e: KeyboardEvent) {
  const step = e.shiftKey ? 10 : 2;
  if (e.key === 'ArrowLeft') split.value = Math.max(0, split.value - step);
  else if (e.key === 'ArrowRight') split.value = Math.min(100, split.value + step);
  else return;
  e.preventDefault();
}

const aspect = computed(() => (item.value?.width && item.value.height ? `${item.value.width} / ${item.value.height}` : '4 / 3'));
const checker = 'background-color:#fff;background-image:linear-gradient(45deg,#e5e5e5 25%,transparent 25%),linear-gradient(-45deg,#e5e5e5 25%,transparent 25%),linear-gradient(45deg,transparent 75%,#e5e5e5 75%),linear-gradient(-45deg,transparent 75%,#e5e5e5 75%);background-size:20px 20px;background-position:0 0,0 10px,10px -10px,-10px 0';
</script>

<template>
  <div v-if="item" class="flex flex-col gap-2 max-w-[560px]">
    <div class="flex items-center gap-2 text-sm text-text-secondary dark:text-text-secondary-dark">
      <span class="truncate">{{ item.input.name }} · {{ t('utilities.cutout.drag') }}</span>
      <span class="flex-1" />
      <template v-if="items.length > 1">
        <span class="tabular-nums">{{ t('utilities.cutout.n_of', { n: index + 1, total: items.length }) }}</span>
        <button type="button" class="btn-icon" :aria-label="t('utilities.cutout.previous')" :disabled="index === 0" @click="index--"><ChevronLeft class="w-4 h-4" aria-hidden="true" /></button>
        <button type="button" class="btn-icon" :aria-label="t('utilities.cutout.next')" :disabled="index >= items.length - 1" @click="index++"><ChevronRight class="w-4 h-4" aria-hidden="true" /></button>
      </template>
    </div>
    <div
      ref="box"
      class="relative w-full rounded-xl overflow-hidden border border-border dark:border-border-dark select-none touch-none"
      :style="{ aspectRatio: aspect }"
      @pointerdown="down"
      @pointermove="dragging && moveTo($event)"
      @pointerup="dragging = false"
      @pointercancel="dragging = false"
    >
      <img :src="fileUrl(item.input.path)" alt="" class="absolute inset-0 w-full h-full object-contain bg-surface-hover dark:bg-surface-hover-dark" draggable="false" />
      <div v-if="after" class="absolute inset-0" :style="`${wb.options.removebg.backdrop === 'transparent' ? checker : `background:${wb.options.removebg.backdrop}`};clip-path:inset(0 0 0 ${split}%)`">
        <img :src="after" alt="" class="w-full h-full object-contain" draggable="false" />
      </div>
      <span class="absolute top-2.5 left-2.5 text-xs font-semibold px-2 py-0.5 rounded-full bg-black/55 text-white">{{ t('utilities.cutout.before') }}</span>
      <span class="absolute top-2.5 right-2.5 text-xs font-semibold px-2 py-0.5 rounded-full bg-black/55 text-white">{{ t('utilities.cutout.after') }}</span>
      <div
        v-if="state === 'ready'"
        role="slider"
        tabindex="0"
        :aria-label="t('utilities.cutout.compare')"
        aria-valuemin="0"
        aria-valuemax="100"
        :aria-valuenow="Math.round(split)"
        class="absolute top-0 bottom-0 w-0.5 bg-white shadow-[0_0_0_1px_rgba(0,0,0,0.2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
        :style="{ left: `${split}%` }"
        @keydown="key"
      >
        <span class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-8 h-8 rounded-full bg-white text-gray-700 grid place-items-center shadow">
          <GripVertical class="w-4 h-4" aria-hidden="true" />
        </span>
      </div>
      <div v-if="state === 'finding'" class="absolute inset-0 grid place-items-center bg-black/25">
        <span class="flex items-center gap-2 text-sm font-medium text-white bg-black/55 rounded-full px-3 py-1.5">
          <Loader2 class="w-4 h-4 animate-spin" aria-hidden="true" />{{ t('utilities.cutout.finding') }}
        </span>
      </div>
    </div>
    <p v-if="state === 'none'" role="status" class="text-sm text-warning m-0">{{ t('utilities.errors.no_subject') }}</p>
    <p v-else-if="state === 'failed'" role="alert" class="text-sm text-danger m-0">{{ t('utilities.errors.other', { reason }) }}</p>
  </div>
</template>
