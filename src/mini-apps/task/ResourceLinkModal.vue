<template>
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[9999] flex items-center justify-center" @click.self="$emit('close')">
      <div class="absolute inset-0 bg-black/40 backdrop-blur-sm" @click="$emit('close')"></div>
      <div class="relative w-full max-w-lg mx-4 bg-white dark:bg-[#1e1e1e] rounded-xl shadow-2xl border border-gray-200 dark:border-gray-700/50 overflow-hidden animate-in">
        
        <!-- Header -->
        <div class="flex items-center justify-between px-5 py-3.5 border-b border-gray-100 dark:border-gray-800">
          <div class="flex items-center gap-2 text-sm font-medium text-gray-800 dark:text-gray-200">
            <LinkIcon class="w-4 h-4 text-emerald-500" />
            <span>Link Resource</span>
          </div>
          <button @click="$emit('close')" class="p-1 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-md transition-colors">
            <X class="w-4 h-4 text-gray-400" />
          </button>
        </div>

        <!-- Search & Filter -->
        <div class="px-4 pt-4 pb-3 border-b border-gray-100 dark:border-gray-800 flex flex-col gap-3">
          <div class="relative w-full">
            <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
            <input
              ref="searchInput"
              v-model="searchQuery"
              placeholder="Search resources..."
              class="w-full pl-9 pr-3 py-2.5 text-sm bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700 rounded-lg outline-none focus:ring-2 focus:ring-emerald-500/30 focus:border-emerald-500 text-gray-800 dark:text-gray-200 placeholder:text-gray-400 transition-all"
              @keydown.escape="$emit('close')"
            />
          </div>
          <!-- Type Filter -->
          <div class="flex items-center gap-2 overflow-x-auto hide-scrollbar">
             <button @click="activeTab = 'all'" :class="activeTab === 'all' ? 'bg-gray-800 text-white dark:bg-gray-200 dark:text-gray-900' : 'bg-gray-100 text-gray-600 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700'" class="px-3.5 py-1.5 text-xs font-medium rounded-full transition-all shrink-0">All</button>
             <button @click="activeTab = 'note'" :class="activeTab === 'note' ? 'bg-gray-800 text-white dark:bg-gray-200 dark:text-gray-900' : 'bg-gray-100 text-gray-600 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700'" class="px-3.5 py-1.5 text-xs font-medium rounded-full transition-all shrink-0">Notes</button>
             <button @click="activeTab = 'whiteboard'" :class="activeTab === 'whiteboard' ? 'bg-gray-800 text-white dark:bg-gray-200 dark:text-gray-900' : 'bg-gray-100 text-gray-600 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700'" class="px-3.5 py-1.5 text-xs font-medium rounded-full transition-all shrink-0">Whiteboards</button>
             <button @click="activeTab = 'file'" :class="activeTab === 'file' ? 'bg-gray-800 text-white dark:bg-gray-200 dark:text-gray-900' : 'bg-gray-100 text-gray-600 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700'" class="px-3.5 py-1.5 text-xs font-medium rounded-full transition-all shrink-0">Files</button>
          </div>
        </div>

        <!-- Content -->
        <div class="max-h-80 overflow-y-auto">
          <div v-if="filteredNodes.length === 0" class="px-5 py-8 text-center text-sm text-gray-400">
            No resources found
          </div>
          <button
            v-for="node in filteredNodes"
            :key="node.id"
            @click="$emit('select', node)"
            class="w-full flex items-center gap-3 px-5 py-3 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors text-left border-b border-gray-50 dark:border-gray-800/50 last:border-b-0"
          >
            <div class="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0"
                 :class="[
                   node.node_type === 'whiteboard' ? 'bg-purple-50 dark:bg-purple-900/20' : 
                   node.node_type === 'file' ? 'bg-emerald-50 dark:bg-emerald-900/20' : 
                   'bg-blue-50 dark:bg-blue-900/20'
                 ]">
              <Palette v-if="node.node_type === 'whiteboard'" class="w-4 h-4 text-purple-600 dark:text-purple-400" />
              <File v-else-if="node.node_type === 'file'" class="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
              <FileText v-else class="w-4 h-4 text-blue-600 dark:text-blue-400" />
            </div>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">{{ node.title || (node.node_type === 'whiteboard' ? 'Untitled Whiteboard' : node.node_type === 'file' ? 'Unnamed File' : 'Untitled Note') }}</div>
              <div class="text-xs text-gray-400 truncate mt-0.5" v-if="node.node_type === 'file'">
                  {{ node.id }}
              </div>
              <div class="text-xs text-gray-400 truncate mt-0.5" v-else>
                  {{ node.content ? node.content.replace(/<[^>]+>/g, '').substring(0, 60) : 'Empty ' + (node.node_type === 'whiteboard' ? 'whiteboard' : 'note') }}
              </div>
            </div>
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { Link as LinkIcon, Search, X, FileText, File, Palette } from 'lucide-vue-next';

interface NodeItem {
  id: string;
  title: string;
  content?: string;
  node_type?: string;
}

const props = defineProps<{
  show: boolean;
  availableNodes: NodeItem[];
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'select', node: NodeItem): void;
}>();

const searchInput = ref<HTMLInputElement | null>(null);
const searchQuery = ref('');
const activeTab = ref('all');

const filteredNodes = computed(() => {
  let list = props.availableNodes;
  
  if (activeTab.value !== 'all') {
    list = list.filter(n => (n.node_type || 'note') === activeTab.value);
  }
  
  if (!searchQuery.value) return list;
  
  const q = searchQuery.value.toLowerCase();
  return list.filter(n => 
    n.title?.toLowerCase().includes(q) || 
    (n.content && n.content.toLowerCase().includes(q))
  );
});

watch(() => props.show, (newVal) => {
  if (newVal) {
    searchQuery.value = '';
    activeTab.value = 'all';
    nextTick(() => {
      searchInput.value?.focus();
    });
  }
});
</script>

<style scoped>
.hide-scrollbar::-webkit-scrollbar {
    display: none;
}
.hide-scrollbar {
    -ms-overflow-style: none;
    scrollbar-width: none;
}
.animate-in {
  animation: fadeIn 0.2s ease-out;
}
@keyframes fadeIn {
  from { opacity: 0; transform: scale(0.95); }
  to { opacity: 1; transform: scale(1); }
}
</style>
