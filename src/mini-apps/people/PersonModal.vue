<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { useNodeService } from '../../composables/useNodeService';
import { ask } from '@tauri-apps/plugin-dialog';
import { X, Save, Trash2, User, Hash, AlignLeft, Gift, Plus, Camera, Clock, Mail, Phone, Building, MapPin, Briefcase, Heart, Globe, Calendar, ChevronRight } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

const props = defineProps<{
    vaultPath: string;
    person: any | null;
    topRelationships?: string[];
    allRelationships?: string[];
}>();

const emit = defineEmits(['close', 'saved']);
const ns = useNodeService();

interface DetailField { label: string; value: string; type: 'text' | 'email' | 'phone' | 'url' }
interface Experience { company: string; role: string; startMonth: string; startYear: string; endMonth: string; endYear: string; current: boolean }

const form = ref({
    title: '',
    nickname: '',
    display_name: 'fullname' as 'fullname' | 'nickname' | 'custom',
    custom_display: '',
    relationships: [] as string[],
    birthday: '',
    tags: [] as string[],
    avatar: '',
    details: [] as DetailField[],
    important_dates: [] as Array<{ label: string; date: string }>,
    experiences: [] as Experience[],
});



const DETAIL_PRESETS: Array<{ label: string; type: DetailField['type']; icon: any }> = [
    { label: 'Email', type: 'email', icon: Mail },
    { label: 'Phone', type: 'phone', icon: Phone },
    { label: 'Location', type: 'text', icon: MapPin },
    { label: 'How We Met', type: 'text', icon: Heart },
    { label: 'LinkedIn', type: 'url', icon: Globe },
    { label: 'Twitter', type: 'url', icon: Globe },
    { label: 'GitHub', type: 'url', icon: Globe },
    { label: 'Website', type: 'url', icon: Globe },
];

const tagInput = ref('');
const relInput = ref('');
const showRelDropdown = ref(false);
const isSaving = ref(false);
const isDeleting = ref(false);
const isUploadingAvatar = ref(false);
const avatarFileInput = ref<HTMLInputElement | null>(null);
const avatarSrc = ref('');
const showPresetPicker = ref(false);
const birthdayInputRef = ref<HTMLInputElement | null>(null);

const addBirthdayFromQuickAdd = () => {
    showKeyDates.value = true;
    showPresetPicker.value = false;
    setTimeout(() => {
        birthdayInputRef.value?.focus();
        try {
            birthdayInputRef.value?.showPicker?.();
        } catch(e) {}
    }, 50);
};

// Migrate legacy fields into unified details[]
const migrateLegacy = (p: any): DetailField[] => {
    const details: DetailField[] = [...(p.details || [])];
    if (details.length > 0) return details; // already migrated

    // Migrate single-value legacy fields
    if (p.email) details.push({ label: 'Email', value: p.email, type: 'email' });
    if (p.phone) details.push({ label: 'Phone', value: p.phone, type: 'phone' });
    if (p.company) details.push({ label: 'Company', value: p.company, type: 'text' });
    if (p.role) details.push({ label: 'Role', value: p.role, type: 'text' });
    if (p.location) details.push({ label: 'Location', value: p.location, type: 'text' });
    if (p.how_we_met) details.push({ label: 'How We Met', value: p.how_we_met, type: 'text' });
    // Migrate arrays
    if (p.emails?.length) for (const e of p.emails) details.push({ label: e.label ? `${e.label} Email` : 'Email', value: e.value, type: 'email' });
    if (p.phones?.length) for (const ph of p.phones) details.push({ label: ph.label ? `${ph.label} Phone` : 'Phone', value: ph.value, type: 'phone' });
    if (p.companies?.length) for (const c of p.companies) details.push({ label: c.label ? `${c.label} Company` : 'Company', value: c.value, type: 'text' });
    // Social links
    const s = p.social || {};
    if (s.linkedin) details.push({ label: 'LinkedIn', value: s.linkedin, type: 'url' });
    if (s.twitter) details.push({ label: 'Twitter', value: s.twitter, type: 'url' });
    if (s.github) details.push({ label: 'GitHub', value: s.github, type: 'url' });
    if (s.website) details.push({ label: 'Website', value: s.website, type: 'url' });
    // Deduplicate by value
    const seen = new Set<string>();
    return details.filter(d => { if (seen.has(d.value)) return false; seen.add(d.value); return true; });
};

onMounted(() => {
    if (props.person) {
        const p = props.person.properties || {};
        form.value.title = props.person.title || '';
        form.value.nickname = p.nickname || '';
        form.value.display_name = p.display_name || 'fullname';
        form.value.custom_display = p.custom_display || '';
        const relVal = p.relationship_type || '';
        form.value.relationships = relVal ? relVal.split(',').map((s:string) => s.trim()).filter(Boolean) : [];
        form.value.birthday = p.birthday || '';
        form.value.tags = [...(p.tags || [])];
        form.value.avatar = p.avatar || '';
        form.value.details = migrateLegacy(p);
        form.value.important_dates = [...(p.important_dates || [])];
        form.value.experiences = (p.experiences || []).map((e: any) => {
            const [startYear = '', startMonth = ''] = (e.start || '').split('-');
            const [endYear = '', endMonth = ''] = (e.end || '').split('-');
            return { company: e.company || '', role: e.role || '', startMonth, startYear, endMonth, endYear, current: !!e.current };
        });

        if (form.value.avatar) {
            avatarSrc.value = convertFileSrc(`${props.vaultPath}/${form.value.avatar}`);
        }
    }

    const handleKeydown = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close'); };
    window.addEventListener('keydown', handleKeydown);
    onUnmounted(() => window.removeEventListener('keydown', handleKeydown));

    // Auto-expand sections that have data
    if (form.value.experiences.length > 0) showExperiences.value = true;
    if (form.value.birthday || form.value.important_dates.length > 0) showKeyDates.value = true;
});

const addTag = () => { const t = tagInput.value.trim().toLowerCase(); if (t && !form.value.tags.includes(t)) form.value.tags.push(t); tagInput.value = ''; };
const removeTag = (i: number) => form.value.tags.splice(i, 1);

const addRelationship = (r?: string) => {
    const val = (r || relInput.value).trim();
    if (!val) return;
    const parts = val.split(',').map(s => s.trim()).filter(Boolean);
    parts.forEach(p => {
        const exactMatch = props.allRelationships?.find(ar => ar.toLowerCase() === p.toLowerCase());
        const finalVal = exactMatch || p.split(' ').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
        if (!form.value.relationships.includes(finalVal)) {
            form.value.relationships.push(finalVal);
        }
    });
    relInput.value = '';
};
const removeRelationship = (i: number) => form.value.relationships.splice(i, 1);

const filteredAllRelationships = computed(() => {
    if (!props.allRelationships) return [];
    const query = relInput.value.toLowerCase().trim();
    return props.allRelationships.filter(r => 
        !form.value.relationships.includes(r) && 
        r.toLowerCase().includes(query)
    );
});

const addImportantDate = () => form.value.important_dates.push({ label: '', date: '' });
const removeImportantDate = (i: number) => form.value.important_dates.splice(i, 1);
const addExperience = () => { form.value.experiences.push({ company: '', role: '', startMonth: '', startYear: '', endMonth: '', endYear: '', current: false }); showExperiences.value = true; };
const removeExperience = (i: number) => form.value.experiences.splice(i, 1);
const yearOptions = computed(() => {
    const current = new Date().getFullYear();
    const years = [];
    for (let y = current + 5; y >= 1950; y--) years.push(y);
    return years;
});

const showExperiences = ref(false);
const showKeyDates = ref(false);

const addDetail = (preset?: typeof DETAIL_PRESETS[0]) => {
    form.value.details.push({ label: preset?.label || '', value: '', type: preset?.type || 'text' });
    showPresetPicker.value = false;
};
const removeDetail = (i: number) => form.value.details.splice(i, 1);

// Auto-detect type from label
const inferType = (label: string): DetailField['type'] => {
    const l = label.toLowerCase();
    if (l.includes('email') || l.includes('mail')) return 'email';
    if (l.includes('phone') || l.includes('tel') || l.includes('mobile') || l.includes('điện thoại')) return 'phone';
    if (l.includes('linkedin') || l.includes('twitter') || l.includes('github') || l.includes('website') || l.includes('http') || l.includes('url') || l.includes('link')) return 'url';
    return 'text';
};

// Sync type whenever label changes
const onLabelChange = (d: DetailField) => {
    d.type = inferType(d.label);
};

const handleAvatarUpload = async (event: Event) => {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (!file) return;
    isUploadingAvatar.value = true;
    try {
        const buffer = await file.arrayBuffer();
        const bytes = Array.from(new Uint8Array(buffer));
        const relPath: string = await invoke('save_asset', { vaultPath: props.vaultPath, filename: file.name, bytes });
        form.value.avatar = relPath;
        avatarSrc.value = convertFileSrc(`${props.vaultPath}/${relPath}`);
    } catch (e) { logger.error('Failed to upload avatar', e); }
    finally { isUploadingAvatar.value = false; }
};

const savePerson = async () => {
    if (!form.value.title.trim()) { alert("Name is required"); return; }
    addTag(); // Commit any pending tag input
    addRelationship(); // Commit any pending relationship input
    isSaving.value = true;
    try {
        const properties: Record<string, any> = {};
        if (form.value.avatar) properties.avatar = form.value.avatar;
        if (form.value.nickname.trim()) {
            properties.nickname = form.value.nickname.trim();
        }
        properties.display_name = form.value.display_name;
        if (form.value.display_name === 'custom' && form.value.custom_display.trim()) {
            properties.custom_display = form.value.custom_display.trim();
        }
        if (form.value.birthday) properties.birthday = form.value.birthday;
        if (form.value.tags.length > 0) properties.tags = form.value.tags;
        if (form.value.relationships.length > 0) properties.relationship_type = form.value.relationships.join(', ');
        else properties.relationship_type = '';
        if (props.person?.properties?.contact_frequency) properties.contact_frequency = props.person.properties.contact_frequency;
        const validDates = form.value.important_dates.filter(d => d.label && d.date);
        if (validDates.length > 0) properties.important_dates = validDates;

        // Work experience
        const validExperiences = form.value.experiences.filter(e => e.company.trim()).map(e => ({
            company: e.company, role: e.role,
            start: e.startYear ? (e.startMonth ? `${e.startYear}-${e.startMonth}` : e.startYear) : '',
            end: e.current ? '' : (e.endYear ? (e.endMonth ? `${e.endYear}-${e.endMonth}` : e.endYear) : ''),
            current: e.current,
        }));
        if (validExperiences.length > 0) properties.experiences = validExperiences;

        // Details (the flexible fields)
        const validDetails = form.value.details.filter(d => d.value.trim());
        if (validDetails.length > 0) properties.details = validDetails;

        // Write backward-compat shortcuts for search + sidebar
        const findDetail = (label: string) => validDetails.find(d => d.label.toLowerCase().includes(label))?.value;
        const email = findDetail('email'); if (email) properties.email = email;
        const phone = findDetail('phone'); if (phone) properties.phone = phone;
        const company = findDetail('company'); if (company) properties.company = company;

        // Preserve runtime-managed fields
        if (props.person?.properties?.interactions) properties.interactions = props.person.properties.interactions;
        if (props.person?.properties?.gifts) properties.gifts = props.person.properties.gifts;
        if (props.person?.properties?.last_contacted) properties.last_contacted = props.person.properties.last_contacted;
        if (props.person?.properties?.connections) properties.connections = props.person.properties.connections;
        if (props.person?.properties?.relations) properties.relations = props.person.properties.relations;
        if (props.person?.properties?.is_owner) properties.is_owner = true;

        let relPath = props.person ? props.person.id : `People/${crypto.randomUUID()}.md`;
        await ns.writeNode({
            relPath, title: form.value.title,
            nodeType: 'person', properties, content: props.person?.content || ''
        });
        emit('saved');
        emit('close');
    } catch (e) {
        logger.error('Failed to save person', e);
        alert("Error saving person");
    } finally { isSaving.value = false; }
};

const deletePerson = async () => {
    if (!props.person) return;
    if (props.person.properties?.is_owner) return;
    const yes = await ask(`This will permanently delete "${props.person.title}" and all associated data. This action cannot be undone.`, { title: 'Delete contact?', kind: 'warning', okLabel: 'Delete', cancelLabel: 'Cancel' });
    if (yes) {
        isDeleting.value = true;
        try {
            await ns.deleteNode({ relPath: props.person.id });
            emit('saved'); emit('close');
        } catch (e) { logger.error('Failed to delete person', e); }
        finally { isDeleting.value = false; }
    }
};

const frequencyOptions = ['weekly', 'biweekly', 'monthly', 'quarterly', 'yearly'];

const getDetailInputType = (type: string) => {
    if (type === 'email') return 'email';
    if (type === 'phone') return 'tel';
    if (type === 'url') return 'url';
    return 'text';
};

const getDetailPlaceholder = (d: DetailField) => {
    if (d.type === 'email') return 'john@example.com';
    if (d.type === 'phone') return '+1 234 567 890';
    if (d.type === 'url') return 'https://...';
    return d.label || 'Value';
};
</script>

<template>
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm" @click="emit('close')">
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-2xl w-full max-w-2xl max-h-[90vh] flex flex-col overflow-hidden" @click.stop>
            <!-- Header -->
            <div class="px-6 py-4 border-b border-border dark:border-border-dark flex items-center justify-between bg-gray-50/50 dark:bg-gray-800/50">
                <h2 class="text-lg font-semibold flex items-center gap-2">
                    <User class="w-5 h-5 text-blue-500" />
                    {{ props.person ? 'Edit Person' : 'Add New Person' }}
                </h2>
                <button @click="emit('close')" class="p-1.5 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-500 transition-colors">
                    <X class="w-5 h-5" />
                </button>
            </div>

            <!-- Body -->
            <div class="flex-1 overflow-y-auto p-6 space-y-5">
                <!-- Avatar + Name -->
                <div class="flex items-start gap-4">
                    <div class="flex-shrink-0">
                        <input ref="avatarFileInput" type="file" accept="image/*" class="hidden" @change="handleAvatarUpload" />
                        <button @click="avatarFileInput?.click()" class="w-20 h-20 rounded-2xl border-2 border-dashed border-gray-300 dark:border-gray-600 flex items-center justify-center overflow-hidden hover:border-blue-400 transition-colors group relative">
                            <img v-if="avatarSrc" :src="avatarSrc" class="w-full h-full object-cover" />
                            <div v-else class="flex flex-col items-center text-gray-400 group-hover:text-blue-500 transition-colors">
                                <Camera class="w-5 h-5" /><span class="text-[10px] mt-0.5">Photo</span>
                            </div>
                            <div v-if="avatarSrc" class="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity"><Camera class="w-5 h-5 text-white" /></div>
                            <div v-if="isUploadingAvatar" class="absolute inset-0 bg-white/60 flex items-center justify-center"><div class="animate-spin rounded-full h-5 w-5 border-b-2 border-blue-500"></div></div>
                        </button>
                    </div>
                    <div class="flex-1">
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{{ $t('people.full_name_req') }}</label>
                        <div class="relative">
                            <User class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                            <input v-model="form.title" type="text" :placeholder="$t('people.full_name_ph')" class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none" autofocus />
                        </div>
                        <div class="relative mt-2">
                            <Heart class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                            <input v-model="form.nickname" type="text" placeholder="Nickname" class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 transition-all outline-none" />
                        </div>
                        <div class="mt-2 flex items-center gap-1.5 flex-wrap">
                            <span class="text-[11px] text-gray-400">Display as:</span>
                            <button @click="form.display_name = 'fullname'"
                                :class="['px-2 py-0.5 text-[11px] font-medium rounded-md border transition-all',
                                    form.display_name === 'fullname'
                                        ? 'bg-blue-500 text-white border-blue-500'
                                        : 'bg-white dark:bg-[#1e1e1e] text-gray-500 border-gray-200 dark:border-gray-700 hover:border-blue-300'
                                ]">Full Name</button>
                            <button v-if="form.nickname.trim()" @click="form.display_name = 'nickname'"
                                :class="['px-2 py-0.5 text-[11px] font-medium rounded-md border transition-all',
                                    form.display_name === 'nickname'
                                        ? 'bg-blue-500 text-white border-blue-500'
                                        : 'bg-white dark:bg-[#1e1e1e] text-gray-500 border-gray-200 dark:border-gray-700 hover:border-blue-300'
                                ]">Nickname</button>
                            <button @click="form.display_name = 'custom'"
                                :class="['px-2 py-0.5 text-[11px] font-medium rounded-md border transition-all',
                                    form.display_name === 'custom'
                                        ? 'bg-blue-500 text-white border-blue-500'
                                        : 'bg-white dark:bg-[#1e1e1e] text-gray-500 border-gray-200 dark:border-gray-700 hover:border-blue-300'
                                ]">Custom</button>
                            <input v-if="form.display_name === 'custom'" v-model="form.custom_display" type="text" :placeholder="$t('people.custom_display_ph')" class="ml-1 px-2 py-0.5 text-[11px] bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-md focus:ring-2 focus:ring-blue-500 outline-none w-36" />
                        </div>
                    </div>
                </div>

                <!-- Relationship -->
                <div>
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">Relationships</label>
                    
                    <!-- Selected Relationships (Tags Style) -->
                    <div class="flex flex-wrap gap-2 mb-2" v-if="form.relationships.length > 0">
                        <span v-for="(rel, index) in form.relationships" :key="index" class="px-2.5 py-1 text-sm bg-purple-50 dark:bg-purple-900/20 text-purple-700 dark:text-purple-300 border border-purple-200 dark:border-purple-800/50 rounded-md flex items-center gap-1.5">
                            <Heart class="w-3 h-3 opacity-70" /> {{ rel }}
                            <button @click="removeRelationship(index)" class="ml-1 opacity-50 hover:opacity-100 hover:text-red-500 outline-none"><X class="w-3 h-3" /></button>
                        </span>
                    </div>

                    <!-- Quick Add Buttons -->
                    <div class="flex flex-wrap gap-1.5 mb-2" v-if="props.topRelationships && props.topRelationships.length > 0">
                        <button v-for="r in props.topRelationships" :key="r"
                            @click="addRelationship(r)"
                            v-show="!form.relationships.includes(r)"
                            class="px-2.5 py-1 text-xs rounded-lg border transition-colors bg-gray-50 dark:bg-[#1a1a1a] border-border dark:border-border-dark text-gray-600 dark:text-gray-400 hover:border-blue-300">
                            + {{ r }}
                        </button>
                    </div>

                    <!-- Input Field -->
                    <div class="relative">
                        <Heart class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                        <input v-model="relInput" 
                            @keydown.enter.prevent="addRelationship()" 
                            @focus="showRelDropdown = true"
                            @blur="showRelDropdown = false"
                            type="text" 
                            :placeholder="$t('people.add_relationship_ph')" 
                            class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 transition-all outline-none" 
                        />
                        <div v-if="showRelDropdown && filteredAllRelationships.length > 0" 
                             class="absolute left-0 top-full mt-1 w-full max-h-48 overflow-y-auto bg-white dark:bg-[#242426] border border-gray-200 dark:border-gray-700 rounded-xl shadow-lg z-10 p-1">
                            <button 
                                v-for="r in filteredAllRelationships" :key="r"
                                @mousedown.prevent="addRelationship(r); showRelDropdown = false"
                                class="w-full text-left px-3 py-1.5 text-sm rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-700 dark:text-gray-300 transition-colors">
                                {{ r }}
                            </button>
                        </div>
                    </div>
                </div>

                <!-- Details (Flexible Fields) -->
                <div>
                    <h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3 flex items-center gap-2">
                        <AlignLeft class="w-4 h-4 text-blue-500" /> Details
                    </h3>
                    <div v-for="(d, i) in form.details" :key="i" class="flex items-center gap-2 mb-2">
                        <input v-model="d.label" @input="onLabelChange(d)" type="text" placeholder="Label"
                            class="w-28 flex-shrink-0 px-2.5 py-2 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg text-xs font-medium focus:ring-2 focus:ring-blue-500 outline-none" />
                        <div class="flex-1 relative">
                            <input v-model="d.value" :type="getDetailInputType(d.type)" :placeholder="getDetailPlaceholder(d)"
                                class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 transition-all outline-none" />
                        </div>
                        <button @click="removeDetail(i)" class="p-1.5 text-gray-400 hover:text-red-500 transition-colors"><X class="w-4 h-4" /></button>
                    </div>

                    <!-- Add detail: presets or custom -->
                    <div class="relative">
                        <div class="flex items-center gap-2">
                            <button @click="addDetail()" class="flex items-center gap-1.5 text-xs text-blue-500 hover:text-blue-600 font-medium">
                                <Plus class="w-3.5 h-3.5" /> Add Field
                            </button>
                            <span class="text-gray-300 dark:text-gray-600">|</span>
                            <button @click="showPresetPicker = !showPresetPicker" class="text-xs text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 font-medium">
                                Quick Add ▾
                            </button>
                        </div>
                        <div v-if="showPresetPicker" class="absolute left-0 top-full mt-1 z-10 bg-white dark:bg-[#242426] border border-gray-200 dark:border-gray-700 rounded-xl shadow-lg p-2 flex flex-wrap gap-1 w-80">
                            <button @click="addExperience(); showPresetPicker = false;"
                                class="flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-lg hover:bg-blue-50 dark:hover:bg-blue-900/20 text-gray-600 dark:text-gray-300 transition-colors">
                                <Building class="w-3.5 h-3.5 text-gray-400" /> Company
                            </button>
                            <button @click="addBirthdayFromQuickAdd"
                                class="flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-lg hover:bg-blue-50 dark:hover:bg-blue-900/20 text-gray-600 dark:text-gray-300 transition-colors">
                                <Gift class="w-3.5 h-3.5 text-gray-400" /> Birthday
                            </button>
                            <button v-for="p in DETAIL_PRESETS" :key="p.label"
                                @click="addDetail(p)"
                                class="flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-lg hover:bg-blue-50 dark:hover:bg-blue-900/20 text-gray-600 dark:text-gray-300 transition-colors">
                                <component :is="p.icon" class="w-3.5 h-3.5 text-gray-400" /> {{ p.label }}
                            </button>
                        </div>
                    </div>
                </div>

                <!-- Work Experience (collapsible) -->
                <div class="border-t border-border dark:border-border-dark pt-4">
                    <button @click="showExperiences = !showExperiences" class="w-full flex items-center gap-2 text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider hover:text-gray-700 dark:hover:text-gray-200 transition-colors">
                        <ChevronRight :class="['w-4 h-4 transition-transform', showExperiences ? 'rotate-90' : '']" />
                        <Briefcase class="w-4 h-4 text-blue-500" /> Work Experience
                        <span v-if="form.experiences.length" class="text-[10px] font-medium px-1.5 py-0.5 rounded-full bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400 normal-case">{{ form.experiences.length }}</span>
                    </button>
                    <div v-show="showExperiences" class="mt-3">
                    <div v-for="(exp, i) in form.experiences" :key="i" class="mb-3 bg-gray-50 dark:bg-[#1a1a1a] border border-border dark:border-border-dark rounded-xl p-3 relative group">
                        <button @click="removeExperience(i)" class="absolute top-2 right-2 p-1 text-gray-400 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-all"><X class="w-3.5 h-3.5" /></button>
                        <div class="grid grid-cols-2 gap-2 mb-2">
                            <div>
                                <label class="block text-[11px] text-gray-400 mb-0.5">Company</label>
                                <input v-model="exp.company" type="text" :placeholder="$t('people.company_ph')" class="w-full px-3 py-1.5 bg-white dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none" />
                            </div>
                            <div>
                                <label class="block text-[11px] text-gray-400 mb-0.5">{{ $t('people.role') }}</label>
                                <input v-model="exp.role" type="text" :placeholder="$t('people.position_ph')" class="w-full px-3 py-1.5 bg-white dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none" />
                            </div>
                        </div>
                        <div class="grid grid-cols-[1fr_1fr_auto] gap-3 items-end">
                            <div>
                                <label class="block text-[11px] text-gray-400 mb-0.5">{{ $t('people.from') }}</label>
                                <div class="flex gap-1.5">
                                    <select v-model="exp.startMonth" class="flex-1 px-2.5 py-1.5 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-gray-700 rounded-lg text-xs text-gray-700 dark:text-gray-300 appearance-none bg-[url('data:image/svg+xml;charset=utf-8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%2210%22%20height%3D%226%22%3E%3Cpath%20d%3D%22M0%200l5%206%205-6z%22%20fill%3D%22%239ca3af%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_6px] bg-[right_8px_center] bg-no-repeat pr-6 focus:ring-2 focus:ring-blue-500/40 outline-none cursor-pointer">
                                        <option value="">{{ $t('people.month') }}</option>
                                        <option v-for="m in 12" :key="m" :value="String(m).padStart(2, '0')">{{ new Date(2000, m-1).toLocaleString('en', { month: 'short' }) }}</option>
                                    </select>
                                    <select v-model="exp.startYear" class="flex-1 px-2.5 py-1.5 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-gray-700 rounded-lg text-xs text-gray-700 dark:text-gray-300 appearance-none bg-[url('data:image/svg+xml;charset=utf-8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%2210%22%20height%3D%226%22%3E%3Cpath%20d%3D%22M0%200l5%206%205-6z%22%20fill%3D%22%239ca3af%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_6px] bg-[right_8px_center] bg-no-repeat pr-6 focus:ring-2 focus:ring-blue-500/40 outline-none cursor-pointer">
                                        <option value="">{{ $t('people.year') }}</option>
                                        <option v-for="y in yearOptions" :key="y" :value="String(y)">{{ y }}</option>
                                    </select>
                                </div>
                            </div>
                            <div :class="exp.current ? 'opacity-30 pointer-events-none' : ''">
                                <label class="block text-[11px] text-gray-400 mb-0.5">{{ $t('people.to_date') }}</label>
                                <div class="flex gap-1.5">
                                    <select v-model="exp.endMonth" :disabled="exp.current" class="flex-1 px-2.5 py-1.5 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-gray-700 rounded-lg text-xs text-gray-700 dark:text-gray-300 appearance-none bg-[url('data:image/svg+xml;charset=utf-8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%2210%22%20height%3D%226%22%3E%3Cpath%20d%3D%22M0%200l5%206%205-6z%22%20fill%3D%22%239ca3af%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_6px] bg-[right_8px_center] bg-no-repeat pr-6 focus:ring-2 focus:ring-blue-500/40 outline-none cursor-pointer">
                                        <option value="">{{ $t('people.month') }}</option>
                                        <option v-for="m in 12" :key="m" :value="String(m).padStart(2, '0')">{{ new Date(2000, m-1).toLocaleString('en', { month: 'short' }) }}</option>
                                    </select>
                                    <select v-model="exp.endYear" :disabled="exp.current" class="flex-1 px-2.5 py-1.5 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-gray-700 rounded-lg text-xs text-gray-700 dark:text-gray-300 appearance-none bg-[url('data:image/svg+xml;charset=utf-8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%2210%22%20height%3D%226%22%3E%3Cpath%20d%3D%22M0%200l5%206%205-6z%22%20fill%3D%22%239ca3af%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_6px] bg-[right_8px_center] bg-no-repeat pr-6 focus:ring-2 focus:ring-blue-500/40 outline-none cursor-pointer">
                                        <option value="">{{ $t('people.year') }}</option>
                                        <option v-for="y in yearOptions" :key="y" :value="String(y)">{{ y }}</option>
                                    </select>
                                </div>
                            </div>
                            <button type="button" @click="exp.current = !exp.current; if (exp.current) { exp.endMonth = ''; exp.endYear = ''; }"
                                :class="['px-3 py-1.5 rounded-lg text-xs font-medium border transition-all',
                                    exp.current
                                        ? 'bg-blue-500 text-white border-blue-500 shadow-sm'
                                        : 'bg-white dark:bg-[#1e1e1e] text-gray-500 border-gray-200 dark:border-gray-700 hover:border-blue-300'
                                ]">
                                Current
                            </button>
                        </div>
                    </div>
                    <button @click="addExperience" class="flex items-center gap-1.5 text-xs text-blue-500 hover:text-blue-600 font-medium">
                        <Plus class="w-3.5 h-3.5" /> Add Experience
                    </button>
                    </div>
                </div>

                <!-- Key Dates (collapsible) -->
                <div class="border-t border-border dark:border-border-dark pt-4">
                    <button @click="showKeyDates = !showKeyDates" class="w-full flex items-center gap-2 text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider hover:text-gray-700 dark:hover:text-gray-200 transition-colors">
                        <ChevronRight :class="['w-4 h-4 transition-transform', showKeyDates ? 'rotate-90' : '']" />
                        <Calendar class="w-4 h-4 text-blue-500" /> Key Dates
                        <span v-if="form.birthday || form.important_dates.length" class="text-[10px] font-medium px-1.5 py-0.5 rounded-full bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400 normal-case">{{ (form.birthday ? 1 : 0) + form.important_dates.length }}</span>
                    </button>
                    <div v-show="showKeyDates" class="mt-3">
                    <div class="mb-3">
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Birthday</label>
                        <div class="relative w-48">
                            <Gift class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                            <input ref="birthdayInputRef" v-model="form.birthday" type="date" class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none" />
                        </div>
                    </div>
                    <div v-for="(d, i) in form.important_dates" :key="i" class="grid grid-cols-[1fr_1fr_auto] gap-2 mb-2">
                        <input v-model="d.label" type="text" :placeholder="$t('people.label_ph')" class="px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none" />
                        <input v-model="d.date" type="date" class="px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none" />
                        <button @click="removeImportantDate(i)" class="p-2 text-gray-400 hover:text-red-500 transition-colors"><X class="w-4 h-4" /></button>
                    </div>
                    <button @click="addImportantDate" class="flex items-center gap-1.5 text-xs text-blue-500 hover:text-blue-600 font-medium">
                        <Plus class="w-3.5 h-3.5" /> Add Date
                    </button>
                    </div>
                </div>

                <!-- Tags -->
                <div class="border-t border-border dark:border-border-dark pt-5">
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Tags</label>
                    <div class="flex flex-wrap gap-2 mb-2">
                        <span v-for="(tag, index) in form.tags" :key="index" class="px-2.5 py-1 text-sm bg-gray-100 dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md flex items-center gap-1.5">
                            <Hash class="w-3 h-3 text-gray-400" /> {{ tag }}
                            <button @click="removeTag(index)" class="ml-1 text-gray-400 hover:text-red-500 outline-none"><X class="w-3 h-3" /></button>
                        </span>
                    </div>
                    <div class="relative">
                        <Hash class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                        <input v-model="tagInput" @keydown.enter.prevent="addTag" type="text" :placeholder="$t('people.add_tag_ph')" class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none" />
                    </div>
                </div>


            </div>

            <!-- Footer -->
            <div class="px-6 py-4 border-t border-border dark:border-border-dark flex items-center justify-between bg-gray-50/50 dark:bg-gray-800/50">
                <button v-if="props.person && !props.person.properties?.is_owner" @click="deletePerson" :disabled="isDeleting || isSaving" class="flex items-center gap-2 text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 px-3 py-1.5 rounded-lg transition-colors font-medium text-sm disabled:opacity-50">
                    <Trash2 class="w-4 h-4" /> Delete
                </button>
                <div v-else></div>
                <div class="flex items-center gap-3">
                    <button @click="emit('close')" class="px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-lg transition-colors">{{ $t('people.cancel') }}</button>
                    <button @click="savePerson" :disabled="isSaving || isDeleting" class="flex items-center gap-2 bg-blue-500 hover:bg-blue-600 text-white px-4 py-2 rounded-lg transition-colors font-medium text-sm disabled:opacity-50">
                        <Save class="w-4 h-4" /> {{ isSaving ? 'Saving...' : 'Save Person' }}
                    </button>
                </div>
            </div>
        </div>
    </div>
</template>
