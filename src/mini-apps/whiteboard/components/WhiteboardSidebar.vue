<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Plus, Trash2, PenTool, PanelLeftClose, Search, FileText, GripVertical, ChevronDown, ChevronRight, SquarePlus } from 'lucide-vue-next';
import { useAppStore } from '../../../stores/useAppStore';
import { storeToRefs } from 'pinia';
import { logger } from '../../../utils/logger';
import { localDay } from '../../../shared/localDay';

const props = defineProps<{
  boards: any[];
  currentBoardId: string;
  currentBoardData: any;
  notes: any[];
}>();

const { vaultPath } = storeToRefs(useAppStore());

const emit = defineEmits<{
  (e: 'switch-board', boardId: string): void;
  (e: 'create-board'): void;
  (e: 'delete-board', boardId: string): void;
  (e: 'note-drag-start', event: DragEvent, note: any): void;
  /** Put the note on the open board without dragging it there. */
  (e: 'add-note', note: any): void;
}>();

// ─── Sidebar State ────────────────────────────────────────
const sidebarOpen = ref(true);
const sidebarTab = ref<'boards' | 'notes'>('boards');
const noteSearch = ref('');
const dailyNotesExpanded = ref(false);

/** Detect daily notes (title is a date like 2026-05-04) */
const isDailyNote = (title: string) => /^\d{4}-\d{2}-\d{2}$/.test(title?.trim());

/** Extract a 1-line preview from note content (strip frontmatter + markdown) */
const notePreview = (content: string) => {
  if (!content) return '';
  let text = content;
  if (text.startsWith('---')) {
    const end = text.indexOf('---', 3);
    if (end > 3) text = text.substring(end + 3);
  }
  // Strip markdown syntax and get first meaningful line
  const line = text.split('\n').map(l => l.trim()).find(l => l && !l.startsWith('#') && !l.startsWith('---'));
  if (!line) return '';
  const clean = line.replace(/[\*\_\[\]\(\)\#\\\>\`]/g, '').trim();
  return clean.length > 60 ? clean.substring(0, 60) + '…' : clean;
};

/**
 * Ids of notes matching the search, from the full-text index.
 *
 * The sidebar carries each note's opening rather than its body, so searching it
 * in the browser could only ever match the first line. The index has the whole
 * text, ranks by relevance and handles diacritics — it is both a smaller
 * payload and a better search than the substring scan this replaces. `null`
 * means no search is active.
 */
const searchMatchIds = ref<Set<string> | null>(null);
let searchTimer: ReturnType<typeof setTimeout>;

watch(noteSearch, (q) => {
  clearTimeout(searchTimer);
  const query = q.trim();
  if (!query) {
    searchMatchIds.value = null;
    return;
  }
  searchTimer = setTimeout(async () => {
    try {
      const resp = await invoke<{ results: { id: string }[] }>('search_notes', {
        vaultPath: vaultPath.value,
        query,
      });
      // Ignore a reply the user has already typed past.
      if (noteSearch.value.trim() === query) {
        searchMatchIds.value = new Set(resp.results.map(r => r.id));
      }
    } catch (e) {
      logger.error('Whiteboard sidebar search failed', e);
    }
  }, 200);
});

/** Title matching, shown immediately while the indexed search is in flight. */
const matchesSearch = (note: any) => {
  const q = noteSearch.value.toLowerCase().trim();
  if (!q) return true;
  if (searchMatchIds.value !== null) return searchMatchIds.value.has(note.id);
  return (note.title || '').toLowerCase().includes(q);
};

const filteredRegularNotes = computed(() =>
  props.notes.filter(n => !isDailyNote(n.title)).filter(matchesSearch)
);

const filteredDailyNotes = computed(() =>
  props.notes.filter(n => isDailyNote(n.title)).filter(matchesSearch)
);

function handleNoteDragStart(event: DragEvent, note: any) {
  if (event.dataTransfer) {
    event.dataTransfer.setData('application/synabit-note-id', note.id);
    event.dataTransfer.setData('application/synabit-note-title', note.title);
    event.dataTransfer.effectAllowed = 'copy';
  }
  emit('note-drag-start', event, note);
}

// ─── Sidebar Resizing ─────────────────────────────────────
const wSidebar = ref(260);
const isDraggingSidebar = ref(false);

const startDragSidebar = (e: MouseEvent) => {
  isDraggingSidebar.value = true;
  const onMouseMove = (ev: MouseEvent) => {
    wSidebar.value = Math.max(180, Math.min(480, ev.clientX));
  };
  const onMouseUp = () => {
    isDraggingSidebar.value = false;
    window.removeEventListener('mousemove', onMouseMove);
    window.removeEventListener('mouseup', onMouseUp);
  };
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);
};

defineExpose({ sidebarOpen, isDraggingSidebar });
</script>

<template>
  <div v-if="sidebarOpen" class="md:hidden absolute inset-0 bg-black/20 dark:bg-black/40 z-[48]" @click="sidebarOpen = false" />
  
  <!-- Sidebar: Board List -->
  <div
    v-if="sidebarOpen"
    class="wb-sidebar flex flex-col absolute md:relative z-[49] shrink-0 bg-surface-alt dark:bg-surface-alt-dark border-r border-border dark:border-border-dark"
    :style="{ width: wSidebar + 'px' }"
  >
    <div class="hidden md:block absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragSidebar"></div>

    <div class="flex items-center justify-between p-3 border-b border-border dark:border-border-dark" data-tauri-drag-region>
      <div class="flex gap-4">
        <button @click="sidebarTab = 'boards'" :class="sidebarTab === 'boards' ? 'text-sm font-bold text-text dark:text-text-dark' : 'text-sm font-semibold text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors'">{{ $t('whiteboard.boards') }}</button>
        <button @click="sidebarTab = 'notes'" :class="sidebarTab === 'notes' ? 'text-sm font-bold text-text dark:text-text-dark' : 'text-sm font-semibold text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors'">{{ $t('whiteboard.notes') }}</button>
      </div>
      <div class="flex items-center gap-1" @mousedown.stop>
        <button
          v-if="sidebarTab === 'boards'"
          @click="emit('create-board')"
          class="btn-primary"
          :title="$t('whiteboard.new_board')"
        >
          <Plus class="w-4 h-4" />
          <span>{{ $t('whiteboard.new_board') }}</span>
        </button>
        <button @click="sidebarOpen = false" class="wb-icon-btn" :title="$t('whiteboard.close_sidebar')" :aria-label="$t('whiteboard.close_sidebar')">
          <PanelLeftClose class="w-4 h-4" />
        </button>
      </div>
    </div>

    <div v-if="sidebarTab === 'boards'" class="flex-1 overflow-y-auto p-2 space-y-1" @mousedown.stop>
      <!--
        Two buttons side by side rather than one inside the other: a button
        inside a button is invalid, and the delete one was a 20px target that
        only existed under a mouse. Now it is a full icon button, shown on
        hover, on keyboard focus and always on a touch screen.
      -->
      <div
        v-for="board in boards"
        :key="board.id"
        :class="[
          'group flex items-center rounded-lg text-sm transition-all',
          currentBoardId === board.id
            ? 'bg-accent/10 text-accent dark:text-accent-dark font-semibold'
            : 'text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark'
        ]"
      >
        <button
          type="button"
          @click="emit('switch-board', board.id)"
          class="flex-1 min-w-0 text-left px-3 py-2.5 rounded-lg cursor-pointer"
          :aria-current="currentBoardId === board.id ? 'page' : undefined"
        >
          <span class="flex items-center gap-2 min-w-0">
            <PenTool class="w-3.5 h-3.5 flex-shrink-0 opacity-50" aria-hidden="true" />
            <span class="truncate">{{ board.title }}</span>
          </span>
          <span class="block text-xs text-gray-500 dark:text-gray-400 font-normal mt-0.5 ml-5.5">{{ localDay(board.updated_at) }}</span>
        </button>
        <button
          type="button"
          @click.stop="emit('delete-board', board.id)"
          class="btn-icon shrink-0 mr-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 pointer-coarse:opacity-100 transition-opacity hover:!text-danger hover:!bg-danger/10"
          :title="$t('whiteboard.delete')"
          :aria-label="$t('whiteboard.delete')"
        >
          <Trash2 class="w-4 h-4" aria-hidden="true" />
        </button>
      </div>

      <div v-if="!boards.length" class="text-center text-xs text-muted dark:text-muted-dark py-8">
        <PenTool class="w-8 h-8 mx-auto mb-2 opacity-30" />
        <p>{{ $t('whiteboard.no_whiteboards') }}</p>
        <button @click="emit('create-board')" class="text-accent dark:text-accent-dark mt-1 hover:underline">
          {{ $t('whiteboard.create_one') }}
        </button>
      </div>
    </div>

    <div v-else-if="sidebarTab === 'notes'" class="flex-1 overflow-y-auto flex flex-col" @mousedown.stop>
      <!-- Search -->
      <div class="p-2 border-b border-border dark:border-border-dark">
        <div class="relative">
          <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted dark:text-muted-dark" />
          <input
            v-model="noteSearch"
            :placeholder="$t('whiteboard.search_notes')"
            class="w-full pl-8 pr-3 py-1.5 text-xs bg-surface-hover/50 dark:bg-surface-hover-dark/50 border border-border dark:border-border-dark rounded-md outline-none focus:ring-1 focus:ring-accent/40 text-text dark:text-text-dark placeholder:text-muted dark:placeholder:text-muted-dark transition-all"
          />
        </div>
      </div>

      <div class="flex-1 overflow-y-auto p-2 space-y-1">
        <!-- Regular Notes -->
        <div
          v-for="note in filteredRegularNotes"
          :key="note.id"
          draggable="true"
          @dragstart="(e) => handleNoteDragStart(e, note)"
          class="group px-3 py-2 rounded-lg transition-all hover:bg-surface-hover dark:hover:bg-surface-hover-dark cursor-grab active:cursor-grabbing border border-transparent hover:border-border dark:hover:border-border-dark"
        >
          <div class="flex items-center gap-2 min-w-0">
            <GripVertical class="w-3 h-3 flex-shrink-0 opacity-0 group-hover:opacity-40 transition-opacity text-muted" />
            <FileText class="w-3.5 h-3.5 flex-shrink-0 text-accent/60" />
            <span class="text-sm font-medium text-text dark:text-text-dark truncate flex-1 min-w-0">{{ note.title || $t('whiteboard.untitled') }}</span>
            <button
              v-if="currentBoardData"
              @click.stop="emit('add-note', note)"
              class="wb-add-note opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
              :title="$t('whiteboard.add_to_board')"
              :aria-label="$t('whiteboard.add_note_to_board', { title: note.title || $t('whiteboard.untitled') })"
            >
              <SquarePlus class="w-3.5 h-3.5" />
            </button>
          </div>
          <p v-if="notePreview(note.preview)" class="text-xs text-muted dark:text-muted-dark truncate mt-0.5 ml-[34px]">
            {{ notePreview(note.preview) }}
          </p>
        </div>

        <!-- Daily Notes Group -->
        <div v-if="filteredDailyNotes.length > 0" class="mt-2">
          <button
            @click="dailyNotesExpanded = !dailyNotesExpanded"
            class="flex items-center gap-1.5 px-2 py-1.5 w-full text-left text-xs font-semibold uppercase tracking-wider text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors"
          >
            <component :is="dailyNotesExpanded ? ChevronDown : ChevronRight" class="w-3 h-3" />
            {{ $t('whiteboard.daily_notes') }}
            <span class="text-xs font-normal opacity-60">({{ filteredDailyNotes.length }})</span>
          </button>
          <div v-if="dailyNotesExpanded" class="space-y-0.5 mt-0.5">
            <div
              v-for="note in filteredDailyNotes"
              :key="note.id"
              draggable="true"
              @dragstart="(e) => handleNoteDragStart(e, note)"
              class="group flex items-center gap-2 px-3 py-1.5 rounded-md transition-all hover:bg-surface-hover dark:hover:bg-surface-hover-dark cursor-grab active:cursor-grabbing"
            >
              <GripVertical class="w-3 h-3 flex-shrink-0 opacity-0 group-hover:opacity-40 transition-opacity text-muted" />
              <FileText class="w-3 h-3 flex-shrink-0 text-muted/50" />
              <span class="text-xs text-text-secondary dark:text-text-secondary-dark truncate flex-1 min-w-0">{{ note.title }}</span>
              <button
                v-if="currentBoardData"
                @click.stop="emit('add-note', note)"
                class="wb-add-note opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
                :title="$t('whiteboard.add_to_board')"
                :aria-label="$t('whiteboard.add_note_to_board', { title: note.title })"
              >
                <SquarePlus class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        </div>

        <!-- Empty state -->
        <div v-if="filteredRegularNotes.length === 0 && filteredDailyNotes.length === 0" class="text-center text-xs text-muted dark:text-muted-dark py-8">
          <FileText class="w-8 h-8 mx-auto mb-2 opacity-30" />
          <p>{{ noteSearch ? $t('whiteboard.no_matching_notes') : $t('whiteboard.no_notes') }}</p>
        </div>
      </div>
    </div>
  </div>

</template>

<style scoped>
.wb-sidebar {
  width: 220px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
}
.wb-icon-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--color-text-secondary, #52525b);
  cursor: pointer;
  transition: all 0.15s;
}
:global(.dark) .wb-icon-btn {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-icon-btn:hover {
  background: var(--color-surface-hover, #f5f5f5);
}
:global(.dark) .wb-icon-btn:hover {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.wb-add-note {
  flex-shrink: 0;
  padding: 4px;
  border-radius: 6px;
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: opacity 0.15s, background-color 0.15s, color 0.15s;
}
.wb-add-note:hover,
.wb-add-note:focus-visible {
  opacity: 1;
  background: color-mix(in oklab, var(--color-accent) 10%, transparent);
  color: var(--color-accent);
}
:global(.dark) .wb-add-note {
  color: var(--color-text-secondary-dark);
}
:global(.dark) .wb-add-note:hover,
:global(.dark) .wb-add-note:focus-visible {
  color: var(--color-accent-dark);
}
</style>
