<script setup lang="ts">
import { ref, computed, toRef } from 'vue';
import { useRouter } from 'vue-router';
import { invoke } from '@tauri-apps/api/core';
import { Clock, Plus, PhoneCall, MessageSquare, Coffee, Gift, Users, Smile, Meh, Frown, ThumbsUp, X, CheckSquare, FileText, Zap, Filter, CreditCard, Repeat } from 'lucide-vue-next';
import { useRelationshipHealth } from './composables/useRelationshipHealth';
import { logger } from '../../utils/logger';

const props = defineProps<{
    person: any;
    vaultPath: string;
    linkedNodes: any[];
    allDebts?: any[];
    allTransactions?: any[];
}>();

const emit = defineEmits(['updated', 'open-linked-node']);

const router = useRouter();

const personRef = toRef(props, 'person');
const { health } = useRelationshipHealth(personRef);

// Filter
const activeFilter = ref<string>('all');

const interactionTypes = [
    { value: 'meeting', label: 'Meeting', icon: Users },
    { value: 'call', label: 'Call', icon: PhoneCall },
    { value: 'message', label: 'Message', icon: MessageSquare },
    { value: 'coffee', label: 'Coffee', icon: Coffee },
    { value: 'gift', label: 'Gift', icon: Gift },
    { value: 'other', label: 'Other', icon: Clock },
];

const moodOptions = [
    { value: 'great', label: 'Great', icon: ThumbsUp },
    { value: 'good', label: 'Good', icon: Smile },
    { value: 'neutral', label: 'Neutral', icon: Meh },
    { value: 'difficult', label: 'Difficult', icon: Frown },
];

// --- Finance Data ---
const personDebts = computed(() => {
    if (!props.allDebts || !props.person) return [];
    return props.allDebts.filter(d => {
        if (d.personId && d.personId === props.person.id) return true;
        if (!d.personId && d.person && d.person.toLowerCase() === props.person.title.toLowerCase()) return true;
        return false;
    });
});

const personTransactions = computed(() => {
    if (!props.allTransactions || !props.person) return [];
    return props.allTransactions.filter(t => t.personId === props.person.id);
});
// --------------------

// Unified timeline: merge manual interactions + linked activity
const unifiedTimeline = computed(() => {
    const items: any[] = [];

    // Financial Transactions
    for (const tx of personTransactions.value) {
        items.push({
            id: `tx-${tx.id}`,
            date: tx.date,
            sortDate: new Date(tx.date).getTime(),
            source: 'finance',
            type: 'transaction',
            transaction: tx,
        });
    }

    // Debts
    for (const debt of personDebts.value) {
        items.push({
            id: `debt-${debt.id}`,
            date: debt.startDate,
            sortDate: new Date(debt.startDate).getTime(),
            source: 'finance',
            type: 'debt',
            debt: debt,
        });
    }

    // Manual interactions
    const interactions = props.person?.properties?.interactions || [];
    for (const i of interactions) {
        items.push({
            id: i.id,
            date: i.date,
            sortDate: new Date(i.date).getTime(),
            source: 'interaction',
            type: i.type,
            note: i.note,
            mood: i.mood,
        });
    }

    const getNodeDate = (node: any) => {
        if (node.properties && node.properties.created_at) {
            const dateStr = new Date(node.properties.created_at).toISOString().split('T')[0];
            return { date: dateStr, sortDate: new Date(node.properties.created_at).getTime() };
        }
        return { date: new Date(node.timestamp).toISOString().split('T')[0], sortDate: node.timestamp };
    };

    // Linked Tasks
    for (const node of props.linkedNodes.filter(n => n.node_type === 'task')) {
        const { date, sortDate } = getNodeDate(node);
        items.push({
            id: `linked-${node.id}`,
            date,
            sortDate,
            source: 'task',
            type: 'task',
            title: node.title,
            status: node.properties?.status,
            node,
        });
    }

    // Linked Notes
    for (const node of props.linkedNodes.filter(n => n.node_type === 'note')) {
        const { date, sortDate } = getNodeDate(node);
        items.push({
            id: `linked-${node.id}`,
            date,
            sortDate,
            source: 'note',
            type: 'note',
            title: node.title,
            preview: (node.content || '').replace(/^---[\s\S]*?---\n?/, '').replace(/\[([^\]]*?)\]\([^)]*\)/g, '$1').trim().substring(0, 120),
            node,
        });
    }

    // Linked QuickCaps
    for (const node of props.linkedNodes.filter(n => n.node_type === 'quickcap')) {
        const { date, sortDate } = getNodeDate(node);
        items.push({
            id: `linked-${node.id}`,
            date,
            sortDate,
            source: 'quickcap',
            type: 'quickcap',
            title: node.content || node.title,
            node,
        });
    }

    // Sort descending by date
    items.sort((a, b) => b.sortDate - a.sortDate);
    return items;
});

const filteredTimeline = computed(() => {
    if (activeFilter.value === 'all') return unifiedTimeline.value;
    if (activeFilter.value === 'interactions') return unifiedTimeline.value.filter(i => i.source === 'interaction');
    if (activeFilter.value === 'linked') return unifiedTimeline.value.filter(i => i.source !== 'interaction');
    return unifiedTimeline.value.filter(i => i.type === activeFilter.value);
});

const filterOptions = computed(() => [
    { value: 'all', label: 'All', count: unifiedTimeline.value.length },
    { value: 'interactions', label: 'Interactions', count: unifiedTimeline.value.filter(i => i.source === 'interaction').length },
    { value: 'linked', label: 'Linked', count: unifiedTimeline.value.filter(i => i.source !== 'interaction').length },
]);

// Quick-add form
const showAddForm = ref(false);
const newInteraction = ref({
    type: 'meeting',
    date: new Date().toISOString().split('T')[0],
    note: '',
    mood: ''
});

const getTypeColor = (type: string) => {
    const colors: Record<string, string> = {
        meeting: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300',
        call: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300',
        message: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300',
        coffee: 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300',
        gift: 'bg-pink-100 text-pink-700 dark:bg-pink-900/30 dark:text-pink-300',
        task: 'bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-300',
        note: 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-300',
        quickcap: 'bg-sky-100 text-sky-700 dark:bg-sky-900/30 dark:text-sky-300',
        transaction: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300',
        debt: 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-300',
        other: 'bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-300',
    };
    return colors[type] || colors.other;
};

const getTypeIcon = (type: string) => {
    const icons: Record<string, any> = {
        meeting: Users, call: PhoneCall, message: MessageSquare,
        coffee: Coffee, gift: Gift, task: CheckSquare,
        note: FileText, quickcap: Zap, transaction: Repeat, debt: CreditCard, other: Clock,
    };
    return icons[type] || Clock;
};

const getTypeLabel = (type: string) => {
    const found = interactionTypes.find(t => t.value === type);
    if (found) return found.label;
    const labels: Record<string, string> = { task: 'Task', note: 'Note', quickcap: 'Quick Capture', transaction: 'Transaction', debt: 'Debt' };
    return labels[type] || type;
};

const getMoodIcon = (mood: string) => moodOptions.find(m => m.value === mood)?.icon || null;

const formatDate = (dateStr: string) => {
    if (!dateStr) return '';
    const d = new Date(dateStr);
    const now = new Date();
    const dDay = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    const nDay = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const diffDays = Math.round((nDay.getTime() - dDay.getTime()) / (1000 * 60 * 60 * 24));
    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays}d ago`;
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
};

const formatCurrency = (amount: number) => {
    return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(amount);
};

const resetForm = () => {
    newInteraction.value = { type: 'meeting', date: new Date().toISOString().split('T')[0], note: '', mood: '' };
    showAddForm.value = false;
};

const saveInteraction = async () => {
    if (!newInteraction.value.note.trim()) return;
    try {
        const currentInteractions = [...(props.person.properties.interactions || [])];
        currentInteractions.push({ id: crypto.randomUUID(), ...newInteraction.value });
        const properties = { ...props.person.properties, interactions: currentInteractions, last_contacted: newInteraction.value.date };
        await invoke('write_node_file', {
            vaultPath: props.vaultPath, relPath: props.person.id,
            title: props.person.title, nodeType: 'person', properties, content: props.person.content || ''
        });
        resetForm();
        emit('updated');
    } catch (e) {
        logger.error('Failed to save interaction', e);
    }
};

const deleteInteraction = async (id: string) => {
    try {
        const currentInteractions = (props.person.properties.interactions || []).filter((i: any) => i.id !== id);
        const properties = { ...props.person.properties, interactions: currentInteractions };
        await invoke('write_node_file', {
            vaultPath: props.vaultPath, relPath: props.person.id,
            title: props.person.title, nodeType: 'person', properties, content: props.person.content || ''
        });
        emit('updated');
    } catch (e) {
        logger.error('Failed to delete interaction', e);
    }
};

const handleLinkedClick = (item: any) => {
    if (item.node) {
        emit('open-linked-node', item.node);
    } else if (item.source === 'finance') {
        if (item.type === 'transaction') {
            router.push({ name: 'finance', query: { txId: item.transaction.id, date: item.transaction.date } });
        } else if (item.type === 'debt') {
            router.push({ name: 'finance', query: { view: 'debts', debtId: item.debt.id } });
        }
    }
};
</script>

<template>
    <div class="space-y-4">
        <!-- Health Banner -->
        <div v-if="health.status !== 'unknown'" :class="['flex items-center gap-3 px-4 py-3 rounded-xl border', health.bgColor, health.status === 'overdue' ? 'border-red-200 dark:border-red-900/30' : health.status === 'due_soon' ? 'border-yellow-200 dark:border-yellow-900/30' : 'border-transparent']">
            <!-- Progress Ring -->
            <div class="relative w-10 h-10 flex-shrink-0">
                <svg class="w-10 h-10 -rotate-90" viewBox="0 0 36 36">
                    <circle cx="18" cy="18" r="15.5" fill="none" stroke="currentColor" stroke-width="2.5" class="text-gray-200 dark:text-gray-700" />
                    <circle cx="18" cy="18" r="15.5" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
                        :class="health.color"
                        :stroke-dasharray="`${health.percent * 0.975} 100`" />
                </svg>
                <span class="absolute inset-0 flex items-center justify-center text-[10px] font-bold" :class="health.color">{{ health.percent }}</span>
            </div>
            <div class="flex-1 min-w-0">
                <p class="text-sm font-semibold" :class="health.color">{{ health.label }}</p>
                <p class="text-xs text-gray-500 dark:text-gray-400">
                    <template v-if="health.daysSinceContact !== null">Last contact {{ health.daysSinceContact }}d ago</template>
                    <template v-if="health.nextContactDue !== null && health.nextContactDue > 0"> · Due in {{ health.nextContactDue }}d</template>
                    <template v-else-if="health.nextContactDue !== null && health.nextContactDue <= 0"> · {{ Math.abs(health.nextContactDue) }}d overdue</template>
                </p>
            </div>
            <div class="text-right flex-shrink-0">
                <p class="text-lg font-bold" :class="health.color">{{ health.interactionCount }}</p>
                <p class="text-[10px] text-gray-400 uppercase">interactions</p>
            </div>
        </div>

        <!-- Add Button / Form -->
        <div v-if="!showAddForm">
            <button @click="showAddForm = true" class="w-full flex items-center justify-center gap-2 py-3 border-2 border-dashed border-gray-300 dark:border-gray-600 rounded-xl text-sm text-gray-500 dark:text-gray-400 hover:border-blue-400 hover:text-blue-500 transition-colors">
                <Plus class="w-4 h-4" /> Log Interaction
            </button>
        </div>

        <!-- Inline Add Form -->
        <div v-else class="bg-surface dark:bg-surface-dark border border-blue-200 dark:border-blue-800 rounded-xl p-4 space-y-4 shadow-sm">
            <div class="flex flex-wrap gap-2">
                <button v-for="t in interactionTypes" :key="t.value" @click="newInteraction.type = t.value"
                    :class="['flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all',
                        newInteraction.type === t.value
                            ? getTypeColor(t.value) + ' ring-2 ring-offset-1 ring-blue-500/50'
                            : 'bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-700']">
                    <component :is="t.icon" class="w-3.5 h-3.5" /> {{ t.label }}
                </button>
            </div>
            <input v-model="newInteraction.date" type="date" class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none" />
            <textarea v-model="newInteraction.note" placeholder="What happened?" rows="2"
                class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none resize-none"
                @keydown.meta.enter="saveInteraction"></textarea>
            <div class="flex items-center gap-2">
                <span class="text-xs text-gray-500 dark:text-gray-400">Mood:</span>
                <button v-for="m in moodOptions" :key="m.value" @click="newInteraction.mood = newInteraction.mood === m.value ? '' : m.value"
                    :class="['p-1.5 rounded-lg transition-all', newInteraction.mood === m.value ? 'bg-blue-100 dark:bg-blue-900/30 ring-1 ring-blue-500' : 'hover:bg-gray-100 dark:hover:bg-gray-800']" :title="m.label">
                    <component :is="m.icon" class="w-4 h-4" :class="newInteraction.mood === m.value ? 'text-blue-600 dark:text-blue-400' : 'text-gray-400'" />
                </button>
            </div>
            <div class="flex justify-end gap-2">
                <button @click="resetForm" class="px-3 py-1.5 text-sm text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors">Cancel</button>
                <button @click="saveInteraction" :disabled="!newInteraction.note.trim()" class="px-4 py-1.5 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors disabled:opacity-50 font-medium">Save</button>
            </div>
        </div>

        <!-- Filter Bar -->
        <div v-if="unifiedTimeline.length > 0" class="flex items-center gap-2">
            <Filter class="w-3.5 h-3.5 text-gray-400" />
            <button v-for="f in filterOptions" :key="f.value" @click="activeFilter = f.value"
                :class="['px-2.5 py-1 text-xs rounded-md font-medium transition-all',
                    activeFilter === f.value
                        ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300'
                        : 'text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800']">
                {{ f.label }} <span class="opacity-60">({{ f.count }})</span>
            </button>
        </div>

        <!-- Empty State -->
        <div v-if="filteredTimeline.length === 0 && !showAddForm" class="text-center py-10 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-dashed border-gray-300 dark:border-gray-700">
            <Clock class="w-10 h-10 mx-auto mb-3 text-gray-300 dark:text-gray-600" />
            <p class="text-gray-500 dark:text-gray-400">No activity yet.</p>
            <p class="text-xs text-gray-400 mt-1">Log an interaction or mention <code class="bg-gray-200 dark:bg-gray-700 px-1 py-0.5 rounded">[[{{ person.title }}]]</code> in a Note.</p>
        </div>

        <!-- Unified Timeline -->
        <div v-else class="relative">
            <div class="absolute left-5 top-0 bottom-0 w-px bg-gray-200 dark:bg-gray-700"></div>
            <div class="space-y-4">
                <div v-for="item in filteredTimeline" :key="item.id" class="relative flex gap-4 pl-1 group"
                    :class="item.source !== 'interaction' ? 'cursor-pointer' : ''"
                    @click="item.source !== 'interaction' && handleLinkedClick(item)">
                    <!-- Dot -->
                    <div :class="['w-10 h-10 rounded-full flex items-center justify-center flex-shrink-0 z-10', getTypeColor(item.type)]">
                        <component :is="getTypeIcon(item.type)" class="w-4 h-4" />
                    </div>
                    <!-- Content -->
                    <div class="flex-1 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl p-4 min-w-0"
                         :class="item.source !== 'interaction' ? 'hover:shadow-sm border-l-2 border-l-gray-300 dark:border-l-gray-600' : ''">
                        <div class="flex items-start justify-between gap-2 mb-1">
                            <div class="flex items-center gap-2 min-w-0">
                                <span class="text-xs font-semibold uppercase tracking-wider flex-shrink-0"
                                    :class="getTypeColor(item.type).split(' ').filter((c: string) => c.startsWith('text-')).join(' ')">
                                    {{ getTypeLabel(item.type) }}
                                </span>

                                <component v-if="item.mood && getMoodIcon(item.mood)" :is="getMoodIcon(item.mood)" class="w-3.5 h-3.5 text-gray-400 flex-shrink-0" />
                            </div>
                            <div class="flex items-center gap-1 flex-shrink-0">
                                <span class="text-xs text-gray-400">{{ formatDate(item.date) }}</span>
                                <button v-if="item.source === 'interaction'" @click.stop="deleteInteraction(item.id)"
                                    class="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-red-50 dark:hover:bg-red-900/20 text-gray-400 hover:text-red-500 transition-all">
                                    <X class="w-3 h-3" />
                                </button>
                            </div>
                        </div>
                        <!-- Interaction note -->
                        <p v-if="item.source === 'interaction'" class="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap">{{ item.note }}</p>
                        <!-- Linked node -->
                        <template v-else-if="item.source !== 'finance'">
                            <p class="text-sm font-medium text-blue-600 dark:text-blue-400 truncate">{{ item.title }}</p>
                            <p v-if="item.preview" class="text-xs text-gray-500 mt-1 line-clamp-2">{{ item.preview }}</p>
                            <div v-if="item.status" class="mt-1">
                                <span :class="['text-xs px-1.5 py-0.5 rounded', item.status === 'completed' ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300' : 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400']">
                                    {{ item.status }}
                                </span>
                            </div>
                        </template>
                        <!-- Finance block -->
                        <template v-else>
                            <div v-if="item.type === 'transaction'">
                                <p class="text-sm font-medium" :class="item.transaction.type === 'income' ? 'text-green-600' : item.transaction.type === 'expense' ? 'text-red-600' : 'text-blue-600'">
                                    {{ item.transaction.type === 'income' ? '+' : item.transaction.type === 'expense' ? '-' : '' }}{{ formatCurrency(item.transaction.amount) }}
                                </p>
                                <p class="text-xs text-gray-600 dark:text-gray-400 mt-1">{{ item.transaction.category }} <span v-if="item.transaction.note">· {{ item.transaction.note }}</span></p>
                            </div>
                            <div v-else-if="item.type === 'debt'">
                                <p class="text-sm font-medium" :class="item.debt.type === 'lend' ? 'text-green-600' : 'text-red-600'">
                                    {{ item.debt.type === 'lend' ? 'Lent' : 'Borrowed' }}: {{ formatCurrency(item.debt.totalAmount) }}
                                </p>
                                <p v-if="item.debt.note" class="text-xs text-gray-600 dark:text-gray-400 mt-1">{{ item.debt.note }}</p>
                                <div class="mt-2 text-xs">
                                    <span v-if="item.debt.status === 'active'" class="px-2 py-0.5 rounded-full font-medium" :class="item.debt.type === 'lend' ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300' : 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300'">
                                        Remaining: {{ formatCurrency(item.debt.totalAmount - item.debt.paidAmount) }}
                                    </span>
                                    <span v-else class="px-2 py-0.5 rounded-full bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-400 font-medium">Paid</span>
                                </div>
                            </div>
                        </template>
                    </div>
                </div>
            </div>
        </div>
    </div>
</template>
