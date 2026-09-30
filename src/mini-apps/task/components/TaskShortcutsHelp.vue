<script setup lang="ts">
/**
 * The list of shortcuts, opened with `?`.
 *
 * Without it the shortcuts do not exist: a bare-letter binding nobody has been
 * told about is only ever found by accident, and being found by accident is
 * indistinguishable from a bug.
 */
import { X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import AppDialog from '../../../shared/components/AppDialog.vue';

defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();
const { t } = useI18n();

const GROUPS: { rows: { keys: string[]; label: string }[] }[] = [
  {
    rows: [
      { keys: ['J', 'K'], label: 'task.shortcut_move' },
      { keys: ['↵'], label: 'task.shortcut_open' },
      { keys: ['Space'], label: 'task.shortcut_toggle' },
      { keys: ['⌫'], label: 'task.shortcut_delete' },
    ],
  },
  {
    rows: [
      { keys: ['X'], label: 'task.shortcut_select' },
      { keys: ['⇧', 'X'], label: 'task.shortcut_select_range' },
      { keys: ['⌘/Ctrl', 'A'], label: 'task.shortcut_select_all' },
      { keys: ['Esc'], label: 'task.shortcut_escape' },
    ],
  },
  {
    rows: [
      { keys: ['N'], label: 'task.shortcut_new' },
      { keys: ['/'], label: 'task.shortcut_search' },
      { keys: ['1', '2', '3', '4'], label: 'task.shortcut_views' },
      { keys: ['?'], label: 'task.shortcut_help' },
    ],
  },
];
</script>

<template>
  <AppDialog :show="show" labelledby="task-shortcuts-title" @close="emit('close')">
        <div class="overflow-hidden">
          <div class="flex items-center justify-between px-5 py-4 border-b border-gray-100 dark:border-border-dark">
            <h3 id="task-shortcuts-title" class="text-base font-semibold text-text dark:text-text-dark">{{ t('task.shortcuts') }}</h3>
            <button @click="emit('close')" class="p-1 rounded-md text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer" :aria-label="t('task.a11y_close_shortcuts')" :title="t('task.a11y_close_shortcuts')">
              <X class="w-4 h-4" />
            </button>
          </div>

          <div class="px-5 py-4 space-y-4">
            <div
              v-for="(group, gi) in GROUPS"
              :key="gi"
              class="space-y-2"
              :class="gi > 0 ? 'pt-4 border-t border-gray-100 dark:border-border-dark' : ''"
            >
              <div v-for="row in group.rows" :key="row.label" class="flex items-center justify-between gap-4">
                <span class="text-[13px] text-gray-600 dark:text-gray-400">{{ t(row.label) }}</span>
                <span class="flex items-center gap-1 shrink-0">
                  <kbd
                    v-for="key in row.keys"
                    :key="key"
                    class="px-2 py-0.5 rounded-md bg-gray-100 dark:bg-surface-hover-dark border border-gray-200 dark:border-border-subtle-dark text-xs font-mono text-text dark:text-text-dark"
                  >{{ key }}</kbd>
                </span>
              </div>
            </div>
          </div>

          <div class="px-5 py-3 border-t border-gray-100 dark:border-border-dark bg-gray-50/50 dark:bg-base-dark/50">
            <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('task.shortcuts_hint') }}</p>
          </div>
        </div>
  </AppDialog>
</template>
