<script setup lang="ts">
import { Wrench } from 'lucide-vue-next';
import type { SynToolCallEvent } from '../types';

defineProps<{
  toolCalls?: SynToolCallEvent[];
}>();
</script>

<template>
  <div class="flex items-center gap-2 px-4 py-3">
    <!-- Tool calls in progress -->
    <div v-if="toolCalls?.length" class="flex items-center gap-2">
      <Wrench class="w-4 h-4 text-violet-500 animate-tool-spin" />
      <span class="text-sm text-violet-500 font-medium font-mono">
        {{ toolCalls[toolCalls.length - 1].tool_name }}
      </span>
    </div>
    <!-- Default thinking dots -->
    <div v-else class="flex items-center gap-1.5">
      <div
        class="w-2 h-2 rounded-full animate-pulse"
        style="animation-delay: 0ms"
        :class="'bg-violet-500'"
      />
      <div
        class="w-2 h-2 rounded-full animate-pulse"
        style="animation-delay: 200ms"
        :class="'bg-violet-400'"
      />
      <div
        class="w-2 h-2 rounded-full animate-pulse"
        style="animation-delay: 400ms"
        :class="'bg-violet-300'"
      />
    </div>
    <span class="text-sm text-gray-500 dark:text-gray-400 italic">
      {{ toolCalls?.length ? `${toolCalls.length} tool call${toolCalls.length > 1 ? 's' : ''}...` : $t('syn.thinking') }}
    </span>
  </div>
</template>

<style scoped>
@keyframes pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.4; transform: scale(0.75); }
}

.animate-pulse {
  animation: pulse 1.4s ease-in-out infinite;
}

@keyframes toolSpin {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

.animate-tool-spin {
  animation: toolSpin 2s linear infinite;
}
</style>

