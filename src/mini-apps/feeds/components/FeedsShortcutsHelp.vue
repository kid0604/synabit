<script setup lang="ts">
/**
 * The list of shortcuts, opened with `?`.
 *
 * Feeds has had j/k/s/m/b/o/r bound since it was written and has never said so
 * anywhere. A bare-letter binding nobody has been told about is only found by
 * accident, and being found by accident is indistinguishable from a bug.
 */
import { X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';

defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();
const { t } = useI18n();

const GROUPS: { rows: { keys: string[]; label: string }[] }[] = [
  {
    rows: [
      { keys: ['J', 'K'], label: 'feeds.shortcut_move' },
      { keys: ['N'], label: 'feeds.shortcut_next_unread' },
      { keys: ['O'], label: 'feeds.shortcut_open_original' },
      { keys: ['Esc'], label: 'feeds.shortcut_escape' },
    ],
  },
  {
    rows: [
      { keys: ['S'], label: 'feeds.shortcut_star' },
      { keys: ['B'], label: 'feeds.shortcut_read_later' },
      { keys: ['M'], label: 'feeds.shortcut_toggle_read' },
    ],
  },
  {
    rows: [
      { keys: ['R'], label: 'feeds.shortcut_refresh' },
      { keys: ['?'], label: 'feeds.shortcut_help' },
    ],
  },
];
</script>

<template>
  <Teleport to="body">
    <Transition name="help-fade">
      <div
        v-if="show"
        class="fixed inset-0 z-[9500] flex items-center justify-center p-4 bg-black/40 backdrop-blur-sm"
        role="dialog"
        aria-modal="true"
        :aria-label="t('feeds.shortcuts')"
        @click.self="emit('close')"
      >
        <div class="w-full max-w-lg bg-white dark:bg-[#1c1c1e] rounded-2xl shadow-2xl border border-gray-200 dark:border-[#2c2c2c] overflow-hidden">
          <div class="flex items-center justify-between px-5 py-4 border-b border-gray-100 dark:border-[#2c2c2c]">
            <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ t('feeds.shortcuts') }}</h3>
            <button @click="emit('close')" class="p-1 rounded-md text-gray-400 hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer" :aria-label="t('feeds.close_shortcuts')">
              <X class="w-4 h-4" />
            </button>
          </div>

          <div class="px-5 py-4 space-y-4">
            <div
              v-for="(group, gi) in GROUPS"
              :key="gi"
              class="space-y-2"
              :class="gi > 0 ? 'pt-4 border-t border-gray-100 dark:border-[#2c2c2c]' : ''"
            >
              <div v-for="row in group.rows" :key="row.label" class="flex items-center justify-between gap-4">
                <span class="text-[13px] text-gray-600 dark:text-gray-400">{{ t(row.label) }}</span>
                <span class="flex items-center gap-1 shrink-0">
                  <kbd
                    v-for="key in row.keys"
                    :key="key"
                    class="px-2 py-0.5 rounded-md bg-gray-100 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#3a3a3a] text-[11px] font-mono text-[#1c1c1e] dark:text-[#f4f4f5]"
                  >{{ key }}</kbd>
                </span>
              </div>
            </div>
          </div>

          <div class="px-5 py-3 border-t border-gray-100 dark:border-[#2c2c2c] bg-gray-50/50 dark:bg-[#242424]/50">
            <p class="text-[11px] text-gray-400">{{ t('feeds.shortcuts_hint') }}</p>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.help-fade-enter-active, .help-fade-leave-active { transition: opacity 0.18s ease; }
.help-fade-enter-from, .help-fade-leave-to { opacity: 0; }
</style>
