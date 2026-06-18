<script setup lang="ts">
import { FileText, MoreVertical, Pin, Lock } from 'lucide-vue-next';
import { useAppLockStore } from '../../../stores/useAppLockStore';
import NoteContextMenu from './NoteContextMenu.vue';

defineProps<{
  note: { id: string; title: string; tags: string[]; pinned: boolean };
  isActive: boolean;
  showContextMenu: boolean;
  isPinnedSection?: boolean;
}>();

const emit = defineEmits<{
  (e: 'select', id: string): void;
  (e: 'toggle-context', id: string, event: Event): void;
  (e: 'pin', id: string): void;
  (e: 'open-window', id: string): void;
  (e: 'rename', id: string): void;
  (e: 'toggle-lock', id: string): void;
  (e: 'delete', id: string): void;
}>();

const appLockStore = useAppLockStore();
</script>

<template>
  <div
    @click="emit('select', note.id)"
    class="px-3 py-2 border border-transparent rounded-lg cursor-pointer transition-colors relative group"
    :class="isActive ? 'bg-white dark:bg-[#2a2a2a] shadow-sm border-[#e6e6e6] dark:border-[#3a3a3a]' : 'hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f]'">
    <!-- Context menu trigger -->
    <div class="absolute right-2 top-2 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity z-10" :class="{'md:opacity-100': showContextMenu}">
      <button @click.stop="(e) => emit('toggle-context', note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 border border-gray-200 dark:border-gray-600">
        <MoreVertical class="w-3.5 h-3.5 text-gray-500" />
      </button>
      <NoteContextMenu
        v-if="showContextMenu"
        :note-id="note.id"
        :is-pinned="note.pinned"
        variant="sidebar"
        @pin="emit('pin', $event)"
        @open-window="emit('open-window', $event)"
        @rename="emit('rename', $event)"
        @toggle-lock="emit('toggle-lock', $event)"
        @delete="emit('delete', $event)"
      />
    </div>
    <!-- Note content -->
    <div class="flex items-center gap-2 mb-1.5 pr-6">
      <Pin v-if="isPinnedSection" class="w-3 h-3 text-orange-500 shrink-0 fill-orange-500/20" />
      <FileText v-else class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80" />
      <Lock v-if="appLockStore.isNoteProtected(note.id)" class="w-3 h-3 text-amber-500 shrink-0" />
      <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ note.title || $t('note.untitled_note') }}</span>
    </div>
    <div class="flex flex-wrap gap-1" v-if="note.tags.length">
      <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200/60 dark:bg-[#333] text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
    </div>
  </div>
</template>
