<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { FileQuestion, ExternalLink } from 'lucide-vue-next';

const props = defineProps<{
  filePath: string;
  vaultPath: string;
}>();

const filename = props.filePath.split('/').pop() || 'File';
const extension = filename.includes('.') ? filename.split('.').pop()!.toUpperCase() : 'FILE';

const openInNative = async () => {
  try {
    await invoke('open_local_file', { vaultPath: props.vaultPath, path: props.filePath });
  } catch (_) {}
};
</script>

<template>
  <div class="flex-1 flex flex-col items-center justify-center gap-6 bg-gray-50 dark:bg-[#1a1a1a]">
    <div class="w-24 h-24 rounded-2xl bg-gray-200/80 dark:bg-white/5 flex items-center justify-center">
      <FileQuestion class="w-12 h-12 text-gray-400 dark:text-gray-500" />
    </div>
    <div class="text-center space-y-1">
      <p class="text-base font-semibold text-gray-700 dark:text-gray-300">{{ filename }}</p>
      <p class="text-xs text-gray-400">{{ extension }} file — no built-in viewer available</p>
    </div>
    <button
      @click="openInNative"
      class="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-gray-900 dark:bg-white text-white dark:text-gray-900 text-sm font-semibold hover:scale-105 active:scale-95 transition-transform shadow-lg cursor-pointer"
    >
      <ExternalLink class="w-4 h-4" />
      Open in Native App
    </button>
  </div>
</template>
