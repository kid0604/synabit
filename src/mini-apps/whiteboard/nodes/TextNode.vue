<script setup lang="ts">
import { ref } from 'vue';

const props = defineProps<{
  id: string;
  data: {
    label: string;
    fontSize?: number;
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
</script>

<template>
  <div
    class="wb-text-node"
    @dblclick.stop="startEdit"
  >
    <textarea
      v-if="isEditing"
      v-model="editText"
      @blur="finishEdit"
      @keydown.escape="isEditing = false"
      class="wb-text-input"
      :style="{ fontSize: (data.fontSize || 14) + 'px' }"
      autofocus
      rows="3"
    />
    <div
      v-else
      class="wb-text-content text-text dark:text-text-dark"
      :style="{ fontSize: (data.fontSize || 14) + 'px' }"
    >
      {{ data.label || 'Type here...' }}
    </div>
  </div>
</template>

<style scoped>
.wb-text-node {
  min-width: 120px;
  max-width: 400px;
  padding: 8px 12px;
  cursor: grab;
}
.wb-text-content {
  white-space: pre-wrap;
  word-break: break-word;
  opacity: 0.85;
}
.wb-text-input {
  width: 100%;
  min-width: 200px;
  background: transparent;
  border: 1px dashed var(--color-border, #e6e6e6);
  border-radius: 6px;
  padding: 4px;
  outline: none;
  color: inherit;
  resize: both;
  font-family: inherit;
}
</style>
