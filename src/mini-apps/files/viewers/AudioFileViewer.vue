<script setup lang="ts">
import { computed } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Music } from 'lucide-vue-next';

const props = defineProps<{
  filePath: string;
  vaultPath: string;
}>();

const audioSrc = computed(() => convertFileSrc(props.filePath));
const filename = computed(() => props.filePath.split('/').pop() || 'Audio');
</script>

<template>
  <div class="flex-1 flex flex-col items-center justify-center gap-8 bg-gradient-to-br from-purple-50 to-indigo-50 dark:from-[#1a1225] dark:to-[#15192a]">
    <div class="w-32 h-32 rounded-3xl bg-gradient-to-br from-purple-500 to-indigo-600 flex items-center justify-center shadow-2xl shadow-purple-500/30">
      <Music class="w-16 h-16 text-white" />
    </div>
    <p class="text-sm font-medium text-gray-700 dark:text-gray-300 max-w-xs truncate">{{ filename }}</p>
    <audio :src="audioSrc" controls class="w-80" preload="metadata" />
  </div>
</template>
