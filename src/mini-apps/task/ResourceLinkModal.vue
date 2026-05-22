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
        <div class="px-4 py-2.5 border-b border-gray-100 dark:border-gray-800 flex items-center gap-2">
          <div class="relative flex-1">
            <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
            <input
              ref="searchInput"
              v-model="searchQuery"
              placeholder="Search resources..."
              class="w-full pl-9 pr-3 py-2 text-sm bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700 rounded-lg outline-none focus:ring-2 focus:ring-emerald-500/30 focus:border-emerald-500 text-gray-800 dark:text-gray-200 placeholder:text-gray-400 transition-all"
              @keydown.escape="$emit('close')"
            />
          </div>
          <!-- Type Filter Placeholder for future (Files, Events) -->
          <div class="flex items-center bg-gray-100 dark:bg-gray-800 rounded-lg p-1">
             <button class="px-3 py-1 text-xs font-medium rounded-md bg-white shadow-sm dark:bg-[#333] text-gray-800 dark:text-gray-200">Notes</button>
             <!-- <button class="px-3 py-1 text-xs font-medium rounded-md text-gray-500 hover:text-gray-700">Files</button> -->
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
            <div class="w-8 h-8 rounded-lg bg-emerald-50 dark:bg-emerald-900/20 flex items-center justify-center flex-shrink-0">
              <FileText v-if="node.node_type === 'note'" class="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
              <!-- Other icons for files, events can be added here later -->
              <File v-else class="w-4 h-4 text-gray-500" />
            </div>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">{{ node.title }}</div>
              <div class="text-xs text-gray-400 truncate mt-0.5" v-if="node.node_type === 'note'">
                  {{ node.content?.substring(0, 60) || 'Empty note' }}
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
import { Link as LinkIcon, Search, X, FileText, File } from 'lucide-vue-next';

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

const filteredNodes = computed(() => {
  if (!searchQuery.value) return props.availableNodes;
  const q = searchQuery.value.toLowerCase();
  return props.availableNodes.filter(n => 
    n.title.toLowerCase().includes(q) || 
    (n.content && n.content.toLowerCase().includes(q))
  );
});

watch(() => props.show, (newVal) => {
  if (newVal) {
    searchQuery.value = '';
    nextTick(() => {
      searchInput.value?.focus();
    });
  }
});
</script>

<style scoped>
.animate-in {
  animation: fadeIn 0.2s ease-out;
}
@keyframes fadeIn {
  from { opacity: 0; transform: scale(0.95); }
  to { opacity: 1; transform: scale(1); }
}
</style>
