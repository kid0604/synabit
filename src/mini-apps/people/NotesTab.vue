<script setup lang="ts">
import { ref, computed } from 'vue';
import { FileText, CheckSquare, Zap, Calendar, Folder, File as FileIcon, Link as LinkIcon, Package, Search, ArrowUpDown } from 'lucide-vue-next';

const props = defineProps<{
    person: any;
    linkedNodes: any[];
    loadingLinks: boolean;
}>();

const emit = defineEmits(['open-linked-node']);

const filterType = ref('All');
const searchQuery = ref('');
const sortMode = ref<'recent' | 'oldest' | 'alpha'>('recent');

const cycleSortMode = () => {
    const modes: Array<'recent' | 'oldest' | 'alpha'> = ['recent', 'oldest', 'alpha'];
    const idx = modes.indexOf(sortMode.value);
    sortMode.value = modes[(idx + 1) % modes.length];
};

const sortModeLabel = computed(() => {
    if (sortMode.value === 'recent') return 'Recent';
    if (sortMode.value === 'oldest') return 'Oldest';
    return 'A-Z';
});

const normalizeType = (type: string) => {
    type = type || 'unknown';
    if (type === 'quickcap') return 'Quick Captures';
    if (type === 'task') return 'Tasks';
    if (type === 'note') return 'Notes';
    if (type === 'event') return 'Events';
    if (type === 'file') return 'Files';
    if (type === 'project') return 'Projects';
    return type.charAt(0).toUpperCase() + type.slice(1);
};

const availableTypes = computed(() => {
    const types = new Set<string>();
    for (const node of props.linkedNodes) {
        types.add(normalizeType(node.node_type));
    }
    return ['All', ...Array.from(types).sort()];
});

const filteredNodes = computed(() => {
    let list = [...props.linkedNodes];
    
    if (filterType.value !== 'All') {
        list = list.filter(n => normalizeType(n.node_type) === filterType.value);
    }

    if (searchQuery.value) {
        const q = searchQuery.value.toLowerCase();
        list = list.filter(n => {
            return (n.title && n.title.toLowerCase().includes(q)) || 
                   (n.content && n.content.toLowerCase().includes(q));
        });
    }

    list.sort((a, b) => {
        if (sortMode.value === 'alpha') {
            return (a.title || '').localeCompare(b.title || '');
        } else {
            const timeA = a.timestamp || 0;
            const timeB = b.timestamp || 0;
            return sortMode.value === 'recent' ? timeB - timeA : timeA - timeB;
        }
    });

    return list;
});

const groupedLinkedNodes = computed(() => {
    const groups: Record<string, any[]> = {};
    for (const node of filteredNodes.value) {
        const type = normalizeType(node.node_type);
        if (!groups[type]) groups[type] = [];
        groups[type].push(node);
    }
    return groups;
});

const getTypeIcon = (type: string) => {
    switch (type.toLowerCase()) {
        case 'tasks': return CheckSquare;
        case 'notes': return FileText;
        case 'quick captures': return Zap;
        case 'events': return Calendar;
        case 'files': return FileIcon;
        case 'projects': return Folder;
        default: return Package;
    }
};

const formatDate = (timestamp: number) => {
    if (!timestamp) return 'Unknown Date';
    return new Date(timestamp).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
};

const renderPreview = (content: string) => {
    if (!content) return 'No content preview available.';
    let text = content.replace(/\[([^\]]+)\]\(synabit:\/\/[^)]+\)/g, '@$1');
    text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
    text = text.replace(/^#{1,6}\s+/gm, '');
    text = text.replace(/[*_]{1,3}(.*?)[*_]{1,3}/g, '$1');
    return text.trim();
};
</script>

<template>
    <div class="space-y-8">
        <!-- Personal Notes -->
        <div v-if="person.content && person.content.trim() !== ''" class="bg-yellow-50/50 dark:bg-yellow-900/10 border border-yellow-200 dark:border-yellow-900/30 rounded-xl p-5">
            <h3 class="text-sm font-semibold text-yellow-800 dark:text-yellow-500 mb-2 flex items-center gap-2">
                <FileText class="w-4 h-4" />
                Personal Notes
            </h3>
            <div class="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap font-mono">{{ person.content }}</div>
        </div>

        <!-- Linked Activity -->
        <div>
            <h2 class="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-4 flex items-center gap-2">
                <LinkIcon class="w-4 h-4 text-orange-500" />
                Linked Nodes
            </h2>

            <div v-if="loadingLinks" class="flex justify-center py-8">
                <div class="animate-spin rounded-full h-6 w-6 border-b-2 border-blue-500"></div>
            </div>

            <div v-else-if="linkedNodes.length === 0" class="text-center py-8 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-dashed border-gray-300 dark:border-gray-700">
                <p class="text-gray-500 dark:text-gray-400">{{ $t('people.no_linked_activity') }}</p>
                <p class="text-xs text-gray-400 mt-1">Mention <code class="bg-gray-200 dark:bg-gray-700 px-1 py-0.5 rounded">[[{{person.title}}]]</code> in any Note or Task to see it here.</p>
            </div>

            <div v-else class="space-y-6">
                <!-- Toolbar -->
                <div class="flex flex-col sm:flex-row gap-3">
                    <div class="relative flex-1">
                        <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                        <input v-model="searchQuery" type="text" :placeholder="$t('people.search_linked_ph')" class="w-full pl-9 pr-3 py-2 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none transition-all" />
                    </div>
                    <div class="flex items-center gap-2">
                        <select v-model="filterType" class="px-3 py-2 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg text-sm outline-none focus:ring-2 focus:ring-blue-500 text-gray-700 dark:text-gray-300 appearance-none min-w-[100px]">
                            <option v-for="t in availableTypes" :key="t" :value="t">{{ t }}</option>
                        </select>
                        <button @click="cycleSortMode" class="flex items-center gap-1.5 px-3 py-2 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg text-sm hover:bg-gray-50 dark:hover:bg-[#2c2c2c] transition-colors text-gray-700 dark:text-gray-300">
                            <ArrowUpDown class="w-4 h-4 text-gray-500" />
                            {{ sortModeLabel }}
                        </button>
                    </div>
                </div>

                <div v-if="filteredNodes.length === 0" class="text-center py-8 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-dashed border-gray-300 dark:border-gray-700">
                    <p class="text-gray-500 dark:text-gray-400">{{ $t('people.no_nodes_match') }}</p>
                </div>

                <div v-else class="space-y-8">
                    <div v-for="(nodes, typeName) in groupedLinkedNodes" :key="typeName">
                    <h3 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <component :is="getTypeIcon(typeName)" class="w-3.5 h-3.5" /> {{ typeName }} ({{ nodes.length }})
                    </h3>

                    <!-- Tasks layout -->
                    <div v-if="typeName === 'Tasks'" class="space-y-2">
                        <div v-for="node in nodes" :key="node.id" @click="emit('open-linked-node', node)" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg p-3 hover:shadow-sm transition-all flex items-start gap-3 cursor-pointer">
                            <input type="checkbox" :checked="node.properties.status === 'completed'" disabled class="mt-1 flex-shrink-0 rounded text-blue-500">
                            <div>
                                <p class="text-sm font-medium" :class="node.properties.status === 'completed' ? 'line-through text-gray-400' : ''">{{ node.title }}</p>
                                <p class="text-xs text-gray-500 mt-1">{{ formatDate(node.timestamp) }}</p>
                            </div>
                        </div>
                    </div>

                    <!-- QuickCaps layout -->
                    <div v-else-if="typeName === 'Quick Captures'" class="space-y-2">
                        <div v-for="node in nodes" :key="node.id" class="bg-blue-50/50 dark:bg-blue-900/10 border border-blue-100 dark:border-blue-900/30 rounded-lg p-3 text-sm flex items-start gap-3">
                            <div class="w-1.5 h-1.5 rounded-full bg-blue-500 mt-1.5 flex-shrink-0"></div>
                            <div class="flex-1 text-gray-700 dark:text-gray-300 whitespace-pre-wrap">{{ node.content || node.title }}</div>
                            <span class="text-xs text-gray-400 whitespace-nowrap">{{ formatDate(node.timestamp) }}</span>
                        </div>
                    </div>

                    <!-- Notes, Events, Files, etc. layout -->
                    <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-3">
                        <div v-for="node in nodes" :key="node.id" @click="emit('open-linked-node', node)" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg p-4 hover:shadow-sm transition-all cursor-pointer">
                            <h4 class="text-sm font-semibold mb-1 text-blue-600 dark:text-blue-400 truncate">{{ node.title }}</h4>
                            <p class="text-xs text-gray-500 mb-2">{{ formatDate(node.timestamp) }}</p>
                            <p v-if="node.content" class="text-xs text-gray-600 dark:text-gray-300 line-clamp-3">{{ renderPreview(node.content) }}</p>
                            <!-- Show properties for non-note types if any -->
                            <div v-if="typeName !== 'Notes' && Object.keys(node.properties || {}).length > 0" class="mt-2 flex flex-wrap gap-1">
                                <span v-for="(val, key) in node.properties" :key="key" v-show="key !== 'status' && key !== 'id' && typeof val !== 'object'" class="px-1.5 py-0.5 bg-gray-100 dark:bg-gray-800 text-gray-500 rounded text-[10px]">
                                    {{ key }}: {{ val }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>
</div>
</template>
