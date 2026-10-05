<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue';
import { MessageSquare, Check, RotateCcw } from 'lucide-vue-next';

/**
 * A comment: a note about the board, pinned beside what it is about.
 *
 * Its words are in `label`, like every other item's, so search, Syn and the
 * previews read it without knowing about comments. `on` is the item it is
 * about: it moves when that item is dragged. A resolved comment stays, faded,
 * until it is deleted — what was decided is worth seeing.
 *
 * Comments are for whoever works on the board: they are left out of exports,
 * shared pages and presentations.
 */
const props = defineProps<{
  id: string;
  selected?: boolean;
  data: { label?: string; resolved?: boolean; editing?: boolean; locked?: boolean; at?: number };
}>();
const emit = defineEmits<{ (e: 'update:data', data: any): void }>();

const isEditing = ref(!!props.data.editing);
const text = ref(props.data.label ?? '');
const input = ref<HTMLTextAreaElement | null>(null);

function focus() {
  nextTick(() => input.value?.focus());
}
onMounted(() => { if (isEditing.value) focus(); });

function startEdit() {
  if (props.data.locked) return;
  text.value = props.data.label ?? '';
  isEditing.value = true;
  focus();
}
function finishEdit() {
  if (!isEditing.value) return;
  isEditing.value = false;
  emit('update:data', { label: text.value.trim(), editing: undefined });
}
function cancelEdit() {
  if (!isEditing.value) return;
  isEditing.value = false;
  if (props.data.editing) emit('update:data', { editing: undefined });
}
function toggleResolved() {
  emit('update:data', { resolved: props.data.resolved ? undefined : true });
}

defineExpose({ startEdit });
</script>

<template>
  <div class="wb-comment" :class="{ 'wb-comment--resolved': data.resolved, 'wb-comment--selected': selected }" @dblclick.stop="startEdit">
    <MessageSquare class="wb-comment__pin" :size="14" aria-hidden="true" />
    <textarea
      v-if="isEditing"
      ref="input"
      v-model="text"
      class="wb-comment__input nodrag nopan"
      rows="3"
      :placeholder="$t('whiteboard.comment.placeholder')"
      :aria-label="$t('whiteboard.comment.label')"
      @blur="finishEdit"
      @keydown.escape.stop="cancelEdit"
      @keydown.enter.exact.meta.prevent="finishEdit"
      @keydown.enter.exact.ctrl.prevent="finishEdit"
    />
    <p v-else class="wb-comment__text">{{ data.label || $t('whiteboard.comment.empty') }}</p>
    <button
      v-if="selected && !isEditing"
      class="wb-comment__resolve nodrag"
      :title="$t(data.resolved ? 'whiteboard.comment.reopen' : 'whiteboard.comment.resolve')"
      :aria-label="$t(data.resolved ? 'whiteboard.comment.reopen' : 'whiteboard.comment.resolve')"
      @click.stop="toggleResolved"
    >
      <RotateCcw v-if="data.resolved" :size="13" />
      <Check v-else :size="13" />
    </button>
  </div>
</template>

<style scoped>
.wb-comment {
  position: relative;
  display: flex;
  gap: 6px;
  align-items: flex-start;
  width: 220px;
  padding: 8px 10px;
  border-radius: 12px 12px 12px 2px;
  border: 1.5px solid color-mix(in oklab, var(--color-accent) 55%, transparent);
  background: var(--color-surface, #fff);
  color: var(--color-text, #18181b);
  box-shadow: 0 2px 8px rgb(0 0 0 / 0.08);
  font-size: 13px;
  line-height: 1.4;
}
:global(.dark .wb-comment) {
  background: var(--color-surface-dark, #1e1e1e);
  color: var(--color-text-dark, #fafafa);
}
.wb-comment--selected {
  border-color: var(--wb-selection, var(--color-accent));
}
.wb-comment--resolved {
  opacity: 0.55;
}
.wb-comment__pin {
  flex: none;
  margin-top: 2px;
  color: var(--color-accent);
}
:global(.dark .wb-comment__pin) {
  color: var(--color-accent-dark);
}
.wb-comment__text {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  display: -webkit-box;
  -webkit-line-clamp: 6;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.wb-comment__input {
  flex: 1;
  min-width: 0;
  resize: none;
  border: none;
  outline: none;
  background: transparent;
  color: inherit;
  font: inherit;
}
.wb-comment__resolve {
  position: absolute;
  top: -12px;
  right: -12px;
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: 999px;
  color: #fff;
  background: var(--color-accent);
}
</style>
