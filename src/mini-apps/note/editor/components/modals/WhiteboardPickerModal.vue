<script setup lang="ts">
import AppDialog from '../../../../../shared/components/AppDialog.vue';
import { PenTool as PenToolIcon } from 'lucide-vue-next';

export interface WhiteboardItem {
  id?: string;
  path?: string;
  title?: string;
  tags?: string[];
  updated_at?: string;
}

defineProps<{
  show: boolean;
  boards: WhiteboardItem[];
  loading: boolean;
  search: string;
}>();

const emit = defineEmits<{
  (e: 'update:search', value: string): void;
  (e: 'select', board: WhiteboardItem): void;
  (e: 'close'): void;
}>();
</script>

<template>
  <AppDialog :show="show" labelledby="note-wbpicker-title" size="sm" panel-class="!max-h-[520px] !overflow-hidden flex flex-col" @close="emit('close')">
    <!-- Header -->
    <div class="flex items-center gap-2 p-4 pb-0">
      <PenToolIcon class="w-4 h-4 text-accent dark:text-accent-dark" />
      <h3 id="note-wbpicker-title" class="text-sm font-semibold text-gray-800 dark:text-gray-200">{{ $t('note.editor.whiteboard.picker_title') }}</h3>
    </div>

    <!-- Search -->
    <div class="px-4 pt-3 pb-2">
      <input
        :value="search"
        @input="emit('update:search', ($event.target as HTMLInputElement).value)"
        type="text"
        :placeholder="$t('note.editor.whiteboard.search')"
        class="w-full px-3 py-2 text-sm rounded-lg border border-border-subtle dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-accent transition-colors"
        autofocus
      />
    </div>

    <!-- Board List -->
    <div class="flex-1 overflow-y-auto px-4 pb-4">
      <!-- Loading -->
      <div v-if="loading" class="flex flex-col items-center justify-center py-12 gap-2 text-gray-500 dark:text-gray-400 text-sm">
        <div class="w-5 h-5 border-2 border-gray-200 dark:border-gray-600 border-t-accent rounded-full animate-spin"></div>
        <span>{{ $t('note.editor.whiteboard.loading_list') }}</span>
      </div>

      <!-- Empty -->
      <div v-else-if="boards.length === 0" class="flex flex-col items-center justify-center py-12 gap-2 text-gray-500 dark:text-gray-400 text-sm">
        <PenToolIcon class="w-6 h-6 opacity-40" />
        <span>{{ search ? $t('note.editor.whiteboard.none_matching') : $t('note.editor.whiteboard.none_found') }}</span>
      </div>

      <!-- List -->
      <div v-else class="space-y-1 mt-1">
        <button
          v-for="board in boards"
          :key="board.id || board.path"
          @click="emit('select', board)"
          class="w-full flex items-center gap-3 px-3 py-2.5 rounded-xl text-left hover:bg-accent/10 transition-colors group cursor-pointer"
        >
          <div class="w-9 h-9 rounded-lg bg-accent/10 flex items-center justify-center flex-shrink-0 group-hover:bg-accent/20 transition-colors">
            <PenToolIcon class="w-4 h-4 text-accent dark:text-accent-dark" />
          </div>
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">
              {{ board.title || $t('note.editor.whiteboard.untitled') }}
            </div>
            <div class="flex items-center gap-2 mt-0.5">
              <span v-if="board.tags && board.tags.length" class="text-xs text-gray-500 dark:text-gray-400 truncate">
                {{ board.tags.slice(0, 3).join(', ') }}
              </span>
              <span class="text-xs text-gray-500 dark:text-gray-400">
                {{ board.updated_at ? new Date(board.updated_at).toLocaleDateString($i18n.locale) : '' }}
              </span>
            </div>
          </div>
        </button>
      </div>
    </div>

    <!-- Footer -->
    <div class="flex justify-end p-4 pt-2 border-t border-[#f3f4f6] dark:border-[#2a2a2a]">
      <button @click="emit('close')" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">{{ $t('note.cancel') }}</button>
    </div>
  </AppDialog>
</template>
