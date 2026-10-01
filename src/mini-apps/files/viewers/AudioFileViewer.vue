<script setup lang="ts">
import { computed, ref } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Music } from 'lucide-vue-next';
import { usePlaysFrom } from '../composables/usePlaysFrom';

const props = defineProps<{
  filePath: string;
  vaultPath: string;
  /** Seconds to start at, from a `#t=` citation. */
  initialTime?: number;
  /** Seconds to pause at, once. */
  endTime?: number;
}>();

const player = ref<HTMLAudioElement | null>(null);
usePlaysFrom(player, () => props.initialTime, () => props.endTime);

const audioSrc = computed(() => convertFileSrc(props.filePath));
const filename = computed(() => props.filePath.split('/').pop() || 'Audio');
</script>

<template>
  <div class="flex-1 flex flex-col items-center justify-center gap-8 bg-accent/5 dark:bg-accent-dark/5">
    <div class="w-32 h-32 rounded-3xl bg-accent flex items-center justify-center shadow-2xl shadow-accent/30">
      <Music class="w-16 h-16 text-white" />
    </div>
    <p class="text-sm font-medium text-gray-700 dark:text-gray-300 max-w-xs truncate">{{ filename }}</p>
    <audio ref="player" :src="audioSrc" controls class="w-80" preload="metadata" />
  </div>
</template>
