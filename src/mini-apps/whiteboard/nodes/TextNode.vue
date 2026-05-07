<script setup lang="ts">
import { ref } from 'vue';
import { NodeResizer } from '@vue-flow/node-resizer';

const props = defineProps<{
  id: string;
  selected?: boolean;
  data: {
    label: string;
    fontSize?: number;
    fontWeight?: string;
    fontStyle?: string;
    textAlign?: string;
    color?: string;
    backgroundColor?: string;
    opacity?: number;
    width?: number;
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
}>();

const isEditing = ref(false);
const editText = ref('');

function startEdit() {
  isEditing.value = true;
  editText.value = props.data.label;
}

function finishEdit() {
  isEditing.value = false;
  emit('update:data', { ...props.data, label: editText.value });
}

function onResizeEnd(event: any) {
  emit('update:data', { ...props.data, width: Math.round(event.params.width) });
}
</script>

<template>
  <div
    class="wb-text-node"
    :class="{ 'wb-text-node--editing': isEditing }"
    :style="{
      fontSize: (data.fontSize || 16) + 'px',
      fontWeight: data.fontWeight || 'normal',
      fontStyle: data.fontStyle || 'normal',
      textAlign: data.textAlign || 'left',
      color: data.color || 'inherit',
      backgroundColor: data.backgroundColor || 'transparent',
      opacity: data.opacity || 1,
      width: (data.width || 200) + 'px'
    } as any"
    @dblclick.stop="startEdit"
  >
    <NodeResizer
      :is-visible="!!selected && !isEditing"
      :min-width="80"
      :min-height="24"
      color="var(--color-accent, #7c3aed)"
      @resize-end="onResizeEnd"
    />

    <textarea
      v-if="isEditing"
      v-model="editText"
      @blur="finishEdit"
      @keydown.escape="isEditing = false"
      class="wb-text-input"
      :style="{
        fontSize: (data.fontSize || 16) + 'px',
        fontWeight: data.fontWeight || 'normal',
        fontStyle: data.fontStyle || 'normal',
        textAlign: data.textAlign || 'left',
        color: 'inherit'
      } as any"
      autofocus
      rows="3"
    />
    <div
      v-else
      class="wb-text-content"
    >
      {{ data.label || 'Type here...' }}
    </div>
  </div>
</template>

<style scoped>
.wb-text-node {
  width: 240px;
  min-width: 80px;
  height: 100%;
  padding: 8px 12px;
  border-radius: 8px;
  cursor: grab;
  border: 2px solid transparent;
  transition: border-color 0.15s, background-color 0.15s, opacity 0.15s, box-shadow 0.15s;
}
.wb-text-node--editing {
  border-color: var(--color-accent, #7c3aed);
  box-shadow: 0 0 0 3px rgba(124, 58, 237, 0.15);
}
.dark .wb-text-node--editing {
  border-color: #a78bfa;
  box-shadow: 0 0 0 3px rgba(167, 139, 250, 0.2);
}
.wb-text-content {
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: break-word;
}
.wb-text-input {
  width: 100%;
  height: 100%;
  min-height: 60px;
  background: transparent;
  border: none;
  padding: 0;
  outline: none;
  color: inherit;
  resize: none;
  font-family: inherit;
}
</style>
