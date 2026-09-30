<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { Tag, FileText, CheckCircle2, X } from 'lucide-vue-next';
import AppDialog from '../../shared/components/AppDialog.vue';

const { t } = useI18n();

const props = defineProps<{
    note: {
        title: string;
        content: string;
        tags: string;
    };
}>();

const emit = defineEmits(['save', 'close']);

const editingParams = ref({
    title: props.note?.title || t('note.untitled_note'),
    content: props.note?.content || '',
    tags: props.note?.tags || ''
});

const activeDropdown = ref<string | null>(null);

const handleGlobalClick = () => {
    activeDropdown.value = null;
};

onMounted(() => {
    document.addEventListener('click', handleGlobalClick);
});

onUnmounted(() => {
    document.removeEventListener('click', handleGlobalClick);
});

const save = () => {
    emit('save', editingParams.value);
};

const close = () => {
    emit('close');
};

</script>

<template>
  <AppDialog :show="true" labelledby="note-edit-title" panel-class="!overflow-hidden flex flex-col" @close="close">
    
    <!-- Header, phones only: on a desktop the title field says enough -->
    <div class="flex justify-between items-center px-5 py-4 md:hidden shrink-0 border-b border-gray-100 dark:border-border-dark">
        <h3 id="note-edit-title" class="font-semibold text-lg text-text dark:text-text-dark">{{ $t('note.note_details') }}</h3>
        <button @click="close" class="p-2 -mr-2 text-gray-500 dark:text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 rounded-full bg-gray-100 dark:bg-[#2c2c2c]" :aria-label="$t('note.close')" :title="$t('note.close')">
            <X class="w-4 h-4" />
        </button>
    </div>

    <div class="p-5 flex flex-col pt-5 md:pt-6 flex-1 overflow-y-auto">
        
        <!-- Title -->
        <div class="flex items-start gap-4 mb-3">
             <div class="shrink-0 mt-0.5 text-gray-500 dark:text-gray-400">
                 <FileText class="w-5 h-5"/>
             </div>
             <input 
                 v-model="editingParams.title" 
                 class="flex-1 bg-transparent border-none outline-none text-[1.1rem] font-medium text-text dark:text-text-dark placeholder-gray-300 focus:ring-0 p-0"
                 :placeholder="$t('note.note_title')"
             />
        </div>
        
        <!-- Content -->
        <div class="pl-9 mb-4 flex-1 flex flex-col">
            <textarea 
                 v-model="editingParams.content" 
                 class="w-full flex-1 bg-transparent border-none outline-none text-[15px] leading-relaxed text-gray-500 dark:text-gray-400 placeholder-gray-300 focus:ring-0 p-0 resize-none md:resize-y md:min-h-[120px] md:max-h-[300px]"
                 :placeholder="$t('note.note_content')"
            ></textarea>
        </div>
    </div>
    
    <!-- Footer Meta Bar -->
    <div class="px-5 py-3 border-t border-gray-50 dark:border-border-dark bg-white dark:bg-[#1c1c1e] flex items-center justify-start gap-2 flex-wrap">
        <!-- Tags -->
        <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingParams.tags.length > 0 ? 'bg-gray-50 dark:bg-surface-hover-dark px-2 text-text dark:text-text-dark' : 'justify-center text-gray-500'" :title="$t('note.manage_tags')" @click.stop="activeDropdown = activeDropdown === 'tags' ? null : 'tags'">
            <Tag class="w-[18px] h-[18px]" :class="editingParams.tags.length > 0 ? 'text-accent dark:text-accent-dark mr-2' : ''"/>
            
            <span v-if="editingParams.tags.length > 0" class="text-xs font-semibold max-w-[150px] truncate">{{ editingParams.tags }}</span>
            
            <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'tags' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 md:group-hover:visible'" @click.stop>
                <div class="w-56 bg-white dark:bg-surface-dark border border-gray-200 dark:border-border-dark rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                    <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 mb-1">{{ $t('note.tags_comma_separated') }}</label>
                    <input v-model="editingParams.tags" :placeholder="$t('note.tags_example')" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-2 outline-none focus:ring-1 focus:ring-accent text-text dark:text-text-dark" />
                </div>
            </div>
        </div>
    </div>

    <!-- Bottom Actions -->
    <div class="pt-4 px-6 bg-gray-50 dark:bg-surface-alt-dark border-t border-border dark:border-border-dark flex items-center justify-end gap-3 shrink-0 pb-4">
        <button @click="close" class="px-5 py-2 hover:bg-gray-200 dark:hover:bg-[#2c2c2c] text-gray-700 dark:text-gray-300 rounded-lg text-sm font-medium transition-all cursor-pointer border border-transparent">
            {{ $t('note.cancel') }}
        </button>
        <button @click="save" class="btn-primary">
            <CheckCircle2 class="w-4 h-4" /> {{ $t('note.create_note') }}
        </button>
    </div>
  </AppDialog>
</template>
