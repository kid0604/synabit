<script setup lang="ts">
import { ref, watch, nextTick, onMounted, computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';

const props = defineProps<{
  id: string;
  data: {
    label: string;
    color: string;
    level: number;
    editing?: boolean;
    direction?: 'left' | 'right';
    collapsed?: boolean;
    /** Worked out by the canvas, not stored: how many children, and how many items a fold hides. */
    _childCount?: number;
    _hiddenCount?: number;
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'add-child', payload: { parentId: string; direction: 'right' | 'left' }): void;
  (e: 'add-sibling', nodeId: string): void;
  (e: 'remove-node', nodeId: string): void;
  (e: 'toggle-collapse', nodeId: string): void;
}>();

const isEditing = ref(props.data.editing || false);
const editText = ref(props.data.label);
const inputRef = ref<HTMLInputElement | null>(null);

// Root node (level 0) shows + on both sides
// Non-root: show + only on its direction side
const isRoot = computed(() => (props.data.level || 0) === 0);
const nodeDirection = computed(() => props.data.direction || 'right');

// React to external editing state changes
watch(() => props.data.editing, (val) => {
  if (val && !isEditing.value) {
    isEditing.value = true;
    editText.value = props.data.label;
  }
});

// Auto-focus input whenever entering edit mode
watch(isEditing, (val) => {
  if (val) {
    nextTick(() => {
      inputRef.value?.focus();
    });
  }
});

// Handle mounting with editing already true
onMounted(() => {
  if (isEditing.value) {
    nextTick(() => {
      inputRef.value?.focus();
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
  if (e.key === 'Enter') {
    e.preventDefault();
    finishEdit();
    emit('add-sibling', props.id);
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
    <!-- Label or Input -->
    <input
      v-if="isEditing"
      ref="inputRef"
      v-model="editText"
      @blur="finishEdit"
      @keydown="handleKeydown"
      class="wb-mindmap-input"
      :style="{ color: 'inherit' }"
      :placeholder="$t('whiteboard.type_here2')"
    />
    <span v-else class="wb-mindmap-label" :style="{ fontSize: data.level === 0 ? '15px' : '13px' }">
      {{ data.label || $t('whiteboard.idea') }}
    </span>

    <!-- Left + button: root or left-direction nodes -->
    <button
      v-if="!(data as any).locked && (isRoot || nodeDirection === 'left')"
      class="wb-mindmap-add wb-mindmap-add--left"
      @click.stop="addChild('left')"
      :style="{ backgroundColor: data.color }"
      :title="$t('whiteboard.add_child_left')"
      :aria-label="$t('whiteboard.add_child_left')"
    >+</button>

    <!-- Right + button: root or right-direction nodes -->
    <button
      v-if="!(data as any).locked && (isRoot || nodeDirection === 'right')"
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

    <!-- Handles with IDs for directional edges -->
    <Handle id="right-source" type="source" :position="Position.Right" class="wb-mm-handle" />
    <Handle id="left-target"  type="target" :position="Position.Left"  class="wb-mm-handle" />
    <Handle id="left-source"  type="source" :position="Position.Left"  class="wb-mm-handle" />
    <Handle id="right-target" type="target" :position="Position.Right" class="wb-mm-handle" />
  </div>
</template>

<style scoped>
.wb-mindmap-node {
  position: relative;
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
  word-break: break-word;
  pointer-events: none;
}
.wb-mindmap-input {
  width: 100%;
  text-align: center;
  font-size: 13px;
  font-weight: 600;
  background: transparent;
  border: none;
  outline: none;
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
.wb-mindmap-fold:focus-visible,
.wb-mindmap-fold.is-folded {
  opacity: 1;
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
