<script setup lang="ts">
import { ref } from 'vue';
import { X, FileText, Download, LayoutDashboard } from 'lucide-vue-next';

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'export', payload: ExportOptions): void;
}>();

export interface ExportOptions {
  format: 'md' | 'pdf' | 'html';
  includeTitle: boolean;
  includeTags: boolean;
  pdfOrientation: 'portrait' | 'landscape';
  pdfFormat: 'a4' | 'a3' | 'letter' | 'legal';
}

const format = ref<'md' | 'pdf' | 'html'>('pdf');
const includeTitle = ref(true);
const includeTags = ref(true);
const pdfOrientation = ref<'portrait' | 'landscape'>('portrait');
const pdfFormat = ref<'a4' | 'a3' | 'letter' | 'legal'>('a4');

const formats = [
  { id: 'pdf', label: 'PDF' },
  { id: 'md', label: 'Markdown' },
  { id: 'html', label: 'HTML' }
];

const paperSizes = [
  { id: 'a4', label: 'A4' },
  { id: 'a3', label: 'A3' },
  { id: 'letter', label: 'Letter' },
  { id: 'legal', label: 'Legal' }
];

const handleExport = () => {
  emit('export', {
    format: format.value,
    includeTitle: includeTitle.value,
    includeTags: includeTags.value,
    pdfOrientation: pdfOrientation.value,
    pdfFormat: pdfFormat.value
  });
};
</script>

<template>
  <div class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl w-[400px] border border-[#e6e6e6] dark:border-[#3a3a3a] overflow-hidden flex flex-col">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-4 border-b border-[#e6e6e6] dark:border-[#3a3a3a]">
        <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
          <Download class="w-4 h-4 text-gray-500" /> Export Note
        </h3>
        <button @click="emit('close')" class="p-1 rounded-md hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 transition-colors">
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Content -->
      <div class="p-5 space-y-5">
        <!-- Format Selection -->
        <div class="space-y-2">
          <label class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Format</label>
          <div class="flex bg-gray-100 dark:bg-[#1f1f1f] p-1 rounded-lg">
            <button
              v-for="fmt in formats"
              :key="fmt.id"
              @click="format = fmt.id as any"
              class="flex-1 py-1.5 text-sm rounded-md transition-colors font-medium"
              :class="format === fmt.id ? 'bg-white dark:bg-[#2c2c2c] text-[#1c1c1e] dark:text-[#f4f4f5] shadow-sm' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'"
            >
              {{ fmt.label }}
            </button>
          </div>
        </div>

        <!-- Metadata Options -->
        <div class="space-y-2">
          <label class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Include Metadata</label>
          <div class="space-y-2">
            <label class="flex items-center gap-3 cursor-pointer group">
              <div class="relative flex items-center justify-center">
                <input type="checkbox" v-model="includeTitle" class="peer sr-only" />
                <div class="w-4 h-4 border border-gray-300 dark:border-gray-500 rounded bg-white dark:bg-[#1e1e1e] peer-checked:bg-black dark:peer-checked:bg-white peer-checked:border-black dark:peer-checked:border-white transition-colors"></div>
                <div class="absolute inset-0 flex items-center justify-center text-white dark:text-black opacity-0 peer-checked:opacity-100 transition-opacity">
                  <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"></path></svg>
                </div>
              </div>
              <span class="text-sm text-[#1c1c1e] dark:text-[#f4f4f5] group-hover:text-black dark:group-hover:text-white transition-colors">Note Title</span>
            </label>
            <label class="flex items-center gap-3 cursor-pointer group">
              <div class="relative flex items-center justify-center">
                <input type="checkbox" v-model="includeTags" class="peer sr-only" />
                <div class="w-4 h-4 border border-gray-300 dark:border-gray-500 rounded bg-white dark:bg-[#1e1e1e] peer-checked:bg-black dark:peer-checked:bg-white peer-checked:border-black dark:peer-checked:border-white transition-colors"></div>
                <div class="absolute inset-0 flex items-center justify-center text-white dark:text-black opacity-0 peer-checked:opacity-100 transition-opacity">
                  <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"></path></svg>
                </div>
              </div>
              <span class="text-sm text-[#1c1c1e] dark:text-[#f4f4f5] group-hover:text-black dark:group-hover:text-white transition-colors">Tags</span>
            </label>
          </div>
        </div>

        <!-- PDF Options -->
        <div v-if="format === 'pdf'" class="space-y-4 pt-1 border-t border-[#e6e6e6] dark:border-[#3a3a3a]">
          <div class="space-y-2">
            <label class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Orientation</label>
            <div class="flex gap-2">
              <button
                @click="pdfOrientation = 'portrait'"
                class="flex-1 py-2 px-3 flex items-center justify-center gap-2 rounded-lg border text-sm transition-colors"
                :class="pdfOrientation === 'portrait' ? 'border-black dark:border-white bg-black/5 dark:bg-white/10 text-black dark:text-white font-medium' : 'border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-gray-500 hover:bg-gray-50 dark:hover:bg-[#252525]'"
              >
                <FileText class="w-4 h-4" />
                Portrait
              </button>
              <button
                @click="pdfOrientation = 'landscape'"
                class="flex-1 py-2 px-3 flex items-center justify-center gap-2 rounded-lg border text-sm transition-colors"
                :class="pdfOrientation === 'landscape' ? 'border-black dark:border-white bg-black/5 dark:bg-white/10 text-black dark:text-white font-medium' : 'border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-gray-500 hover:bg-gray-50 dark:hover:bg-[#252525]'"
              >
                <LayoutDashboard class="w-4 h-4" />
                Landscape
              </button>
            </div>
          </div>
          
          <div class="space-y-2">
            <label class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Paper Size</label>
            <select v-model="pdfFormat" class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20 transition-all appearance-none cursor-pointer">
              <option v-for="size in paperSizes" :key="size.id" :value="size.id">{{ size.label }}</option>
            </select>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="p-5 border-t border-[#e6e6e6] dark:border-[#3a3a3a] bg-gray-50/50 dark:bg-[#242424]/50 flex justify-end gap-2">
        <button @click="emit('close')" class="px-4 py-2 text-sm rounded-lg text-gray-600 dark:text-gray-300 font-medium hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">
          Cancel
        </button>
        <button @click="handleExport" class="px-4 py-2 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity flex items-center gap-2">
          <Download class="w-4 h-4" />
          Export
        </button>
      </div>
    </div>
  </div>
</template>
