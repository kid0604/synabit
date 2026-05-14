<script setup lang="ts">
import { ref, computed } from 'vue';
import { FileText, Trash2, FileDown } from 'lucide-vue-next';
import ConfirmModal from '../../../shared/components/ConfirmModal.vue';
import type { PdfAnnotation } from '../composables/usePdfAnnotations';

const props = defineProps<{
  annotations: PdfAnnotation[];
  pdfTitle: string;
}>();

const emit = defineEmits<{
  (e: 'go-to', annotation: PdfAnnotation): void;
  (e: 'delete', id: string): void;
  (e: 'export-note'): void;
}>();

const showConfirmDelete = ref(false);
const itemToDelete = ref<string | null>(null);

const requestDelete = (id: string) => {
  itemToDelete.value = id;
  showConfirmDelete.value = true;
};

const executeDelete = () => {
  if (itemToDelete.value) {
    emit('delete', itemToDelete.value);
  }
  showConfirmDelete.value = false;
  itemToDelete.value = null;
};

// Group annotations by page
const groupedAnnotations = computed(() => {
  const groups = new Map<number, PdfAnnotation[]>();
  for (const ann of props.annotations) {
    if (!groups.has(ann.page)) groups.set(ann.page, []);
    groups.get(ann.page)!.push(ann);
  }
  return Array.from(groups.entries()).sort((a, b) => a[0] - b[0]);
});

const colorDot: Record<string, string> = {
  yellow: 'bg-yellow-400',
  green: 'bg-green-400',
  blue: 'bg-blue-400',
  pink: 'bg-pink-400',
};

const colorMap: Record<string, string> = {
  yellow: '#facc15', // yellow-400
  green: '#4ade80', // green-400
  blue: '#60a5fa', // blue-400
  pink: '#f472b6', // pink-400
};
</script>

<template>
  <div class="w-72 xl:w-80 flex-shrink-0 bg-white/90 dark:bg-[#1e1e1e]/90 backdrop-blur-xl border-l border-gray-200/50 dark:border-white/5 flex flex-col">
    <!-- Header -->
    <div class="h-12 px-4 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5 flex-shrink-0">
      <h3 class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Annotations</h3>
      <div class="flex items-center gap-1">
        <button @click="emit('export-note')" :disabled="annotations.length === 0"
          class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-white/10 text-gray-500 dark:text-gray-400 disabled:opacity-30 cursor-pointer transition-colors" title="Export to Note">
          <FileText class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>

    <!-- Empty state -->
    <div v-if="annotations.length === 0" class="flex-1 flex items-center justify-center p-6">
      <div class="text-center">
        <div class="w-12 h-12 rounded-full bg-gray-100 dark:bg-white/5 flex items-center justify-center mx-auto mb-3">
          <FileText class="w-5 h-5 text-gray-300 dark:text-gray-600" />
        </div>
        <p class="text-xs text-gray-400 dark:text-gray-500">No annotations yet</p>
        <p class="text-[10px] text-gray-300 dark:text-gray-600 mt-1">Select text and highlight to start</p>
      </div>
    </div>

    <!-- Annotation list -->
    <div v-else class="flex-1 overflow-y-auto">
      <div v-for="[page, anns] in groupedAnnotations" :key="page" class="border-b border-gray-100 dark:border-white/5 last:border-b-0">
        <!-- Page header -->
        <div class="px-4 py-2 bg-gray-50/50 dark:bg-black/20 sticky top-0 z-10">
          <span class="text-[10px] font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider">Page {{ page }}</span>
        </div>
        <!-- Annotations for page -->
        <div class="divide-y divide-gray-50 dark:divide-white/[0.03]">
          <div v-for="ann in anns" :key="ann.id"
            @click="emit('go-to', ann)"
            class="px-4 py-3 hover:bg-gray-50 dark:hover:bg-white/[0.03] cursor-pointer transition-colors group">
            <div class="flex items-start gap-2.5">
              <!-- Color indicator -->
              <div :class="['w-2.5 h-2.5 rounded-full flex-shrink-0 mt-1', colorDot[ann.color] || 'bg-yellow-400']" />
              <div class="flex-1 min-w-0">
                <!-- Highlighted text -->
                <p class="text-xs text-gray-700 dark:text-gray-300 line-clamp-2 italic leading-relaxed">
                  "{{ ann.text.substring(0, 120) }}{{ ann.text.length > 120 ? '…' : '' }}"
                </p>
                <!-- Note preview -->
                <p v-if="ann.content" class="text-[10px] text-gray-400 dark:text-gray-500 mt-1 line-clamp-1">
                  📝 {{ ann.content }}
                </p>
              </div>
              <!-- Delete button -->
              <button
                @click.stop="requestDelete(ann.id)"
                class="opacity-0 group-hover:opacity-100 p-1.5 rounded-md hover:bg-surface dark:hover:bg-surface-dark text-red-500 transition-all focus:opacity-100 cursor-pointer"
                title="Delete highlight"
              >
                <Trash2 class="w-3.5 h-3.5" />
              </button>
            </div>
            <!-- Annotation note -->
            <p v-if="ann.content" class="text-[11px] mt-2 text-text dark:text-text-dark bg-surface-hover/30 dark:bg-surface-hover-dark/30 p-2 rounded leading-relaxed border-l-2"
               :style="{ borderLeftColor: colorMap[ann.color] || colorMap.yellow }">
              {{ ann.content }}
            </p>
          </div>
        </div>
      </div>
    </div>

    <!-- Footer: count -->
    <div v-if="annotations.length > 0" class="px-4 py-2 border-t border-gray-200/50 dark:border-white/5 flex-shrink-0">
      <span class="text-[10px] text-gray-400">{{ annotations.length }} annotation{{ annotations.length > 1 ? 's' : '' }}</span>
    </div>

    <ConfirmModal
      :show="showConfirmDelete"
      title="Delete Highlight"
      message="Are you sure you want to delete this highlight? This action cannot be undone."
      confirm-text="Delete"
      :is-destructive="true"
      @confirm="executeDelete"
      @cancel="showConfirmDelete = false; itemToDelete = null"
    />
  </div>
</template>
