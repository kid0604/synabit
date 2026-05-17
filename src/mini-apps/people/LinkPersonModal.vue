<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { X, Search, UserPlus, Users } from 'lucide-vue-next';
import { convertFileSrc } from '@tauri-apps/api/core';

const props = defineProps<{
    vaultPath: string;
    person: any;
    allPeople: any[];
    preselectedPersonId?: string;
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'link', targetPerson: any, relationType: string): void;
}>();

const searchQuery = ref('');
const selectedRelation = ref('');
const customRelation = ref('');
const selectedPerson = ref<any>(null);

onMounted(() => {
    if (props.preselectedPersonId) {
        const p = props.allPeople.find(pp => pp.id === props.preselectedPersonId);
        if (p) {
            selectedPerson.value = p;
            const existingConn = props.person?.properties?.connections?.find((c: any) => c.person_id === props.preselectedPersonId);
            if (existingConn) {
                const rt = existingConn.relation_type;
                if (RELATION_TYPES.find(r => r.value === rt)) {
                    selectedRelation.value = rt;
                } else {
                    selectedRelation.value = 'custom';
                    customRelation.value = rt;
                }
            }
        }
    }
});

const RELATION_TYPES = [
    { value: 'friend', label: '👫 Friend', color: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300' },
    { value: 'family', label: '👨‍👩‍👧 Family', color: 'bg-pink-100 text-pink-700 dark:bg-pink-900/30 dark:text-pink-300' },
    { value: 'colleague', label: '💼 Colleague', color: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300' },
    { value: 'partner', label: '❤️ Partner', color: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300' },
    { value: 'mentor', label: '🎓 Mentor', color: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300' },
    { value: 'mentee', label: '📚 Mentee', color: 'bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-300' },
    { value: 'neighbor', label: '🏠 Neighbor', color: 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300' },
    { value: 'introduced_by', label: '🤝 Introduced by', color: 'bg-teal-100 text-teal-700 dark:bg-teal-900/30 dark:text-teal-300' },
    { value: 'client', label: '📋 Client', color: 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-300' },
];

const existingConnectionIds = computed(() => {
    const conns: Array<{person_id: string}> = props.person?.properties?.connections || [];
    return new Set(conns.map(c => c.person_id));
});

const filteredPeople = computed(() => {
    return props.allPeople.filter(p => {
        if (p.id === props.person.id) return false;
        // Allow updating existing connections
        if (!searchQuery.value) return true;
        return p.title.toLowerCase().includes(searchQuery.value.toLowerCase());
    });
});

const getAvatarSrc = (p: any) => {
    if (p.properties?.avatar) {
        return convertFileSrc(`${props.vaultPath}/${p.properties.avatar}`);
    }
    return '';
};

const getInitials = (name: string) => {
    return name.split(' ').map(w => w[0]).join('').substring(0, 2).toUpperCase();
};

const canLink = computed(() => {
    if (!selectedPerson.value) return false;
    if (!selectedRelation.value) return false;
    if (selectedRelation.value === 'custom' && !customRelation.value.trim()) return false;
    return true;
});

const handleLink = () => {
    if (!canLink.value) return;
    const relationStr = selectedRelation.value === 'custom' 
        ? customRelation.value.trim() 
        : selectedRelation.value;
    emit('link', selectedPerson.value, relationStr);
};

onMounted(() => {
    const handleKeydown = (e: KeyboardEvent) => {
        if (e.key === 'Escape') emit('close');
    };
    window.addEventListener('keydown', handleKeydown);
    onUnmounted(() => window.removeEventListener('keydown', handleKeydown));
});
</script>

<template>
    <div class="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center p-4" @mousedown.self="emit('close')">
        <div class="w-full max-w-md bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl flex flex-col max-h-[80vh] overflow-hidden border border-gray-200 dark:border-gray-700">
            <!-- Header -->
            <div class="p-5 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between flex-shrink-0">
                <div class="flex items-center gap-3">
                    <div class="w-9 h-9 rounded-full bg-gradient-to-br from-purple-500 to-blue-500 flex items-center justify-center">
                        <UserPlus class="w-4 h-4 text-white" />
                    </div>
                    <div>
                        <h2 class="text-sm font-bold">Link a person</h2>
                        <p class="text-[11px] text-gray-500 dark:text-gray-400">to <strong>{{ person.title }}</strong></p>
                    </div>
                </div>
                <button @click="emit('close')" class="p-1.5 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors">
                    <X class="w-4 h-4" />
                </button>
            </div>

            <!-- Search -->
            <div class="px-5 pt-4 pb-2 border-b border-gray-100 dark:border-gray-800 flex-shrink-0">
                <div class="relative">
                    <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                    <input v-model="searchQuery" type="text" placeholder="Search contacts..."
                        class="w-full pl-9 pr-3 py-2 bg-gray-50 dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none transition-all" />
                </div>
            </div>

            <!-- People List -->
            <div class="flex-1 overflow-y-auto p-3">
                <div v-if="filteredPeople.length === 0" class="text-center py-8">
                    <Users class="w-8 h-8 mx-auto mb-2 text-gray-300 dark:text-gray-600" />
                    <p class="text-sm text-gray-500">{{ searchQuery ? 'No matching contacts' : 'All contacts are already linked' }}</p>
                </div>
                <button v-for="p in filteredPeople" :key="p.id"
                    @click="selectedPerson = p"
                    :class="['w-full text-left px-3 py-2.5 rounded-xl flex items-center gap-3 transition-colors mb-1 group',
                        selectedPerson?.id === p.id 
                            ? 'bg-blue-50 dark:bg-blue-900/40 ring-1 ring-blue-500' 
                            : 'hover:bg-gray-50 dark:hover:bg-gray-800/50']"
                >
                    <div class="w-9 h-9 rounded-full flex-shrink-0 flex items-center justify-center text-xs font-bold overflow-hidden"
                        :class="getAvatarSrc(p) ? '' : 'bg-gradient-to-br from-gray-200 to-gray-300 dark:from-gray-600 dark:to-gray-700 text-gray-600 dark:text-gray-300'">
                        <img v-if="getAvatarSrc(p)" :src="getAvatarSrc(p)" class="w-full h-full object-cover" />
                        <span v-else>{{ getInitials(p.title) }}</span>
                    </div>
                    <div class="flex-1 min-w-0">
                        <p class="text-sm font-medium truncate transition-colors"
                           :class="selectedPerson?.id === p.id ? 'text-blue-700 dark:text-blue-300' : 'group-hover:text-blue-600 dark:group-hover:text-blue-400'">{{ p.title }}</p>
                        <p v-if="p.properties?.companies?.length" class="text-[11px] text-gray-500 dark:text-gray-400 truncate">{{ p.properties.companies[0].value }}</p>
                        <p v-else-if="p.properties?.company" class="text-[11px] text-gray-500 dark:text-gray-400 truncate">{{ p.properties.company }}</p>
                    </div>
                    
                    <div class="w-5 h-5 rounded-full flex items-center justify-center border flex-shrink-0 transition-colors"
                        :class="selectedPerson?.id === p.id ? 'border-blue-500 bg-blue-500 text-white' : 'border-gray-300 dark:border-gray-600'">
                        <svg v-if="selectedPerson?.id === p.id" xmlns="http://www.w3.org/2000/svg" class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>
                    </div>
                </button>
            </div>

            <!-- Relation Type Selector (Bottom) -->
            <div class="px-5 pt-3 pb-4 border-t border-gray-100 dark:border-gray-800 flex-shrink-0 bg-gray-50/30 dark:bg-gray-800/20">
                <label class="text-[10px] font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-2 block">Relationship</label>
                <div class="flex flex-wrap gap-1.5">
                    <button v-for="rel in RELATION_TYPES" :key="rel.value"
                        @click="selectedRelation = rel.value"
                        :class="['px-2.5 py-1 text-xs font-medium rounded-lg border transition-all',
                            selectedRelation === rel.value
                                ? rel.color + ' border-transparent ring-2 ring-offset-1 ring-blue-500/30 dark:ring-offset-[#1e1e1e]'
                                : 'border-gray-200 dark:border-gray-700 text-gray-500 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800']"
                    >{{ rel.label }}</button>
                    <!-- Custom Option -->
                    <button @click="selectedRelation = 'custom'"
                        :class="['px-2.5 py-1 text-xs font-medium rounded-lg border transition-all',
                            selectedRelation === 'custom'
                                ? 'bg-gray-800 text-white dark:bg-gray-200 dark:text-gray-900 border-transparent ring-2 ring-offset-1 ring-gray-500/30 dark:ring-offset-[#1e1e1e]'
                                : 'border-gray-200 dark:border-gray-700 text-gray-500 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800']"
                    >✨ Custom</button>
                </div>
                
                <div v-if="selectedRelation === 'custom'" class="mt-3">
                    <input v-model="customRelation" type="text" placeholder="E.g. Boss, Doctor, Ex-colleague..."
                        class="w-full px-3 py-1.5 bg-gray-50 dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg text-sm focus:ring-2 focus:ring-gray-500 outline-none transition-all" />
                </div>
            </div>

            <!-- Footer Actions -->
            <div class="p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-black/20 flex justify-end gap-3 flex-shrink-0">
                <button @click="emit('close')" class="px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors">
                    Cancel
                </button>
                <button @click="handleLink" :disabled="!canLink"
                    :class="['px-5 py-2 text-sm font-bold rounded-lg transition-all flex items-center gap-2',
                        canLink ? 'bg-blue-600 hover:bg-blue-700 text-white shadow-sm hover:shadow-md' : 'bg-gray-200 dark:bg-gray-800 text-gray-400 dark:text-gray-600 cursor-not-allowed']"
                >
                    <UserPlus class="w-4 h-4" />
                    Link Connection
                </button>
            </div>
        </div>
    </div>
</template>
