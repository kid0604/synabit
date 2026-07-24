<script setup lang="ts">
import { ref, watch, nextTick, computed } from 'vue';
import { onClickOutside } from '@vueuse/core';
import { X, Trash2, MessageSquare } from 'lucide-vue-next';
import ConfirmModal from '../../../shared/components/ConfirmModal.vue';
import type { PdfAnnotation } from '../composables/usePdfAnnotations';

const props = defineProps<{
  show: boolean;
  annotation?: PdfAnnotation | null;
  selectedText?: string;
  position?: { top: number; left: number };
  mode: 'create' | 'edit';
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', payload: { color: PdfAnnotation['color']; note: string; text: string }): void;
  (e: 'update', payload: { color: PdfAnnotation['color']; note: string; text: string }): void;
  (e: 'delete'): void;
}>();

const colors: { value: PdfAnnotation['color']; label: string; bg: string; ring: string }[] = [
  { value: 'yellow', label: 'Yellow', bg: 'bg-yellow-300', ring: 'ring-yellow-400' },
  { value: 'green', label: 'Green', bg: 'bg-green-400', ring: 'ring-green-500' },
  { value: 'blue', label: 'Blue', bg: 'bg-blue-400', ring: 'ring-blue-500' },
  { value: 'pink', label: 'Pink', bg: 'bg-pink-400', ring: 'ring-pink-500' },
];

const selectedColor = ref<PdfAnnotation['color']>('yellow');
const noteText = ref('');
const showNote = ref(false);
const noteInputRef = ref<HTMLTextAreaElement | null>(null);
const localSelectedText = ref('');
const popupRef = ref<HTMLElement | null>(null);
const showConfirmDelete = ref(false);

onClickOutside(popupRef, () => {
  if (props.show && !showConfirmDelete.value) {
    emit('close');
  }
});

const popupStyle = computed(() => {
  const top = props.position?.top || 200;
  const left = props.position?.left || 300;
  
  // Constrain to viewport to prevent falling out of bounds
  const maxTop = window.innerHeight - 350; // approximate max height of popup
  const maxLeft = window.innerWidth - 380; // approximate max width of popup
  
  return {
    top: `${Math.max(10, Math.min(top, maxTop))}px`,
    left: `${Math.max(10, Math.min(left, maxLeft))}px`,
  };
});

watch(
  () => [props.show, props.selectedText, props.annotation],
  () => {
    if (props.show) {
      localSelectedText.value = props.selectedText || props.annotation?.text || '';
      if (props.mode === 'edit' && props.annotation) {
        selectedColor.value = props.annotation.color;
        noteText.value = props.annotation.content;
        showNote.value = !!props.annotation.content;
      } else {
        selectedColor.value = 'yellow';
        noteText.value = '';
        showNote.value = false;
      }
      showConfirmDelete.value = false;
    }
  },
  { immediate: true }
);

const toggleNote = () => {
  showNote.value = !showNote.value;
  if (showNote.value) {
    nextTick(() => noteInputRef.value?.focus());
  }
};

const handleSave = () => {
  if (props.mode === 'create') {
    emit('save', { color: selectedColor.value, note: noteText.value, text: localSelectedText.value });
  } else {
    emit('update', { color: selectedColor.value, note: noteText.value, text: localSelectedText.value });
  }
};

const handleColorClick = (color: PdfAnnotation['color']) => {
  selectedColor.value = color;
  // In edit mode, auto-save color change
  if (props.mode === 'edit') {
    emit('update', { color, note: noteText.value, text: localSelectedText.value });
  }
};

const requestDelete = () => {
  showConfirmDelete.value = true;
};

const executeDelete = () => {
  emit('delete');
  showConfirmDelete.value = false;
};
</script>

<template>
  <Teleport to="body">
    <Transition name="popup">
      <div
        v-if="show"
        ref="popupRef"
        class="fixed z-[9999] pdf-annotation-popup"
        :style="popupStyle"
        @mousedown.stop
        @mouseup.stop
        @click.stop
      >
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl shadow-2xl p-3 min-w-[260px] max-w-[360px]">
          <!-- Header -->
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-semibold text-text-secondary dark:text-text-secondary-dark uppercase tracking-wide">
              {{ mode === 'create' ? 'Highlight' : 'Edit Highlight' }}
            </span>
            <button @click="emit('close')" class="p-1 rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-muted dark:text-muted-dark cursor-pointer" aria-label="More Options">
              <X class="w-3.5 h-3.5" />
            </button>
          </div>

          <!-- Selected text preview (editable) -->
          <div v-if="localSelectedText" class="mb-3">
            <textarea
              v-model="localSelectedText"
              class="w-full text-xs text-text dark:text-text-dark bg-surface-hover/50 dark:bg-surface-hover-dark/50 border border-transparent focus:border-accent focus:ring-1 focus:ring-accent dark:focus:ring-accent-dark rounded-lg p-2 max-h-24 overflow-y-auto leading-relaxed resize-none focus:outline-none"
              rows="3"
            />
          </div>

          <!-- Color picker -->
          <div class="flex items-center gap-2 mb-3">
            <button
              v-for="c in colors"
              :key="c.value"
              @click="handleColorClick(c.value)"
              :class="[
                'w-7 h-7 rounded-full transition-all cursor-pointer',
                c.bg,
                selectedColor === c.value ? `ring-2 ${c.ring} ring-offset-2 ring-offset-surface dark:ring-offset-surface-dark scale-110` : 'hover:scale-105'
              ]"
              :title="c.label"
            />
            <div class="flex-1" />
            <button
              @click="toggleNote"
              :class="[
                'flex items-center gap-1 px-2 py-1 rounded-md text-xs transition-colors cursor-pointer',
                showNote ? 'bg-accent/10 text-accent dark:bg-accent-dark/10 dark:text-accent-dark' : 'text-muted dark:text-muted-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark'
              ]"
            >
              <MessageSquare class="w-3.5 h-3.5" />
              Note
            </button>
          </div>

          <!-- Note input -->
          <Transition name="slide">
            <div v-if="showNote" class="mb-3">
              <textarea
                ref="noteInputRef"
                v-model="noteText"
                placeholder="Add a note…"
                class="w-full text-xs bg-surface-hover/50 dark:bg-surface-hover-dark/50 border border-border dark:border-border-dark rounded-lg p-2 resize-none focus:outline-none focus:ring-1 focus:ring-accent dark:focus:ring-accent-dark text-text dark:text-text-dark placeholder:text-muted dark:placeholder:text-muted-dark"
                rows="3"
                @keydown.meta.enter="handleSave"
                @keydown.ctrl.enter="handleSave"
              />
            </div>
          </Transition>

          <!-- Actions -->
          <div class="flex items-center gap-2">
            <button
              v-if="mode === 'edit'"
              @click="requestDelete"
              class="flex items-center gap-1 px-2 py-1.5 rounded-md text-xs text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors cursor-pointer"
            >
              <Trash2 class="w-3.5 h-3.5" />
              Delete
            </button>
            <div class="flex-1" />
            <button
              @click="emit('close')"
              class="px-3 py-1.5 rounded-md text-xs text-muted dark:text-muted-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              @click="handleSave"
              class="px-4 py-1.5 rounded-md text-xs font-medium bg-primary dark:bg-white text-white dark:text-black hover:opacity-90 transition-opacity cursor-pointer"
            >
              {{ mode === 'create' ? 'Save' : 'Update' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
    
    <ConfirmModal
      :show="showConfirmDelete"
      title="Delete Highlight"
      message="Are you sure you want to delete this highlight? This action cannot be undone."
      confirm-text="Delete"
      :is-destructive="true"
      @confirm="executeDelete"
      @cancel="showConfirmDelete = false"
    />
  </Teleport>
</template>

<style scoped>
.popup-enter-active,
.popup-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.popup-enter-from,
.popup-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}

.slide-enter-active,
.slide-leave-active {
  transition: all 0.2s ease;
}
.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  max-height: 0;
  margin-bottom: 0;
}
.slide-enter-to,
.slide-leave-from {
  max-height: 120px;
}
</style>
