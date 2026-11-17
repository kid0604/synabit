<script setup lang="ts">
/**
 * Photos of pages, one at a time, with the page's four corners on top: found
 * by Vision on a Mac, dragged into place by hand anywhere (or moved with the
 * arrow keys, ⇧ for bigger steps). What is inside the corners is what becomes
 * the flat page.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronLeft, ChevronRight, Loader2, Maximize, ScanSearch } from 'lucide-vue-next';
import { fileUrl } from '../render';
import { WHOLE, type Point, type Quad } from '../straighten';
import { useWorkbench, type Item } from '../useWorkbench';

const props = defineProps<{ items: Item[] }>();
const { t } = useI18n();
const wb = useWorkbench();
const corners = wb.options.img2pdf.corners;

const index = ref(0);
watch(
  () => props.items.length,
  (n) => {
    if (index.value > n - 1) index.value = Math.max(0, n - 1);
  },
);
const item = computed(() => props.items[Math.min(index.value, props.items.length - 1)]);
const quad = computed<Quad | null>(() => (item.value ? corners[item.value.input.id] ?? null : null));
const finding = ref(false);

watch(
  () => item.value?.input.id,
  async () => {
    const it = item.value;
    if (!it || corners[it.input.id]) return;
    finding.value = true;
    await wb.findCorners(it);
    finding.value = false;
  },
  { immediate: true },
);

async function findAgain() {
  const it = item.value;
  if (!it) return;
  delete corners[it.input.id];
  finding.value = true;
  await wb.findCorners(it);
  finding.value = false;
}

function whole() {
  if (item.value) corners[item.value.input.id] = WHOLE.map((p) => [...p] as Point) as Quad;
}

const box = ref<HTMLElement | null>(null);
const dragging = ref<number | null>(null);

function setCorner(i: number, p: Point) {
  const it = item.value;
  const q = quad.value;
  if (!it || !q) return;
  const next = q.map((c) => [...c] as Point) as Quad;
  next[i] = [Math.min(1, Math.max(0, p[0])), Math.min(1, Math.max(0, p[1]))];
  corners[it.input.id] = next;
}

function down(i: number, e: PointerEvent) {
  e.preventDefault();
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  dragging.value = i;
}

function move(e: PointerEvent) {
  if (dragging.value === null || !box.value) return;
  const r = box.value.getBoundingClientRect();
  setCorner(dragging.value, [(e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height]);
}

function key(i: number, e: KeyboardEvent) {
  const q = quad.value;
  if (!q) return;
  const step = e.shiftKey ? 0.02 : 0.004;
  const by: Record<string, Point> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
  const m = by[e.key];
  if (!m) return;
  e.preventDefault();
  setCorner(i, [q[i][0] + m[0], q[i][1] + m[1]]);
}

const aspect = computed(() => (item.value?.width && item.value.height ? item.value.width / item.value.height : 3 / 4));
const outline = computed(() => (quad.value ? quad.value.map((p) => p.join(',')).join(' ') : ''));
const CORNER_KEYS = ['top_left', 'top_right', 'bottom_right', 'bottom_left'] as const;
</script>

<template>
  <div v-if="item" class="flex flex-col gap-2 w-full max-w-[520px]">
    <div class="flex items-center gap-2 text-sm text-text-secondary dark:text-text-secondary-dark">
      <span class="truncate">{{ item.input.name }}</span>
      <span class="flex-1" />
      <template v-if="items.length > 1">
        <span class="tabular-nums">{{ t('utilities.cutout.n_of', { n: index + 1, total: items.length }) }}</span>
        <button type="button" class="btn-icon" :aria-label="t('utilities.cutout.previous')" :disabled="index === 0" @click="index--"><ChevronLeft class="w-4 h-4" aria-hidden="true" /></button>
        <button type="button" class="btn-icon" :aria-label="t('utilities.cutout.next')" :disabled="index >= items.length - 1" @click="index++"><ChevronRight class="w-4 h-4" aria-hidden="true" /></button>
      </template>
    </div>
    <div
      ref="box"
      class="relative w-full rounded-xl overflow-hidden border border-border dark:border-border-dark select-none touch-none bg-surface-hover dark:bg-surface-hover-dark"
      :style="{ aspectRatio: aspect }"
      @pointermove="move"
      @pointerup="dragging = null"
      @pointercancel="dragging = null"
    >
      <img :src="fileUrl(item.input.path)" alt="" class="absolute inset-0 w-full h-full object-fill" draggable="false" />
      <svg v-if="quad" viewBox="0 0 1 1" preserveAspectRatio="none" class="absolute inset-0 w-full h-full pointer-events-none" aria-hidden="true">
        <path :d="`M0,0 H1 V1 H0 Z M${outline.split(' ').join(' L')} Z`" fill="rgb(0 0 0 / 0.45)" fill-rule="evenodd" />
        <polygon :points="outline" fill="none" stroke="white" stroke-width="2" vector-effect="non-scaling-stroke" />
      </svg>
      <template v-if="quad">
        <span
          v-for="(p, i) in quad"
          :key="i"
          role="button"
          tabindex="0"
          :aria-label="t(`utilities.straighten.${CORNER_KEYS[i]}`)"
          aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown"
          class="absolute w-5 h-5 -ml-2.5 -mt-2.5 rounded-full bg-white border-2 border-accent shadow cursor-grab focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          :style="{ left: `${p[0] * 100}%`, top: `${p[1] * 100}%` }"
          @pointerdown="down(i, $event)"
          @keydown="key(i, $event)"
        />
      </template>
      <div v-if="finding" class="absolute inset-0 grid place-items-center bg-black/25">
        <span class="flex items-center gap-2 text-sm font-medium text-white bg-black/55 rounded-full px-3 py-1.5">
          <Loader2 class="w-4 h-4 animate-spin" aria-hidden="true" />{{ t('utilities.straighten.finding') }}
        </span>
      </div>
    </div>
    <div class="flex flex-wrap gap-1.5">
      <button v-if="wb.vision.value" type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px]" :disabled="finding" @click="findAgain"><ScanSearch class="w-4 h-4" aria-hidden="true" />{{ t('utilities.straighten.find_again') }}</button>
      <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px]" @click="whole"><Maximize class="w-4 h-4" aria-hidden="true" />{{ t('utilities.straighten.whole') }}</button>
    </div>
    <p class="text-xs text-muted dark:text-muted-dark m-0">{{ t(wb.vision.value ? 'utilities.straighten.hint' : 'utilities.straighten.hint_by_hand') }}</p>
  </div>
</template>
