<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Search, X, LayoutGrid, List, Table, Mail, Phone, Edit2, Trash2, Users, Hash, Building } from 'lucide-vue-next';

const props = defineProps<{
    people: any[];
    vaultPath: string;
}>();

const emit = defineEmits<{
    (e: 'select', person: any): void;
    (e: 'edit', person: any): void;
    (e: 'delete', person: any): void;
}>();

type ViewMode = 'table' | 'list' | 'card';
const viewMode = ref<ViewMode>('table');
const searchQuery = ref('');
const currentPage = ref(1);
const itemsPerPage = 50;

const getInitials = (name: string) => {
    if (!name) return '?';
    return name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
};

const getAvatarSrc = (person: any) => {
    if (!person.properties?.avatar) return '';
    return convertFileSrc(`${props.vaultPath}/${person.properties.avatar}`);
};

const getDetail = (person: any, keyword: string): string => {
    const d = person?.properties?.details?.find((d: any) => d.label.toLowerCase().includes(keyword));
    return d?.value || person?.properties?.[keyword] || '';
};

const filtered = computed(() => {
    if (!searchQuery.value.trim()) return props.people;
    const q = searchQuery.value.toLowerCase();
    return props.people.filter(p => {
        if (p.title.toLowerCase().includes(q)) return true;
        if (p.properties?.relationship_type?.toLowerCase().includes(q)) return true;
        if (p.properties?.details?.some((d: any) => d.value.toLowerCase().includes(q) || d.label.toLowerCase().includes(q))) return true;
        if (p.properties?.email?.toLowerCase().includes(q)) return true;
        if (p.properties?.company?.toLowerCase().includes(q)) return true;
        return false;
    });
});

const totalPages = computed(() => Math.ceil(filtered.value.length / itemsPerPage));
const paginated = computed(() => {
    const start = (currentPage.value - 1) * itemsPerPage;
    return filtered.value.slice(start, start + itemsPerPage);
});

watch([searchQuery], () => { currentPage.value = 1; });

const formatDate = (d: string) => {
    if (!d) return '—';
    return d.split('T')[0];
};
</script>

<template>
    <div class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] h-full relative z-0 overflow-y-auto">
        <!-- Header bar -->
        <div class="flex items-center justify-between px-6 h-10 border-b border-[#e6e6e6] dark:border-[#2c2c2c] shrink-0 sticky top-0 bg-[#fdfdfc] dark:bg-[#242424] z-10" data-tauri-drag-region>
            <div class="flex items-center gap-3">
                <Users class="w-4.5 h-4.5 text-blue-500" />
                <h1 class="text-lg font-bold text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
                    All People
                    <span class="text-[12px] font-medium px-2 py-0.5 mt-0.5 rounded-full bg-gray-100 dark:bg-[#333] text-gray-500">{{ filtered.length }}</span>
                </h1>
            </div>
            <div class="flex items-center gap-3">
            <div class="flex items-center gap-1 bg-gray-100 dark:bg-[#333] rounded-lg p-0.5">
                <button @click="viewMode = 'table'" :class="['p-1.5 rounded-md transition-colors', viewMode === 'table' ? 'bg-white dark:bg-[#555] shadow-sm text-blue-600' : 'text-gray-400 hover:text-gray-600']" title="Table">
                    <Table class="w-3.5 h-3.5" />
                </button>
                <button @click="viewMode = 'list'" :class="['p-1.5 rounded-md transition-colors', viewMode === 'list' ? 'bg-white dark:bg-[#555] shadow-sm text-blue-600' : 'text-gray-400 hover:text-gray-600']" title="List">
                    <List class="w-3.5 h-3.5" />
                </button>
                <button @click="viewMode = 'card'" :class="['p-1.5 rounded-md transition-colors', viewMode === 'card' ? 'bg-white dark:bg-[#555] shadow-sm text-blue-600' : 'text-gray-400 hover:text-gray-600']" title="Cards">
                    <LayoutGrid class="w-3.5 h-3.5" />
                </button>
            </div>
            </div>
        </div>

        <!-- Content -->
        <div class="flex-1 flex flex-col p-8 w-full max-w-5xl mx-auto">
            <!-- Search -->
            <div class="relative w-full mb-8">
                <Search class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-[#8b8b8b]" />
                <input v-model="searchQuery" type="text" placeholder="Search people..." class="w-full pl-12 pr-12 py-3 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl text-base text-text dark:text-text-dark shadow-sm focus:outline-none focus:ring-2 focus:ring-blue-500/50 transition-shadow placeholder:text-gray-400" />
                <button v-if="searchQuery" @click="searchQuery = ''" class="absolute right-4 top-1/2 -translate-y-1/2 p-1.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-400 hover:text-gray-600 transition-colors">
                    <X class="w-4 h-4" />
                </button>
            </div>

            <!-- TABLE VIEW -->
            <div v-if="viewMode === 'table'" class="w-full">
                <div class="bg-white dark:bg-[#252525] border border-[#e6e6e6] dark:border-[#333] rounded-xl overflow-hidden shadow-sm">
                    <table class="w-full text-left border-collapse">
                        <thead>
                            <tr class="bg-gray-50 dark:bg-[#1a1a1a] border-b border-[#e6e6e6] dark:border-[#333]">
                                <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-8"></th>
                                <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase">Name</th>
                                <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase">Info</th>
                                <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase">Tags</th>
                                <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase text-right whitespace-nowrap">Added</th>
                                <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-16 text-center">Action</th>
                            </tr>
                        </thead>
                        <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#333] text-sm">
                            <tr v-for="person in paginated" :key="person.id" @click="emit('select', person)" class="hover:bg-gray-50 dark:hover:bg-[#2a2a2a] cursor-pointer transition-colors group">
                                <td class="py-3 px-4 w-8">
                                    <div class="w-8 h-8 rounded-full flex items-center justify-center text-xs font-bold overflow-hidden"
                                         :class="person.properties?.avatar ? '' : 'bg-gradient-to-br from-blue-500 to-purple-600 text-white'">
                                        <img v-if="getAvatarSrc(person)" :src="getAvatarSrc(person)" class="w-full h-full object-cover" />
                                        <span v-else>{{ getInitials(person.title) }}</span>
                                    </div>
                                </td>
                                <td class="py-3 px-4 font-medium text-[#1c1c1e] dark:text-[#f4f4f5] max-w-[200px] truncate">{{ person.title }}</td>
                                <td class="py-3 px-4 text-xs text-gray-500 dark:text-gray-400 max-w-[200px]">
                                    <span v-if="getDetail(person, 'company')">{{ getDetail(person, 'company') }}</span>
                                    <span v-else-if="person.properties?.relationship_type" class="capitalize">{{ person.properties.relationship_type }}</span>
                                    <span v-else class="italic text-gray-300 dark:text-gray-600">—</span>
                                </td>
                                <td class="py-3 px-4">
                                    <div class="flex flex-wrap gap-1" v-if="person.properties?.tags?.length">
                                        <span v-for="tag in person.properties.tags.slice(0, 3)" :key="tag" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300">{{ tag }}</span>
                                        <span v-if="person.properties.tags.length > 3" class="text-[10px] px-1.5 py-0.5 text-gray-400">+{{ person.properties.tags.length - 3 }}</span>
                                    </div>
                                    <span v-else class="text-xs text-gray-400 italic">No tags</span>
                                </td>
                                <td class="py-3 px-4 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap text-right">{{ formatDate(person.created_at) }}</td>
                                <td class="py-3 px-4 w-16 text-center" @click.stop>
                                    <div class="flex items-center justify-center gap-1">
                                        <button @click="emit('edit', person)" class="p-1 rounded hover:bg-gray-200 dark:hover:bg-[#444] md:opacity-0 opacity-100 group-hover:opacity-100 transition text-gray-400 hover:text-blue-500" title="Edit">
                                            <Edit2 class="w-3.5 h-3.5" />
                                        </button>
                                        <button @click="emit('delete', person)" class="p-1 rounded hover:bg-red-50 dark:hover:bg-red-900/20 md:opacity-0 opacity-100 group-hover:opacity-100 transition text-gray-400 hover:text-red-500" title="Delete">
                                            <Trash2 class="w-3.5 h-3.5" />
                                        </button>
                                    </div>
                                </td>
                            </tr>
                            <tr v-if="filtered.length === 0">
                                <td colspan="6" class="py-12 text-center text-gray-500">No people found.</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>

            <!-- LIST VIEW -->
            <div v-else-if="viewMode === 'list'" class="w-full space-y-2">
                <div v-for="person in paginated" :key="person.id" @click="emit('select', person)"
                    class="bg-white dark:bg-[#252525] border border-[#e6e6e6] dark:border-[#333] rounded-xl px-5 py-3 flex items-center gap-4 hover:shadow-md hover:border-blue-300 dark:hover:border-blue-700 cursor-pointer transition-all group">
                    <div class="w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold flex-shrink-0 overflow-hidden"
                         :class="person.properties?.avatar ? '' : 'bg-gradient-to-br from-blue-500 to-purple-600 text-white'">
                        <img v-if="getAvatarSrc(person)" :src="getAvatarSrc(person)" class="w-full h-full object-cover" />
                        <span v-else>{{ getInitials(person.title) }}</span>
                    </div>
                    <div class="flex-1 min-w-0">
                        <h4 class="text-sm font-semibold truncate group-hover:text-blue-600 dark:group-hover:text-blue-400 transition-colors">{{ person.title }}</h4>
                        <p v-if="getDetail(person, 'company')" class="text-xs text-gray-500 dark:text-gray-400 truncate">{{ getDetail(person, 'company') }}</p>
                        <p v-else-if="person.properties?.relationship_type" class="text-xs text-gray-400 capitalize truncate">{{ person.properties.relationship_type }}</p>
                    </div>
                    <!-- Details -->
                    <div class="hidden md:flex items-center gap-4 text-xs text-gray-500 dark:text-gray-400 flex-shrink-0">
                        <span v-if="getDetail(person, 'email')" class="flex items-center gap-1 truncate max-w-[180px]"><Mail class="w-3 h-3 opacity-50" /> {{ getDetail(person, 'email') }}</span>
                        <span v-if="getDetail(person, 'phone')" class="flex items-center gap-1"><Phone class="w-3 h-3 opacity-50" /> {{ getDetail(person, 'phone') }}</span>
                    </div>
                    <!-- Tags -->
                    <div class="hidden lg:flex flex-wrap gap-1 flex-shrink-0 max-w-[150px]">
                        <span v-for="tag in (person.properties?.tags || []).slice(0, 2)" :key="tag" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400">{{ tag }}</span>
                    </div>
                    <!-- Actions -->
                    <div class="flex items-center gap-1 flex-shrink-0" @click.stop>
                        <button @click="emit('edit', person)" class="p-1.5 rounded hover:bg-gray-200 dark:hover:bg-[#444] md:opacity-0 opacity-100 group-hover:opacity-100 transition text-gray-400 hover:text-blue-500">
                            <Edit2 class="w-3.5 h-3.5" />
                        </button>
                        <button @click="emit('delete', person)" class="p-1.5 rounded hover:bg-red-50 dark:hover:bg-red-900/20 md:opacity-0 opacity-100 group-hover:opacity-100 transition text-gray-400 hover:text-red-500">
                            <Trash2 class="w-3.5 h-3.5" />
                        </button>
                    </div>
                </div>
                <div v-if="filtered.length === 0" class="py-12 text-center text-gray-500">No people found.</div>
            </div>

            <!-- CARD VIEW -->
            <div v-else class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
                <button v-for="person in paginated" :key="person.id" @click="emit('select', person)"
                    class="text-left bg-white dark:bg-[#252525] border border-[#e6e6e6] dark:border-[#333] rounded-xl p-4 hover:shadow-md hover:border-blue-300 dark:hover:border-blue-700 transition-all group">
                    <div class="flex items-center gap-3 mb-3">
                        <div class="w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold flex-shrink-0 overflow-hidden"
                             :class="person.properties?.avatar ? '' : 'bg-gradient-to-br from-blue-500 to-purple-600 text-white'">
                            <img v-if="getAvatarSrc(person)" :src="getAvatarSrc(person)" class="w-full h-full object-cover" />
                            <span v-else>{{ getInitials(person.title) }}</span>
                        </div>
                        <div class="flex-1 min-w-0">
                            <h4 class="text-sm font-semibold truncate group-hover:text-blue-600 dark:group-hover:text-blue-400 transition-colors">{{ person.title }}</h4>
                            <p v-if="getDetail(person, 'company')" class="text-[11px] text-gray-500 dark:text-gray-400 truncate">{{ getDetail(person, 'company') }}</p>
                            <p v-else-if="person.properties?.relationship_type" class="text-[11px] text-gray-400 truncate capitalize">{{ person.properties.relationship_type }}</p>
                        </div>
                    </div>
                    <div class="space-y-1 text-[11px] text-gray-500 dark:text-gray-400">
                        <template v-for="d in (person.properties?.details || []).slice(0, 2)" :key="d.label + d.value">
                            <p class="flex items-center gap-1.5 truncate">
                                <Mail v-if="d.type === 'email'" class="w-3 h-3 flex-shrink-0 opacity-60" />
                                <Phone v-else-if="d.type === 'phone'" class="w-3 h-3 flex-shrink-0 opacity-60" />
                                <Building v-else-if="d.label.toLowerCase().includes('company')" class="w-3 h-3 flex-shrink-0 opacity-60" />
                                <span v-else class="w-3 h-3 flex-shrink-0 opacity-40 text-[9px] font-medium text-center">{{ d.label.charAt(0).toUpperCase() }}</span>
                                <span class="truncate">{{ d.value }}</span>
                            </p>
                        </template>
                        <p v-if="!person.properties?.details?.length && person.properties?.email" class="flex items-center gap-1.5 truncate">
                            <Mail class="w-3 h-3 flex-shrink-0 opacity-60" /> <span class="truncate">{{ person.properties.email }}</span>
                        </p>
                    </div>
                    <div v-if="person.properties?.tags?.length" class="flex flex-wrap gap-1 mt-2">
                        <span v-for="tag in person.properties.tags.slice(0, 3)" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400">{{ tag }}</span>
                        <span v-if="person.properties.tags.length > 3" class="text-[9px] px-1.5 py-0.5 text-gray-400">+{{ person.properties.tags.length - 3 }}</span>
                    </div>
                </button>
                <div v-if="filtered.length === 0" class="col-span-full py-12 text-center text-gray-500">No people found.</div>
            </div>

            <!-- Pagination -->
            <div v-if="totalPages > 1" class="mt-6 flex items-center justify-between text-[13px] text-gray-500">
                <div>Showing {{ (currentPage - 1) * itemsPerPage + 1 }} to {{ Math.min(currentPage * itemsPerPage, filtered.length) }} of {{ filtered.length }}</div>
                <div class="flex items-center gap-2">
                    <button @click="currentPage--" :disabled="currentPage === 1" class="px-3 py-1.5 rounded-lg border border-[#e6e6e6] dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors">Previous</button>
                    <span class="font-medium px-2">Page {{ currentPage }} of {{ totalPages }}</span>
                    <button @click="currentPage++" :disabled="currentPage === totalPages" class="px-3 py-1.5 rounded-lg border border-[#e6e6e6] dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors">Next</button>
                </div>
            </div>
        </div>
    </div>
</template>
