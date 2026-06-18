<script setup lang="ts">
import { Pin, ExternalLink, Edit2, Trash2, Lock, Unlock } from 'lucide-vue-next';
import { useAppLockStore } from '../../../stores/useAppLockStore';

defineProps<{
  noteId: string;
  isPinned: boolean;
  variant?: 'sidebar' | 'manager';
}>();

const emit = defineEmits<{
  (e: 'pin', id: string): void;
  (e: 'open-window', id: string): void;
  (e: 'rename', id: string): void;
  (e: 'toggle-lock', id: string): void;
  (e: 'delete', id: string): void;
}>();

const appLockStore = useAppLockStore();
</script>

<template>
  <div class="absolute right-0 top-6 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
    <button @click.stop="emit('pin', noteId)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
      <Pin class="w-3 h-3" /> {{ isPinned ? $t('note.unpin') : $t('note.pin') }}
    </button>
    <template v-if="variant !== 'manager'">
      <button @click.stop="emit('open-window', noteId)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
        <ExternalLink class="w-3 h-3" /> {{ $t('note.open_new_window') }}
      </button>
      <button @click.stop="emit('rename', noteId)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
        <Edit2 class="w-3 h-3" /> {{ $t('note.rename') }}
      </button>
      <button v-if="appLockStore.isEnabled" @click.stop="emit('toggle-lock', noteId)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
        <component :is="appLockStore.isNoteProtected(noteId) ? Unlock : Lock" class="w-3 h-3" />
        {{ appLockStore.isNoteProtected(noteId) ? $t('note.unlock_note') : $t('note.lock_note') }}
      </button>
    </template>
    <button @click.stop="emit('delete', noteId)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2">
      <Trash2 class="w-3 h-3" /> {{ $t('note.delete') }}
    </button>
  </div>
</template>
