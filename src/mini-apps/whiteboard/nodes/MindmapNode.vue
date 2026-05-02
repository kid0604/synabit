<script setup lang="ts">
import { ref } from 'vue';
import { Handle, Position } from '@vue-flow/core';

const props = defineProps<{
  id: string;
  data: {
    label: string;
    color: string;
    level: number;
    editing?: boolean;
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'add-child', payload: { parentId: string; direction: 'right' | 'bottom' }): void;
  (e: 'add-sibling', nodeId: string): void;
  (e: 'remove-node', nodeId: string): void;
}>();

const isEditing = ref(props.data.editing || false);
const editText = ref(props.data.label);

function startEdit() {
  isEditing.value = true;
  editText.value = props.data.label;
}

function finishEdit() {
  isEditing.value = false;
  if (editText.value.trim() === '' && props.data.label === '') {
    // Empty new node — remove it
    emit('remove-node', props.id);
    return;
  }
  emit('update:data', { ...props.data, label: editText.value, editing: false });
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault();
    finishEdit();
    // Enter = create sibling
    emit('add-sibling', props.id);
  } else if (e.key === 'Tab') {
    e.preventDefault();
    finishEdit();
    // Tab = create child
    emit('add-child', { parentId: props.id, direction: 'right' });
  } else if (e.key === 'Escape') {
    isEditing.value = false;
  }
}

function addChild(direction: 'right' | 'bottom') {
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
      v-model="editText"
      @blur="finishEdit"
      @keydown="handleKeydown"
      class="wb-mindmap-input"
      :style="{ color: 'inherit' }"
      autofocus
      placeholder="Type here..."
    />
    <span v-else class="wb-mindmap-label" :style="{ fontSize: data.level === 0 ? '15px' : '13px' }">
      {{ data.label || 'Idea' }}
    </span>

    <!-- Add child buttons -->
    <button
      class="wb-mindmap-add wb-mindmap-add--right"
      @click.stop="addChild('right')"
      :style="{ backgroundColor: data.color }"
      title="Add child (Tab)"
    >+</button>
    <button
      class="wb-mindmap-add wb-mindmap-add--bottom"
      @click.stop="addChild('bottom')"
      :style="{ backgroundColor: data.color }"
      title="Add sibling"
    >+</button>

    <!-- Handles -->
    <Handle type="source" :position="Position.Right" class="wb-mm-handle" />
    <Handle type="target" :position="Position.Left" class="wb-mm-handle" />
    <Handle type="source" :position="Position.Bottom" class="wb-mm-handle" />
    <Handle type="target" :position="Position.Top" class="wb-mm-handle" />
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
.wb-mindmap-node:hover .wb-mindmap-add {
  opacity: 0.8;
}
.wb-mindmap-add:hover {
  opacity: 1 !important;
  transform: scale(1.15);
}
.wb-mindmap-add--right {
  right: -12px;
  top: 50%;
  transform: translateY(-50%);
}
.wb-mindmap-add--bottom {
  bottom: -12px;
  left: 50%;
  transform: translateX(-50%);
}
.wb-mindmap-node:hover .wb-mindmap-add--right {
  transform: translateY(-50%);
}
.wb-mindmap-node:hover .wb-mindmap-add--bottom {
  transform: translateX(-50%);
}
.wb-mm-handle {
  width: 6px !important;
  height: 6px !important;
  background: transparent !important;
  border: none !important;
  opacity: 0;
}
</style>
