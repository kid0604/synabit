<script setup lang="ts">
import { ref, watch, nextTick, onMounted, computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import { useOnlySelected } from '../composables/useOnlySelected';
import { MINDMAP_COLORS } from '../composables/useWhiteboardStore';
import { ICONS, tagsOf } from '../mindmap';

/** The colours a branch can be given, by the names the board's colours go by. */
const SWATCHES: { value: string; name: string }[] = [
  { value: MINDMAP_COLORS[0], name: 'purple' }, { value: MINDMAP_COLORS[1], name: 'blue' },
  { value: MINDMAP_COLORS[2], name: 'green' }, { value: MINDMAP_COLORS[3], name: 'amber' },
  { value: MINDMAP_COLORS[4], name: 'red' }, { value: MINDMAP_COLORS[5], name: 'pink' },
  { value: MINDMAP_COLORS[7], name: 'cyan' }, { value: MINDMAP_COLORS[8], name: 'lime' },
  { value: MINDMAP_COLORS[9], name: 'orange' },
];

const props = defineProps<{
  id: string;
  selected?: boolean;
  data: {
    label: string;
    color: string;
    level: number;
    editing?: boolean;
    direction?: 'left' | 'right';
    collapsed?: boolean;
    /** A checkbox, and whether it is ticked. */
    task?: boolean;
    done?: boolean;
    /** A mark before the words (`ICONS`). */
    icon?: string;
    /** Worked out by the canvas, not stored: how many children, and how many items a fold hides. */
    _childCount?: number;
    _hiddenCount?: number;
    /** Its map's layout, and how many of the tasks below it are done. */
    _layout?: 'map' | 'right' | 'down';
    _progress?: { done: number; total: number };
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'add-child', payload: { parentId: string; direction: 'right' | 'left' }): void;
  (e: 'add-sibling', nodeId: string): void;
  (e: 'remove-node', nodeId: string): void;
  (e: 'toggle-collapse', nodeId: string): void;
  (e: 'recolour', nodeId: string, colour: string): void;
  (e: 'toggle-done', nodeId: string): void;
  (e: 'set-icon', nodeId: string, icon: string | null): void;
}>();

const alone = useOnlySelected(() => !!props.selected);

const isEditing = ref(props.data.editing || false);
const editText = ref(props.data.label);
const inputRef = ref<HTMLTextAreaElement | null>(null);

// Root node (level 0) shows + on both sides
// Non-root: show + only on its direction side
const isRoot = computed(() => (props.data.level || 0) === 0);
const nodeDirection = computed(() => props.data.direction || 'right');
/** An org chart grows downwards: one + under the item. */
const down = computed(() => props.data._layout === 'down');
/** A logic chart grows to the right only, from its centre too. */
const oneSide = computed(() => props.data._layout === 'right');
/** The words, and the `#tags` in them drawn as chips. */
const shown = computed(() => tagsOf(props.data.label || ''));
/** The row of marks, opened from the colour row. */
const picking = ref(false);
watch(alone, (only) => { if (!only) picking.value = false; });

// React to external editing state changes
watch(() => props.data.editing, (val) => {
  if (val && !isEditing.value) {
    isEditing.value = true;
    editText.value = props.data.label;
  }
});

/** The box as tall as what is typed in it: a label can run to several lines. */
function fitHeight() {
  const el = inputRef.value;
  if (!el) return;
  el.style.height = 'auto';
  el.style.height = `${el.scrollHeight}px`;
}

// Auto-focus input whenever entering edit mode
watch(isEditing, (val) => {
  if (val) {
    nextTick(() => {
      inputRef.value?.focus();
      fitHeight();
    });
  }
});

// Handle mounting with editing already true
onMounted(() => {
  if (isEditing.value) {
    nextTick(() => {
      inputRef.value?.focus();
      fitHeight();
    });
  }
});

function startEdit() {
  if ((props.data as any).locked) return;
  isEditing.value = true;
  editText.value = props.data.label;
}

function finishEdit() {
  // Enter ends the edit and the blur as the input goes would end it again;
  // only the first one writes.
  if (!isEditing.value) return;
  isEditing.value = false;
  if (editText.value.trim() === '' && props.data.label === '') {
    emit('remove-node', props.id);
    return;
  }
  // Only what changed: the canvas copy of `data` also carries counts worked
  // out for drawing (`_childCount`, `_hiddenCount`) that are not the board's.
  emit('update:data', { label: editText.value, editing: undefined });
}

function handleKeydown(e: KeyboardEvent) {
  // Shift+Enter is a new line in the label, as the box does on its own.
  if (e.key === 'Enter' && e.shiftKey) return;
  if (e.key === 'Enter') {
    e.preventDefault();
    finishEdit();
    // On a central idea Enter only keeps the name: it has no siblings, and
    // its first branch is Tab.
    if (!isRoot.value) emit('add-sibling', props.id);
  } else if (e.key === 'Tab') {
    e.preventDefault();
    finishEdit();
    // Tab creates child in same direction as this node (or right for root)
    emit('add-child', { parentId: props.id, direction: nodeDirection.value });
  } else if (e.key === 'Escape') {
    cancelEdit();
  }
}

/**
 * Leave the edit without keeping what was typed.
 *
 * A node that was never named — the child Tab or Enter just made — goes
 * with the edit: it was only ever a place to type. Any other node keeps its
 * label, and is told it is no longer being edited; left set, `editing` was
 * saved with the board and the node opened in edit mode on the next visit.
 */
function cancelEdit() {
  if (!isEditing.value) return;
  isEditing.value = false;
  if (props.data.label === '') {
    emit('remove-node', props.id);
    return;
  }
  if (props.data.editing) emit('update:data', { editing: undefined });
}

function addChild(direction: 'right' | 'left') {
  emit('add-child', { parentId: props.id, direction });
}
</script>

<template>
  <div
    class="wb-mindmap-node"
    :style="{
      borderColor: data.color,
      backgroundColor: data.color + '12',
      minWidth: data.level === 0 ? '140px' : '100px',
    }"
    @dblclick.stop="startEdit"
  >
    <!-- Label or the box it is typed in: several lines, with Shift+Enter. -->
    <textarea
      v-if="isEditing"
      ref="inputRef"
      v-model="editText"
      rows="1"
      @blur="finishEdit"
      @keydown="handleKeydown"
      @input="fitHeight"
      class="wb-mindmap-input nodrag"
      :style="{ color: 'inherit', fontSize: data.level === 0 ? '15px' : '13px' }"
      :placeholder="$t('whiteboard.type_here2')"
    />
    <div v-else class="wb-mindmap-body">
      <div class="wb-mindmap-line">
        <!-- A task's box: ticked here, and in Tasks when it is one of the vault's. -->
        <button
          v-if="data.task"
          type="button"
          class="wb-mindmap-check nodrag"
          :class="{ 'is-done': data.done }"
          :style="{ borderColor: data.color, backgroundColor: data.done ? data.color : undefined }"
          role="checkbox"
          :aria-checked="!!data.done"
          :aria-label="$t(data.done ? 'whiteboard.mindmap.undo_done' : 'whiteboard.mindmap.mark_done')"
          :title="$t(data.done ? 'whiteboard.mindmap.undo_done' : 'whiteboard.mindmap.mark_done')"
          :disabled="(data as any).locked"
          @click.stop="emit('toggle-done', id)"
          @dblclick.stop
        >
          <svg v-if="data.done" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="#fff" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12" /></svg>
        </button>
        <span v-if="data.icon" class="wb-mindmap-icon" aria-hidden="true">{{ data.icon }}</span>
        <span
          class="wb-mindmap-label"
          :class="{ 'is-done': data.task && data.done }"
          :style="{ fontSize: data.level === 0 ? '15px' : '13px' }"
        >{{ shown.text || (shown.tags.length ? '' : $t('whiteboard.idea')) }}</span>
        <span
          v-if="data._progress"
          class="wb-mindmap-progress"
          :style="{ color: data.color }"
          :aria-label="$t('whiteboard.mindmap.progress', { done: data._progress.done, total: data._progress.total })"
        >{{ data._progress.done }}/{{ data._progress.total }}</span>
      </div>
      <div v-if="shown.tags.length" class="wb-mindmap-tags">
        <span v-for="tag in shown.tags" :key="tag" class="wb-mindmap-tag" :style="{ color: data.color, borderColor: data.color + '55' }">#{{ tag }}</span>
      </div>
    </div>

    <!-- An org chart's one +, under the item. -->
    <button
      v-if="!(data as any).locked && down"
      class="wb-mindmap-add wb-mindmap-add--down"
      @click.stop="addChild('right')"
      :style="{ backgroundColor: data.color }"
      :title="$t('whiteboard.add_child_below')"
      :aria-label="$t('whiteboard.add_child_below')"
    >+</button>

    <!-- Left + button: root or left-direction nodes -->
    <button
      v-if="!(data as any).locked && !down && !(isRoot && oneSide) && (isRoot || nodeDirection === 'left')"
      class="wb-mindmap-add wb-mindmap-add--left"
      @click.stop="addChild('left')"
      :style="{ backgroundColor: data.color }"
      :title="$t('whiteboard.add_child_left')"
      :aria-label="$t('whiteboard.add_child_left')"
    >+</button>

    <!-- Right + button: root or right-direction nodes -->
    <button
      v-if="!(data as any).locked && !down && (isRoot || nodeDirection === 'right')"
      class="wb-mindmap-add wb-mindmap-add--right"
      @click.stop="addChild('right')"
      :style="{ backgroundColor: data.color }"
      :title="$t('whiteboard.add_child_right')"
      :aria-label="$t('whiteboard.add_child_right')"
    >+</button>

    <!-- Fold or unfold the branch below. Shown when there is one; when folded,
         it says how much is hidden. -->
    <button
      v-if="data._childCount"
      class="wb-mindmap-fold nodrag"
      :class="[nodeDirection === 'left' && !isRoot ? 'wb-mindmap-fold--left' : 'wb-mindmap-fold--right', data.collapsed && 'is-folded']"
      :style="{ borderColor: data.color, color: data.collapsed ? '#fff' : data.color, backgroundColor: data.collapsed ? data.color : undefined }"
      :title="data.collapsed ? $t('whiteboard.mindmap.hidden_count', { count: data._hiddenCount }) : $t('whiteboard.mindmap.collapse')"
      :aria-label="data.collapsed ? $t('whiteboard.mindmap.hidden_count', { count: data._hiddenCount }) : $t('whiteboard.mindmap.collapse')"
      :aria-expanded="!data.collapsed"
      @click.stop="emit('toggle-collapse', id)"
    >{{ data.collapsed ? data._hiddenCount : '−' }}</button>

    <!-- The branch's colour, one click away while the item is selected: it
         goes to the item and to what below it had its colour. -->
    <div v-if="alone && !isEditing && !(data as any).locked" class="wb-mindmap-colors nodrag nopan" role="toolbar" :aria-label="$t('whiteboard.color')">
      <button
        v-for="c in SWATCHES"
        :key="c.value"
        class="wb-mindmap-color"
        :class="{ active: data.color?.toLowerCase() === c.value.toLowerCase() }"
        :style="{ background: c.value }"
        :title="$t(`whiteboard.colors.${c.name}`)"
        :aria-label="$t(`whiteboard.colors.${c.name}`)"
        :aria-pressed="data.color?.toLowerCase() === c.value.toLowerCase()"
        @click.stop="emit('recolour', id, c.value)"
      />
      <span class="wb-mindmap-sep" aria-hidden="true" />
      <button
        type="button"
        class="wb-mindmap-mark-toggle"
        :aria-expanded="picking"
        :title="$t('whiteboard.mindmap.marks')"
        :aria-label="$t('whiteboard.mindmap.marks')"
        @click.stop="picking = !picking"
      >{{ data.icon || '☺' }}</button>
    </div>
    <!-- The marks an idea can carry; the one it has, again, takes it off. -->
    <div v-if="alone && picking && !isEditing && !(data as any).locked" class="wb-mindmap-marks nodrag nopan" role="toolbar" :aria-label="$t('whiteboard.mindmap.marks')">
      <button
        v-for="icon in ICONS"
        :key="icon"
        type="button"
        class="wb-mindmap-mark"
        :class="{ active: data.icon === icon }"
        :aria-pressed="data.icon === icon"
        @click.stop="emit('set-icon', id, data.icon === icon ? null : icon); picking = false"
      >{{ icon }}</button>
    </div>

    <!-- Handles with IDs for directional edges -->
    <Handle id="right-source" type="source" :position="Position.Right" class="wb-mm-handle" />
    <Handle id="left-target"  type="target" :position="Position.Left"  class="wb-mm-handle" />
    <Handle id="left-source"  type="source" :position="Position.Left"  class="wb-mm-handle" />
    <Handle id="right-target" type="target" :position="Position.Right" class="wb-mm-handle" />
    <Handle id="bottom-source" type="source" :position="Position.Bottom" class="wb-mm-handle" />
    <Handle id="top-target" type="target" :position="Position.Top" class="wb-mm-handle" />
  </div>
</template>

<style scoped>
.wb-mindmap-node {
  position: relative;
  max-width: 320px; /* MAX_ITEM_WIDTH: longer words wrap */
  padding: 8px 16px;
  border-radius: 12px;
  border: 2px solid;
  cursor: grab;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: box-shadow 0.15s;
}
.wb-mindmap-node:hover {
  box-shadow: 0 2px 12px rgba(0,0,0,0.12);
}
.wb-mindmap-label {
  font-weight: 600;
  text-align: center;
  white-space: pre-wrap;
  word-break: break-word;
  pointer-events: none;
}
.wb-mindmap-body {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  min-width: 0;
}
.wb-mindmap-line {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.wb-mindmap-label.is-done {
  text-decoration: line-through;
  opacity: 0.6;
}
.wb-mindmap-check {
  flex: none;
  width: 16px;
  height: 16px;
  border-radius: 4px;
  border: 2px solid;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  background: var(--color-surface, #fff);
}
:global(.dark) .wb-mindmap-check:not(.is-done) {
  background: var(--color-surface-dark, #1e1e1e);
}
.wb-mindmap-icon {
  flex: none;
  font-size: 15px;
  line-height: 1;
}
.wb-mindmap-progress {
  flex: none;
  font-size: 12px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.wb-mindmap-tags {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 4px;
  pointer-events: none;
}
.wb-mindmap-tag {
  font-size: 12px;
  font-weight: 600;
  line-height: 16px;
  padding: 0 6px;
  border: 1px solid;
  border-radius: 999px;
}
.wb-mindmap-sep {
  width: 1px;
  align-self: stretch;
  margin: 0 2px;
  background: var(--color-border, #e6e6e6);
}
.wb-mindmap-mark-toggle {
  width: 22px;
  height: 18px;
  border-radius: 999px;
  font-size: 13px;
  line-height: 18px;
  cursor: pointer;
}
.wb-mindmap-marks {
  position: absolute;
  left: 50%;
  bottom: calc(100% + 44px);
  transform: translateX(-50%);
  display: grid;
  grid-template-columns: repeat(8, 28px);
  gap: 2px;
  padding: 6px;
  border-radius: 12px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.12);
  z-index: 12;
  cursor: default;
}
:global(.dark .wb-mindmap-marks) {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
}
.wb-mindmap-mark {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  font-size: 16px;
  cursor: pointer;
}
.wb-mindmap-mark:hover,
.wb-mindmap-mark.active {
  background: var(--color-accent-soft, rgba(124, 58, 237, 0.12));
}
.wb-mindmap-add--down {
  top: auto;
  bottom: -12px;
  left: 50%;
  transform: translateX(-50%);
}
.wb-mindmap-node:focus-within .wb-mindmap-add--down,
:global(.vue-flow__node.selected) .wb-mindmap-add--down,
.wb-mindmap-node:hover .wb-mindmap-add--down {
  transform: translateX(-50%);
}
.wb-mindmap-add--down:hover {
  transform: translateX(-50%) scale(1.15);
}
.wb-mindmap-input {
  width: 100%;
  min-width: 80px;
  text-align: center;
  font: inherit;
  font-weight: 600;
  line-height: 1.4;
  background: transparent;
  border: none;
  outline: none;
  resize: none;
  overflow: hidden;
  padding: 0;
}
.wb-mindmap-colors {
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
  z-index: 11;
}
:global(.dark .wb-mindmap-colors) {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
}
.wb-mindmap-color {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: none;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
  cursor: pointer;
}
.wb-mindmap-color.active,
.wb-mindmap-color:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 1px;
}
.wb-mindmap-add {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: none;
  color: white;
  font-size: 14px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s, transform 0.15s;
  z-index: 10;
}
.wb-mindmap-add--right {
  right: -12px;
}
.wb-mindmap-add--left {
  left: -12px;
}
.wb-mindmap-node:focus-within .wb-mindmap-add,
:global(.vue-flow__node.selected) .wb-mindmap-add,
.wb-mindmap-node:hover .wb-mindmap-add {
  opacity: 0.8;
  transform: translateY(-50%);
}
.wb-mindmap-add:hover {
  opacity: 1 !important;
  transform: translateY(-50%) scale(1.15);
}
.wb-mindmap-fold {
  position: absolute;
  top: calc(100% + 2px);
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 999px;
  border: 1.5px solid;
  background: var(--color-surface, #fff);
  font-size: 12px;
  font-weight: 700;
  line-height: 14px;
  opacity: 0;
  transition: opacity 0.15s;
}
.wb-mindmap-fold--right { right: 6px; }
.wb-mindmap-fold--left { left: 6px; }
.wb-mindmap-node:hover .wb-mindmap-fold,
:global(.vue-flow__node.selected) .wb-mindmap-fold,
.wb-mindmap-fold:focus-visible,
.wb-mindmap-fold.is-folded {
  opacity: 1;
}
/* A finger has no hover: on a touch screen the buttons show with the
   selection (above), and are big enough to hit. */
@media (pointer: coarse) {
  .wb-mindmap-add {
    width: 28px;
    height: 28px;
    font-size: 16px;
  }
  .wb-mindmap-add--right { right: -16px; }
  .wb-mindmap-add--left { left: -16px; }
  .wb-mindmap-fold {
    min-width: 26px;
    height: 26px;
    line-height: 22px;
  }
  .wb-mindmap-color {
    width: 26px;
    height: 26px;
  }
}
.wb-mm-handle {
  width: 6px !important;
  height: 6px !important;
  background: transparent !important;
  border: none !important;
  opacity: 0;
}
:global(.dark .wb-mindmap-fold) {
  background: var(--color-surface-dark, #1e1e1e);
}
</style>
