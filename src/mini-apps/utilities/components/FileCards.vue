<script setup lang="ts">
/**
 * The files a tool will work on, as cards that can be put in order.
 *
 * Order is shown with a number only where it changes the result (merge,
 * images → PDF). Reordering is by dragging, or with the arrow buttons that
 * appear on focus, so it works without a mouse.
 */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { ArrowLeft, ArrowRight, Lock, Plus, TriangleAlert, X } from 'lucide-vue-next';
import PdfThumb from './PdfThumb.vue';
import { fileUrl } from '../render';
import { formatSize } from '../tools';
import type { Item } from '../useWorkbench';

const props = defineProps<{ items: Item[]; ordered: boolean; canAdd: boolean }>();
const emit = defineEmits<{
  (e: 'remove', index: number): void;
  (e: 'move', from: number, to: number): void;
  (e: 'add'): void;
}>();

const { t, locale } = useI18n();
const dragging = ref<number | null>(null);
const over = ref<number | null>(null);

function onDragStart(e: DragEvent, i: number) {
  dragging.value = i;
  // Our own drag, so the window's file drop leaves it alone.
  e.dataTransfer?.setData('application/x-synabit-utility-card', String(i));
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
}

/**
 * Only a card's own drag is taken here. Files dragged in from outside go on
 * up to the window's drop, which adds them; stopping them here lost them.
 */
function onOver(e: DragEvent, i: number) {
  if (dragging.value === null) return;
  e.preventDefault();
  over.value = i;
}

function onDrop(e: DragEvent, i: number) {
  if (dragging.value === null) return;
  e.preventDefault();
  e.stopPropagation();
  if (dragging.value !== i) emit('move', dragging.value, i);
  dragging.value = over.value = null;
}

function caption(item: Item): string {
  const size = formatSize(item.input.size, locale.value);
  const parts = [size];
  if (item.pages) parts.unshift(t('utilities.files.pages', { n: item.pages }, item.pages));
  if (item.width) parts.unshift(`${item.width} × ${item.height}`);
  if (item.lock) parts.push(t(`utilities.files.lock_${item.lock}`));
  return parts.join(' · ');
}
</script>

<template>
  <ul class="grid gap-3.5 list-none p-0 m-0" style="grid-template-columns: repeat(auto-fill, minmax(150px, 1fr))">
    <li
      v-for="(item, i) in props.items"
      :key="item.input.id"
      class="group relative rounded-xl border bg-surface dark:bg-surface-dark p-2.5 cursor-grab transition-shadow"
      :class="[
        over === i && dragging !== i ? 'border-accent ring-2 ring-accent/40' : 'border-border dark:border-border-dark',
        dragging === i ? 'opacity-40' : '',
      ]"
      draggable="true"
      @dragstart="onDragStart($event, i)"
      @dragover="onOver($event, i)"
      @dragleave="over = over === i ? null : over"
      @drop="onDrop($event, i)"
      @dragend="dragging = over = null"
    >
      <span
        v-if="ordered"
        class="absolute z-10 top-2 left-2 w-5 h-5 rounded-md bg-text dark:bg-text-dark text-base dark:text-base-dark text-xs font-bold grid place-items-center"
        aria-hidden="true"
      >{{ i + 1 }}</span>
      <button
        type="button"
        class="absolute z-10 top-1.5 right-1.5 w-6 h-6 rounded-md grid place-items-center bg-surface dark:bg-surface-dark text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark opacity-0 group-hover:opacity-100 focus:opacity-100"
        :aria-label="t('utilities.files.remove', { name: item.input.name })"
        :title="t('utilities.files.remove', { name: item.input.name })"
        @click="emit('remove', i)"
      >
        <X class="w-4 h-4" aria-hidden="true" />
      </button>

      <div class="flex justify-center">
        <div v-if="item.problem" class="w-full aspect-[1/1.3] rounded-sm bg-surface-hover dark:bg-surface-hover-dark grid place-items-center text-muted dark:text-muted-dark">
          <Lock v-if="item.problem === 'locked'" class="w-6 h-6" aria-hidden="true" />
          <TriangleAlert v-else class="w-6 h-6" aria-hidden="true" />
        </div>
        <PdfThumb v-else-if="item.kind === 'pdf'" :path="item.input.path" :width="128" />
        <img
          v-else
          :src="fileUrl(item.input.path)"
          alt=""
          loading="lazy"
          class="w-full aspect-square object-cover rounded-sm border border-border dark:border-border-dark bg-surface-hover"
        />
      </div>
      <p class="mt-2 text-[13px] font-medium truncate text-text dark:text-text-dark" :title="item.input.name">{{ item.input.name }}</p>
      <p v-if="item.problem" class="text-xs text-danger">{{ t(`utilities.files.problem_${item.problem}`) }}</p>
      <p v-else class="text-xs text-muted dark:text-muted-dark">{{ caption(item) }}</p>

      <div v-if="ordered && props.items.length > 1" class="flex gap-1 mt-1.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100">
        <button type="button" class="btn-icon !w-7 !h-7" :disabled="i === 0" :aria-label="t('utilities.files.move_earlier', { name: item.input.name })" @click="emit('move', i, i - 1)">
          <ArrowLeft class="w-4 h-4" aria-hidden="true" />
        </button>
        <button type="button" class="btn-icon !w-7 !h-7" :disabled="i === props.items.length - 1" :aria-label="t('utilities.files.move_later', { name: item.input.name })" @click="emit('move', i, i + 1)">
          <ArrowRight class="w-4 h-4" aria-hidden="true" />
        </button>
      </div>
    </li>
    <li v-if="canAdd" class="list-none">
      <button
        type="button"
        class="w-full h-full min-h-[190px] rounded-xl border-[1.5px] border-dashed border-border-subtle dark:border-border-subtle-dark flex flex-col items-center justify-center gap-1.5 text-muted dark:text-muted-dark hover:text-accent hover:border-accent dark:hover:text-accent-dark"
        @click="emit('add')"
      >
        <Plus class="w-5 h-5" aria-hidden="true" />
        <span class="text-sm">{{ t('utilities.files.add') }}</span>
      </button>
    </li>
  </ul>
</template>
