<script setup lang="ts">
import { inject, computed } from 'vue';
import { ArrowLeft, ArrowRight } from 'lucide-vue-next';
import { usePlatform } from '../../composables/usePlatform';

const canGoBack = inject<{ value: boolean }>('canGoBack');
const canGoForward = inject<{ value: boolean }>('canGoForward');
const goBack = inject<() => void>('goBack');
const goForward = inject<() => void>('goForward');

const showBack = computed(() => canGoBack?.value ?? false);
const showForward = computed(() => canGoForward?.value ?? false);

// App.vue takes either Cmd or Ctrl, so the hint names the one this keyboard has.
const { isMac } = usePlatform();
const modKey = computed(() => (isMac.value ? '⌘' : 'Ctrl+'));
</script>

<template>
  <div class="flex items-center gap-0.5 shrink-0">
    <button
      @click.stop="goBack?.()"
      :disabled="!showBack"
      class="p-1.5 rounded-lg transition-colors"
      :class="showBack 
        ? 'hover:bg-gray-200 dark:hover:bg-[#333] text-gray-600 dark:text-gray-300 cursor-pointer' 
        : 'text-gray-500 dark:text-gray-400 opacity-40 cursor-default'"
      :title="$t('shell.nav.back_shortcut', { shortcut: `${modKey}[` })"
      :aria-label="$t('shell.nav.back')"
    >
      <ArrowLeft class="w-4 h-4" />
    </button>
    <button
      @click.stop="goForward?.()"
      :disabled="!showForward"
      class="p-1.5 rounded-lg transition-colors"
      :class="showForward 
        ? 'hover:bg-gray-200 dark:hover:bg-[#333] text-gray-600 dark:text-gray-300 cursor-pointer' 
        : 'text-gray-500 dark:text-gray-400 opacity-40 cursor-default'"
      :title="$t('shell.nav.forward_shortcut', { shortcut: `${modKey}]` })"
      :aria-label="$t('shell.nav.forward')"
    >
      <ArrowRight class="w-4 h-4" />
    </button>
  </div>
</template>
