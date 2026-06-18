<script setup lang="ts">
import { BookOpen as BookOpenIcon } from 'lucide-vue-next';

defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: 'select-file'): void;
  (e: 'close'): void;
}>();
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('close')">
      <div class="bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl border border-[#e5e7eb] dark:border-[#333] w-[400px] p-6">
        <h3 class="text-base font-semibold text-[#111827] dark:text-[#f4f4f5] mb-1">Embed PDF</h3>
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-5">Select a PDF file to embed in this note</p>
        <button
          @click="emit('select-file')"
          class="w-full flex items-center justify-center gap-2 px-4 py-3 bg-[#f3f4f6] dark:bg-[#2a2a2a] hover:bg-[#e5e7eb] dark:hover:bg-[#333] border border-[#e5e7eb] dark:border-[#444] rounded-xl text-sm font-medium text-[#111827] dark:text-[#f4f4f5] transition-colors cursor-pointer"
        >
          <BookOpenIcon class="w-5 h-5 text-red-500" />
          Choose PDF File
        </button>
        <button
          @click="emit('close')"
          class="w-full mt-2 px-4 py-2 text-sm text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors cursor-pointer"
        >
          Cancel
        </button>
      </div>
    </div>
  </Teleport>
</template>
