<script setup lang="ts">
import { X } from 'lucide-vue-next';

export interface FileTab {
  id: string;
  filename: string;
  extension: string;
  path: string;
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
  <div v-if="tabs.length > 0" class="flex items-center gap-0.5 px-2 py-1 bg-[#f5f5f7] dark:bg-[#0f0f0f] border-b border-gray-200/50 dark:border-white/5 overflow-x-auto scrollbar-none">
    <button
      v-for="tab in tabs" :key="tab.id"
      @click="emit('select', tab.id)"
      class="group flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium transition-all cursor-pointer max-w-[180px] flex-shrink-0"
      :class="activeTabId === tab.id
        ? 'bg-white dark:bg-[#2a2a2a] text-gray-900 dark:text-white shadow-sm'
        : 'text-gray-500 dark:text-gray-400 hover:bg-white/50 dark:hover:bg-white/5'"
    >
      <span class="truncate">{{ tab.filename }}</span>
      <button
        @click.stop="emit('close', tab.id)"
        class="p-0.5 rounded hover:bg-gray-200 dark:hover:bg-white/10 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0 cursor-pointer"
      >
        <X class="w-3 h-3" />
      </button>
    </button>
  </div>
</template>
