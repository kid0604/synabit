<script setup lang="ts">
/**
 * Every page of one PDF, for the tools that work on pages.
 *
 * - **Split** shows where the cuts fall: each part a colour along the bottom,
 *   scissors between parts. In "pick pages" mode a click selects, ⇧-click a
 *   run.
 * - **Organize** lets pages be dragged into a new order, turned, or left out.
 *   From the keyboard, ⌥← and ⌥→ on a page's buttons move it.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Check, RotateCcw, RotateCw, Scissors, Trash2 } from 'lucide-vue-next';
import PdfThumb from './PdfThumb.vue';
import type { PageSlot } from '../useWorkbench';

const props = defineProps<{
  path: string;
  mode: 'split' | 'organize';
  pageCount: number;
  /** Split: the pages of each part. */
  parts?: number[][];
  /** Split in pick mode: pages are clicked, not shown in parts. */
  picking?: boolean;
  picked?: Set<number>;
  /** Organize: the pages in their new order. */
  slots?: PageSlot[];
}>();

const emit = defineEmits<{
  (e: 'toggle', page: number, extend: boolean): void;
  (e: 'turn', index: number, by: number): void;
  (e: 'drop-page', index: number): void;
  (e: 'move', from: number, to: number): void;
}>();

const { t } = useI18n();

/** Colours for parts, repeating, chosen to tell apart side by side. */
const PART_COLOURS = ['#60a5fa', '#f59e0b', '#a78bfa', '#34d399', '#f472b6'];

const partOf = computed(() => {
  const map = new Map<number, number>();
  (props.parts ?? []).forEach((pages, k) => pages.forEach((p) => map.set(p, k)));
  return map;
});

/** A cut after this page: the next page starts another part. */
function cutAfter(page: number): boolean {
  if (props.picking || page >= props.pageCount) return false;
  const here = partOf.value.get(page);
  const next = partOf.value.get(page + 1);
  return here !== undefined && next !== undefined && here !== next;
}

const pages = computed(() => Array.from({ length: props.pageCount }, (_, i) => i + 1));

const dragging = ref<number | null>(null);
const over = ref<number | null>(null);

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
</script>

<template>
  <ul v-if="mode === 'split'" class="grid gap-x-5 gap-y-4 list-none p-0 m-0" style="grid-template-columns: repeat(auto-fill, minmax(112px, 1fr))">
    <li v-for="page in pages" :key="page" class="relative flex flex-col items-center">
      <button
        type="button"
        class="relative rounded-sm"
        :class="[
          picking && picked?.has(page) ? 'ring-[2.5px] ring-accent ring-offset-2 ring-offset-base dark:ring-offset-base-dark' : '',
          !picking && partOf.has(page) ? '' : !picking ? 'opacity-40' : '',
          picking ? 'cursor-pointer' : 'cursor-default',
        ]"
        :aria-pressed="picking ? picked?.has(page) : undefined"
        :aria-label="t('utilities.pages.page', { n: page })"
        :tabindex="picking ? 0 : -1"
        @click="picking && emit('toggle', page, $event.shiftKey)"
      >
        <PdfThumb :path="path" :page="page" :width="104" />
        <span
          v-if="!picking && partOf.has(page)"
          class="absolute left-0 right-0 bottom-0 h-1"
          :style="{ background: PART_COLOURS[partOf.get(page)! % PART_COLOURS.length] }"
          aria-hidden="true"
        />
        <span
          v-if="picking"
          class="absolute top-1.5 right-1.5 w-[18px] h-[18px] rounded-[5px] border-[1.5px] grid place-items-center"
          :class="picked?.has(page) ? 'bg-accent border-accent text-white' : 'bg-surface border-border-subtle'"
          aria-hidden="true"
        >
          <Check v-if="picked?.has(page)" class="w-3 h-3" />
        </span>
      </button>
      <Scissors v-if="cutAfter(page)" class="absolute -right-4 top-[38%] w-4 h-4 text-accent dark:text-accent-dark" :aria-label="t('utilities.pages.cut_after', { n: page })" />
      <span class="mt-1 text-xs text-muted dark:text-muted-dark tabular-nums">{{ page }}</span>
    </li>
  </ul>

  <ul v-else class="grid gap-x-5 gap-y-4 list-none p-0 m-0" style="grid-template-columns: repeat(auto-fill, minmax(112px, 1fr))">
    <li
      v-for="(slot, i) in slots"
      :key="slot.key"
      class="group relative flex flex-col items-center cursor-grab"
      :class="dragging === i ? 'opacity-40' : ''"
      draggable="true"
      @dragstart="dragging = i; $event.dataTransfer?.setData('application/x-synabit-utility-card', String(i))"
      @dragover="onOver($event, i)"
      @dragleave="over = over === i ? null : over"
      @drop="onDrop($event, i)"
      @dragend="dragging = over = null"
      @keydown.alt.left.prevent="i > 0 && emit('move', i, i - 1)"
      @keydown.alt.right.prevent="slots && i < slots.length - 1 && emit('move', i, i + 1)"
    >
      <div class="relative rounded-sm" :class="over === i && dragging !== i ? 'ring-2 ring-accent ring-offset-2 ring-offset-base dark:ring-offset-base-dark' : ''">
        <PdfThumb :path="path" :page="slot.page" :width="104" :rotate="slot.rotate" />
        <div class="absolute inset-x-0 bottom-1 flex justify-center gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100">
          <button type="button" class="w-7 h-7 rounded-md grid place-items-center bg-surface/95 dark:bg-surface-dark/95 shadow border border-border dark:border-border-dark" :aria-label="t('utilities.pages.turn_left', { n: slot.page })" aria-keyshortcuts="Alt+ArrowLeft Alt+ArrowRight" :title="t('utilities.pages.turn_left', { n: slot.page })" @click="emit('turn', i, -90)">
            <RotateCcw class="w-3.5 h-3.5" aria-hidden="true" />
          </button>
          <button type="button" class="w-7 h-7 rounded-md grid place-items-center bg-surface/95 dark:bg-surface-dark/95 shadow border border-border dark:border-border-dark" :aria-label="t('utilities.pages.turn_right', { n: slot.page })" :title="t('utilities.pages.turn_right', { n: slot.page })" @click="emit('turn', i, 90)">
            <RotateCw class="w-3.5 h-3.5" aria-hidden="true" />
          </button>
          <button type="button" class="w-7 h-7 rounded-md grid place-items-center bg-surface/95 dark:bg-surface-dark/95 shadow border border-border dark:border-border-dark text-danger" :aria-label="t('utilities.pages.leave_out', { n: slot.page })" :title="t('utilities.pages.leave_out', { n: slot.page })" @click="emit('drop-page', i)">
            <Trash2 class="w-3.5 h-3.5" aria-hidden="true" />
          </button>
        </div>
      </div>
      <span class="mt-1 text-xs text-muted dark:text-muted-dark tabular-nums">{{ slot.page }}</span>
    </li>
  </ul>
</template>
