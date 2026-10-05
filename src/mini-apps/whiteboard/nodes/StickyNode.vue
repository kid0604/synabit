<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { NodeResizer } from '@vue-flow/node-resizer';
import { Handle, Position } from '@vue-flow/core';
import { STICKY_COLORS, stickyColor } from '../sticky';
import RotateHandle from '../components/RotateHandle.vue';
import { useOnlySelected } from '../composables/useOnlySelected';

const props = defineProps<{
  id: string;
  selected?: boolean;
  data: {
    label: string;
    color?: string;
    width?: number;
    height?: number;
    locked?: boolean;
    editing?: boolean;
    rotation?: number;
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'rotate', degrees: number, final: boolean): void;
}>();

const isEditing = ref(!!props.data.editing);
const alone = useOnlySelected(() => props.selected);
const editText = ref(props.data.label ?? '');
const inputRef = ref<HTMLTextAreaElement | null>(null);
const textRef = ref<HTMLElement | null>(null);

/**
 * The largest type the words fit in.
 *
 * A sticky note is a fixed square: a few words are big and a paragraph is
 * small, as on paper. Worked out by trying sizes on the text as it is drawn,
 * largest first, so it is right for the font and the words rather than for
 * a guess about how long a character is.
 */
const fontSize = ref(24);
const MAX_FONT = 28;
const MIN_FONT = 10;

async function fit() {
  await nextTick();
  const el = textRef.value;
  if (!el) return;
  let size = MAX_FONT;
  el.style.fontSize = `${size}px`;
  while (size > MIN_FONT && (el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth)) {
    size -= 1;
    el.style.fontSize = `${size}px`;
  }
  fontSize.value = size;
}

onMounted(() => {
  fit();
  if (isEditing.value) focusEditor();
});
watch(() => [props.data.label, props.data.width, props.data.height], () => fit());

const paper = computed(() => stickyColor(props.data.color));

function focusEditor() {
  nextTick(() => {
    inputRef.value?.focus();
    inputRef.value?.select();
  });
}

function startEdit() {
  if (props.data.locked) return;
  isEditing.value = true;
  editText.value = props.data.label ?? '';
  focusEditor();
}

function finishEdit() {
  if (!isEditing.value) return;
  isEditing.value = false;
  emit('update:data', { label: editText.value, editing: undefined });
}

function cancelEdit() {
  if (!isEditing.value) return;
  isEditing.value = false;
  if (props.data.editing) emit('update:data', { editing: undefined });
}

function onResizeEnd(event: any) {
  emit('update:data', { width: Math.round(event.params.width), height: Math.round(event.params.height) });
}

function setColor(value: string) {
  emit('update:data', { color: value });
}

defineExpose({ startEdit });
</script>

<template>
  <div
    class="wb-sticky"
    :style="{ background: paper.fill, '--sticky-edge': paper.edge, transform: data.rotation ? `rotate(${data.rotation}deg)` : undefined }"
    @dblclick.stop="startEdit"
  >
    <NodeResizer
      :is-visible="!!selected && !isEditing && !data.locked"
      :min-width="80"
      :min-height="80"
      color="var(--wb-selection, var(--color-accent))"
      @resize-end="onResizeEnd"
    />

    <textarea
      v-if="isEditing"
      ref="inputRef"
      v-model="editText"
      class="wb-sticky__input nodrag nopan"
      :style="{ fontSize: fontSize + 'px' }"
      :aria-label="$t('whiteboard.sticky')"
      @blur="finishEdit"
      @keydown.escape.stop="cancelEdit"
      @keydown.enter.exact.meta.prevent="finishEdit"
      @keydown.enter.exact.ctrl.prevent="finishEdit"
    />
    <div v-else ref="textRef" class="wb-sticky__text" :style="{ fontSize: fontSize + 'px' }">
      {{ data.label || '' }}
    </div>

    <RotateHandle
      v-if="alone && !isEditing && !data.locked"
      :node-id="id"
      :rotation="data.rotation"
      :label="$t('whiteboard.rotate')"
      @rotate="(deg: number, final: boolean) => emit('rotate', deg, final)"
    />

    <!-- The paper colours, one click away while the note is selected. -->
    <div v-if="alone && !isEditing && !data.locked" class="wb-sticky__colors nodrag nopan" role="toolbar" :aria-label="$t('whiteboard.color')">
      <button
        v-for="c in STICKY_COLORS"
        :key="c.value"
        class="wb-sticky__color"
        :class="{ active: paper.value === c.value }"
        :style="{ background: c.fill }"
        :title="$t(c.labelKey)"
        :aria-label="$t(c.labelKey)"
        :aria-pressed="paper.value === c.value"
        @click.stop="setColor(c.value)"
      />
    </div>

    <Handle id="top" type="source" :position="Position.Top" class="wb-sticky__handle" />
    <Handle id="right" type="source" :position="Position.Right" class="wb-sticky__handle" />
    <Handle id="bottom" type="source" :position="Position.Bottom" class="wb-sticky__handle" />
    <Handle id="left" type="source" :position="Position.Left" class="wb-sticky__handle" />
  </div>
</template>

<style scoped>
.wb-sticky {
  position: relative;
  width: 100%;
  height: 100%;
  padding: 14px;
  /* Paper on any canvas, light or dark: its ink is always dark. */
  color: #1f2937;
  border-radius: 3px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08), 0 6px 14px -6px rgba(0, 0, 0, 0.25);
  border-bottom: 3px solid var(--sticky-edge);
  cursor: grab;
}
.wb-sticky__text,
.wb-sticky__input {
  width: 100%;
  height: 100%;
  line-height: 1.25;
  font-weight: 500;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
}
.wb-sticky__input {
  display: block;
  resize: none;
  border: none;
  outline: none;
  background: transparent;
  color: inherit;
  font-family: inherit;
}
.wb-sticky__colors {
  position: absolute;
  left: 50%;
  bottom: calc(100% + 10px);
  transform: translateX(-50%);
  display: flex;
  gap: 4px;
  padding: 5px 6px;
  border-radius: 999px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.12);
  cursor: default;
}
.dark .wb-sticky__colors {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
}
.wb-sticky__color {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
}
.wb-sticky__color.active,
.wb-sticky__color:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 1px;
}
.wb-sticky__handle {
  width: 10px !important;
  height: 10px !important;
  background: var(--color-accent) !important;
  border: 2px solid white !important;
  opacity: 0;
  transition: opacity 0.15s;
}
.wb-sticky:hover .wb-sticky__handle {
  opacity: 1;
}
</style>
