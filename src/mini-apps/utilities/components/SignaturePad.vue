<script setup lang="ts">
/**
 * Draw a signature with the mouse, a trackpad or a pen. Strokes are smoothed
 * the way the whiteboard's are (perfect-freehand), and pressure is used where
 * the pen gives it. "Use" hands back the signature cut to its ink, on a clear
 * background, ready to place.
 *
 * A `<dialog>` opened modal: focus stays in it, Esc closes it.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { inkFromStrokes, strokePath, type Ink } from '../marks';

const props = defineProps<{ color: string }>();
const emit = defineEmits<{ (e: 'done', ink: Ink): void; (e: 'close'): void }>();
const { t } = useI18n();

const W = 560;
const H = 220;
const SIZE = 4.5;

const dialog = ref<HTMLDialogElement | null>(null);
const strokes = ref<number[][][]>([]);
const current = ref<number[][] | null>(null);

onMounted(() => dialog.value?.showModal());

const paths = computed(() => [...strokes.value, ...(current.value ? [current.value] : [])].map((s) => strokePath(s, SIZE)));

function at(e: PointerEvent): number[] {
  const r = (e.currentTarget as SVGElement).getBoundingClientRect();
  // A mouse reports 0.5 (or 0); a pen reports what it feels.
  return [e.clientX - r.left, e.clientY - r.top, e.pointerType === 'pen' ? e.pressure : 0.5];
}

function down(e: PointerEvent) {
  (e.currentTarget as Element).setPointerCapture(e.pointerId);
  current.value = [at(e)];
}

function moved(e: PointerEvent) {
  if (current.value) current.value = [...current.value, at(e)];
}

function up() {
  if (current.value && current.value.length > 1) strokes.value = [...strokes.value, current.value];
  current.value = null;
}

function use() {
  const ink = inkFromStrokes(strokes.value, W, H, props.color, SIZE);
  if (ink) emit('done', ink);
}
</script>

<template>
  <dialog
    ref="dialog"
    class="m-auto rounded-2xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark text-text dark:text-text-dark p-5 shadow-2xl backdrop:bg-black/40"
    :aria-label="t('utilities.sign.pad_title')"
    @close="emit('close')"
  >
    <h2 class="text-lg font-semibold m-0">{{ t('utilities.sign.pad_title') }}</h2>
    <p class="text-sm text-text-secondary dark:text-text-secondary-dark mt-1 mb-3">{{ t('utilities.sign.pad_hint') }}</p>
    <svg
      :width="W"
      :height="H"
      class="block rounded-lg bg-white border border-border dark:border-border-dark touch-none cursor-crosshair"
      role="img"
      :aria-label="t('utilities.sign.pad_area')"
      @pointerdown="down"
      @pointermove="moved"
      @pointerup="up"
      @pointercancel="up"
    >
      <line :x1="24" :x2="W - 24" :y1="H - 48" :y2="H - 48" stroke="#d4d4d8" stroke-dasharray="4 4" />
      <path v-for="(d, i) in paths" :key="i" :d="d" :fill="color" />
    </svg>
    <div class="flex items-center gap-2 mt-4">
      <button type="button" class="btn-secondary" :disabled="!strokes.length" @click="strokes = []">{{ t('utilities.sign.clear') }}</button>
      <span class="flex-1" />
      <button type="button" class="btn-secondary" @click="dialog?.close()">{{ t('utilities.sign.cancel') }}</button>
      <button type="button" class="btn-primary" :disabled="!strokes.length" @click="use">{{ t('utilities.sign.use') }}</button>
    </div>
  </dialog>
</template>
