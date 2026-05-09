<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { X, Save, Trash2, User, Mail, Phone, Building, Hash, AlignLeft, Gift } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

const props = defineProps<{
    vaultPath: string;
    person: any | null; // null if creating new
}>();

const emit = defineEmits(['close', 'saved']);

const form = ref({
    title: '',
    email: '',
    phone: '',
    company: '',
    birthday: '',
    tags: [] as string[],
    notes: ''
});

const tagInput = ref('');
const isSaving = ref(false);
const isDeleting = ref(false);

onMounted(() => {
    if (props.person) {
        form.value.title = props.person.title || '';
        form.value.email = props.person.properties.email || '';
        form.value.phone = props.person.properties.phone || '';
        form.value.company = props.person.properties.company || '';
        form.value.birthday = props.person.properties.birthday || '';
        form.value.tags = [...(props.person.properties.tags || [])];
        form.value.notes = props.person.content || '';
    }
    
    // Close on Escape key
    const handleKeydown = (e: KeyboardEvent) => {
        if (e.key === 'Escape') emit('close');
    };
    window.addEventListener('keydown', handleKeydown);
    
    onUnmounted(() => {
        window.removeEventListener('keydown', handleKeydown);
    });
});

const addTag = () => {
    const t = tagInput.value.trim().toLowerCase();
    if (t && !form.value.tags.includes(t)) {
        form.value.tags.push(t);
    }
    tagInput.value = '';
};

const removeTag = (index: number) => {
    form.value.tags.splice(index, 1);
};

const savePerson = async () => {
    if (!form.value.title.trim()) {
        alert("Name is required");
        return;
    }
    
    isSaving.value = true;
    try {
        // Construct frontmatter
        const properties: Record<string, any> = {};
        if (form.value.email) properties.email = form.value.email;
        if (form.value.phone) properties.phone = form.value.phone;
        if (form.value.company) properties.company = form.value.company;
        if (form.value.birthday) properties.birthday = form.value.birthday;
        if (form.value.tags.length > 0) properties.tags = form.value.tags;
        
        const content = form.value.notes;
        
        // Determine file path
        let relPath = '';
        if (props.person) {
            relPath = props.person.id; // Existing file path
        } else {
            relPath = `People/${crypto.randomUUID()}.md`;
        }
        
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath,
            title: form.value.title,
            nodeType: "person",
            properties,
            content
        });
        
        emit('saved');
        emit('close');
    } catch (e) {
        logger.error('Failed to save person', e);
        alert("Error saving person");
    } finally {
        isSaving.value = false;
    }
};

const deletePerson = async () => {
    if (!props.person) return;
    
    if (confirm(`Are you sure you want to delete ${props.person.title}?`)) {
        isDeleting.value = true;
        try {
            await invoke('delete_node_file', {
                vaultPath: props.vaultPath,
                relPath: props.person.id
            });
            emit('saved');
            emit('close');
        } catch (e) {
            logger.error('Failed to delete person', e);
            alert("Error deleting person");
        } finally {
            isDeleting.value = false;
        }
    }
};
</script>

<template>
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm" @click="emit('close')">
        <div 
            class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-2xl w-full max-w-2xl max-h-[90vh] flex flex-col overflow-hidden" 
            @click.stop
        >
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
            <div class="flex-1 overflow-y-auto p-6 space-y-6">
                <!-- Name -->
                <div>
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Full Name *</label>
                    <div class="relative">
                        <User class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                        <input 
                            v-model="form.title" 
                            type="text" 
                            placeholder="John Doe" 
                            class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none"
                            autofocus
                        />
                    </div>
                </div>
                
                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <!-- Email -->
                    <div>
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Email</label>
                        <div class="relative">
                            <Mail class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                            <input 
                                v-model="form.email" 
                                type="email" 
                                placeholder="john@example.com" 
                                class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none"
                            />
                        </div>
                    </div>
                    
                    <!-- Phone -->
                    <div>
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Phone</label>
                        <div class="relative">
                            <Phone class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                            <input 
                                v-model="form.phone" 
                                type="tel" 
                                placeholder="+1 234 567 890" 
                                class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none"
                            />
                        </div>
                    </div>
                </div>
                
                <!-- Company & Birthday -->
                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div>
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Company / Organization</label>
                        <div class="relative">
                            <Building class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                            <input 
                                v-model="form.company" 
                                type="text" 
                                placeholder="Acme Corp" 
                                class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none"
                            />
                        </div>
                    </div>
                    
                    <div>
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Birthday</label>
                        <div class="relative">
                            <Gift class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                            <input 
                                v-model="form.birthday" 
                                type="date" 
                                class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none"
                            />
                        </div>
                    </div>
                </div>
                
                <!-- Tags -->
                <div>
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Tags</label>
                    <div class="flex flex-wrap gap-2 mb-2">
                        <span 
                            v-for="(tag, index) in form.tags" 
                            :key="index"
                            class="px-2.5 py-1 text-sm bg-gray-100 dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md flex items-center gap-1.5"
                        >
                            <Hash class="w-3 h-3 text-gray-400" />
                            {{ tag }}
                            <button @click="removeTag(index)" class="ml-1 text-gray-400 hover:text-red-500 outline-none">
                                <X class="w-3 h-3" />
                            </button>
                        </span>
                    </div>
                    <div class="relative">
                        <Hash class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                        <input 
                            v-model="tagInput" 
                            @keydown.enter.prevent="addTag"
                            type="text" 
                            placeholder="Add tag and press Enter..." 
                            class="w-full pl-9 pr-4 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none"
                        />
                    </div>
                </div>
                
                <!-- Notes -->
                <div class="flex-1 flex flex-col min-h-[150px]">
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1 flex items-center gap-2">
                        <AlignLeft class="w-4 h-4" />
                        Notes (Markdown supported)
                    </label>
                    <textarea 
                        v-model="form.notes"
                        placeholder="Additional details, meeting notes, etc..."
                        class="flex-1 w-full p-3 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 transition-all outline-none resize-none font-mono text-sm"
                    ></textarea>
                </div>
            </div>
            
            <!-- Footer -->
            <div class="px-6 py-4 border-t border-border dark:border-border-dark flex items-center justify-between bg-gray-50/50 dark:bg-gray-800/50">
                <button 
                    v-if="props.person" 
                    @click="deletePerson"
                    :disabled="isDeleting || isSaving"
                    class="flex items-center gap-2 text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 px-3 py-1.5 rounded-lg transition-colors font-medium text-sm disabled:opacity-50"
                >
                    <Trash2 class="w-4 h-4" />
                    Delete
                </button>
                <div v-else></div> <!-- Spacer -->
                
                <div class="flex items-center gap-3">
                    <button 
                        @click="emit('close')"
                        class="px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-lg transition-colors"
                    >
                        Cancel
                    </button>
                    <button 
                        @click="savePerson"
                        :disabled="isSaving || isDeleting"
                        class="flex items-center gap-2 bg-blue-500 hover:bg-blue-600 text-white px-4 py-2 rounded-lg transition-colors font-medium text-sm disabled:opacity-50"
                    >
                        <Save class="w-4 h-4" />
                        {{ isSaving ? 'Saving...' : 'Save Person' }}
                    </button>
                </div>
            </div>
        </div>
    </div>
</template>
