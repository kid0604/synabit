<script setup lang="ts">
import { Wrench, Zap } from 'lucide-vue-next';
import type { SynToolCallEvent, Tempo } from '../types';

defineProps<{
  toolCalls?: SynToolCallEvent[];
  /**
   * How heavy this turn is, if the backend has said yet.
   *
   * The same three dots for a count answered from the index and for a question
   * that will take four rounds is what makes the fast one feel slow and the
   * slow one feel broken. An instant turn says so; a working one says it may
   * take a moment, which is the sentence that lets somebody look away.
   */
  tempo?: Tempo | null;
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
    <!-- Answered from the index: one round, no tools. Saying so is the point —
         a spinner that implies work, for work that is not happening, is the
         thing this tempo exists to remove. -->
    <div v-else-if="tempo === 'instant'" class="flex items-center gap-2">
      <Zap class="w-4 h-4 text-emerald-500" />
      <span class="text-sm text-emerald-600 dark:text-emerald-400">{{ $t('syn.tempo_instant') }}</span>
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
      {{
        toolCalls?.length
          ? `${toolCalls.length} tool call${toolCalls.length > 1 ? 's' : ''}...`
          : tempo === 'instant'
            ? ''
            : tempo === 'working'
              ? $t('syn.tempo_working')
              : $t('syn.thinking')
      }}
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

