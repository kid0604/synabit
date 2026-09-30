<script setup lang="ts">
import { ref } from 'vue';
import { X, FileText, Download, LayoutDashboard } from 'lucide-vue-next';
import AppDialog from '../../shared/components/AppDialog.vue';

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
  <AppDialog :show="true" labelledby="note-export-title" size="sm" panel-class="!overflow-hidden flex flex-col" @close="emit('close')">
    <!-- Header -->
    <div class="flex items-center justify-between px-5 py-4 border-b border-border dark:border-border-subtle-dark">
      <h3 id="note-export-title" class="text-base font-semibold text-text dark:text-text-dark flex items-center gap-2">
        <Download class="w-4 h-4 text-gray-500 dark:text-gray-400" /> {{ $t('note.export_note') }}
      </h3>
      <button @click="emit('close')" class="p-1 rounded-md hover:bg-gray-100 dark:hover:bg-[#333] text-gray-500 dark:text-gray-400 transition-colors" :aria-label="$t('note.close')" :title="$t('note.close')">
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Content -->
    <div class="p-5 space-y-5">
      <!-- Format Selection -->
      <div class="space-y-2">
        <label class="text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('note.export_format') }}</label>
        <div class="flex bg-gray-100 dark:bg-[#1f1f1f] p-1 rounded-lg">
          <button
            v-for="fmt in formats"
            :key="fmt.id"
            @click="format = fmt.id as any"
            class="flex-1 py-1.5 text-sm rounded-md transition-colors font-medium"
            :class="format === fmt.id ? 'bg-white dark:bg-[#2c2c2c] text-text dark:text-text-dark shadow-sm' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'"
          >
            {{ fmt.label }}
          </button>
        </div>
      </div>

      <!-- Metadata Options -->
      <div class="space-y-2">
        <label class="text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('note.export_include') }}</label>
        <div class="space-y-2">
          <label class="flex items-center gap-3 cursor-pointer group">
            <div class="relative flex items-center justify-center">
              <input type="checkbox" v-model="includeTitle" class="peer sr-only" />
              <div class="w-4 h-4 border border-gray-300 dark:border-gray-500 rounded bg-white dark:bg-surface-dark peer-checked:bg-accent peer-checked:border-accent transition-colors"></div>
              <div class="absolute inset-0 flex items-center justify-center text-white opacity-0 peer-checked:opacity-100 transition-opacity">
                <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"></path></svg>
              </div>
            </div>
            <span class="text-sm text-text dark:text-text-dark group-hover:text-black dark:group-hover:text-white transition-colors">{{ $t('note.note_title') }}</span>
          </label>
          <label class="flex items-center gap-3 cursor-pointer group">
            <div class="relative flex items-center justify-center">
              <input type="checkbox" v-model="includeTags" class="peer sr-only" />
              <div class="w-4 h-4 border border-gray-300 dark:border-gray-500 rounded bg-white dark:bg-surface-dark peer-checked:bg-accent peer-checked:border-accent transition-colors"></div>
              <div class="absolute inset-0 flex items-center justify-center text-white opacity-0 peer-checked:opacity-100 transition-opacity">
                <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"></path></svg>
              </div>
            </div>
            <span class="text-sm text-text dark:text-text-dark group-hover:text-black dark:group-hover:text-white transition-colors">{{ $t('note.tags_col') }}</span>
          </label>
        </div>
      </div>

      <!-- PDF Options -->
      <div v-if="format === 'pdf'" class="space-y-4 pt-1 border-t border-border dark:border-border-subtle-dark">
        <div class="space-y-2">
          <label class="text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('note.export_orientation') }}</label>
          <div class="flex gap-2">
            <button
              @click="pdfOrientation = 'portrait'"
              class="flex-1 py-2 px-3 flex items-center justify-center gap-2 rounded-lg border text-sm transition-colors"
              :class="pdfOrientation === 'portrait' ? 'border-accent bg-accent/10 text-accent dark:text-accent-dark font-medium' : 'border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-gray-500 hover:bg-gray-50 dark:hover:bg-[#252525]'"
            >
              <FileText class="w-4 h-4" />
              {{ $t('note.export_portrait') }}
            </button>
            <button
              @click="pdfOrientation = 'landscape'"
              class="flex-1 py-2 px-3 flex items-center justify-center gap-2 rounded-lg border text-sm transition-colors"
              :class="pdfOrientation === 'landscape' ? 'border-accent bg-accent/10 text-accent dark:text-accent-dark font-medium' : 'border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-gray-500 hover:bg-gray-50 dark:hover:bg-[#252525]'"
            >
              <LayoutDashboard class="w-4 h-4" />
              {{ $t('note.export_landscape') }}
            </button>
          </div>
        </div>
        
        <div class="space-y-2">
          <label class="text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('note.export_paper_size') }}</label>
          <select v-model="pdfFormat" class="w-full px-3 py-2 rounded-lg border border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20 transition-all appearance-none cursor-pointer">
            <option v-for="size in paperSizes" :key="size.id" :value="size.id">{{ size.label }}</option>
          </select>
        </div>
      </div>
    </div>

    <!-- Footer -->
    <div class="p-5 border-t border-border dark:border-border-subtle-dark bg-gray-50/50 dark:bg-base-dark/50 flex justify-end gap-2">
      <button @click="emit('close')" class="px-4 py-2 text-sm rounded-lg text-gray-600 dark:text-gray-300 font-medium hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">
        {{ $t('note.cancel') }}
      </button>
      <button @click="handleExport" class="btn-primary">
        <Download class="w-4 h-4" />
        {{ $t('note.export') }}
      </button>
    </div>
  </AppDialog>
</template>
