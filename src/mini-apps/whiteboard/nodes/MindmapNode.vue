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
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'add-child', payload: { parentId: string; direction: 'right' | 'left' }): void;
  (e: 'add-sibling', nodeId: string): void;
  (e: 'remove-node', nodeId: string): void;
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
  isEditing.value = true;
  editText.value = props.data.label;
}

function finishEdit() {
  isEditing.value = false;
  if (editText.value.trim() === '' && props.data.label === '') {
    emit('remove-node', props.id);
    return;
  }
  emit('update:data', { ...props.data, label: editText.value, editing: false });
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
    isEditing.value = false;
  }
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
      {{ data.label || 'Idea' }}
    </span>

    <!-- Left + button: root or left-direction nodes -->
    <button
      v-if="isRoot || nodeDirection === 'left'"
      class="wb-mindmap-add wb-mindmap-add--left"
      @click.stop="addChild('left')"
      :style="{ backgroundColor: data.color }"
      :title="$t('whiteboard.add_child_left')"
    >+</button>

    <!-- Right + button: root or right-direction nodes -->
    <button
      v-if="isRoot || nodeDirection === 'right'"
      class="wb-mindmap-add wb-mindmap-add--right"
      @click.stop="addChild('right')"
      :style="{ backgroundColor: data.color }"
      :title="$t('whiteboard.add_child_right')"
    >+</button>

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
.wb-mindmap-node:hover .wb-mindmap-add {
  opacity: 0.8;
  transform: translateY(-50%);
}
.wb-mindmap-add:hover {
  opacity: 1 !important;
  transform: translateY(-50%) scale(1.15);
}
.wb-mm-handle {
  width: 6px !important;
  height: 6px !important;
  background: transparent !important;
  border: none !important;
  opacity: 0;
}
</style>
