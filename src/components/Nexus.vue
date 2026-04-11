<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { Search, FileText, CheckSquare, Zap, Clock, X, ChevronRight, Globe, Tag, File } from 'lucide-vue-next';
import { marked } from 'marked';
import DOMPurify from 'dompurify';

const emit = defineEmits<{
    (e: 'edit-item', id: string, type: string): void
}>();

const props = defineProps<{
    vaultPath: string;
}>();

interface NexusItem {
    id: string;
    item_type: string;
    title: string;
    preview: string;
    tags: string[];
    date: string;
    path: string;
    content: string;
}

interface TagStat {
    name: string;
    total_count: number;
    distribution: Record<string, number>;
}

interface VaultStats {
    total_items: number;
    type_distribution: Record<string, number>;
    tags: TagStat[];
}

const items = ref<NexusItem[]>([]);
const vaultStats = ref<VaultStats | null>(null);
const searchQuery = ref('');
const isSearching = ref(false);

const selectedItem = ref<NexusItem | null>(null);

let searchTimeout: ReturnType<typeof setTimeout>;

const fetchStats = async () => {
    try {
        vaultStats.value = await invoke<VaultStats>('get_nexus_stats', { vaultPath: props.vaultPath });
    } catch(e) {
        console.error("Failed to fetch nexus stats", e);
    }
};

let currentSearchId = 0;

const performSearch = async () => {
    isSearching.value = true;
    const searchId = ++currentSearchId;
    try {
        const results = await invoke<NexusItem[]>('search_nexus', { 
            vaultPath: props.vaultPath, 
            query: searchQuery.value 
        });
        if (searchId === currentSearchId) {
            items.value = results;
        }
    } catch(e) {
        console.error(e);
    } finally {
        if (searchId === currentSearchId) {
            isSearching.value = false;
        }
    }
};

watch(searchQuery, () => {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
        performSearch();
    }, 250);
});

onMounted(() => {
    performSearch();
    fetchStats();
});

const getTypeIcon = (type: string) => {
    if (type === 'note') return FileText;
    if (type === 'task') return CheckSquare;
    if (type === 'quickcap') return Zap;
    if (type === 'file') return File;
    return FileText;
};

const getTypeColor = (type: string) => {
    if (type === 'note') return 'text-blue-600 bg-blue-100 dark:bg-blue-500/20 dark:text-blue-400';
    if (type === 'task') return 'text-emerald-600 bg-emerald-100 dark:bg-emerald-500/20 dark:text-emerald-400';
    if (type === 'quickcap') return 'text-amber-600 bg-amber-100 dark:bg-amber-500/20 dark:text-amber-400';
    if (type === 'file') return 'text-purple-600 bg-purple-100 dark:bg-purple-500/20 dark:text-purple-400';
    return 'text-gray-600 bg-gray-100 dark:bg-gray-500/20 dark:text-gray-400';
};

const openPreview = async (item: NexusItem) => {
    if (item.item_type === 'file') {
        try {
            await invoke('open_local_file', { path: item.path });
        } catch(e) {
            console.error("Failed to open file", e);
        }
        return;
    }
    selectedItem.value = item;
};

const closePreview = () => {
    selectedItem.value = null;
};

const renderMarkdownPreview = (text: string, type: string) => {
    if (!text) return '';
    
    let parsed = text;
    // Strip frontmatter if present (only for notes/tasks)
    if (type !== 'quickcap' && text.startsWith('---\n')) {
        const splitIdx = text.indexOf('---', 3);
        if (splitIdx > 0) {
            parsed = text.substring(splitIdx + 3).trim();
        }
    }
    
    // Convert relative asset links so they load properly in preview
    parsed = parsed.replace(/!\[.*?\]\((.*?)\)/g, (_match, path) => {
        let absPath = path;
        if (path.startsWith('assets/')) {
            absPath = `${props.vaultPath}/${path}`;
        }
        const src = convertFileSrc(absPath);
        return `![image](${src})`;
    });
    
    const html = marked.parse(parsed, { async: false, breaks: true }) as string;
    return DOMPurify.sanitize(html);
};
</script>

<template>
  <div class="h-full w-full flex relative overflow-hidden bg-[#fdfdfc] dark:bg-[#121212] font-sans">
    
    <!-- Main UI -->
    <div v-show="!selectedItem" class="flex-1 flex flex-col h-full bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/40 to-[#fdfdfc] dark:from-indigo-900/10 dark:to-[#121212] transition-all">
        
        <!-- Header / Search -->
        <div class="relative w-full max-w-3xl mx-auto pt-16 px-8 pb-8 flex-shrink-0">
            <div class="text-center mb-10 mt-6">
                <div class="w-16 h-16 bg-white dark:bg-[#1a1a1a] shadow-sm border border-gray-200 dark:border-[#2c2c2c] rounded-2xl flex items-center justify-center mx-auto mb-6 transform -rotate-6">
                    <Globe class="w-8 h-8 text-black dark:text-white" />
                </div>
                <h1 class="text-4xl font-bold text-[#1c1c1e] dark:text-[#f4f4f5] tracking-tight">
                    Nexus
                </h1>
                <p class="text-[#52525b] dark:text-[#a1a1aa] mt-3 max-w-md mx-auto">Omni-search across all your Notes, Tasks, QuickCaps, and Files in an instant.</p>
            </div>

            <div class="relative group max-w-2xl mx-auto">
               <div class="absolute inset-y-0 left-0 pl-5 flex items-center pointer-events-none">
                   <Search class="h-6 w-6 text-gray-400 group-focus-within:text-black dark:group-focus-within:text-white transition-colors delay-75" />
               </div>
               <input 
                   v-model="searchQuery" 
                   type="text" 
                   class="block w-full pl-14 pr-12 py-5 text-xl font-medium border border-gray-200 dark:border-[#2c2c2c]/80 rounded-[24px] bg-white dark:bg-[#1a1a1a] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-4 focus:ring-black/5 focus:border-black dark:focus:ring-white/10 dark:focus:border-white transition-all shadow-[0_8px_30px_rgb(0,0,0,0.04)]" 
                   placeholder="Type anything or #tag..." 
               />
               <button v-if="searchQuery" @click="searchQuery = ''" class="absolute inset-y-0 right-0 pr-5 flex items-center cursor-pointer">
                   <X class="h-6 w-6 text-gray-400 hover:text-black dark:hover:text-white transition-colors" />
               </button>
            </div>
        </div>

        <!-- Dashboard Section -->
        <div v-if="!searchQuery && vaultStats" class="w-full max-w-2xl mx-auto px-1 mb-8 animate-in fade-in slide-in-from-bottom-2 duration-500 shrink-0">
            <!-- 4 Cards -->
            <div class="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-8">
                <div class="bg-white dark:bg-[#1a1a1a] p-4 rounded-2xl border border-gray-100 dark:border-[#2c2c2c] shadow-sm flex flex-col justify-center items-center">
                   <div class="text-3xl font-black text-gray-800 dark:text-gray-100 mb-1">{{ vaultStats.total_items }}</div>
                   <div class="text-[11px] font-bold text-gray-400 uppercase tracking-wider">Total Items</div>
                </div>
                <div class="bg-blue-50/50 dark:bg-blue-500/10 p-4 rounded-2xl border border-blue-100 dark:border-blue-500/20 shadow-sm flex flex-col justify-center items-center">
                   <div class="text-3xl font-black text-blue-600 dark:text-blue-400 mb-1">{{ vaultStats.type_distribution['note'] || 0 }}</div>
                   <div class="text-[11px] font-bold text-blue-400/80 dark:text-blue-500/80 uppercase tracking-wider">Notes</div>
                </div>
                <div class="bg-emerald-50/50 dark:bg-emerald-500/10 p-4 rounded-2xl border border-emerald-100 dark:border-emerald-500/20 shadow-sm flex flex-col justify-center items-center">
                   <div class="text-3xl font-black text-emerald-600 dark:text-emerald-400 mb-1">{{ vaultStats.type_distribution['task'] || 0 }}</div>
                   <div class="text-[11px] font-bold text-emerald-400/80 dark:text-emerald-500/80 uppercase tracking-wider">Tasks</div>
                </div>
                <div class="bg-amber-50/50 dark:bg-amber-500/10 p-4 rounded-2xl border border-amber-100 dark:border-amber-500/20 shadow-sm flex flex-col justify-center items-center">
                   <div class="text-3xl font-black text-amber-600 dark:text-amber-400 mb-1">{{ vaultStats.type_distribution['quickcap'] || 0 }}</div>
                   <div class="text-[11px] font-bold text-amber-400/80 dark:text-amber-500/80 uppercase tracking-wider">QuickCaps</div>
                </div>
            </div>

            <!-- Tag Cloud -->
            <div class="p-6 bg-white dark:bg-[#1a1a1a] rounded-3xl border border-gray-100 dark:border-[#2c2c2c] shadow-sm">
                <h3 class="text-[11px] font-bold text-gray-400 dark:text-gray-500 uppercase tracking-widest mb-5 flex items-center gap-2">
                    <Tag class="w-3.5 h-3.5" /> Taxonomy Cloud
                </h3>
                <div class="flex flex-wrap gap-2.5 max-h-[180px] overflow-y-auto pr-2">
                    <button v-for="tag in vaultStats.tags" :key="tag.name"
                        @click="searchQuery = '#' + tag.name"
                        class="group flex items-center gap-2 px-3 py-1.5 rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50/50 dark:bg-[#222] hover:bg-black dark:hover:bg-white hover:border-black dark:hover:border-white transition-all cursor-pointer">
                        <span class="text-[13px] font-semibold text-gray-600 dark:text-gray-300 group-hover:text-white dark:group-hover:text-black">#{{ tag.name }}</span>
                        <div class="flex items-center gap-1">
                           <span class="text-[10px] font-bold text-white bg-gray-300 dark:bg-gray-600 group-hover:bg-gray-700/50 dark:group-hover:bg-gray-200 px-1.5 py-0.5 rounded min-w-[20px] text-center inline-block leading-none transition-colors">
                               {{ tag.total_count }}
                           </span>
                        </div>
                    </button>
                    <div v-if="vaultStats.tags.length === 0" class="text-[13px] text-gray-400 italic">No tags populated in your vault yet.</div>
                </div>
            </div>
        </div>

        <!-- Results Stream -->
        <div class="flex-1 overflow-y-auto px-8 pb-16 scroll-smooth">
            <div class="max-w-2xl mx-auto">
                <div v-if="isSearching" class="text-center py-10 opacity-50 flex items-center justify-center gap-2">
                    <div class="w-5 h-5 rounded-full border-2 border-black dark:border-white border-t-transparent animate-spin"></div>
                </div>
                
                <div v-else-if="items.length === 0" class="text-center py-16">
                    <div class="w-20 h-20 bg-gray-50 dark:bg-white/5 rounded-full flex flex-col items-center justify-center mx-auto mb-4 border border-dashed border-gray-200 dark:border-white/10">
                        <Search class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                    </div>
                    <p class="text-[#52525b] dark:text-[#a1a1aa] font-medium">No results found for "{{ searchQuery }}"</p>
                </div>
                
                <div v-else class="space-y-4">
                    <h3 class="text-[11px] font-bold text-gray-400 dark:text-gray-500 uppercase tracking-widest mb-6 px-1 flex items-center gap-2" v-if="!searchQuery">
                        <Clock class="w-3.5 h-3.5" /> Recent Activity
                    </h3>
                    
                    <div v-for="item in items" :key="item.id"
                         @click="openPreview(item)"
                         class="group flex gap-5 p-5 rounded-2xl bg-white dark:bg-[#1a1a1a] border border-gray-100 dark:border-[#2c2c2c]/80 hover:border-gray-300 dark:hover:border-[#404040] shadow-sm hover:shadow-md cursor-pointer transition-all active:scale-[0.99]"
                         :class="{'ring-2 ring-black/10 border-black/20 dark:border-white/20 dark:ring-white/10': selectedItem?.id === item.id}"
                    >
                        <!-- Icon Badge -->
                        <div class="flex-shrink-0 mt-0.5">
                            <div class="w-12 h-12 rounded-xl flex items-center justify-center shadow-inner" :class="getTypeColor(item.item_type)">
                                <component :is="getTypeIcon(item.item_type)" class="w-6 h-6 stroke-[1.5]" />
                            </div>
                        </div>

                        <!-- Content -->
                        <div class="flex-1 min-w-0 flex flex-col justify-center">
                            <div class="flex items-start justify-between gap-4 mb-2">
                                <h4 class="font-bold text-[16px] text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ item.title }}</h4>
                                <span class="flex-shrink-0 text-[11px] font-semibold tracking-wider text-gray-400 dark:text-gray-500 flex items-center gap-1 bg-gray-50 dark:bg-white/5 px-2 py-1 rounded-md">
                                    {{ item.date.split(' ')[0] }}
                                </span>
                            </div>
                            
                            <p v-if="item.item_type !== 'file'" class="text-[14px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-2 leading-relaxed preview-markdown break-words" v-html="renderMarkdownPreview(item.preview, item.item_type)"></p>
                            <p v-else class="text-[14px] text-purple-600/70 dark:text-purple-400/70 font-mono">{{ item.preview }}</p>
                            
                            <div class="flex items-center gap-2 mt-4" v-if="item.tags.length > 0">
                                <span v-for="tag in item.tags" :key="tag" class="text-[11px] font-medium px-2.5 py-1 rounded-md bg-gray-100 dark:bg-white/5 text-gray-600 dark:text-gray-400 flex items-center gap-1">
                                    <span class="opacity-50">#</span>{{ tag.split('/').pop() }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <!-- Full-page Preview Panel -->
    <div v-if="selectedItem" class="absolute inset-0 bg-[#fdfdfc] dark:bg-[#121212] flex flex-col z-30 animate-in fade-in zoom-in-95 duration-200">
        
        <!-- Header -->
        <div class="h-16 border-b border-gray-200 dark:border-[#2c2c2c] flex items-center justify-between px-6 flex-shrink-0 bg-white/80 dark:bg-[#1a1a1a]/80 backdrop-blur-md">
            <div class="flex items-center gap-4">
                <button @click="closePreview" class="p-2 -ml-2 rounded-xl hover:bg-gray-100 dark:hover:bg-white/10 text-gray-500 transition-colors flex items-center gap-1 group">
                    <ChevronRight class="w-5 h-5 rotate-180 transition-transform group-hover:-translate-x-0.5" /> <span class="text-sm font-semibold">Back</span>
                </button>
                <div class="h-4 w-px bg-gray-300 dark:bg-[#444]"></div>
                <div class="flex items-center gap-2">
                    <div class="p-1.5 rounded-lg" :class="getTypeColor(selectedItem.item_type)">
                        <component :is="getTypeIcon(selectedItem.item_type)" class="w-4 h-4" />
                    </div>
                    <span class="text-xs font-bold tracking-widest text-gray-800 dark:text-gray-200 uppercase">{{ selectedItem.item_type }}</span>
                </div>
            </div>
            
            <button @click="emit('edit-item', selectedItem.id, selectedItem.item_type)" class="px-4 py-2 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg text-sm font-medium transition-all active:scale-95 flex items-center gap-2 shadow-sm border border-transparent">
                <component :is="getTypeIcon(selectedItem.item_type)" class="w-4 h-4" /> Edit Source
            </button>
        </div>

        <!-- Content Area -->
        <div class="flex-1 overflow-y-auto px-8 sm:px-16 md:px-32 py-12">
            <div class="max-w-4xl mx-auto">
                <h2 class="text-4xl font-extrabold text-[#1c1c1e] dark:text-white mb-6 leading-tight tracking-tight">{{ selectedItem.title }}</h2>
                
                <div class="flex flex-wrap gap-2 mb-10" v-if="selectedItem.tags.length">
                    <span v-for="tag in selectedItem.tags" :key="tag" class="text-xs font-medium px-2.5 py-1 rounded bg-gray-100 dark:bg-white/10 text-gray-700 dark:text-gray-300 flex items-center gap-1">
                        <span class="opacity-50">#</span>{{ tag.split('/').pop() }}
                    </span>
                </div>

                <div class="prose prose-lg dark:prose-invert prose-zinc max-w-none leading-loose preview-markdown" v-html="renderMarkdownPreview(selectedItem.content, selectedItem.item_type)">
                </div>
                
                <div class="mt-16 p-4 bg-gray-50 dark:bg-[#222] rounded-xl border border-gray-200 dark:border-[#333]">
                    <div class="text-xs font-medium text-gray-500 flex justify-between items-center">
                        <div class="flex-1 min-w-0">
                            <span class="block opacity-70 mb-1">Source Path:</span>
                            <code class="block truncate text-gray-800 dark:text-gray-300">{{ selectedItem.path }}</code>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>

  </div>
</template>

<style scoped>
.preview-markdown :deep(img) {
    display: inline-block;
    max-height: 80px;
    margin: 4px 0;
    border-radius: 6px;
}
</style>
