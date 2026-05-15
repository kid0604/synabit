<script setup lang="ts">
import { inject, computed } from 'vue';
import { ArrowLeft, ArrowRight } from 'lucide-vue-next';

const canGoBack = inject<{ value: boolean }>('canGoBack');
const canGoForward = inject<{ value: boolean }>('canGoForward');
const goBack = inject<() => void>('goBack');
const goForward = inject<() => void>('goForward');

const showBack = computed(() => canGoBack?.value ?? false);
const showForward = computed(() => canGoForward?.value ?? false);
</script>

<template>
  <div class="flex items-center gap-0.5 shrink-0">
    <button
      @click.stop="goBack?.()"
      :disabled="!showBack"
      class="p-1.5 rounded-lg transition-colors"
      :class="showBack 
        ? 'hover:bg-gray-200 dark:hover:bg-[#333] text-gray-600 dark:text-gray-300 cursor-pointer' 
        : 'text-gray-300 dark:text-gray-600 cursor-default'"
      title="Back (⌘[)"
    >
      <ArrowLeft class="w-4 h-4" />
    </button>
    <button
      @click.stop="goForward?.()"
      :disabled="!showForward"
      class="p-1.5 rounded-lg transition-colors"
      :class="showForward 
        ? 'hover:bg-gray-200 dark:hover:bg-[#333] text-gray-600 dark:text-gray-300 cursor-pointer' 
        : 'text-gray-300 dark:text-gray-600 cursor-default'"
      title="Forward (⌘])"
    >
      <ArrowRight class="w-4 h-4" />
    </button>
  </div>
</template>
