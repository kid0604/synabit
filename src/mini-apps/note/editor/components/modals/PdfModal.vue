<script setup lang="ts">
import AppDialog from '../../../../../shared/components/AppDialog.vue';
import { BookOpen as BookOpenIcon } from 'lucide-vue-next';

defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: 'select-file'): void;
  (e: 'close'): void;
}>();
</script>

<template>
  <AppDialog :show="show" labelledby="note-pdf-title" size="sm" panel-class="p-6" @close="emit('close')">
    <h3 id="note-pdf-title" class="text-base font-semibold text-[#111827] dark:text-text-dark mb-1">{{ $t('note.editor.pdf.title') }}</h3>
    <p class="text-sm text-gray-500 dark:text-gray-400 mb-5">{{ $t('note.editor.pdf.hint') }}</p>
    <button
      @click="emit('select-file')"
      class="w-full flex items-center justify-center gap-2 px-4 py-3 bg-[#f3f4f6] dark:bg-surface-hover-dark hover:bg-[#e5e7eb] dark:hover:bg-[#333] border border-[#e5e7eb] dark:border-[#444] rounded-xl text-sm font-medium text-[#111827] dark:text-text-dark transition-colors cursor-pointer"
    >
      <BookOpenIcon class="w-5 h-5 text-red-500" />
      {{ $t('note.editor.pdf.choose') }}
    </button>
    <button
      @click="emit('close')"
      class="w-full mt-2 px-4 py-2 text-sm text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors cursor-pointer"
    >
      {{ $t('note.cancel') }}
    </button>
  </AppDialog>
</template>
