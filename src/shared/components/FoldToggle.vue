<script setup lang="ts">
/**
 * A sidebar section's heading, as the button that folds it.
 *
 * Only the heading's words and the chevron: the row around it — a "Show all"
 * beside it, a refresh button — stays the caller's, because a button inside a
 * button is not something a screen reader or a click can make sense of. The
 * heading's look comes from the caller too, through `class`, so each sidebar
 * keeps the typography it already had.
 */
import { ChevronRight } from 'lucide-vue-next';

defineProps<{ folded: boolean }>();
defineEmits<{ toggle: [] }>();
</script>

<template>
  <button
    type="button"
    class="flex items-center gap-1 min-w-0 cursor-pointer hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
    :aria-expanded="!folded"
    @click="$emit('toggle')"
  >
    <ChevronRight class="w-3 h-3 shrink-0 transition-transform" :class="folded ? '' : 'rotate-90'" aria-hidden="true" />
    <span class="truncate"><slot /></span>
  </button>
</template>
