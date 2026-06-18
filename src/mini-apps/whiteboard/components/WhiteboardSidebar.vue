<script setup lang="ts">
import { ref, computed } from 'vue';
import { Plus, Trash2, PenTool, PanelLeftClose, PanelLeft, Search, FileText, GripVertical, ChevronDown, ChevronRight } from 'lucide-vue-next';

const props = defineProps<{
  boards: any[];
  currentBoardId: string;
  currentBoardData: any;
  notes: any[];
}>();

const emit = defineEmits<{
  (e: 'switch-board', boardId: string): void;
  (e: 'create-board'): void;
  (e: 'delete-board', boardId: string): void;
  (e: 'note-drag-start', event: DragEvent, note: any): void;
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

const filteredRegularNotes = computed(() => {
  const q = noteSearch.value.toLowerCase().trim();
  return props.notes
    .filter(n => !isDailyNote(n.title))
    .filter(n => !q || (n.title || '').toLowerCase().includes(q) || (n.content || '').toLowerCase().includes(q));
});

const filteredDailyNotes = computed(() => {
  const q = noteSearch.value.toLowerCase().trim();
  return props.notes
    .filter(n => isDailyNote(n.title))
    .filter(n => !q || (n.title || '').toLowerCase().includes(q) || (n.content || '').toLowerCase().includes(q));
});

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
  <!-- Sidebar: Board List -->
  <div
    v-if="sidebarOpen"
    class="wb-sidebar flex flex-col relative shrink-0"
    :style="{ width: wSidebar + 'px' }"
  >
    <div class="hidden md:block absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragSidebar"></div>

    <div class="flex items-center justify-between p-3 border-b border-border dark:border-border-dark" data-tauri-drag-region>
      <div class="flex gap-4">
        <button @click="sidebarTab = 'boards'" :class="sidebarTab === 'boards' ? 'text-sm font-bold text-text dark:text-text-dark' : 'text-sm font-semibold text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors'">Boards</button>
        <button @click="sidebarTab = 'notes'" :class="sidebarTab === 'notes' ? 'text-sm font-bold text-text dark:text-text-dark' : 'text-sm font-semibold text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors'">Notes</button>
      </div>
      <div class="flex items-center gap-1" @mousedown.stop>
        <button
          v-if="sidebarTab === 'boards'"
          @click="emit('create-board')"
          class="wb-icon-btn"
          :title="$t('whiteboard.new_board')"
        >
          <Plus class="w-4 h-4" />
        </button>
        <button @click="sidebarOpen = false" class="wb-icon-btn" :title="$t('whiteboard.close_sidebar')">
          <PanelLeftClose class="w-4 h-4" />
        </button>
      </div>
    </div>

    <div v-if="sidebarTab === 'boards'" class="flex-1 overflow-y-auto p-2 space-y-1" @mousedown.stop>
      <button
        v-for="board in boards"
        :key="board.id"
        @click="emit('switch-board', board.id)"
        :class="[
          'w-full text-left px-3 py-2.5 rounded-lg text-sm transition-all group',
          currentBoardId === board.id
            ? 'bg-accent/10 text-accent dark:text-accent-dark font-semibold'
            : 'text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark'
        ]"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2 min-w-0">
            <PenTool class="w-3.5 h-3.5 flex-shrink-0 opacity-50" />
            <span class="truncate">{{ board.title }}</span>
          </div>
          <button
            @click.stop="emit('delete-board', board.id)"
            class="opacity-0 group-hover:opacity-60 hover:!opacity-100 transition-opacity p-1 rounded hover:bg-danger/10 hover:text-danger"
          >
            <Trash2 class="w-3 h-3" />
          </button>
        </div>
        <p class="text-[10px] opacity-40 mt-0.5 ml-5.5">{{ board.updated_at?.split(' ')[0] }}</p>
      </button>

      <div v-if="!boards.length" class="text-center text-xs text-muted dark:text-muted-dark py-8">
        <PenTool class="w-8 h-8 mx-auto mb-2 opacity-30" />
        <p>No whiteboards yet</p>
        <button @click="emit('create-board')" class="text-accent dark:text-accent-dark mt-1 hover:underline">
          Create one
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
            <span class="text-sm font-medium text-text dark:text-text-dark truncate">{{ note.title || 'Untitled' }}</span>
          </div>
          <p v-if="notePreview(note.content)" class="text-[11px] text-muted dark:text-muted-dark truncate mt-0.5 ml-[34px]">
            {{ notePreview(note.content) }}
          </p>
        </div>

        <!-- Daily Notes Group -->
        <div v-if="filteredDailyNotes.length > 0" class="mt-2">
          <button
            @click="dailyNotesExpanded = !dailyNotesExpanded"
            class="flex items-center gap-1.5 px-2 py-1.5 w-full text-left text-[11px] font-semibold uppercase tracking-wider text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors"
          >
            <component :is="dailyNotesExpanded ? ChevronDown : ChevronRight" class="w-3 h-3" />
            Daily Notes
            <span class="text-[10px] font-normal opacity-60">({{ filteredDailyNotes.length }})</span>
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
              <span class="text-xs text-text-secondary dark:text-text-secondary-dark truncate">{{ note.title }}</span>
            </div>
          </div>
        </div>

        <!-- Empty state -->
        <div v-if="filteredRegularNotes.length === 0 && filteredDailyNotes.length === 0" class="text-center text-xs text-muted dark:text-muted-dark py-8">
          <FileText class="w-8 h-8 mx-auto mb-2 opacity-30" />
          <p>{{ noteSearch ? 'No matching notes' : 'No notes found' }}</p>
        </div>
      </div>
    </div>
  </div>

  <!-- Toggle sidebar button when closed -->
  <button
    v-if="!sidebarOpen"
    @click="sidebarOpen = true"
    class="absolute top-3 left-1 z-50 wb-icon-btn bg-surface dark:bg-surface-dark border border-border dark:border-border-dark shadow-md"
    :title="$t('whiteboard.open_sidebar')"
  >
    <PanelLeft class="w-4 h-4" />
  </button>
</template>

<style scoped>
.wb-sidebar {
  width: 220px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--color-border, #e6e6e6);
  background: var(--color-surface-alt, #fbfbfc);
  height: 100%;
}
:global(.dark) .wb-sidebar {
  border-color: var(--color-border-dark, #2c2c2c);
  background: var(--color-surface-alt-dark, #191919);
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
</style>
