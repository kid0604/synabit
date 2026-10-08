<script setup lang="ts">
/**
 * What is in the vault's `.trash/`, and what can still be done about it.
 *
 * Everything the app deletes goes here rather than being unlinked, so this is
 * the last place something can be got back from — and the only place it can be
 * got rid of on purpose.
 *
 * It belongs to the vault, not to any one mini-app. `.trash/` holds notes,
 * whiteboards, people and captures alongside tasks, so it is reached from
 * Settings rather than from a sidebar that would imply it only has that app's
 * deletions in it.
 */
import { ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Trash2, Undo2, X, Loader2 } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { logger } from '../../utils/logger';
import AppDialog from './AppDialog.vue';
import ConfirmModal from './ConfirmModal.vue';
import { errorText } from '../errorText';

interface TrashEntry {
  trash_path: string;
  original_path: string;
  title: string;
  node_type: string;
  deleted_at: number;
  size: number;
}

const props = defineProps<{ show: boolean; vaultPath: string }>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'restored', path: string): void }>();

const { t } = useI18n();
const titleId = `trash-title-${Math.random().toString(36).slice(2, 9)}`;
const entries = ref<TrashEntry[]>([]);
const loading = ref(false);
const busyPath = ref<string | null>(null);
const error = ref('');

const load = async () => {
  if (!props.vaultPath) return;
  loading.value = true;
  error.value = '';
  try {
    entries.value = await invoke<TrashEntry[]>('list_trash', { vaultPath: props.vaultPath });
  } catch (e) {
    logger.error('Failed to list the trash', e);
    error.value = errorText(e);
  } finally {
    loading.value = false;
  }
};

// Read on opening rather than on a timer: the trash changes only when the user
// deletes something, and a panel that is shut has nothing to keep fresh.
watch(() => props.show, (open) => { if (open) void load(); }, { immediate: true });

const restore = async (entry: TrashEntry) => {
  busyPath.value = entry.trash_path;
  try {
    const restoredTo = await invoke<string>('restore_from_trash', {
      vaultPath: props.vaultPath,
      trashPath: entry.trash_path,
    });
    entries.value = entries.value.filter(e => e.trash_path !== entry.trash_path);
    emit('restored', restoredTo);
  } catch (e) {
    logger.error('Failed to restore from the trash', e);
    error.value = t('trash.restore_failed');
  } finally {
    busyPath.value = null;
  }
};

/**
 * The one delete in the app with nothing behind it — no trash, no undo — so
 * it is the one that asks first.
 */
const confirmingDelete = ref<TrashEntry | null>(null);

const confirmDeleteForever = async () => {
  const entry = confirmingDelete.value;
  confirmingDelete.value = null;
  if (entry) await deleteForever(entry);
};

const deleteForever = async (entry: TrashEntry) => {
  busyPath.value = entry.trash_path;
  try {
    await invoke('delete_trash_entry', {
      vaultPath: props.vaultPath,
      trashPath: entry.trash_path,
    });
    entries.value = entries.value.filter(e => e.trash_path !== entry.trash_path);
  } catch (e) {
    logger.error('Failed to remove a trash entry', e);
    error.value = t('trash.delete_failed');
  } finally {
    busyPath.value = null;
  }
};

const formatDate = (ms: number) =>
  ms ? new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : '';

const formatSize = (bytes: number) =>
  bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024
    ? `${(bytes / 1024).toFixed(1)} KB`
    : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
</script>

<template>
  <!-- Elevated: it is opened from inside Settings, which is a dialog too. -->
  <AppDialog
    :show="show"
    :labelledby="titleId"
    size="lg"
    elevated
    unstyled
    @close="emit('close')"
  >
    <div class="w-full max-h-[80vh] flex flex-col bg-white dark:bg-[#1c1c1e] rounded-2xl shadow-2xl border border-gray-200 dark:border-border-dark overflow-hidden">
      <div class="flex items-center justify-between px-5 py-4 border-b border-gray-100 dark:border-border-dark">
        <h3 :id="titleId" class="flex items-center gap-2 text-base font-semibold text-text dark:text-text-dark">
          <Trash2 class="w-4 h-4 text-gray-500 dark:text-gray-400" /> {{ t('trash.title') }}
          <span v-if="entries.length" class="text-xs font-normal text-gray-500 dark:text-gray-400">({{ entries.length }})</span>
        </h3>
        <button @click="emit('close')" class="p-1 rounded-md text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer" :aria-label="t('trash.a11y_close_trash')" :title="t('trash.a11y_close_trash')">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="flex-1 overflow-y-auto px-5 py-3 min-h-0">
        <div v-if="loading" class="flex items-center justify-center py-10 text-gray-500 dark:text-gray-400">
          <Loader2 class="w-5 h-5 animate-spin" />
        </div>

        <p v-else-if="error" class="py-6 text-sm text-red-500">{{ error }}</p>

        <p v-else-if="!entries.length" class="py-10 text-center text-sm text-gray-500 dark:text-gray-400">
          {{ t('trash.empty') }}
        </p>

        <div v-else class="space-y-1">
          <div
            v-for="entry in entries"
            :key="entry.trash_path"
            class="group flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-gray-50 dark:hover:bg-base-dark transition-colors"
          >
            <div class="flex-1 min-w-0">
              <p class="text-sm font-medium text-text dark:text-text-dark truncate">{{ entry.title }}</p>
              <p class="text-xs text-gray-500 dark:text-gray-400 truncate">
                <span v-if="entry.node_type" class="uppercase tracking-wide mr-1.5">{{ entry.node_type }}</span>
                {{ entry.original_path }} &middot; {{ t('trash.deleted_on', { date: formatDate(entry.deleted_at) }) }} &middot; {{ formatSize(entry.size) }}
              </p>
            </div>

            <div class="shrink-0 flex items-center gap-1 md:opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 transition-opacity">
              <button
                @click="restore(entry)"
                :disabled="busyPath === entry.trash_path"
                class="flex items-center gap-1 px-2 py-1 rounded-md text-xs font-medium text-accent dark:text-accent-dark hover:bg-accent/10 transition-colors cursor-pointer disabled:opacity-40"
              >
                <Undo2 class="w-3.5 h-3.5" /> {{ t('trash.restore') }}
              </button>
              <button
                @click="confirmingDelete = entry"
                :disabled="busyPath === entry.trash_path"
                class="flex items-center gap-1 px-2 py-1 rounded-md text-xs font-medium text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/30 transition-colors cursor-pointer disabled:opacity-40"
              >
                <Trash2 class="w-3.5 h-3.5" /> {{ t('trash.delete_forever') }}
              </button>
            </div>
          </div>
        </div>
      </div>

      <div class="px-5 py-3 border-t border-gray-100 dark:border-border-dark bg-gray-50/50 dark:bg-base-dark/50 space-y-1">
        <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('trash.purge_note') }}</p>
        <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('trash.restore_note') }}</p>
      </div>
    </div>
  </AppDialog>

  <ConfirmModal
    :show="confirmingDelete !== null"
    :title="t('trash.delete_forever_title', { name: confirmingDelete?.title ?? '' })"
    :message="t('trash.delete_forever_body')"
    :confirm-text="t('trash.delete_forever')"
    :cancel-text="t('common.cancel')"
    is-destructive
    @confirm="confirmDeleteForever"
    @cancel="confirmingDelete = null"
  />
</template>

