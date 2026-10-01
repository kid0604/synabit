<script setup lang="ts">
/**
 * The bin on a task row or card.
 *
 * One press deletes — the same as every other delete in the app. It asks
 * nothing itself: the caller runs the app-wide "Ask before deleting"
 * (`confirmDelete`) and the undo toast is the way back.
 */
import { Trash2 } from 'lucide-vue-next';

withDefaults(defineProps<{
  /** Tighter styling for the board and matrix cards. */
  compact?: boolean;
}>(), { compact: false });

const emit = defineEmits<{ (e: 'confirm'): void }>();
</script>

<template>
  <button
    @click.stop="emit('confirm')"
    class="rounded-md transition-colors cursor-pointer flex items-center text-gray-500 dark:text-gray-400 hover:text-red-500 hover:bg-gray-100 dark:hover:bg-[#2c2c2c]"
    :class="compact ? 'p-0.5' : 'p-1.5'"
    :aria-label="$t('task.a11y_delete_task')"
    :title="$t('task.a11y_delete_task')"
  >
    <Trash2 :class="compact ? 'w-3.5 h-3.5' : 'w-4 h-4'" />
  </button>
</template>
