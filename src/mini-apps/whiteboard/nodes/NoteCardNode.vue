<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import { NodeResizer } from '@vue-flow/node-resizer';
import { ExternalLink, FileText, Loader2 } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';

const props = defineProps<{
  id: string;
  data: {
    noteId: string;
    noteTitle?: string;
    blockId?: string;
    width?: number;
    height?: number;
    color?: string;
  };
  selected?: boolean;
}>();

const emit = defineEmits(['update:data', 'open-note']);

const isLoading = ref(true);
const content = ref('');
const error = ref<string | null>(null);
const title = ref(props.data.noteTitle || 'Loading Note...');

/** Strip frontmatter from markdown */
const stripFrontmatter = (text: string): string => {
  if (!text.startsWith('---')) return text;
  const end = text.indexOf('---', 3);
  return end > 3 ? text.substring(end + 3).trim() : text;
};

/** Clean markdown syntax for plain-text preview */
const cleanMarkdown = (text: string): string => {
  return text
    .replace(/^#{1,6}\s+/gm, '')          // headings
    .replace(/\*\*(.+?)\*\*/g, '$1')       // bold
    .replace(/\*(.+?)\*/g, '$1')           // italic
    .replace(/__(.+?)__/g, '$1')           // bold alt
    .replace(/_(.+?)_/g, '$1')             // italic alt
    .replace(/~~(.+?)~~/g, '$1')           // strikethrough
    .replace(/`(.+?)`/g, '$1')             // inline code
    .replace(/^\s*[-*+]\s+/gm, '• ')       // list items
    .replace(/^\s*\d+\.\s+/gm, '')         // numbered lists
    .replace(/^\s*>\s+/gm, '')             // blockquotes
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1') // links
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, '') // images
    .replace(/```[\s\S]*?```/g, '')         // code blocks
    .replace(/\s*\^[a-z0-9]{6}$/gm, '')    // ^block-id markers
    .replace(/\n{3,}/g, '\n\n')            // excessive newlines
    .trim();
};

const loadNoteData = async () => {
  isLoading.value = true;
  error.value = null;
  
  try {
    const nodes = await invoke<any[]>('get_nodes', { nodeType: 'note' });
    const targetNode = nodes.find(n => n.id === props.data.noteId);
    
    if (targetNode) {
      title.value = targetNode.title;
      if (props.data.blockId) {
        const blockContent = await invoke<string | null>('get_node_block', { 
            nodeId: targetNode.id, 
            blockId: props.data.blockId 
        });
        if (blockContent) {
           content.value = blockContent.replace(/\s*\^([a-zA-Z0-9\-]+)\s*$/, '').trim();
        } else {
           error.value = "Block not found.";
        }
      } else {
        const body = stripFrontmatter(targetNode.content);
        content.value = cleanMarkdown(body);
      }
    } else {
      error.value = "Note not found.";
    }
  } catch (err: any) {
    error.value = err.toString();
  } finally {
    isLoading.value = false;
  }
};

onMounted(() => {
  loadNoteData();
});

const cardWidth = computed(() => (props.data.width || 280) + 'px');
const cardHeight = computed(() => (props.data.height || 180) + 'px');

const onResize = (event: any) => {
  emit('update:data', {
    ...props.data,
    width: event.width,
    height: event.height,
  });
};

const handleOpenNote = (e: MouseEvent) => {
  e.stopPropagation();
  emit('open-note', { id: props.data.noteId });
};
</script>

<template>
  <div class="note-card-node" :class="{ 'is-selected': selected }" :style="{ width: cardWidth, height: cardHeight }">
    <NodeResizer
      v-if="selected"
      :color="data.color || '#7c3aed'"
      :is-visible="true"
      :min-width="180"
      :min-height="120"
      :max-width="500"
      @resize="onResize"
    />

    <Handle type="target" :position="Position.Top" />
    <Handle type="target" :position="Position.Left" />
    <Handle type="source" :position="Position.Bottom" />
    <Handle type="source" :position="Position.Right" />

    <div 
        class="w-full h-full bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-200 dark:border-gray-700 flex flex-col overflow-hidden"
        :style="{ borderColor: selected ? (data.color || '#7c3aed') : undefined }"
    >
      <!-- Header -->
      <div class="flex items-center px-3 py-1.5 bg-gray-50/80 dark:bg-gray-800/80 border-b border-gray-200 dark:border-gray-700 flex-shrink-0">
        <FileText class="w-3.5 h-3.5 text-violet-500 mr-2 flex-shrink-0" />
        <span class="text-[11px] font-semibold text-gray-700 dark:text-gray-300 truncate flex-1">{{ title }}</span>
        
        <button @click="handleOpenNote" class="p-0.5 hover:bg-gray-200 dark:hover:bg-gray-600 rounded text-gray-400 flex-shrink-0 transition-colors" :title="$t('whiteboard.open_note')">
          <ExternalLink class="w-3 h-3" />
        </button>
      </div>
      
      <!-- Content -->
      <div class="flex-1 px-3 py-2 overflow-hidden text-[11px] leading-relaxed text-gray-600 dark:text-gray-400 relative">
        <div v-if="isLoading" class="absolute inset-0 flex items-center justify-center">
            <Loader2 class="w-4 h-4 animate-spin text-gray-400" />
        </div>
        <div v-else-if="error" class="text-red-500 italic text-center text-[10px] mt-2">{{ error }}</div>
        <div v-else class="note-content-preview">
            {{ content }}
        </div>
        <!-- Fade-out gradient at bottom -->
        <div class="absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-white dark:from-gray-800 to-transparent pointer-events-none"></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.note-card-node {
  min-width: 180px;
  min-height: 120px;
  max-width: 500px;
  border-radius: 12px;
  transition: transform 0.1s;
}

.note-card-node.is-selected {
  z-index: 10;
}

.note-content-preview {
  white-space: pre-wrap;
  word-break: break-word;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 8;
  -webkit-box-orient: vertical;
}

/* Make handles slightly smaller and more elegant */
:deep(.vue-flow__handle) {
  width: 8px;
  height: 8px;
  background: var(--color-surface, #fff);
  border: 2px solid var(--color-border, #d4d4d8);
}
.dark :deep(.vue-flow__handle) {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #52525b);
}
.is-selected :deep(.vue-flow__handle) {
  border-color: var(--color-accent, #7c3aed);
}
</style>
