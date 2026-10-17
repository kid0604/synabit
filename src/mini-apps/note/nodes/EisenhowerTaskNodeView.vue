<template>
  <node-view-wrapper
    as="span"
    class="eis-task"
    :class="{ 'is-done': done, 'is-missing': task === null, 'is-selected': selected }"
  >
    <button
      type="button"
      class="eis-task-check"
      data-eis-interactive
      role="checkbox"
      :aria-checked="done"
      :disabled="!canToggle"
      :title="task === null ? $t('note.eisenhower.task_missing') : ''"
      @click="toggle"
    >
      <Check v-if="done" class="w-3 h-3" />
    </button>
    <button
      type="button"
      class="eis-task-title"
      data-eis-interactive
      :title="task === null ? $t('note.eisenhower.task_missing') : $t('note.eisenhower.open_task')"
      @click="open"
    >{{ title }}</button>
    <span v-if="dueLabel" class="eis-task-due">{{ dueLabel }}</span>
    <span v-if="elsewhere" class="eis-task-elsewhere">{{ elsewhere }}</span>
  </node-view-wrapper>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { NodeViewWrapper, nodeViewProps } from '@tiptap/vue-3';
import { Check } from 'lucide-vue-next';
import { useEventBus } from '../../../composables/useEventBus';
import { formatDueDate } from '../../task/dueLabel';
import { logger } from '../../../utils/logger';
import { quadrantAt, type EisenhowerOptions, type MatrixTask } from '../extensions/eisenhowerShared';

const props = defineProps(nodeViewProps);
const { t, locale } = useI18n();
const bus = useEventBus();

const options = computed(() => props.extension.options as EisenhowerOptions);
const id = computed<string>(() => props.node.attrs.id);

/** `undefined` until read, or when there is no vault to read it from; `null` when the task is gone. */
const task = ref<MatrixTask | null | undefined>(undefined);
const busy = ref(false);

async function load() {
  const loadTask = options.value.loadTask;
  if (!loadTask || !id.value) return;
  try {
    task.value = await loadTask(id.value);
  } catch (e) {
    // Shown as written in the note rather than as missing: not finding out
    // is not the same as finding it gone.
    logger.warn('[Eisenhower] Could not read a task', e);
  }
}

onMounted(load);
watch(id, load);

// The task as it is now, after an edit anywhere: Tasks, Syn, another device.
bus.on('node:updated', (p) => { if (p.id === id.value) void load(); });
bus.on('node:deleted', (p) => { if (p.id === id.value) void load(); });
bus.on('task:status-changed', (p) => { if (p.id === id.value) void load(); });
bus.on('vault:sync-completed', () => { void load(); });

// The live title; the one written in the note is for reading the file elsewhere.
const title = computed(() => task.value?.title || props.node.attrs.title || id.value);
const done = computed(() => task.value?.status === 'done');
const canToggle = computed(() => !!task.value && !!options.value.toggleTask && !busy.value);

const dueLabel = computed(() => (task.value?.due_date && !done.value ? formatDueDate(task.value.due_date, locale.value) : ''));

/** The quadrant of the note this chip sits in. */
const here = computed(() => {
  // Read off the node so the computed follows it when the chip is moved.
  void props.node;
  const pos = typeof props.getPos === 'function' ? props.getPos() : undefined;
  return typeof pos === 'number' ? quadrantAt(props.editor.state.doc, pos) : null;
});

/**
 * Where Tasks has it, when that is somewhere else. Said, not fixed: the
 * note's matrix is for thinking and the Tasks one for doing, and which of
 * the two is right is not this chip's to decide.
 */
const elsewhere = computed(() => {
  if (!task.value || done.value || !here.value || task.value.quadrant === here.value) return '';
  return t('note.eisenhower.elsewhere', { quadrant: t(`task.matrix_${task.value.quadrant}`) });
});

async function toggle() {
  const toggleTask = options.value.toggleTask;
  if (!toggleTask || !canToggle.value) return;
  busy.value = true;
  try {
    await toggleTask(id.value);
    await load();
  } catch (e) {
    logger.error('[Eisenhower] Could not tick a task', e);
  } finally {
    busy.value = false;
  }
}

function open() {
  if (task.value === null) return;
  options.value.openTask?.(id.value);
}
</script>
