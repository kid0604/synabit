<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { Tag, FileText, CheckCircle2, X } from 'lucide-vue-next';

const props = defineProps<{
    note: {
        title: string;
        content: string;
        tags: string;
    };
}>();

const emit = defineEmits(['save', 'close']);

const editingParams = ref({
    title: props.note?.title || 'Untitled Note',
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

const handleBackgroundClick = () => {
    close();
};
</script>

<template>
  <div class="fixed inset-0 z-[110] flex items-center justify-center md:p-4 bg-black/10 dark:bg-black/40 backdrop-blur-[2px]" @mousedown.self="handleBackgroundClick">
      <div class="w-full h-full md:h-auto md:max-w-lg bg-white dark:bg-[#1e1e1e] md:rounded-2xl shadow-none md:shadow-[0_20px_40px_rgba(0,0,0,0.1)] md:dark:shadow-[0_20px_40px_rgba(0,0,0,0.4)] border-none md:border md:border-gray-100 md:dark:border-[#2c2c2c] overflow-hidden flex flex-col" @mousedown.stop>
          
          <!-- Mobile Header -->
          <div class="flex justify-between items-center px-5 pb-4 md:hidden shrink-0 border-b border-gray-100 dark:border-[#2c2c2c]" style="padding-top: max(env(safe-area-inset-top), 36px);">
              <h3 class="font-semibold text-lg text-[#1c1c1e] dark:text-[#f4f4f5]">Note Details</h3>
              <button @click="handleBackgroundClick" class="p-2 -mr-2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 rounded-full bg-gray-100 dark:bg-[#2c2c2c]">
                  <X class="w-4 h-4" />
              </button>
          </div>

          <div class="p-5 flex flex-col pt-5 md:pt-6 flex-1 overflow-y-auto">
              
              <!-- Title -->
              <div class="flex items-start gap-4 mb-3">
                   <div class="shrink-0 mt-0.5 text-gray-500">
                       <FileText class="w-5 h-5"/>
                   </div>
                   <input 
                       v-model="editingParams.title" 
                       class="flex-1 bg-transparent border-none outline-none text-[1.1rem] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 focus:ring-0 p-0"
                       placeholder="Note Title"
                   />
              </div>
              
              <!-- Content -->
              <div class="pl-9 mb-4 flex-1 flex flex-col">
                  <textarea 
                       v-model="editingParams.content" 
                       class="w-full flex-1 bg-transparent border-none outline-none text-[15px] leading-relaxed text-gray-500 dark:text-gray-400 placeholder-gray-300 focus:ring-0 p-0 resize-none md:resize-y md:min-h-[120px] md:max-h-[300px]"
                       placeholder="Note Content"
                  ></textarea>
              </div>
          </div>
          
          <!-- Footer Meta Bar -->
          <div class="px-5 py-3 border-t border-gray-50 dark:border-[#2c2c2c] bg-white dark:bg-[#1c1c1e] flex items-center justify-start gap-2 flex-wrap">
              <!-- Tags -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingParams.tags.length > 0 ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Manage Tags" @click.stop="activeDropdown = activeDropdown === 'tags' ? null : 'tags'">
                  <Tag class="w-[18px] h-[18px]" :class="editingParams.tags.length > 0 ? 'text-blue-500 mr-2' : ''"/>
                  
                  <span v-if="editingParams.tags.length > 0" class="text-xs font-semibold max-w-[150px] truncate">{{ editingParams.tags }}</span>
                  
                  <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'tags' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-56 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Tags (comma separated)</label>
                          <input v-model="editingParams.tags" placeholder="e.g. docs, idea" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-2 outline-none focus:ring-1 focus:ring-blue-500 text-[#1c1c1e] dark:text-[#f4f4f5]" />
                      </div>
                  </div>
              </div>
          </div>

          <!-- Bottom Actions -->
          <div class="pt-4 px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-end gap-3 shrink-0 md:pb-4" style="padding-bottom: max(env(safe-area-inset-bottom), 16px);">
              <button @click="close" class="px-5 py-2 hover:bg-gray-200 dark:hover:bg-[#2c2c2c] text-gray-700 dark:text-gray-300 rounded-lg text-sm font-medium transition-all cursor-pointer border border-transparent">
                  Cancel
              </button>
              <button @click="save" class="px-5 py-2 bg-blue-600 hover:bg-blue-700 dark:bg-blue-500 dark:hover:bg-blue-600 text-white rounded-lg text-sm font-medium transition-all shadow-sm cursor-pointer flex items-center gap-1.5 border border-transparent active:scale-95">
                  <CheckCircle2 class="w-4 h-4" /> Create Note
              </button>
          </div>
      </div>
  </div>
</template>
