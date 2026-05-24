<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { Users, Plus, Mail, Phone, Building, Hash, Search, Edit2, Gift, Briefcase, LayoutDashboard, Clock, FileText, Share2, ArrowUpDown, AlertCircle, CalendarPlus, UserPlus } from 'lucide-vue-next';
import PersonModal from './PersonModal.vue';
import GiftModal from './GiftModal.vue';
import OverviewTab from './OverviewTab.vue';
import NotesTab from './NotesTab.vue';
import TimelineTab from './TimelineTab.vue';
import GraphTab from './GraphTab.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import RemindersWidget from './RemindersWidget.vue';
import LinkPersonModal from './LinkPersonModal.vue';
import PeopleManager from './PeopleManager.vue';

import { logger } from '../../utils/logger';

// Helper: get first detail value by label keyword
const getPersonDetail = (person: any, keyword: string): string => {
    const d = person?.properties?.details?.find((d: any) => d.label.toLowerCase().includes(keyword));
    return d?.value || person?.properties?.[keyword] || '';
};

const getDisplayName = (person: any): string => {
    const p = person?.properties;
    if (p?.display_name === 'nickname' && p?.nickname) return p.nickname;
    if (p?.display_name === 'custom' && p?.custom_display) return p.custom_display;
    return person.title;
};

const props = defineProps<{
    vaultPath: string;
}>();

const emit = defineEmits(['open-node']);

const people = ref<any[]>([]);
const searchQuery = ref('');
const loading = ref(true);
const showModal = ref(false);
const selectedPerson = ref<any | null>(null);
const activeTab = ref<'overview' | 'timeline' | 'notes' | 'graph'>('overview');
const sortMode = ref<'alpha' | 'recent' | 'attention'>('recent');
const showGiftModal = ref(false);

// Linked nodes for Notes/Timeline tabs
const linkedNodes = ref<any[]>([]);
const loadingLinks = ref(false);

const fetchPeople = async () => {
    loading.value = true;
    try {
        const nodes = await invoke<any[]>('get_nodes', { nodeType: 'person' });
        people.value = nodes;
        if (selectedPerson.value) {
            const updated = nodes.find(n => n.id === selectedPerson.value.id);
            selectedPerson.value = updated || null;
        }
        // Ensure owner person exists
        await ensureOwner();
    } catch (e) {
        logger.error('Failed to fetch people nodes', e);
    } finally {
        loading.value = false;
    }
};

const ensureOwner = async () => {
    const hasOwner = people.value.some(p => p.properties?.is_owner === true);
    if (hasOwner) return;
    try {
        const relPath = `People/owner.md`;
        await invoke('write_node_file', {
            vaultPath: props.vaultPath, relPath, title: 'Me',
            nodeType: 'person',
            properties: { is_owner: true, tags: ['owner'] },
            content: ''
        });
        const nodes = await invoke<any[]>('get_nodes', { nodeType: 'person' });
        people.value = nodes;
    } catch (e) {
        logger.error('Failed to create owner person', e);
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
    activeTab.value = 'overview';
});

onMounted(() => {
    fetchPeople();
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
    let list = people.value.filter(p => !p.properties?.is_owner);
    if (searchQuery.value) {
        const q = searchQuery.value.toLowerCase();
        list = list.filter(p => {
            if (p.title.toLowerCase().includes(q)) return true;
            if (p.properties.relationship_type && p.properties.relationship_type.toLowerCase().includes(q)) return true;
            // Search across all details
            if (p.properties.details?.some((d: any) => d.value.toLowerCase().includes(q) || d.label.toLowerCase().includes(q))) return true;
            // Legacy fallback
            if (p.properties.email && p.properties.email.toLowerCase().includes(q)) return true;
            if (p.properties.company && p.properties.company.toLowerCase().includes(q)) return true;
            return false;
        });
    }
    // Sort
    if (sortMode.value === 'alpha') {
        list = [...list].sort((a, b) => a.title.localeCompare(b.title));
    } else if (sortMode.value === 'recent') {
        list = [...list].sort((a, b) => {
            const aTime = a.updated_at ? new Date(a.updated_at).getTime() : (a.created_at ? new Date(a.created_at).getTime() : 0);
            const bTime = b.updated_at ? new Date(b.updated_at).getTime() : (b.created_at ? new Date(b.created_at).getTime() : 0);
            return bTime - aTime;
        });
    } else if (sortMode.value === 'attention') {
        list = [...list].sort((a, b) => {
            const aScore = getHealthScore(a);
            const bScore = getHealthScore(b);
            return aScore - bScore;
        });
    }
    return list;
});

const sidebarPeople = computed(() => {
    return filteredPeople.value.slice(0, 20);
});

const needsAttentionCount = computed(() => {
    return people.value.filter(p => {
        const dot = getContactHealthDot(p);
        return dot === 'bg-red-500' || dot === 'bg-yellow-500';
    }).length;
});

const topRelationships = computed(() => {
    const counts: Record<string, number> = {};
    people.value.forEach(p => {
        if (p.properties?.relationship_type) {
            const relsArr = p.properties.relationship_type.split(',').map((s: string) => s.toLowerCase().trim());
            relsArr.forEach((r: string) => {
                if (r) counts[r] = (counts[r] || 0) + 1;
            });
        }
    });
    return Object.entries(counts)
        .sort((a, b) => b[1] - a[1]) // Sort by frequency descending
        .slice(0, 10) // Top 10
        .map(([r]) => r.split(' ').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '));
});

const allRelationships = computed(() => {
    const rels = new Set<string>();
    people.value.forEach(p => {
        if (p.properties?.relationship_type) {
            const relsArr = p.properties.relationship_type.split(',').map((s: string) => s.toLowerCase().trim());
            relsArr.forEach((r: string) => {
                if (r) rels.add(r);
            });
        }
    });
    return Array.from(rels)
        .filter(r => r)
        .map(r => r.split(' ').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '))
        .sort();
});

const cycleSortMode = () => {
    const modes: Array<'alpha' | 'recent' | 'attention'> = ['alpha', 'recent', 'attention'];
    const idx = modes.indexOf(sortMode.value);
    sortMode.value = modes[(idx + 1) % modes.length];
};

const sortLabel = computed(() => {
    const labels: Record<string, string> = { alpha: 'A-Z', recent: 'Recent', attention: 'Needs Attention' };
    return labels[sortMode.value];
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
    for (let i = 0; i < tag.length; i++) hash = tag.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
};

const openLinkedNode = (node: any) => emit('open-node', node.id, node.node_type);

const getAvatarSrc = (person: any) => {
    if (!person?.properties?.avatar) return '';
    return convertFileSrc(`${props.vaultPath}/${person.properties.avatar}`);
};

const FREQ_DAYS: Record<string, number> = { weekly: 7, biweekly: 14, monthly: 30, quarterly: 90, yearly: 365 };

const getHealthScore = (person: any): number => {
    const last = person?.properties?.last_contacted;
    const freq = person?.properties?.contact_frequency;
    if (!last || !freq) return 50;
    const days = Math.floor((Date.now() - new Date(last).getTime()) / (1000 * 60 * 60 * 24));
    const threshold = FREQ_DAYS[freq] || 60;
    return Math.max(0, Math.min(100, Math.round((1 - days / threshold) * 100)));
};

const getContactHealthDot = (person: any) => {
    const last = person?.properties?.last_contacted;
    const freq = person?.properties?.contact_frequency;
    if (!last || !freq) return '';
    const days = Math.floor((Date.now() - new Date(last).getTime()) / (1000 * 60 * 60 * 24));
    const threshold = FREQ_DAYS[freq] || 60;
    const ratio = days / threshold;
    if (ratio <= 0.5) return 'bg-green-500';
    if (ratio <= 0.85) return 'bg-blue-500';
    if (ratio <= 1.2) return 'bg-yellow-500';
    return 'bg-red-500';
};

const tabs = [
    { id: 'overview', label: 'Overview', icon: LayoutDashboard },
    { id: 'timeline', label: 'Timeline', icon: Clock },
    { id: 'notes', label: 'Notes', icon: FileText },
    { id: 'graph', label: 'Graph', icon: Share2 },
];

const handleTimelineUpdated = () => {
    fetchPeople();
};

const handleGiftSaved = async (gift: any) => {
    if (!selectedPerson.value) return;
    try {
        const currentGifts = [...(selectedPerson.value.properties.gifts || [])];
        currentGifts.unshift(gift);
        const properties = { ...selectedPerson.value.properties, gifts: currentGifts };
        await invoke('write_node_file', {
            vaultPath: props.vaultPath, relPath: selectedPerson.value.id,
            title: selectedPerson.value.title, nodeType: 'person',
            properties, content: selectedPerson.value.content || ''
        });
        fetchPeople();
    } catch (e) {
        logger.error('Failed to save gift', e);
    }
};

const openPersonById = async (id: string) => {
    if (people.value.length === 0) await fetchPeople();
    let p = people.value.find(p => p.id === id);
    if (!p) p = people.value.find(p => p.title.toLowerCase() === id.toLowerCase());
    if (!p) {
        const slug = id.replace(/[^a-z0-9]/gi, '_').toLowerCase();
        if (slug.length > 2) p = people.value.find(p => p.id.toLowerCase().includes(slug));
    }
    if (p) selectedPerson.value = p;
};

const syncBirthdaysToCalendar = async () => {
    const withBirthdays = people.value.filter(p => p.properties?.birthday);
    if (withBirthdays.length === 0) return;

    let synced = 0;
    const thisYear = new Date().getFullYear();
    for (const p of withBirthdays) {
        const bday = p.properties.birthday;
        const parts = bday.split('-');
        let month: string, day: string;
        if (parts.length === 3) {
            month = parts[1]; day = parts[2];
        } else if (parts.length === 2) {
            month = parts[0]; day = parts[1];
        } else continue;

        const eventDate = `${thisYear}-${month}-${day}`;
        const eventTitle = `🎂 ${p.title}'s Birthday`;
        const relPath = `Events/bday_${p.title.toLowerCase().replace(/[^a-z0-9]/g, '_')}_${thisYear}.md`;

        try {
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath,
                title: eventTitle,
                nodeType: 'event',
                properties: {
                    is_all_day: true,
                    start_at: eventDate,
                    end_at: eventDate,
                    tags: ['birthday', 'people'],
                    source_person: p.title,
                },
                content: `Birthday reminder for ${p.title}.`
            });
            synced++;
        } catch (e) {
            logger.error(`Failed to sync birthday for ${p.title}`, e);
        }
    }
    logger.info(`Synced ${synced} birthdays to calendar`);
};

// --- Person-to-Person Linking ---
const showLinkModal = ref(false);
const editLinkTargetId = ref<string | undefined>(undefined);

const openEditLink = (targetId: string) => {
    editLinkTargetId.value = targetId;
    showLinkModal.value = true;
};

const closeLinkModal = () => {
    showLinkModal.value = false;
    editLinkTargetId.value = undefined;
};

const linkPerson = async (targetPerson: any, relationType: string) => {
    if (!selectedPerson.value) return;
    const src = selectedPerson.value;
    const srcProps = { ...(src.properties || {}) };
    let srcConns: Array<{person_id: string; name: string; relation_type: string}> = [...(srcProps.connections || [])];
    
    // Update if exists, otherwise push
    const existingIdx = srcConns.findIndex(c => c.person_id === targetPerson.id);
    if (existingIdx >= 0) {
        srcConns[existingIdx].relation_type = relationType;
    } else {
        srcConns.push({ person_id: targetPerson.id, name: targetPerson.title, relation_type: relationType });
    }
    srcProps.connections = srcConns;

    // Add the relation link to the markdown content (for graph edge)
    const mention = `[${targetPerson.title}](synabit://person/${targetPerson.id})`;
    const relations = [...(srcProps.relations || [])];
    if (!relations.find((r: string) => r.includes(targetPerson.id))) {
        relations.push(mention);
        srcProps.relations = relations;
    }

    try {
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: src.id,
            title: src.title,
            nodeType: 'person',
            properties: srcProps,
            content: src.content || ''
        });

        // Bidirectional: also add connection on target ONLY if it doesn't exist
        // We do NOT update the target's existing relation, to allow asymmetric labels (e.g. Mother -> Child)
        const tgtProps = { ...(targetPerson.properties || {}) };
        
        const REVERSE_RELATIONS: Record<string, string> = {
            'friend': 'friend',
            'family': 'family',
            'colleague': 'colleague',
            'partner': 'partner',
            'mentor': 'mentee',
            'mentee': 'mentor',
            'neighbor': 'neighbor',
            'client': 'provider',
            'introduced_by': 'introduced'
        };
        const reverseType = REVERSE_RELATIONS[relationType] || 'linked';
        
        const tgtConns: Array<{person_id: string; name: string; relation_type: string}> = [...(tgtProps.connections || [])];
        if (!tgtConns.find(c => c.person_id === src.id)) {
            tgtConns.push({ person_id: src.id, name: src.title, relation_type: reverseType });
            tgtProps.connections = tgtConns;
            const tgtRelations = [...(tgtProps.relations || [])];
            const srcMention = `[${src.title}](synabit://person/${src.id})`;
            if (!tgtRelations.find((r: string) => r.includes(src.id))) {
                tgtRelations.push(srcMention);
                tgtProps.relations = tgtRelations;
            }
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: targetPerson.id,
                title: targetPerson.title,
                nodeType: 'person',
                properties: tgtProps,
                content: targetPerson.content || ''
            });
        }

        showLinkModal.value = false;
        await fetchPeople();
        // Re-select to refresh
        const updated = people.value.find(p => p.id === src.id);
        if (updated) selectedPerson.value = updated;
    } catch (e) {
        logger.error('Failed to link person', e);
    }
};

const unlinkPerson = async (targetPersonId: string) => {
    if (!selectedPerson.value) return;
    const src = selectedPerson.value;
    const srcProps = { ...(src.properties || {}) };
    srcProps.connections = (srcProps.connections || []).filter((c: any) => c.person_id !== targetPersonId);
    srcProps.relations = (srcProps.relations || []).filter((r: string) => !r.includes(targetPersonId));

    try {
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: src.id,
            title: src.title,
            nodeType: 'person',
            properties: srcProps,
            content: src.content || ''
        });

        // Remove bidirectional link
        const target = people.value.find(p => p.id === targetPersonId);
        if (target) {
            const tgtProps = { ...(target.properties || {}) };
            tgtProps.connections = (tgtProps.connections || []).filter((c: any) => c.person_id !== src.id);
            tgtProps.relations = (tgtProps.relations || []).filter((r: string) => !r.includes(src.id));
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: target.id,
                title: target.title,
                nodeType: 'person',
                properties: tgtProps,
                content: target.content || ''
            });
        }

        await fetchPeople();
        const updated = people.value.find(p => p.id === src.id);
        if (updated) selectedPerson.value = updated;
    } catch (e) {
        logger.error('Failed to unlink person', e);
    }
};

defineExpose({ openPersonById });
</script>

<template>
    <div class="h-full flex bg-base dark:bg-base-dark text-text dark:text-text-dark overflow-hidden">

        <!-- LEFT PANEL: People List -->
        <div class="w-80 flex-shrink-0 border-r border-border dark:border-border-dark flex flex-col bg-surface dark:bg-surface-dark">
            <!-- Header -->
            <div class="h-14 border-b border-border dark:border-border-dark flex items-center justify-between px-4 flex-shrink-0" data-tauri-drag-region>
                <div class="flex items-center gap-2 font-semibold">
                    <NavButtons />
                    <Users class="w-4 h-4 text-text-secondary dark:text-text-secondary-dark" />
                    <span>People</span>
                </div>
                <div class="flex items-center gap-1">
                    <button @click="openNewModal" class="p-1.5 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors text-blue-500" title="Add contact">
                        <Plus class="w-5 h-5" />
                    </button>
                    <button @click="syncBirthdaysToCalendar" class="p-1.5 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors text-pink-500" title="Sync birthdays to Calendar">
                        <CalendarPlus class="w-4 h-4" />
                    </button>
                </div>
            </div>

            <!-- Search + Sort -->
            <div class="p-3 border-b border-border dark:border-border-dark space-y-2">
                <div class="relative">
                    <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                    <input v-model="searchQuery" type="text" placeholder="Search..." class="w-full pl-9 pr-3 py-1.5 bg-gray-100 dark:bg-gray-800 border-none rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none transition-all" />
                </div>
                <div class="flex items-center justify-between">
                    <button @click="cycleSortMode" class="flex items-center gap-1.5 text-xs text-gray-500 dark:text-gray-400 hover:text-blue-500 transition-colors px-1.5 py-1 rounded">
                        <ArrowUpDown class="w-3 h-3" /> {{ sortLabel }}
                    </button>
                    <div class="flex items-center gap-2">
                        <div v-if="needsAttentionCount > 0" class="flex items-center gap-1 text-xs">
                            <AlertCircle class="w-3 h-3 text-orange-500" />
                            <span class="text-orange-500 font-medium">{{ needsAttentionCount }}</span>
                        </div>
                        <button @click="selectedPerson = null" class="text-[10px] text-blue-500 hover:text-blue-600 font-medium px-1.5 py-1">Show all</button>
                    </div>
                </div>
            </div>

            <!-- List -->
            <div class="flex-1 overflow-y-auto p-2">
                <!-- Reminders Widget -->
                <RemindersWidget :people="people" @select-person="(p: any) => selectedPerson = p" />

                <div v-if="loading" class="flex justify-center p-4">
                    <div class="animate-spin rounded-full h-5 w-5 border-b-2 border-blue-500"></div>
                </div>
                <div v-else-if="sidebarPeople.length === 0" class="text-center p-4 text-sm text-gray-500">No contacts found.</div>
                <div v-else class="space-y-1">
                    <button
                        v-for="person in sidebarPeople" :key="person.id"
                        @click="selectedPerson = person"
                        :class="['w-full text-left px-3 py-2 rounded-lg flex items-center gap-3 transition-colors',
                            selectedPerson?.id === person.id
                                ? 'bg-blue-50 dark:bg-blue-900/30 ring-1 ring-blue-500/50'
                                : 'hover:bg-gray-100 dark:hover:bg-gray-800/50'
                        ]"
                    >
                        <!-- Avatar or Initials -->
                        <div class="w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold flex-shrink-0 overflow-hidden relative"
                             :class="person.properties?.avatar ? '' : 'bg-gradient-to-br from-gray-200 to-gray-300 dark:from-gray-700 dark:to-gray-800 text-gray-700 dark:text-gray-300'">
                            <img v-if="getAvatarSrc(person)" :src="getAvatarSrc(person)" class="w-full h-full object-cover" />
                            <span v-else>{{ getInitials(getDisplayName(person)) }}</span>
                            <!-- Health dot -->
                            <div v-if="getContactHealthDot(person)" :class="['absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-white dark:border-gray-900', getContactHealthDot(person)]"></div>
                        </div>
                        <div class="flex-1 min-w-0">
                            <h4 class="font-medium text-sm truncate">{{ getDisplayName(person) }}</h4>
                            <p v-if="getPersonDetail(person, 'company')" class="text-xs text-gray-500 dark:text-gray-400 truncate flex items-center gap-1 mt-0.5">
                                <Building class="w-3 h-3 flex-shrink-0" />
                                <span class="truncate">{{ getPersonDetail(person, 'company') }}</span>
                            </p>
                            <p v-else-if="person.properties?.relationship_type" class="text-xs text-gray-400 truncate mt-0.5 capitalize">{{ person.properties.relationship_type }}</p>
                            <p v-else-if="person.properties?.tags?.length" class="text-xs text-gray-500 dark:text-gray-400 truncate flex items-center gap-1 mt-0.5">
                                <Hash class="w-3 h-3 flex-shrink-0" />
                                <span class="truncate">{{ person.properties.tags.join(', ') }}</span>
                            </p>
                        </div>
                    </button>
                    
                    <button v-if="filteredPeople.length > 20" @click="selectedPerson = null" class="w-full text-center py-2.5 mt-2 text-xs font-medium text-blue-500 hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors">
                        Show {{ filteredPeople.length - 20 }} more...
                    </button>
                </div>
            </div>
        </div>

        <!-- RIGHT PANEL -->
        <div class="flex-1 flex flex-col bg-base dark:bg-base-dark overflow-hidden relative">
            <!-- All People (when no person selected) -->
            <PeopleManager v-if="!selectedPerson"
                :people="filteredPeople"
                :vault-path="vaultPath"
                @select="(p: any) => { selectedPerson = p; }"
                @edit="(p: any) => editPerson(p)"
                @delete="async (p: any) => { if (p.properties?.is_owner) return; const yes = await ask(`This will permanently delete &quot;${p.title}&quot; and all associated data. This action cannot be undone.`, { title: 'Delete contact?', kind: 'warning', okLabel: 'Delete', cancelLabel: 'Cancel' }); if (yes) { await invoke('delete_node_file', { vaultPath, relPath: p.id }); fetchPeople(); } }"
            />

            <div v-if="selectedPerson" class="flex-1 flex flex-col overflow-hidden">
                <!-- Profile Header -->
                <div class="flex-shrink-0 px-8 pt-8 pb-4">
                    <div class="flex items-start gap-5 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-5 shadow-sm relative group">
                        <button @click="editPerson(selectedPerson)" class="absolute top-4 right-4 p-2 text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/30 rounded-lg opacity-0 group-hover:opacity-100 transition-all">
                            <Edit2 class="w-4 h-4" />
                        </button>

                        <!-- Avatar -->
                        <div class="w-20 h-20 rounded-2xl flex items-center justify-center text-2xl font-bold flex-shrink-0 overflow-hidden shadow-md"
                             :class="getAvatarSrc(selectedPerson) ? '' : 'bg-gradient-to-br from-blue-500 to-purple-600 text-white'">
                            <img v-if="getAvatarSrc(selectedPerson)" :src="getAvatarSrc(selectedPerson)" class="w-full h-full object-cover" />
                            <span v-else>{{ getInitials(getDisplayName(selectedPerson)) }}</span>
                        </div>

                        <div class="flex-1 min-w-0">
                            <div class="flex items-center gap-3 mb-1">
                                <h1 class="text-xl font-bold text-gray-900 dark:text-white truncate">{{ getDisplayName(selectedPerson) }}</h1>
                                <span v-if="selectedPerson.properties?.relationship_type" class="px-2 py-0.5 text-xs font-medium rounded-full bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300 capitalize flex-shrink-0">
                                    {{ selectedPerson.properties.relationship_type }}
                                </span>
                            </div>

                            <!-- Subtitle from details -->
                            <p v-if="getPersonDetail(selectedPerson, 'company') || getPersonDetail(selectedPerson, 'role')" class="text-sm text-gray-600 dark:text-gray-300 flex items-center gap-1.5 mb-3">
                                <Briefcase v-if="getPersonDetail(selectedPerson, 'role')" class="w-3.5 h-3.5 opacity-60" />
                                <span v-if="getPersonDetail(selectedPerson, 'role')">{{ getPersonDetail(selectedPerson, 'role') }}</span>
                                <span v-if="getPersonDetail(selectedPerson, 'role') && getPersonDetail(selectedPerson, 'company')" class="text-gray-400">@</span>
                                <span v-if="getPersonDetail(selectedPerson, 'company')" class="text-blue-600 dark:text-blue-400 font-medium">{{ getPersonDetail(selectedPerson, 'company') }}</span>
                            </p>

                            <!-- Details info row -->
                            <div class="flex flex-wrap gap-x-5 gap-y-1.5 text-xs text-gray-500 dark:text-gray-400">
                                <template v-for="d in (selectedPerson.properties?.details || [])" :key="d.label + d.value">
                                    <a v-if="d.type === 'email'" :href="'mailto:' + d.value" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                        <Mail class="w-3.5 h-3.5" /> <span class="opacity-50">{{ d.label }}:</span> {{ d.value }}
                                    </a>
                                    <a v-else-if="d.type === 'phone'" :href="'tel:' + d.value" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                        <Phone class="w-3.5 h-3.5" /> <span class="opacity-50">{{ d.label }}:</span> {{ d.value }}
                                    </a>
                                    <span v-else-if="d.type === 'url'" class="flex items-center gap-1.5">
                                        <span class="opacity-50">{{ d.label }}:</span>
                                        <a :href="d.value" target="_blank" class="hover:text-blue-500 transition-colors truncate max-w-[180px]">{{ d.value.replace(/^https?:\/\//, '') }}</a>
                                    </span>
                                    <span v-else class="flex items-center gap-1.5">
                                        <span class="opacity-50">{{ d.label }}:</span> {{ d.value }}
                                    </span>
                                </template>
                                <!-- Legacy fallbacks -->
                                <a v-if="!selectedPerson.properties?.details?.length && selectedPerson.properties?.email" :href="'mailto:' + selectedPerson.properties.email" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                    <Mail class="w-3.5 h-3.5" /> {{ selectedPerson.properties.email }}
                                </a>
                                <a v-if="!selectedPerson.properties?.details?.length && selectedPerson.properties?.phone" :href="'tel:' + selectedPerson.properties.phone" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                    <Phone class="w-3.5 h-3.5" /> {{ selectedPerson.properties.phone }}
                                </a>
                                <span v-if="selectedPerson.properties?.birthday" class="flex items-center gap-1.5">
                                    <Gift class="w-3.5 h-3.5 text-pink-500" /> {{ selectedPerson.properties.birthday }}
                                </span>
                            </div>

                            <!-- Tags -->
                            <div v-if="selectedPerson.properties?.tags?.length > 0" class="flex flex-wrap gap-1.5 mt-3">
                                <span v-for="tag in selectedPerson.properties.tags" :key="tag"
                                    :class="['px-2 py-0.5 text-xs font-medium rounded-md flex items-center gap-1', getTagColor(tag)]">
                                    <Hash class="w-2.5 h-2.5 opacity-50" /> {{ tag }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>

                <!-- Tab Bar -->
                <div class="flex-shrink-0 px-8">
                    <div class="flex items-center gap-1 border-b border-border dark:border-border-dark">
                        <button
                            v-for="tab in tabs" :key="tab.id"
                            @click="activeTab = tab.id as any"
                            :class="[
                                'flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium border-b-2 transition-all -mb-px',
                                activeTab === tab.id
                                    ? 'border-blue-500 text-blue-600 dark:text-blue-400'
                                    : 'border-transparent text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 hover:border-gray-300 dark:hover:border-gray-600'
                            ]"
                        >
                            <component :is="tab.icon" class="w-4 h-4" />
                            {{ tab.label }}
                        </button>
                        <div class="ml-auto -mb-px flex items-center gap-1">
                            <button @click="showLinkModal = true" class="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-purple-500 hover:bg-purple-50 dark:hover:bg-purple-900/20 rounded-lg transition-colors">
                                <UserPlus class="w-3.5 h-3.5" /> Link Person
                            </button>
                            <button @click="showGiftModal = true" class="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-pink-500 hover:bg-pink-50 dark:hover:bg-pink-900/20 rounded-lg transition-colors">
                                <Gift class="w-3.5 h-3.5" /> Log Gift
                            </button>
                        </div>
                    </div>
                </div>

                <!-- Tab Content -->
                <div class="flex-1 overflow-y-auto px-8 py-6">
                    <div class="max-w-3xl mx-auto">
                        <OverviewTab v-if="activeTab === 'overview'" :person="selectedPerson" />
                        <TimelineTab v-else-if="activeTab === 'timeline'" :person="selectedPerson" :vault-path="vaultPath" :linked-nodes="linkedNodes" @updated="handleTimelineUpdated" @open-linked-node="openLinkedNode" />
                        <NotesTab v-else-if="activeTab === 'notes'" :person="selectedPerson" :linked-nodes="linkedNodes" :loading-links="loadingLinks" @open-linked-node="openLinkedNode" />
                        <GraphTab v-else-if="activeTab === 'graph'" :person="selectedPerson" :all-people="people" :vault-path="vaultPath" @select-person="(p: any) => selectedPerson = p" @unlink="unlinkPerson" @edit-link="openEditLink" />
                    </div>
                </div>
            </div>
        </div>

        <PersonModal
            v-if="showModal"
            :person="selectedPerson"
            :vault-path="vaultPath"
            :top-relationships="topRelationships"
            :all-relationships="allRelationships"
            @close="showModal = false"
            @saved="fetchPeople"
        />

        <GiftModal
            v-if="showGiftModal && selectedPerson"
            :person="selectedPerson"
            @close="showGiftModal = false"
            @save="handleGiftSaved"
        />

        <LinkPersonModal
            v-if="showLinkModal && selectedPerson"
            :vault-path="vaultPath"
            :person="selectedPerson"
            :all-people="people"
            :preselected-person-id="editLinkTargetId"
            @close="closeLinkModal"
            @link="linkPerson"
        />
    </div>
</template>
