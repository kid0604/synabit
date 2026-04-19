<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { Search, FileText, CheckSquare, Zap, Clock, X, ChevronRight, Globe, Tag, File, LayoutGrid, Inbox, Calendar, ArrowRight } from 'lucide-vue-next';
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
    status?: string;
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

// Dashboard specific state
const todaysTasks = ref<NexusItem[]>([]);
const todaysEvents = ref<NexusItem[]>([]);
const inboxQuickCaps = ref<NexusItem[]>([]);
const recentActivity = ref<NexusItem[]>([]);

let searchTimeout: ReturnType<typeof setTimeout>;

const fetchStats = async () => {
    try {
        vaultStats.value = await invoke<VaultStats>('get_nexus_stats', { vaultPath: props.vaultPath });
    } catch(e) {
        console.error("Failed to fetch nexus stats", e);
    }
};

const loadDashboardData = async () => {
    try {
        const allItems = await invoke<NexusItem[]>('search_nexus', { vaultPath: props.vaultPath, query: '' });
        recentActivity.value = allItems.filter(i => i.item_type !== 'quickcap').slice(0, 10);
        
        const tasks = await invoke<NexusItem[]>('search_nexus', { vaultPath: props.vaultPath, query: 'is:task' });
        // Filter out completed tasks using the explicit status field
        todaysTasks.value = tasks.filter(t => t.status !== 'done').slice(0, 5);

        const events = await invoke<NexusItem[]>('search_nexus', { vaultPath: props.vaultPath, query: 'is:event' });
        
        // Filter for upcoming events (date >= today) and sort nearest first
        const todayStr = new Date().toISOString().split('T')[0];
        const upcomingEvents = events.filter(e => {
            const eventDate = e.date.split(' ')[0];
            return eventDate >= todayStr;
        }).sort((a, b) => a.date.localeCompare(b.date));

        todaysEvents.value = upcomingEvents.slice(0, 3);

        const qcs = await invoke<NexusItem[]>('search_nexus', { vaultPath: props.vaultPath, query: 'is:quickcap' });
        // Inbox = quickcaps with no tags
        inboxQuickCaps.value = qcs.filter(q => q.tags.length === 0).slice(0, 10);
    } catch (e) {
        console.error(e);
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
    loadDashboardData();
});

const getTypeIcon = (type: string) => {
    if (type === 'note') return FileText;
    if (type === 'task') return CheckSquare;
    if (type === 'quickcap') return Zap;
    if (type === 'file') return File;
    if (type === 'event') return Calendar;
    return FileText;
};

const getTypeColor = (type: string) => {
    if (type === 'note') return 'text-blue-600 bg-blue-100 dark:bg-blue-500/20 dark:text-blue-400';
    if (type === 'task') return 'text-emerald-600 bg-emerald-100 dark:bg-emerald-500/20 dark:text-emerald-400';
    if (type === 'quickcap') return 'text-amber-600 bg-amber-100 dark:bg-amber-500/20 dark:text-amber-400';
    if (type === 'file') return 'text-purple-600 bg-purple-100 dark:bg-purple-500/20 dark:text-purple-400';
    if (type === 'event') return 'text-rose-600 bg-rose-100 dark:bg-rose-500/20 dark:text-rose-400';
    return 'text-gray-600 bg-gray-100 dark:bg-gray-500/20 dark:text-gray-400';
};

const openPreview = async (item: NexusItem) => {
    if (item.item_type === 'file') {
        try {
            await invoke('open_local_file', { vaultPath: props.vaultPath, path: item.path });
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

const applySearchFilter = (filter: string) => {
    searchQuery.value = filter;
};
</script>

<template>
  <div class="h-full w-full flex relative overflow-hidden bg-[#fdfdfc] dark:bg-[#1a1a1c] font-sans">
    
    <!-- Main UI -->
    <div v-show="!selectedItem" class="flex-1 flex flex-col h-full bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/40 to-[#fdfdfc] dark:from-indigo-900/10 dark:to-[#1a1a1c] transition-all">
        
        <!-- Header / Search OmniBar -->
        <div class="w-full pt-10 px-8 pb-6 flex-shrink-0 z-10 sticky top-0 bg-[#fdfdfc]/80 dark:bg-[#1a1a1c]/80 backdrop-blur-xl border-b border-gray-200/50 dark:border-[#2c2c2e]/50">
            <div class="max-w-5xl mx-auto flex items-center gap-6">
                <div class="flex-shrink-0 flex items-center gap-3">
                    <div class="w-10 h-10 bg-black dark:bg-white rounded-xl flex items-center justify-center shadow-md">
                        <Globe class="w-6 h-6 text-white dark:text-black" />
                    </div>
                    <h1 class="text-2xl font-bold text-[#1c1c1e] dark:text-[#f4f4f5] tracking-tight hidden sm:block">Nexus</h1>
                </div>

                <div class="flex-1 relative group">
                   <div class="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none">
                       <Search class="h-5 w-5 text-gray-400 group-focus-within:text-black dark:group-focus-within:text-white transition-colors" />
                   </div>
                   <input 
                       v-model="searchQuery" 
                       type="text" 
                       class="block w-full pl-12 pr-12 py-3.5 text-lg font-medium border border-gray-200 dark:border-[#2c2c2e] rounded-2xl bg-gray-50/50 dark:bg-[#242426] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-black/10 focus:border-black dark:focus:ring-white/10 dark:focus:border-white transition-all shadow-sm" 
                       placeholder="Omni Search... (e.g. is:task #urgent)" 
                   />
                   <button v-if="searchQuery" @click="searchQuery = ''" class="absolute inset-y-0 right-0 pr-4 flex items-center cursor-pointer">
                       <X class="h-5 w-5 text-gray-400 hover:text-black dark:hover:text-white transition-colors" />
                   </button>
                </div>
            </div>
        </div>

        <!-- Scrollable Content -->
        <div class="flex-1 overflow-y-auto px-8 pb-16 scroll-smooth">
            
            <!-- Dashboard View (When no search query) -->
            <div v-if="!searchQuery" class="max-w-5xl mx-auto mt-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
                <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                    
                    <!-- Left Column: Focus & Activity -->
                    <div class="md:col-span-2 space-y-6">
                        
                        <!-- Today's Focus Bento Box -->
                        <div class="bg-white dark:bg-[#242426] rounded-3xl p-6 border border-gray-100 dark:border-[#2c2c2e] shadow-sm">
                            <div class="flex items-center justify-between mb-6">
                                <h2 class="text-lg font-bold text-gray-800 dark:text-gray-100 flex items-center gap-2">
                                    <LayoutGrid class="w-5 h-5 text-indigo-500" /> Today's Focus
                                </h2>
                                <button @click="applySearchFilter('is:task')" class="text-xs font-semibold text-gray-500 hover:text-indigo-500 dark:text-gray-400 dark:hover:text-indigo-400 flex items-center gap-1">
                                    View All <ArrowRight class="w-3 h-3" />
                                </button>
                            </div>
                            
                            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                <!-- Tasks Column -->
                                <div>
                                    <h3 class="text-xs font-bold text-emerald-600/80 uppercase tracking-widest mb-3 flex items-center gap-2"><CheckSquare class="w-3.5 h-3.5"/> Pending Tasks</h3>
                                    <div class="space-y-2">
                                        <div v-for="task in todaysTasks" :key="task.id" @click="openPreview(task)" class="p-3 rounded-xl bg-gray-50 hover:bg-gray-100 dark:bg-[#1a1a1c] dark:hover:bg-[#2c2c2e] border border-transparent hover:border-gray-200 dark:hover:border-[#333] transition-colors cursor-pointer group">
                                            <div class="font-medium text-sm text-gray-800 dark:text-gray-200 truncate group-hover:text-emerald-600 dark:group-hover:text-emerald-400 transition-colors">{{ task.title }}</div>
                                            <div class="text-xs text-gray-500 dark:text-gray-500 mt-1 flex items-center gap-2">
                                                <span>{{ task.date.split(' ')[0] }}</span>
                                            </div>
                                        </div>
                                        <div v-if="todaysTasks.length === 0" class="text-sm text-gray-400 italic px-2 py-3">No pending tasks found.</div>
                                    </div>
                                </div>
                                
                                <!-- Events Column -->
                                <div>
                                    <h3 class="text-xs font-bold text-rose-600/80 uppercase tracking-widest mb-3 flex items-center gap-2"><Calendar class="w-3.5 h-3.5"/> Events</h3>
                                    <div class="space-y-2">
                                        <div v-for="event in todaysEvents" :key="event.id" @click="openPreview(event)" class="p-3 rounded-xl bg-gray-50 hover:bg-gray-100 dark:bg-[#1a1a1c] dark:hover:bg-[#2c2c2e] border border-transparent hover:border-gray-200 dark:hover:border-[#333] transition-colors cursor-pointer group">
                                            <div class="font-medium text-sm text-gray-800 dark:text-gray-200 truncate group-hover:text-rose-600 dark:group-hover:text-rose-400 transition-colors">{{ event.title }}</div>
                                            <div class="text-xs text-gray-500 dark:text-gray-500 mt-1 flex items-center gap-2">
                                                <span>{{ event.date.split(' ')[0] }}</span>
                                            </div>
                                        </div>
                                        <div v-if="todaysEvents.length === 0" class="text-sm text-gray-400 italic px-2 py-3">No scheduled events.</div>
                                    </div>
                                </div>
                            </div>
                        </div>

                        <!-- Recent Activity Stream -->
                        <div class="bg-white dark:bg-[#242426] rounded-3xl p-6 border border-gray-100 dark:border-[#2c2c2e] shadow-sm">
                            <h2 class="text-lg font-bold text-gray-800 dark:text-gray-100 flex items-center gap-2 mb-6">
                                <Clock class="w-5 h-5 text-blue-500" /> Recent Activity Stream
                            </h2>
                            <div class="space-y-3">
                                <div v-for="item in recentActivity" :key="item.id"
                                     @click="openPreview(item)"
                                     class="flex items-center gap-4 p-3 rounded-xl hover:bg-gray-50 dark:hover:bg-[#2c2c2e] cursor-pointer transition-colors"
                                >
                                    <div class="w-10 h-10 rounded-lg flex flex-shrink-0 items-center justify-center shadow-sm" :class="getTypeColor(item.item_type)">
                                        <component :is="getTypeIcon(item.item_type)" class="w-5 h-5 stroke-[1.5]" />
                                    </div>
                                    <div class="flex-1 min-w-0">
                                        <div class="font-semibold text-[15px] text-gray-800 dark:text-gray-200 truncate">{{ item.title }}</div>
                                        <div class="text-[12px] text-gray-500 mt-0.5 flex items-center gap-2">
                                            <span class="capitalize">{{ item.item_type }}</span> &bull; <span>{{ item.date }}</span>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </div>

                    </div>

                    <!-- Right Column: Inbox & Taxonomy -->
                    <div class="space-y-6">
                        
                        <!-- Stats Mini Bar -->
                        <div v-if="vaultStats" class="grid grid-cols-2 gap-3">
                            <div class="bg-white dark:bg-[#242426] p-4 rounded-2xl border border-gray-100 dark:border-[#2c2c2e] shadow-sm flex flex-col items-center">
                                <div class="text-2xl font-black text-gray-800 dark:text-gray-100">{{ vaultStats.total_items }}</div>
                                <div class="text-[10px] font-bold text-gray-400 uppercase">Total Items</div>
                            </div>
                            <div class="bg-blue-50 dark:bg-blue-900/20 p-4 rounded-2xl border border-blue-100 dark:border-blue-800/30 shadow-sm flex flex-col items-center">
                                <div class="text-2xl font-black text-blue-600 dark:text-blue-400">{{ vaultStats.type_distribution['note'] || 0 }}</div>
                                <div class="text-[10px] font-bold text-blue-500/80 uppercase">Notes</div>
                            </div>
                        </div>

                        <!-- Triage Inbox -->
                        <div class="bg-amber-50/40 dark:bg-amber-900/10 rounded-3xl p-5 border border-amber-100 dark:border-amber-900/30 shadow-sm">
                            <div class="flex items-center justify-between mb-4">
                                <h2 class="text-md font-bold text-amber-800 dark:text-amber-400 flex items-center gap-2">
                                    <Inbox class="w-4 h-4" /> Triage Inbox
                                </h2>
                                <span class="bg-amber-200 dark:bg-amber-800/50 text-amber-800 dark:text-amber-300 text-xs font-bold px-2 py-0.5 rounded-full">{{ inboxQuickCaps.length }}</span>
                            </div>
                            <p class="text-[12px] text-amber-700/70 dark:text-amber-500/70 mb-4 leading-snug">
                                Untagged QuickCaps waiting to be processed into Tasks or Notes.
                            </p>
                            
                            <div class="space-y-2 max-h-[300px] overflow-y-auto pr-1">
                                <div v-for="qc in inboxQuickCaps" :key="qc.id" class="bg-white dark:bg-[#242426] p-3 rounded-xl border border-amber-100 dark:border-amber-900/30 shadow-sm hover:shadow-md transition-shadow">
                                    <div class="text-xs text-gray-400 mb-1">{{ qc.date.split(' ')[0] }}</div>
                                    <p class="text-sm text-gray-700 dark:text-gray-300 line-clamp-3 mb-3 leading-relaxed">{{ qc.content }}</p>
                                    <button @click="openPreview(qc)" class="w-full py-1.5 bg-amber-100 dark:bg-amber-800/30 hover:bg-amber-200 dark:hover:bg-amber-800/50 text-amber-800 dark:text-amber-300 text-xs font-bold rounded-lg transition-colors">
                                        Process
                                    </button>
                                </div>
                                <div v-if="inboxQuickCaps.length === 0" class="text-sm text-center py-6 text-amber-600/50 italic">
                                    Inbox Zero! 🎉
                                </div>
                            </div>
                        </div>

                        <!-- Taxonomy Cloud -->
                        <div v-if="vaultStats" class="bg-white dark:bg-[#242426] rounded-3xl p-5 border border-gray-100 dark:border-[#2c2c2e] shadow-sm">
                            <h2 class="text-md font-bold text-gray-800 dark:text-gray-100 flex items-center gap-2 mb-4">
                                <Tag class="w-4 h-4 text-purple-500" /> Taxonomy
                            </h2>
                            <div class="flex flex-wrap gap-2 max-h-[250px] overflow-y-auto pr-1">
                                <button v-for="tag in vaultStats.tags" :key="tag.name"
                                    @click="applySearchFilter('#' + tag.name)"
                                    class="group flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-gray-200 dark:border-[#3a3a3c] bg-gray-50 dark:bg-[#1a1a1c] hover:border-purple-300 dark:hover:border-purple-500/50 transition-all cursor-pointer">
                                    <span class="text-[12px] font-medium text-gray-600 dark:text-gray-300 group-hover:text-purple-600 dark:group-hover:text-purple-400">#{{ tag.name }}</span>
                                    <span class="text-[10px] font-bold text-gray-400 group-hover:text-purple-500 bg-white dark:bg-[#2c2c2e] px-1.5 rounded-md shadow-sm border border-gray-100 dark:border-[#3a3a3c]">
                                        {{ tag.total_count }}
                                    </span>
                                </button>
                            </div>
                        </div>

                    </div>
                </div>
            </div>

            <!-- Search Results Stream -->
            <div v-else class="max-w-4xl mx-auto mt-4">
                <div v-if="isSearching" class="text-center py-10 opacity-50 flex items-center justify-center gap-2">
                    <div class="w-5 h-5 rounded-full border-2 border-black dark:border-white border-t-transparent animate-spin"></div>
                </div>
                
                <div v-else-if="items.length === 0" class="text-center py-16">
                    <div class="w-20 h-20 bg-gray-50 dark:bg-white/5 rounded-full flex flex-col items-center justify-center mx-auto mb-4 border border-dashed border-gray-200 dark:border-white/10">
                        <Search class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                    </div>
                    <p class="text-[#52525b] dark:text-[#a1a1aa] font-medium">No results found for "{{ searchQuery }}"</p>
                </div>
                
                <div v-else class="space-y-3">
                    <div class="flex items-center justify-between mb-4">
                        <h3 class="text-sm font-bold text-gray-500 dark:text-gray-400">Search Results</h3>
                        <span class="text-xs font-semibold text-gray-400">{{ items.length }} items</span>
                    </div>

                    <div v-for="item in items" :key="item.id"
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
                            
                            <p v-if="item.item_type !== 'file'" class="text-[13px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-2 leading-relaxed preview-markdown break-words" v-html="renderMarkdownPreview(item.preview, item.item_type)"></p>
                            <p v-else class="text-[13px] text-purple-600/70 dark:text-purple-400/70 font-mono">{{ item.preview }}</p>
                            
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

    <!-- Full-page Preview Panel (No Changes to logic, updated styling) -->
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
</style>
