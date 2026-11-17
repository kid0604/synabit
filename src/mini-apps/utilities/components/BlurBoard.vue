<script setup lang="ts">
/**
 * The blur tool's middle: one picture at a time with the parts to cover.
 * Faces are found as a picture is shown (with Vision); drag on the picture to
 * cover more, a box to move it, its corner to size it.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronLeft, ChevronRight, Loader2 } from 'lucide-vue-next';
import BoxLayer from './BoxLayer.vue';
import { clampBox, newId, type Box } from '../marks';
import { fileUrl } from '../render';
import { useWorkbench } from '../useWorkbench';

const { t } = useI18n();
const wb = useWorkbench();
const b = wb.options.imgblur;

const items = computed(() => wb.usable.value.filter((i) => i.kind === 'image' && i.width));
const item = computed(() => items.value[Math.min(b.index, items.value.length - 1)]);
const boxes = computed(() => (item.value ? b.boxes[item.value.input.id] ?? [] : []));
const finding = ref(false);

watch(
  () => [item.value?.input.id, b.faces],
  async () => {
    const it = item.value;
    if (!it || !b.faces || b.scanned[it.input.id]) return;
    finding.value = true;
    await wb.findFaces(it);
    finding.value = false;
  },
  { immediate: true },
);

const MAX_W = 640;
const MAX_H = 520;
const size = computed(() => {
  const it = item.value;
  if (!it?.width || !it.height) return null;
  const s = Math.min(MAX_W / it.width, MAX_H / it.height, 1);
  return { width: Math.round(it.width * s), height: Math.round(it.height * s) };
});

function set(list: typeof boxes.value) {
  if (item.value) b.boxes[item.value.input.id] = list;
}
function create(box: Box) {
  const made = { ...clampBox(box), id: newId('b'), page: 1 };
  set([...boxes.value, made]);
  b.selected = made.id;
}
function change(id: string, box: Box) {
  set(boxes.value.map((x) => (x.id === id ? { ...x, ...box } : x)));
}
function remove(id: string) {
  set(boxes.value.filter((x) => x.id !== id));
  if (b.selected === id) b.selected = null;
}

const look = computed(() =>
  b.style === 'black'
    ? 'background:#000'
    : `background:rgb(120 120 120 / 0.35);backdrop-filter:blur(${b.style === 'pixel' ? 4 : 10}px);-webkit-backdrop-filter:blur(${b.style === 'pixel' ? 4 : 10}px)`,
);
</script>

<template>
  <div v-if="item && size" class="flex flex-col gap-2">
    <div class="flex items-center gap-2 text-sm text-text-secondary dark:text-text-secondary-dark" :style="{ maxWidth: `${size.width}px` }">
      <span class="truncate">{{ item.input.name }} · {{ t('utilities.imgblur.draw_hint') }}</span>
      <span class="flex-1" />
      <template v-if="items.length > 1">
        <span class="tabular-nums">{{ t('utilities.cutout.n_of', { n: b.index + 1, total: items.length }) }}</span>
        <button type="button" class="btn-icon" :aria-label="t('utilities.cutout.previous')" :disabled="b.index === 0" @click="b.index--"><ChevronLeft class="w-4 h-4" aria-hidden="true" /></button>
        <button type="button" class="btn-icon" :aria-label="t('utilities.cutout.next')" :disabled="b.index >= items.length - 1" @click="b.index++"><ChevronRight class="w-4 h-4" aria-hidden="true" /></button>
      </template>
    </div>
    <div class="relative select-none border border-border dark:border-border-dark" :style="{ width: `${size.width}px`, height: `${size.height}px` }">
      <img :src="fileUrl(item.input.path)" alt="" class="block w-full h-full" draggable="false" />
      <BoxLayer
        :items="boxes"
        :selected="b.selected"
        :width="size.width"
        :height="size.height"
        :label="() => t('utilities.imgblur.box')"
        draw
        @select="b.selected = $event"
        @create="create"
        @change="change"
        @remove="remove"
      >
        <template #default>
          <span class="block w-full h-full" :style="look" />
        </template>
      </BoxLayer>
      <div v-if="finding" class="absolute inset-0 grid place-items-center bg-black/20 pointer-events-none">
        <span class="flex items-center gap-2 text-sm font-medium text-white bg-black/55 rounded-full px-3 py-1.5">
          <Loader2 class="w-4 h-4 animate-spin" aria-hidden="true" />{{ t('utilities.imgblur.finding') }}
        </span>
      </div>
    </div>
  </div>
</template>
