<script setup lang="ts">
import { computed, ref } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { usePlaysFrom } from '../composables/usePlaysFrom';

const props = defineProps<{
  filePath: string;
  vaultPath: string;
  /** Seconds to start at, from a `#t=` citation. */
  initialTime?: number;
  /** Seconds to pause at, once. */
  endTime?: number;
}>();

const player = ref<HTMLVideoElement | null>(null);
usePlaysFrom(player, () => props.initialTime, () => props.endTime);

const videoSrc = computed(() => convertFileSrc(props.filePath));
</script>

<template>
  <div class="flex-1 flex items-center justify-center bg-black">
    <video
      ref="player"
      :src="videoSrc"
      controls
      class="max-w-full max-h-full"
      preload="metadata"
    />
  </div>
</template>
