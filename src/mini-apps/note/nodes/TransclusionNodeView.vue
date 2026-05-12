<template>
  <node-view-wrapper class="transclusion-node my-4 border rounded-lg overflow-hidden shadow-sm transition-all duration-300 hover:shadow-md border-gray-200 dark:border-gray-700/50 bg-gray-50/50 dark:bg-gray-800/30">
    
    <!-- Header -->
    <div class="flex items-center px-4 py-2 border-b border-gray-200 dark:border-gray-700/50 bg-gray-100/50 dark:bg-gray-800/80">
      <div class="flex-1 flex items-center space-x-2 text-sm font-medium text-gray-700 dark:text-gray-300">
        <LinkIcon class="w-4 h-4 text-emerald-500" />
        <span class="truncate">{{ displayTitle }}</span>
      </div>
      <div class="flex items-center space-x-1">
        <button @click="openNote" class="p-1 hover:bg-gray-200 dark:hover:bg-gray-700 rounded transition-colors" title="Open source note">
          <ExternalLinkIcon class="w-4 h-4 text-gray-500 dark:text-gray-400" />
        </button>
        <button @click="removeNode" class="p-1 hover:bg-red-100 dark:hover:bg-red-900/30 rounded transition-colors" title="Remove embed">
          <XIcon class="w-4 h-4 text-gray-400 hover:text-red-500" />
        </button>
      </div>
    </div>
    
    <!-- Body -->
    <div class="p-4 relative">
      <div v-if="loading" class="flex items-center justify-center py-4 text-gray-500 dark:text-gray-400">
        <Loader2Icon class="w-5 h-5 animate-spin mr-2" />
        <span class="text-sm">Loading content...</span>
      </div>
      <div v-else-if="error" class="text-red-500 text-sm py-2">
        {{ error }}
      </div>
      <div v-else class="prose dark:prose-invert max-w-none text-sm break-words whitespace-pre-wrap">
        {{ content }}
      </div>
    </div>
  </node-view-wrapper>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { NodeViewWrapper, nodeViewProps } from '@tiptap/vue-3';
import { invoke } from '@tauri-apps/api/core';
import { Link as LinkIcon, ExternalLink as ExternalLinkIcon, Loader2 as Loader2Icon, X as XIcon } from 'lucide-vue-next';

const props = defineProps(nodeViewProps);

const loading = ref(true);
const content = ref('');
const error = ref<string | null>(null);

const target = computed(() => props.node.attrs.target || '');
const nodeIdAttr = computed(() => props.node.attrs.nodeId || '');
const resolvedTitle = ref('');

const displayTitle = computed(() => {
  if (resolvedTitle.value) return resolvedTitle.value;
  if (!target.value) return 'Embedded content';
  if (!target.value.includes('#')) return target.value;
  return 'Loading…';
});

/** Strip YAML frontmatter from note content */
const stripFrontmatter = (text: string): string => {
  if (!text.startsWith('---')) return text;
  const secondDash = text.indexOf('---', 3);
  if (secondDash > 3) return text.substring(secondDash + 3).trim();
  return text;
};

/** Resolve note ID from target (handles legacy title-based targets) */
const resolveNodeId = async (): Promise<string | null> => {
  const parts = target.value.split('#');
  const firstPart = parts[0];

  if (nodeIdAttr.value) return nodeIdAttr.value;

  const isUUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(firstPart);
  if (isUUID) return firstPart;

  // Legacy: firstPart is a note title
  resolvedTitle.value = firstPart;
  const nodes = await invoke<any[]>('get_linked_nodes', { targetTitle: firstPart, targetId: null });
  return nodes?.length > 0 ? nodes[0].id : null;
};

const loadBlock = async () => {
  if (!target.value) {
    error.value = "No target specified.";
    loading.value = false;
    return;
  }

  try {
    const nodeId = await resolveNodeId();
    if (!nodeId) {
      error.value = "Note not found.";
      loading.value = false;
      return;
    }

    // Resolve title for display
    const allNodes = await invoke<any[]>('get_all_nodes');
    const sourceNode = allNodes.find((n: any) => n.id === nodeId);
    if (sourceNode) resolvedTitle.value = sourceNode.title || nodeId;

    const blockId = target.value.split('#')[1] || null;

    if (blockId) {
      // Block embed — scan source file for ^blockId
      const blockContent = await invoke<string | null>('get_node_block', { nodeId, blockId });
      content.value = blockContent || '(Block deleted from source note)';
    } else {
      // Full-note embed
      content.value = sourceNode ? stripFrontmatter(sourceNode.content) : '(Note not found)';
    }
  } catch (err: any) {
    error.value = err.toString();
  } finally {
    loading.value = false;
  }
};

const openNote = () => {
  const nodeId = nodeIdAttr.value || target.value.split('#')[0];
  window.dispatchEvent(new CustomEvent('synabit-navigate', {
    detail: { type: 'note', id: nodeId }
  }));
};

const removeNode = () => {
  props.deleteNode();
};

// --- Live refresh: auto-update when source note is saved ---
const sourceNodeId = computed(() => {
  if (nodeIdAttr.value) return nodeIdAttr.value;
  return target.value.split('#')[0] || '';
});

const reloadContent = async () => {
  try {
    const nodeId = sourceNodeId.value;
    if (!nodeId) return;

    const blockId = target.value.split('#')[1] || null;

    if (blockId) {
      // Block embed — just re-fetch by ^id (always returns latest content)
      const blockContent = await invoke<string | null>('get_node_block', { nodeId, blockId });
      if (blockContent) content.value = blockContent;
    } else {
      // Full-note embed — reload
      const allNodes = await invoke<any[]>('get_all_nodes');
      const sourceNode = allNodes.find((n: any) => n.id === nodeId);
      if (sourceNode) {
        content.value = stripFrontmatter(sourceNode.content);
        if (sourceNode.title) resolvedTitle.value = sourceNode.title;
      }
    }
  } catch { /* ignore reload errors */ }
};

const onBlockRefresh = (e: Event) => {
  const detail = (e as CustomEvent).detail;
  if (detail?.nodeId && detail.nodeId === sourceNodeId.value) {
    reloadContent();
  }
};

onMounted(() => {
  loadBlock();
  window.addEventListener('synabit-block-refresh', onBlockRefresh);
});

onUnmounted(() => {
  window.removeEventListener('synabit-block-refresh', onBlockRefresh);
});
</script>

<style scoped>
.transclusion-node {
  user-select: none;
}
</style>
