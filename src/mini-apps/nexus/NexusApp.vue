<script setup lang="ts">
import { ref, onMounted, watch, onBeforeUnmount } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { Search, FileText, CheckSquare, Zap, X, ChevronRight, Tag, File, Calendar, PenTool, Users } from 'lucide-vue-next';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import GraphView from './components/GraphView.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import { logger } from '../../utils/logger';

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
    status?: string;
}

interface SearchResult {
    id: string;
    item_type: string;
    title: string;
    snippet: string;
    tags: string[];
    date: string;
    path: string;
    score: number;
    status?: string;
}

interface SearchResponse {
    results: SearchResult[];
    total_count: number;
    query_time_ms: number;
}

interface GraphNode {
    id: string;
    item_type: string;
    title: string;
    tags: string[];
}

interface GraphLink {
    source: string;
    target: string;
}

interface GraphData {
    nodes: GraphNode[];
    links: GraphLink[];
}

const allItems = ref<NexusItem[]>([]);
const graphData = ref<GraphData | null>(null);
const searchResults = ref<SearchResult[]>([]);
const searchQuery = ref('');
const isSearching = ref(false);
const queryTimeMs = ref(0);
const totalCount = ref(0);
const showSyntaxHints = ref(false);
const caseSensitive = ref(false);

const hideSyntaxHints = () => {
    setTimeout(() => {
        showSyntaxHints.value = false;
    }, 200);
};

const selectedItem = ref<NexusItem | null>(null);

let searchTimeout: ReturnType<typeof setTimeout>;

const loadAllData = async () => {
    try {
        const [items, data] = await Promise.all([
            invoke<NexusItem[]>('get_nexus_items', { vaultPath: props.vaultPath }),
            invoke<GraphData>('get_nexus_graph_data', { vaultPath: props.vaultPath })
        ]);
        allItems.value = items;
        graphData.value = data;
    } catch (e) {
        logger.error("Failed to load nexus data", e);
    }
};

let currentSearchId = 0;

const performSearch = async () => {
    if (!searchQuery.value.trim()) {
        searchResults.value = [];
        totalCount.value = 0;
        queryTimeMs.value = 0;
        return;
    }

    isSearching.value = true;
    const searchId = ++currentSearchId;
    try {
        const response = await invoke<SearchResponse>('search_nexus', { 
            vaultPath: props.vaultPath, 
            query: searchQuery.value,
            caseSensitive: caseSensitive.value,
        });
        if (searchId === currentSearchId) {
            searchResults.value = response.results;
            totalCount.value = response.total_count;
            queryTimeMs.value = response.query_time_ms;
        }
    } catch(e) { logger.error(String(e)); } finally {
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

watch(caseSensitive, () => {
    if (searchQuery.value.trim()) performSearch();
});

let unlistenMod: UnlistenFn;
let unlistenDel: UnlistenFn;

onMounted(async () => {
    loadAllData();
    unlistenMod = await listen('vault-file-modified', () => {
        loadAllData();
        if (searchQuery.value) performSearch();
    });
    unlistenDel = await listen('vault-file-created-deleted', () => {
        loadAllData();
        if (searchQuery.value) performSearch();
    });
});

onBeforeUnmount(() => {
    if (unlistenMod) unlistenMod();
    if (unlistenDel) unlistenDel();
});

const getTypeIcon = (type: string) => {
    if (type === 'note') return FileText;
    if (type === 'task') return CheckSquare;
    if (type === 'quickcap') return Zap;
    if (type === 'file') return File;
    if (type === 'event') return Calendar;
    if (type === 'tag') return Tag;
    if (type === 'whiteboard') return PenTool;
    if (type === 'person') return Users;
    return FileText;
};

const getTypeColor = (type: string) => {
    if (type === 'note') return 'text-blue-600 bg-blue-100 dark:bg-blue-500/20 dark:text-blue-400';
    if (type === 'task') return 'text-emerald-600 bg-emerald-100 dark:bg-emerald-500/20 dark:text-emerald-400';
    if (type === 'quickcap') return 'text-amber-600 bg-amber-100 dark:bg-amber-500/20 dark:text-amber-400';
    if (type === 'file') return 'text-purple-600 bg-purple-100 dark:bg-purple-500/20 dark:text-purple-400';
    if (type === 'event') return 'text-rose-600 bg-rose-100 dark:bg-rose-500/20 dark:text-rose-400';
    if (type === 'tag') return 'text-purple-600 bg-purple-100 dark:bg-purple-500/20 dark:text-purple-400';
    if (type === 'whiteboard') return 'text-violet-600 bg-violet-100 dark:bg-violet-500/20 dark:text-violet-400';
    if (type === 'person') return 'text-orange-600 bg-orange-100 dark:bg-orange-500/20 dark:text-orange-400';
    return 'text-gray-600 bg-gray-100 dark:bg-gray-500/20 dark:text-gray-400';
};

const openPreview = async (item: NexusItem | SearchResult) => {
    if (item.item_type === 'file') {
        try {
            await invoke('open_local_file', { vaultPath: props.vaultPath, path: item.path });
        } catch(e) {
            logger.error("Failed to open file", e);
        }
        return;
    }
    emit('edit-item', item.id, item.item_type);
};

const openPreviewFromGraph = async (node: GraphNode) => {
    if (node.item_type === 'tag') return;
    emit('edit-item', node.id, node.item_type);
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

const cleanSnippet = (snippet: string) => {
    if (!snippet) return '';
    // Replace markdown images: ![alt](url) -> 🖼️ alt
    let text = snippet.replace(/!\[([^\]]*)\]\([^)]+\)/g, '🖼️ $1');
    // Replace HTML images
    text = text.replace(/<img[^>]*>/gi, '🖼️ Image');
    // Sanitize to only allow <mark> tags from FTS5
    return DOMPurify.sanitize(text, { ALLOWED_TAGS: ['mark'] });
};
</script>

<template>
  <div class="h-full w-full flex relative overflow-hidden bg-[#fdfdfc] dark:bg-[#1a1a1c] font-sans">
    
    <!-- Main UI -->
    <div v-show="!selectedItem" class="flex-1 flex flex-col h-full relative transition-all">
        
        <!-- Background Graph View -->
        <div class="absolute inset-0 z-0">
            <GraphView v-if="graphData" :graph-data="graphData" @node-click="openPreviewFromGraph" />
            <div v-else class="w-full h-full flex items-center justify-center">
                <div class="w-8 h-8 rounded-full border-2 border-gray-300 dark:border-gray-600 border-t-transparent animate-spin"></div>
            </div>
        </div>

        <!-- Header / Search OmniBar (Floating) -->
        <div class="absolute top-0 inset-x-0 pt-10 px-8 pb-6 z-20 pointer-events-none">
            <div class="max-w-3xl mx-auto flex items-center gap-6 pointer-events-auto">
                <NavButtons />
                <div class="flex-1 relative group">
                   <div class="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none">
                       <Search class="h-5 w-5 text-gray-400 group-focus-within:text-black dark:group-focus-within:text-white transition-colors" />
                   </div>
                   <input 
                       v-model="searchQuery" 
                       type="text" 
                       @focus="showSyntaxHints = true"
                       @blur="hideSyntaxHints"
                       class="block w-full pl-12 pr-20 py-3.5 text-lg font-medium border border-gray-200 dark:border-[#2c2c2e] rounded-2xl bg-white/80 dark:bg-[#242426]/80 backdrop-blur-xl text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-black/10 focus:border-black dark:focus:ring-white/10 dark:focus:border-white transition-all shadow-lg" 
                       placeholder="Universal Search... (e.g. is:task #urgent)" 
                   />
                   <div class="absolute inset-y-0 right-0 flex items-center gap-0.5 pr-3">
                       <button
                           @click="caseSensitive = !caseSensitive"
                           :class="[
                               'w-7 h-7 flex items-center justify-center rounded-md text-xs font-bold font-mono transition-all',
                               caseSensitive
                                   ? 'bg-black text-white dark:bg-white dark:text-black shadow-sm'
                                   : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10'
                           ]"
                           title="Case Sensitive"
                       >Aa</button>
                       <button v-if="searchQuery" @click="searchQuery = ''" class="w-7 h-7 flex items-center justify-center cursor-pointer">
                           <X class="h-4 w-4 text-gray-400 hover:text-black dark:hover:text-white transition-colors" />
                       </button>
                   </div>

                   <!-- Search Syntax Hints Dropdown -->
                   <div v-if="showSyntaxHints && !searchQuery" class="absolute top-full left-0 right-0 mt-2 p-4 bg-white dark:bg-[#242426] border border-gray-200 dark:border-[#2c2c2e] rounded-xl shadow-xl z-50">
                       <p class="text-xs font-bold text-gray-500 dark:text-gray-400 mb-3 tracking-wider uppercase">Search Syntax</p>
                       <div class="grid grid-cols-2 gap-2 text-xs">
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">is:note</code><span class="text-gray-500">Filter by type</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">#tag</code><span class="text-gray-500">Filter by tag</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">"exact phrase"</code><span class="text-gray-500">Phrase match</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">-word</code><span class="text-gray-500">Exclude term</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">in:title</code><span class="text-gray-500">Title only</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">status:done</code><span class="text-gray-500">Task status</span></div>
                       </div>
                   </div>
                </div>
            </div>
        </div>

        <!-- Search Results Overlay -->
        <div v-if="searchQuery" class="absolute inset-0 z-10 pt-32 px-8 pb-16 bg-[#fdfdfc]/90 dark:bg-[#1a1a1c]/90 backdrop-blur-md overflow-y-auto animate-in fade-in duration-200">
            <div class="max-w-3xl mx-auto">
                <div v-if="isSearching" class="text-center py-10 opacity-50 flex items-center justify-center gap-2">
                    <div class="w-5 h-5 rounded-full border-2 border-black dark:border-white border-t-transparent animate-spin"></div>
                </div>
                
                <div v-else-if="searchResults.length === 0" class="text-center py-16">
                    <div class="w-20 h-20 bg-gray-50 dark:bg-white/5 rounded-full flex flex-col items-center justify-center mx-auto mb-4 border border-dashed border-gray-200 dark:border-white/10">
                        <Search class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                    </div>
                    <p class="text-[#52525b] dark:text-[#a1a1aa] font-medium">No results found for "{{ searchQuery }}"</p>
                </div>
                
                <div v-else class="space-y-3">
                    <div class="flex items-center justify-between mb-4">
                        <h3 class="text-sm font-bold text-gray-500 dark:text-gray-400">Search Results</h3>
                        <div class="flex items-center gap-3">
                            <span class="text-xs font-mono text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-200 dark:border-emerald-500/20">{{ queryTimeMs }}ms</span>
                            <span class="text-xs font-semibold text-gray-400">{{ totalCount }} items</span>
                        </div>
                    </div>

                    <div v-for="item in searchResults" :key="item.id"
                         @click="openPreview(item)"
                         class="group flex gap-4 p-4 rounded-2xl bg-white dark:bg-[#242426] border border-gray-100 dark:border-[#2c2c2e] hover:border-indigo-300 dark:hover:border-indigo-500/50 shadow-sm hover:shadow-md cursor-pointer transition-all active:scale-[0.99]"
                    >
                        <!-- Icon Badge -->
                        <div class="flex-shrink-0 mt-1">
                            <div class="w-10 h-10 rounded-xl flex items-center justify-center shadow-inner" :class="getTypeColor(item.item_type)">
                                <component :is="getTypeIcon(item.item_type)" class="w-5 h-5 stroke-[1.5]" />
                            </div>
                        </div>

                        <!-- Content -->
                        <div class="flex-1 min-w-0 flex flex-col justify-center">
                            <div class="flex items-start justify-between gap-4 mb-1">
                                <h4 class="font-bold text-[15px] text-[#1c1c1e] dark:text-[#f4f4f5] truncate group-hover:text-indigo-600 dark:group-hover:text-indigo-400 transition-colors">{{ item.title }}</h4>
                                <span class="flex-shrink-0 text-[10px] font-bold text-gray-400 flex items-center gap-1 bg-gray-50 dark:bg-[#1a1a1c] px-2 py-0.5 rounded-md border border-gray-100 dark:border-[#2c2c2e]">
                                    {{ item.date.split(' ')[0] }}
                                </span>
                            </div>
                            
                            <p v-if="item.item_type !== 'file'" class="text-[13px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-2 leading-relaxed preview-markdown break-words" v-html="cleanSnippet(item.snippet)"></p>
                            <p v-else class="text-[13px] text-purple-600/70 dark:text-purple-400/70 font-mono break-words" v-html="cleanSnippet(item.snippet)"></p>
                            
                            <div class="flex items-center gap-2 mt-3" v-if="item.tags.length > 0">
                                <span v-for="tag in item.tags" :key="tag" class="text-[10px] font-bold px-2 py-0.5 rounded bg-gray-100 dark:bg-[#1a1a1c] border border-gray-200 dark:border-[#2c2c2e] text-gray-600 dark:text-gray-400 flex items-center gap-1">
                                    <span class="opacity-50">#</span>{{ tag.split('/').pop() }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <!-- Full-page Preview Panel (Unchanged logic, floating on top when active) -->
    <div v-if="selectedItem" class="absolute inset-0 bg-[#fdfdfc] dark:bg-[#1a1a1c] flex flex-col z-30 animate-in fade-in zoom-in-95 duration-200">
        <!-- Header -->
        <div class="h-16 border-b border-gray-200 dark:border-[#2c2c2e] flex items-center justify-between px-6 flex-shrink-0 bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md">
            <div class="flex items-center gap-4">
                <button @click="closePreview" class="p-2 -ml-2 rounded-xl hover:bg-gray-100 dark:hover:bg-[#3a3a3c] text-gray-500 transition-colors flex items-center gap-1 group">
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
            
            <button @click="emit('edit-item', selectedItem.id, selectedItem.item_type)" class="px-4 py-2 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg text-sm font-bold transition-all active:scale-95 flex items-center gap-2 shadow-sm">
                <component :is="getTypeIcon(selectedItem.item_type)" class="w-4 h-4" /> Edit Source
            </button>
        </div>

        <!-- Content Area -->
        <div class="flex-1 overflow-y-auto px-8 sm:px-16 md:px-32 py-12">
            <div class="max-w-4xl mx-auto">
                <h2 class="text-4xl font-extrabold text-[#1c1c1e] dark:text-white mb-6 leading-tight tracking-tight">{{ selectedItem.title }}</h2>
                
                <div class="flex flex-wrap gap-2 mb-10" v-if="selectedItem.tags.length">
                    <span v-for="tag in selectedItem.tags" :key="tag" class="text-xs font-medium px-2.5 py-1 rounded bg-gray-100 dark:bg-[#2c2c2e] text-gray-700 dark:text-gray-300 flex items-center gap-1 border border-gray-200 dark:border-[#3a3a3c]">
                        <span class="opacity-50">#</span>{{ tag.split('/').pop() }}
                    </span>
                </div>

                <div class="prose prose-lg dark:prose-invert prose-zinc max-w-none leading-loose preview-markdown" v-html="renderMarkdownPreview(selectedItem.content, selectedItem.item_type)">
                </div>
                
                <div class="mt-16 p-4 bg-gray-50 dark:bg-[#242426] rounded-xl border border-gray-200 dark:border-[#2c2c2e]">
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
    max-height: 120px;
    margin: 8px 0;
    border-radius: 8px;
    box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1);
}
.preview-markdown :deep(input[type="checkbox"]) {
    margin-right: 6px;
    accent-color: #10b981;
}
/* FTS5 search highlight */
.preview-markdown :deep(mark) {
    background: rgba(250, 204, 21, 0.3);
    color: inherit;
    border-radius: 2px;
    padding: 0 2px;
}
@media (prefers-color-scheme: dark) {
    .preview-markdown :deep(mark) {
        background: rgba(250, 204, 21, 0.2);
    }
}
</style>
