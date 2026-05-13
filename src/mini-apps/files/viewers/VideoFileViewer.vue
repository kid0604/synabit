<script setup lang="ts">
import { computed } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';

const props = defineProps<{
  filePath: string;
  vaultPath: string;
}>();

const videoSrc = computed(() => convertFileSrc(props.filePath));
</script>

<template>
  <div class="flex-1 flex items-center justify-center bg-black">
    <video
      :src="videoSrc"
      controls
      class="max-w-full max-h-full"
      preload="metadata"
    />
  </div>
</template>
