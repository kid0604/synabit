<template>
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[9999] flex items-center justify-center" @click.self="$emit('close')">
      <div class="absolute inset-0 bg-black/40 backdrop-blur-sm" @click="$emit('close')"></div>
      <div class="relative w-full max-w-lg mx-4 bg-white dark:bg-[#1e1e1e] rounded-xl shadow-2xl border border-gray-200 dark:border-gray-700/50 overflow-hidden animate-in">
        
        <!-- Header -->
        <div class="flex items-center justify-between px-5 py-3.5 border-b border-gray-100 dark:border-gray-800">
          <div class="flex items-center gap-2 text-sm font-medium text-gray-800 dark:text-gray-200">
            <ArrowLeft v-if="step === 'blocks'" @click="goBackToNotes" class="w-4 h-4 cursor-pointer hover:text-black dark:hover:text-white transition-colors" />
            <LinkIcon class="w-4 h-4 text-emerald-500" />
            <span>{{ step === 'notes' ? 'Embed Content' : selectedNoteTitle }}</span>
          </div>
          <button @click="$emit('close')" class="p-1 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-md transition-colors">
            <X class="w-4 h-4 text-gray-400" />
          </button>
        </div>

        <!-- Search -->
        <div class="px-4 py-2.5 border-b border-gray-100 dark:border-gray-800">
          <div class="relative">
            <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
            <input
              ref="searchInput"
              v-model="searchQuery"
              :placeholder="step === 'notes' ? 'Search notes...' : 'Filter blocks...'"
              class="w-full pl-9 pr-3 py-2 text-sm bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700 rounded-lg outline-none focus:ring-2 focus:ring-emerald-500/30 focus:border-emerald-500 text-gray-800 dark:text-gray-200 placeholder:text-gray-400 transition-all"
              @keydown.escape="$emit('close')"
            />
          </div>
        </div>

        <!-- Content -->
        <div class="max-h-80 overflow-y-auto">
          <!-- Step 1: Note list -->
          <template v-if="step === 'notes'">
            <div v-if="filteredNotes.length === 0" class="px-5 py-8 text-center text-sm text-gray-400">
              No notes found
            </div>
            <button
              v-for="note in filteredNotes"
              :key="note.id"
              @click="selectNote(note)"
              class="w-full flex items-center gap-3 px-5 py-3 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors text-left border-b border-gray-50 dark:border-gray-800/50 last:border-b-0"
            >
              <div class="w-8 h-8 rounded-lg bg-emerald-50 dark:bg-emerald-900/20 flex items-center justify-center flex-shrink-0">
                <FileText class="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
              </div>
              <div class="min-w-0 flex-1">
                <div class="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">{{ note.title }}</div>
                <div class="text-xs text-gray-400 truncate mt-0.5">{{ note.content?.substring(0, 60) || 'Empty note' }}</div>
              </div>
            </button>
          </template>

          <!-- Step 2: Block list -->
          <template v-else-if="step === 'blocks'">
            <!-- Embed entire note option -->
            <button
              @click="embedEntireNote"
              class="w-full flex items-center gap-3 px-5 py-3 hover:bg-emerald-50 dark:hover:bg-emerald-900/10 transition-colors text-left border-b-2 border-gray-100 dark:border-gray-800"
            >
              <div class="w-8 h-8 rounded-lg bg-blue-50 dark:bg-blue-900/20 flex items-center justify-center flex-shrink-0">
                <FileText class="w-4 h-4 text-blue-600 dark:text-blue-400" />
              </div>
              <div class="min-w-0 flex-1">
                <div class="text-sm font-medium text-blue-700 dark:text-blue-300">Embed entire note</div>
                <div class="text-xs text-gray-400">Include all content from this note</div>
              </div>
            </button>

            <div v-if="loadingBlocks" class="px-5 py-8 text-center">
              <Loader2 class="w-5 h-5 animate-spin mx-auto text-gray-400 mb-2" />
              <p class="text-sm text-gray-400">Loading blocks...</p>
            </div>
            <div v-else-if="filteredBlocks.length === 0" class="px-5 py-8 text-center text-sm text-gray-400">
              No blocks found
            </div>
            <button
              v-for="block in filteredBlocks"
              :key="block.block_id"
              @click="embedBlock(block)"
              class="w-full flex items-center gap-3 px-5 py-2.5 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors text-left border-b border-gray-50 dark:border-gray-800/50 last:border-b-0"
            >
              <div class="w-7 h-7 rounded-md flex items-center justify-center flex-shrink-0"
                   :class="blockTypeStyle(block.block_type)">
                <span class="text-[10px] font-bold uppercase">{{ blockTypeLabel(block.block_type) }}</span>
              </div>
              <div class="min-w-0 flex-1">
                <div class="text-sm text-gray-700 dark:text-gray-300 truncate">{{ block.content_preview }}</div>
              </div>
            </button>
          </template>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Link as LinkIcon, Search, X, FileText, ArrowLeft, Loader2 } from 'lucide-vue-next';

interface NoteItem {
  id: string;
  title: string;
  content?: string;
  node_type?: string;
}

interface BlockItem {
  block_id: string;
  content_preview: string;
  raw_content: string;
  block_type: string;
  has_persistent_id: boolean;
}

const props = defineProps<{
  show: boolean;
  notes: NoteItem[];
  vaultPath: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'embed', payload: { nodeId: string; blockId?: string; noteTitle: string }): void;
}>();

const searchInput = ref<HTMLInputElement | null>(null);
const searchQuery = ref('');
const step = ref<'notes' | 'blocks'>('notes');
const selectedNoteId = ref('');
const selectedNoteTitle = ref('');
const blocks = ref<BlockItem[]>([]);
const loadingBlocks = ref(false);

watch(() => props.show, (val) => {
  if (val) {
    step.value = 'notes';
    searchQuery.value = '';
    selectedNoteId.value = '';
    blocks.value = [];
    nextTick(() => searchInput.value?.focus());
  }
});

const filteredNotes = computed(() => {
  const q = searchQuery.value.toLowerCase().trim();
  const noteList = props.notes.filter(n => n.node_type === 'note');
  if (!q) return noteList.slice(0, 50);
  return noteList.filter(n =>
    n.title.toLowerCase().includes(q) ||
    (n.content && n.content.toLowerCase().includes(q))
  ).slice(0, 50);
});

const filteredBlocks = computed(() => {
  const q = searchQuery.value.toLowerCase().trim();
  if (!q) return blocks.value;
  return blocks.value.filter(b => b.content_preview.toLowerCase().includes(q));
});

const selectNote = async (note: NoteItem) => {
  selectedNoteId.value = note.id;
  selectedNoteTitle.value = note.title;
  searchQuery.value = '';
  step.value = 'blocks';
  loadingBlocks.value = true;
  
  try {
    const result = await invoke<BlockItem[]>('get_node_headings', { nodeId: note.id });
    blocks.value = result;
  } catch (err) {
    console.error('Failed to load blocks:', err);
    blocks.value = [];
  } finally {
    loadingBlocks.value = false;
  }
  
  nextTick(() => searchInput.value?.focus());
};

const goBackToNotes = () => {
  step.value = 'notes';
  searchQuery.value = '';
  blocks.value = [];
};

const embedEntireNote = () => {
  emit('embed', { nodeId: selectedNoteId.value, noteTitle: selectedNoteTitle.value });
};

const embedBlock = async (block: BlockItem) => {
  let blockId = block.block_id;

  // If block doesn't have a persistent ^id yet, inject one into the source file
  if (!block.has_persistent_id) {
    try {
      blockId = await invoke<string>('create_block_reference', {
        vaultPath: props.vaultPath,
        nodeId: selectedNoteId.value,
        contentSnippet: block.raw_content,
      });
    } catch (err) {
      console.error('Failed to create block reference:', err);
      return;
    }
  }

  emit('embed', { nodeId: selectedNoteId.value, blockId, noteTitle: selectedNoteTitle.value });
};

const blockTypeStyle = (type: string) => {
  switch (type) {
    case 'h1': return 'bg-violet-100 dark:bg-violet-900/30 text-violet-600 dark:text-violet-400';
    case 'h2': return 'bg-sky-100 dark:bg-sky-900/30 text-sky-600 dark:text-sky-400';
    case 'h3': return 'bg-teal-100 dark:bg-teal-900/30 text-teal-600 dark:text-teal-400';
    default:   return 'bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400';
  }
};

const blockTypeLabel = (type: string) => {
  switch (type) {
    case 'h1': return 'H1';
    case 'h2': return 'H2';
    case 'h3': return 'H3';
    default:   return '¶';
  }
};
</script>

<style scoped>
.animate-in {
  animation: modalIn 0.15s ease-out;
}
@keyframes modalIn {
  from { opacity: 0; transform: scale(0.97) translateY(6px); }
  to   { opacity: 1; transform: scale(1) translateY(0); }
}
</style>
