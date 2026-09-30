<script setup lang="ts">
import { X } from 'lucide-vue-next';

export interface FileTab {
  id: string;
  filename: string;
  /** Where a recording or video starts, from a `#t=` citation. */
  time?: { start: number; end?: number; at?: number };
  extension: string;
  path: string;
  /**
   * The page a search sent the reader to, when the file was opened from a hit
   * inside it rather than from the list.
   */
  page?: number;
}

const props = defineProps<{
  tabs: FileTab[];
  activeTabId: string | null;
}>();

const emit = defineEmits<{
  (e: 'select', id: string): void;
  (e: 'close', id: string): void;
}>();
</script>

<template>
  <!--
    Each tab is a wrapper holding two sibling buttons, select and close. A
    button inside a button is invalid HTML: the inner one is not reachable by
    keyboard and screen readers announce the pair as one control.
  -->
  <div v-if="tabs.length > 0" class="flex items-center gap-0.5 px-2 py-1 bg-[#f5f5f7] dark:bg-[#0f0f0f] border-b border-gray-200/50 dark:border-white/5 overflow-x-auto scrollbar-none">
    <div
      v-for="tab in tabs" :key="tab.id"
      class="group flex items-center gap-1 pr-1 rounded-lg text-xs font-medium transition-all max-w-[180px] flex-shrink-0"
      :class="activeTabId === tab.id
        ? 'bg-white dark:bg-surface-hover-dark text-gray-900 dark:text-white shadow-sm'
        : 'text-gray-500 dark:text-gray-400 hover:bg-white/50 dark:hover:bg-white/5'"
    >
      <button
        type="button"
        @click="emit('select', tab.id)"
        class="min-w-0 pl-3 py-1.5 truncate cursor-pointer text-left"
        :aria-current="activeTabId === tab.id ? 'page' : undefined"
      >
        {{ tab.filename }}
      </button>
      <button
        type="button"
        @click="emit('close', tab.id)"
        class="p-0.5 rounded hover:bg-gray-200 dark:hover:bg-white/10 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 transition-opacity flex-shrink-0 cursor-pointer"
        :aria-label="$t('common.close')"
        :title="$t('common.close')"
      >
        <X class="w-3 h-3" />
      </button>
    </div>
  </div>
</template>
