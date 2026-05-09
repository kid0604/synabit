<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Users, Plus, Mail, Phone, Building, Hash, Search, CheckSquare, FileText, Zap, Edit2, Gift } from 'lucide-vue-next';
import PersonModal from './PersonModal.vue';
import { logger } from '../../utils/logger';

defineProps<{
    vaultPath: string;
}>();

const emit = defineEmits(['open-node']);

const people = ref<any[]>([]);
const searchQuery = ref('');
const loading = ref(true);

const showModal = ref(false);
const selectedPerson = ref<any | null>(null);

// Relationship Dashboard State
const linkedNodes = ref<any[]>([]);
const loadingLinks = ref(false);

const fetchPeople = async () => {
    loading.value = true;
    try {
        const nodes = await invoke<any[]>('get_nodes', { nodeType: 'person' });
        people.value = nodes;
        
        // Update selected person reference if it exists
        if (selectedPerson.value) {
            const updated = nodes.find(n => n.id === selectedPerson.value.id);
            if (updated) {
                selectedPerson.value = updated;
            } else {
                selectedPerson.value = null; // Was deleted
            }
        }
    } catch (e) {
        logger.error('Failed to fetch people nodes', e);
    } finally {
        loading.value = false;
    }
};

const fetchLinkedNodes = async (personTitle: string, personId: string) => {
    loadingLinks.value = true;
    try {
        const links = await invoke<any[]>('get_linked_nodes', { targetTitle: personTitle, targetId: personId });
        linkedNodes.value = links;
    } catch (e) {
        logger.error('Failed to fetch linked nodes', e);
        linkedNodes.value = [];
    } finally {
        loadingLinks.value = false;
    }
};

watch(selectedPerson, (newPerson) => {
    if (newPerson && newPerson.title) {
        fetchLinkedNodes(newPerson.title, newPerson.id);
    } else {
        linkedNodes.value = [];
    }
});

onMounted(() => {
    fetchPeople();
    
    // Listen for file changes
    listen('vault-file-created-deleted', () => {
        fetchPeople();
        if (selectedPerson.value) fetchLinkedNodes(selectedPerson.value.title, selectedPerson.value.id);
    });
    listen('vault-file-modified', () => {
        fetchPeople();
        if (selectedPerson.value) fetchLinkedNodes(selectedPerson.value.title, selectedPerson.value.id);
    });
});

const filteredPeople = computed(() => {
    if (!searchQuery.value) return people.value;
    const q = searchQuery.value.toLowerCase();
    return people.value.filter(p => {
        return p.title.toLowerCase().includes(q) || 
               (p.properties.email && p.properties.email.toLowerCase().includes(q)) ||
               (p.properties.company && p.properties.company.toLowerCase().includes(q));
    });
});

const openNewModal = () => {
    selectedPerson.value = null;
    showModal.value = true;
};

const editPerson = (person: any) => {
    selectedPerson.value = person;
    showModal.value = true;
};

const getInitials = (name: string) => {
    if (!name) return '?';
    return name.split(' ').map(n => n[0]).join('').substring(0, 2).toUpperCase();
};

const getTagColor = (tag: string) => {
    const colors = [
        'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300',
        'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300',
        'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-300',
        'bg-orange-100 text-orange-800 dark:bg-orange-900/30 dark:text-orange-300',
        'bg-pink-100 text-pink-800 dark:bg-pink-900/30 dark:text-pink-300',
    ];
    let hash = 0;
    for (let i = 0; i < tag.length; i++) {
        hash = tag.charCodeAt(i) + ((hash << 5) - hash);
    }
    return colors[Math.abs(hash) % colors.length];
};

const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
};

// Computed categories for Dashboard
const linkedTasks = computed(() => linkedNodes.value.filter(n => n.node_type === 'task'));
const linkedNotes = computed(() => linkedNodes.value.filter(n => n.node_type === 'note'));
const linkedQuickCaps = computed(() => linkedNodes.value.filter(n => n.node_type === 'quickcap'));

const renderPreview = (content: string) => {
    if (!content) return 'No content preview available.';
    // Convert [Title](synabit://...) to @Title
    let text = content.replace(/\[([^\]]+)\]\(synabit:\/\/[^)]+\)/g, '@$1');
    // Strip other simple markdown links to just their text
    text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
    // Strip headers
    text = text.replace(/^#{1,6}\s+/gm, '');
    // Strip bold/italic
    text = text.replace(/[*_]{1,3}(.*?)[*_]{1,3}/g, '$1');
    return text.trim();
};

const openLinkedNode = (node: any) => {
    emit('open-node', node.id, node.node_type);
};

const openPersonById = async (id: string) => {
    // Ensure data is loaded
    if (people.value.length === 0) {
        await fetchPeople();
    }
    let p = people.value.find(p => p.id === id);
    if (!p) {
        // Fallback for legacy links that used title instead of ID
        p = people.value.find(p => p.title.toLowerCase() === id.toLowerCase());
    }
    if (!p) {
        // Fallback 2: Check if the slugified legacy title matches the prefix of the ID
        const slug = id.replace(/[^a-z0-9]/gi, '_').toLowerCase();
        if (slug.length > 2) {
            p = people.value.find(p => p.id.toLowerCase().includes(slug));
        }
    }
    if (p) {
        selectedPerson.value = p;
    }
};

defineExpose({ openPersonById });
</script>

<template>
    <div class="h-full flex bg-base dark:bg-base-dark text-text dark:text-text-dark overflow-hidden">
        
        <!-- LEFT PANEL: People List (Master) -->
        <div class="w-80 flex-shrink-0 border-r border-border dark:border-border-dark flex flex-col bg-surface dark:bg-surface-dark">
            <!-- Header -->
            <div class="h-14 border-b border-border dark:border-border-dark flex items-center justify-between px-4 flex-shrink-0" data-tauri-drag-region>
                <div class="flex items-center gap-2 font-semibold">
                    <Users class="w-4 h-4 text-text-secondary dark:text-text-secondary-dark" />
                    <span>People</span>
                </div>
                <button @click="openNewModal" class="p-1.5 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors text-blue-500">
                    <Plus class="w-5 h-5" />
                </button>
            </div>
            
            <!-- Search -->
            <div class="p-3 border-b border-border dark:border-border-dark">
                <div class="relative">
                    <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                    <input 
                        v-model="searchQuery" 
                        type="text" 
                        placeholder="Search..." 
                        class="w-full pl-9 pr-3 py-1.5 bg-gray-100 dark:bg-gray-800 border-none rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none transition-all"
                    />
                </div>
            </div>
            
            <!-- List -->
            <div class="flex-1 overflow-y-auto p-2">
                <div v-if="loading" class="flex justify-center p-4">
                    <div class="animate-spin rounded-full h-5 w-5 border-b-2 border-blue-500"></div>
                </div>
                <div v-else-if="filteredPeople.length === 0" class="text-center p-4 text-sm text-gray-500">
                    No contacts found.
                </div>
                <div v-else class="space-y-1">
                    <button 
                        v-for="person in filteredPeople" 
                        :key="person.id"
                        @click="selectedPerson = person"
                        :class="[
                            'w-full text-left px-3 py-2 rounded-lg flex items-center gap-3 transition-colors',
                            selectedPerson?.id === person.id 
                                ? 'bg-blue-50 dark:bg-blue-900/30 ring-1 ring-blue-500/50' 
                                : 'hover:bg-gray-100 dark:hover:bg-gray-800/50'
                        ]"
                    >
                        <div class="w-10 h-10 rounded-full bg-gradient-to-br from-gray-200 to-gray-300 dark:from-gray-700 dark:to-gray-800 text-gray-700 dark:text-gray-300 flex items-center justify-center text-sm font-bold flex-shrink-0">
                            {{ getInitials(person.title) }}
                        </div>
                        <div class="flex-1 min-w-0">
                            <h4 class="font-medium text-sm truncate">{{ person.title }}</h4>
                            <p v-if="person.properties.company" class="text-xs text-gray-500 dark:text-gray-400 truncate flex items-center gap-1 mt-0.5">
                                <Building class="w-3 h-3 flex-shrink-0" />
                                <span class="truncate">{{ person.properties.company }}</span>
                            </p>
                            <p v-else-if="person.properties.tags?.length" class="text-xs text-gray-500 dark:text-gray-400 truncate flex items-center gap-1 mt-0.5">
                                <Hash class="w-3 h-3 flex-shrink-0" />
                                <span class="truncate">{{ person.properties.tags.join(', ') }}</span>
                            </p>
                        </div>
                    </button>
                </div>
            </div>
        </div>

        <!-- RIGHT PANEL: Relationship Dashboard (Detail) -->
        <div class="flex-1 flex flex-col bg-base dark:bg-base-dark overflow-y-auto relative">
            <div v-if="!selectedPerson" class="absolute inset-0 flex flex-col items-center justify-center text-gray-400 dark:text-gray-500">
                <Users class="w-16 h-16 mb-4 opacity-20" />
                <h3 class="text-lg font-medium text-gray-600 dark:text-gray-300">No Person Selected</h3>
                <p class="text-sm">Select a contact to view their relationship dashboard.</p>
            </div>
            
            <div v-else class="max-w-4xl w-full mx-auto p-8 flex flex-col gap-8 pb-20">
                
                <!-- Profile Header -->
                <div class="flex items-start gap-6 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-6 shadow-sm relative group">
                    <button @click="editPerson(selectedPerson)" class="absolute top-4 right-4 p-2 text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/30 rounded-lg opacity-0 group-hover:opacity-100 transition-all">
                        <Edit2 class="w-4 h-4" />
                    </button>
                    
                    <div class="w-24 h-24 rounded-full bg-gradient-to-br from-blue-500 to-purple-600 text-white flex items-center justify-center text-3xl font-bold shadow-lg flex-shrink-0">
                        {{ getInitials(selectedPerson.title) }}
                    </div>
                    
                    <div class="flex-1 min-w-0">
                        <h1 class="text-2xl font-bold text-gray-900 dark:text-white mb-1">{{ selectedPerson.title }}</h1>
                        <p v-if="selectedPerson.properties.company" class="text-blue-600 dark:text-blue-400 font-medium flex items-center gap-1.5 mb-4">
                            <Building class="w-4 h-4" />
                            {{ selectedPerson.properties.company }}
                        </p>
                        
                        <div class="flex flex-wrap gap-x-6 gap-y-2 text-sm text-gray-600 dark:text-gray-300">
                            <div v-if="selectedPerson.properties.email" class="flex items-center gap-2">
                                <Mail class="w-4 h-4 opacity-70" />
                                <a :href="'mailto:' + selectedPerson.properties.email" class="hover:underline">{{ selectedPerson.properties.email }}</a>
                            </div>
                            <div v-if="selectedPerson.properties.phone" class="flex items-center gap-2">
                                <Phone class="w-4 h-4 opacity-70" />
                                <a :href="'tel:' + selectedPerson.properties.phone" class="hover:underline">{{ selectedPerson.properties.phone }}</a>
                            </div>
                            <div v-if="selectedPerson.properties.birthday" class="flex items-center gap-2">
                                <Gift class="w-4 h-4 opacity-70 text-pink-500" />
                                <span>{{ selectedPerson.properties.birthday }}</span>
                            </div>
                        </div>
                        
                        <div v-if="selectedPerson.properties.tags && selectedPerson.properties.tags.length > 0" class="flex flex-wrap gap-2 mt-4">
                            <span 
                                v-for="tag in selectedPerson.properties.tags" 
                                :key="tag"
                                :class="['px-2.5 py-1 text-xs font-medium rounded-md flex items-center gap-1', getTagColor(tag)]"
                            >
                                <Hash class="w-3 h-3 opacity-50" />
                                {{ tag }}
                            </span>
                        </div>
                    </div>
                </div>

                <!-- Personal Notes (if any) -->
                <div v-if="selectedPerson.content && selectedPerson.content.trim() !== ''" class="bg-yellow-50/50 dark:bg-yellow-900/10 border border-yellow-200 dark:border-yellow-900/30 rounded-xl p-5">
                    <h3 class="text-sm font-semibold text-yellow-800 dark:text-yellow-500 mb-2 flex items-center gap-2">
                        <FileText class="w-4 h-4" />
                        Personal Notes
                    </h3>
                    <div class="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap font-mono">{{ selectedPerson.content }}</div>
                </div>

                <!-- Relationship Activity Dashboard -->
                <div>
                    <h2 class="text-lg font-bold mb-4 flex items-center gap-2 border-b border-border dark:border-border-dark pb-2">
                        <Zap class="w-5 h-5 text-orange-500" />
                        Relationship Activity
                    </h2>
                    
                    <div v-if="loadingLinks" class="flex justify-center py-8">
                        <div class="animate-spin rounded-full h-6 w-6 border-b-2 border-blue-500"></div>
                    </div>
                    
                    <div v-else-if="linkedNodes.length === 0" class="text-center py-10 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-dashed border-gray-300 dark:border-gray-700">
                        <p class="text-gray-500 dark:text-gray-400">No linked activity yet.</p>
                        <p class="text-xs text-gray-400 mt-1">Mention <code class="bg-gray-200 dark:bg-gray-700 px-1 py-0.5 rounded">[[{{selectedPerson.title}}]]</code> in any Note or Task to see it here.</p>
                    </div>
                    
                    <div v-else class="space-y-6">
                        
                        <!-- Tasks -->
                        <div v-if="linkedTasks.length > 0">
                            <h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                                <CheckSquare class="w-4 h-4" /> Pending Tasks
                            </h3>
                            <div class="space-y-2">
                                <div v-for="node in linkedTasks" :key="node.id" @click="openLinkedNode(node)" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg p-3 hover:shadow-sm transition-all flex items-start gap-3 cursor-pointer">
                                    <input type="checkbox" :checked="node.properties.status === 'completed'" disabled class="mt-1 flex-shrink-0 rounded text-blue-500">
                                    <div>
                                        <p class="text-sm font-medium" :class="node.properties.status === 'completed' ? 'line-through text-gray-400' : ''">{{ node.title }}</p>
                                        <p class="text-xs text-gray-500 mt-1">{{ formatDate(node.timestamp) }}</p>
                                    </div>
                                </div>
                            </div>
                        </div>
                        
                        <!-- Notes -->
                        <div v-if="linkedNotes.length > 0">
                            <h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                                <FileText class="w-4 h-4" /> Mentioned in Notes
                            </h3>
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                <div v-for="node in linkedNotes" :key="node.id" @click="openLinkedNode(node)" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg p-4 hover:shadow-sm transition-all cursor-pointer">
                                    <h4 class="text-sm font-semibold mb-1 text-blue-600 dark:text-blue-400">{{ node.title }}</h4>
                                    <p class="text-xs text-gray-500 mb-2">{{ formatDate(node.timestamp) }}</p>
                                    <p class="text-xs text-gray-600 dark:text-gray-300 line-clamp-3">{{ renderPreview(node.content) }}</p>
                                </div>
                            </div>
                        </div>

                        <!-- QuickCaps -->
                        <div v-if="linkedQuickCaps.length > 0">
                            <h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                                <Zap class="w-4 h-4" /> Quick Captures
                            </h3>
                            <div class="space-y-2">
                                <div v-for="node in linkedQuickCaps" :key="node.id" class="bg-blue-50/50 dark:bg-blue-900/10 border border-blue-100 dark:border-blue-900/30 rounded-lg p-3 text-sm flex items-start gap-3">
                                    <div class="w-1.5 h-1.5 rounded-full bg-blue-500 mt-1.5"></div>
                                    <div class="flex-1 text-gray-700 dark:text-gray-300 whitespace-pre-wrap">{{ node.content || node.title }}</div>
                                    <span class="text-xs text-gray-400 whitespace-nowrap">{{ formatDate(node.timestamp) }}</span>
                                </div>
                            </div>
                        </div>
                        
                    </div>
                </div>
            </div>
        </div>

        <PersonModal 
            v-if="showModal"
            :person="selectedPerson"
            :vault-path="vaultPath"
            @close="showModal = false"
            @saved="fetchPeople"
        />
    </div>
</template>
