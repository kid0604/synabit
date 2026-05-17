<script setup lang="ts">
import { computed } from 'vue';
import { FileText, CheckSquare, Zap } from 'lucide-vue-next';

const props = defineProps<{
    person: any;
    linkedNodes: any[];
    loadingLinks: boolean;
}>();

const emit = defineEmits(['open-linked-node']);

const linkedTasks = computed(() => props.linkedNodes.filter(n => n.node_type === 'task'));
const linkedNotes = computed(() => props.linkedNodes.filter(n => n.node_type === 'note'));
const linkedQuickCaps = computed(() => props.linkedNodes.filter(n => n.node_type === 'quickcap'));

const formatDate = (timestamp: number) => {
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
    <div class="space-y-6">
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
                <Zap class="w-4 h-4 text-orange-500" />
                Linked Activity
            </h2>

            <div v-if="loadingLinks" class="flex justify-center py-8">
                <div class="animate-spin rounded-full h-6 w-6 border-b-2 border-blue-500"></div>
            </div>

            <div v-else-if="linkedNodes.length === 0" class="text-center py-8 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-dashed border-gray-300 dark:border-gray-700">
                <p class="text-gray-500 dark:text-gray-400">No linked activity yet.</p>
                <p class="text-xs text-gray-400 mt-1">Mention <code class="bg-gray-200 dark:bg-gray-700 px-1 py-0.5 rounded">[[{{person.title}}]]</code> in any Note or Task to see it here.</p>
            </div>

            <div v-else class="space-y-6">
                <!-- Tasks -->
                <div v-if="linkedTasks.length > 0">
                    <h3 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <CheckSquare class="w-3.5 h-3.5" /> Tasks ({{ linkedTasks.length }})
                    </h3>
                    <div class="space-y-2">
                        <div v-for="node in linkedTasks" :key="node.id" @click="emit('open-linked-node', node)" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg p-3 hover:shadow-sm transition-all flex items-start gap-3 cursor-pointer">
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
                    <h3 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <FileText class="w-3.5 h-3.5" /> Notes ({{ linkedNotes.length }})
                    </h3>
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                        <div v-for="node in linkedNotes" :key="node.id" @click="emit('open-linked-node', node)" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg p-4 hover:shadow-sm transition-all cursor-pointer">
                            <h4 class="text-sm font-semibold mb-1 text-blue-600 dark:text-blue-400">{{ node.title }}</h4>
                            <p class="text-xs text-gray-500 mb-2">{{ formatDate(node.timestamp) }}</p>
                            <p class="text-xs text-gray-600 dark:text-gray-300 line-clamp-3">{{ renderPreview(node.content) }}</p>
                        </div>
                    </div>
                </div>

                <!-- QuickCaps -->
                <div v-if="linkedQuickCaps.length > 0">
                    <h3 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <Zap class="w-3.5 h-3.5" /> Quick Captures ({{ linkedQuickCaps.length }})
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
</template>
