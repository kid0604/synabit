<script setup lang="ts">
/**
 * What to do with the tasks that are selected.
 *
 * Sits above the list rather than floating over it: a bar that covers the last
 * row hides one of the things being acted on, which is the worst row to hide.
 */
import { CheckCircle2, Trash2, X, Flag, Folder } from 'lucide-vue-next';
import type { TaskMetadata } from '../types';

defineProps<{
  selected: TaskMetadata[];
  allVisibleSelected: boolean;
  projects: any[];
}>();

const emit = defineEmits<{
  (e: 'complete'): void;
  (e: 'delete'): void;
  (e: 'set-priority', priority: string): void;
  (e: 'set-project', projectId: string): void;
  (e: 'toggle-all'): void;
  (e: 'clear'): void;
}>();
</script>

<template>
  <div class="sticky top-0 z-20 flex items-center gap-2 flex-wrap px-3 py-2 mb-2 rounded-xl bg-blue-50/90 dark:bg-blue-950/40 border border-blue-100 dark:border-blue-900/50 backdrop-blur">
    <span class="text-xs font-semibold text-blue-700 dark:text-blue-300 mr-1">
      {{ $t('task.selected_count', { count: selected.length }) }}
    </span>

    <button
      @click="emit('toggle-all')"
      class="text-xs font-medium px-2 py-1 rounded-md text-blue-700 dark:text-blue-300 hover:bg-blue-100 dark:hover:bg-blue-900/40 transition-colors cursor-pointer"
    >
      {{ allVisibleSelected ? $t('task.select_none') : $t('task.select_all') }}
    </button>

    <div class="h-4 w-px bg-blue-200 dark:bg-blue-900" />

    <button
      @click="emit('complete')"
      class="flex items-center gap-1.5 text-xs font-medium px-2 py-1 rounded-md text-green-700 dark:text-green-400 hover:bg-green-100 dark:hover:bg-green-900/30 transition-colors cursor-pointer"
    >
      <CheckCircle2 class="w-3.5 h-3.5" /> {{ $t('task.bulk_complete') }}
    </button>

    <div class="relative flex items-center">
      <Flag class="w-3.5 h-3.5 text-orange-500 mr-1" />
      <span class="text-xs font-medium text-orange-700 dark:text-orange-400">{{ $t('task.bulk_priority') }}</span>
      <select
        @change="emit('set-priority', ($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''"
        class="absolute inset-0 opacity-0 cursor-pointer"
        :aria-label="$t('task.bulk_priority')"
      >
        <option value="" disabled selected></option>
        <option value="P1">P1</option>
        <option value="P2">P2</option>
        <option value="P3">P3</option>
        <option value="P4">P4</option>
        <option value="">—</option>
      </select>
    </div>

    <div v-if="projects.length" class="relative flex items-center">
      <Folder class="w-3.5 h-3.5 text-indigo-500 mr-1" />
      <span class="text-xs font-medium text-indigo-700 dark:text-indigo-400">{{ $t('task.bulk_project') }}</span>
      <select
        @change="emit('set-project', ($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''"
        class="absolute inset-0 opacity-0 cursor-pointer"
        :aria-label="$t('task.bulk_project')"
      >
        <option value="" disabled selected></option>
        <option v-for="proj in projects" :key="proj.id" :value="proj.id">{{ proj.title }}</option>
      </select>
    </div>

    <button
      @click="emit('delete')"
      class="flex items-center gap-1.5 text-xs font-medium px-2 py-1 rounded-md text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/30 transition-colors cursor-pointer"
    >
      <Trash2 class="w-3.5 h-3.5" /> {{ $t('task.bulk_delete') }}
    </button>

    <button
      @click="emit('clear')"
      class="ml-auto p-1 rounded-md text-blue-600 dark:text-blue-400 hover:bg-blue-100 dark:hover:bg-blue-900/40 transition-colors cursor-pointer"
      :aria-label="$t('task.select_none')"
    >
      <X class="w-4 h-4" />
    </button>
  </div>
</template>
