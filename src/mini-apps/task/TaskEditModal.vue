<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue';
import { CheckCircle2, Calendar, Tag, Flag, X, Send, Eye, EyeOff, Trash2 } from 'lucide-vue-next';
import TiptapEditor from '../note/TiptapEditor.vue';

const props = defineProps<{
    task: any;
    vaultPath?: string;
    showActions?: boolean;
    projects?: any[];
}>();

const emit = defineEmits(['save', 'close', 'delete']);

// Create a reactive clone of the passed task params
const editingTaskParams = ref({
    title: props.task?.title || '',
    content: props.task?.content || '',
    is_transferred: props.task?.is_transferred || false,
    transferred_to: props.task?.transferred_to || '',
    track_progress: props.task?.track_progress || false,
    priority: props.task?.priority || '',
    start_date: props.task?.start_date || '',
    due_date: props.task?.due_date || '',
    comment: props.task?.comment || '',
    tags: props.task?.tags || '',
    status: props.task?.status || 'todo',
    project_id: props.task?.project_id || ''
});

const todayStr = computed(() => {
    const today = new Date();
    return today.toISOString().split('T')[0];
});

const activeDropdown = ref<string | null>(null);

const handleGlobalClick = () => {
    activeDropdown.value = null;
};

const titleInputRef = ref<HTMLTextAreaElement | null>(null);
const tiptapRef = ref<any>(null);

const handleTitleEnter = () => {
    if (tiptapRef.value) {
        tiptapRef.value.focus();
    }
};

const adjustTitleHeight = () => {
    nextTick(() => {
        if (titleInputRef.value) {
            titleInputRef.value.style.height = 'auto';
            titleInputRef.value.style.height = titleInputRef.value.scrollHeight + 'px';
        }
    });
};

onMounted(() => {
    document.addEventListener('click', handleGlobalClick);
    adjustTitleHeight();
});

onUnmounted(() => {
    document.removeEventListener('click', handleGlobalClick);
});

const save = () => {
    if (editingTaskParams.value.is_transferred && !editingTaskParams.value.transferred_to.trim()) {
        editingTaskParams.value.is_transferred = false;
        editingTaskParams.value.track_progress = false;
        editingTaskParams.value.transferred_to = '';
    }
    emit('save', editingTaskParams.value);
};

const close = () => {
    emit('close');
};

const handleBackgroundClick = () => {
    if (props.showActions) {
        close();
    } else {
        save();
    }
};
</script>

<template>
  <div class="fixed inset-0 z-[110] flex items-center justify-center md:p-4 bg-black/10 dark:bg-black/40 backdrop-blur-[2px]" @mousedown.self="handleBackgroundClick">
      <div class="w-full h-full md:h-auto md:max-w-lg bg-white dark:bg-[#1e1e1e] md:rounded-2xl shadow-none md:shadow-[0_20px_40px_rgba(0,0,0,0.1)] md:dark:shadow-[0_20px_40px_rgba(0,0,0,0.4)] border-none md:border md:border-gray-100 md:dark:border-[#2c2c2c] overflow-hidden flex flex-col" @mousedown.stop>
          
          <!-- Mobile Header -->
          <div class="flex justify-between items-center px-5 pb-4 md:hidden shrink-0 border-b border-gray-100 dark:border-[#2c2c2c]" style="padding-top: max(env(safe-area-inset-top), 36px);">
              <h3 class="font-semibold text-lg text-[#1c1c1e] dark:text-[#f4f4f5]">{{ props.showActions ? 'New Task' : 'Edit Task' }}</h3>
              <button @click="handleBackgroundClick" class="p-2 -mr-2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 rounded-full bg-gray-100 dark:bg-[#2c2c2c]">
                  <X class="w-4 h-4" />
              </button>
          </div>

          <div class="p-5 flex flex-col pt-5 md:pt-6 flex-1 overflow-y-auto">
              
              <!-- Title & Checkbox -->
              <div class="flex items-start gap-4 mb-3">
                   <button @click="editingTaskParams.status = (editingTaskParams.status === 'done' ? 'todo' : 'done')" class="shrink-0 mt-0.5 cursor-pointer">
                       <div v-if="editingTaskParams.status === 'done'" class="w-5 h-5 rounded border border-gray-300 dark:border-gray-600 bg-gray-100 dark:bg-[#2c2c2c] flex items-center justify-center">
                           <div class="w-2.5 h-2.5 bg-gray-400 dark:bg-gray-500 rounded-sm"></div>
                       </div>
                       <div v-else class="w-5 h-5 rounded border-[1.5px] border-gray-300 dark:border-gray-600 hover:border-gray-400 dark:hover:border-gray-500 transition-colors"></div>
                   </button>
                   <textarea 
                       ref="titleInputRef"
                       v-model="editingTaskParams.title" 
                       class="flex-1 bg-transparent border-none outline-none text-[1.1rem] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 focus:ring-0 p-0 resize-none overflow-hidden leading-snug"
                       placeholder="New Task"
                       rows="1"
                       @input="adjustTitleHeight"
                       @keydown.enter.prevent="handleTitleEnter"
                   ></textarea>
              </div>
              
              <!-- Notes -->
              <div class="pl-9 mb-4 flex-1 flex flex-col min-h-[40px] max-h-[300px] overflow-y-auto overflow-x-hidden custom-scrollbar">
                  <TiptapEditor 
                       ref="tiptapRef"
                       v-model="editingTaskParams.content" 
                       :vaultPath="props.vaultPath || ''"
                       class="w-full flex-1"
                  />
              </div>
              
          </div>
          
          <!-- Footer Meta Bar -->
          <div class="px-5 pt-3 border-t border-gray-50 dark:border-[#2c2c2c] bg-white dark:bg-[#1c1c1e] flex items-center justify-start gap-2 flex-wrap relative" :style="!props.showActions ? 'padding-bottom: max(env(safe-area-inset-bottom), 12px);' : 'padding-bottom: 12px;'">
              <!-- Dates -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="(editingTaskParams.start_date || editingTaskParams.due_date) ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Set Dates" @click.stop="activeDropdown = activeDropdown === 'dates' ? null : 'dates'">
                  <Calendar class="w-[18px] h-[18px]" :class="(editingTaskParams.start_date || editingTaskParams.due_date) ? 'text-blue-500 mr-2' : ''"/>
                  
                  <span v-if="editingTaskParams.start_date || editingTaskParams.due_date" class="text-xs font-semibold">
                      <template v-if="editingTaskParams.start_date && editingTaskParams.due_date">
                          {{ editingTaskParams.start_date === todayStr ? 'Today' : editingTaskParams.start_date }} &rarr; {{ editingTaskParams.due_date === todayStr ? 'Today' : editingTaskParams.due_date }}
                      </template>
                      <template v-else-if="editingTaskParams.start_date">
                          {{ editingTaskParams.start_date === todayStr ? 'Today' : editingTaskParams.start_date }}
                      </template>
                      <template v-else-if="editingTaskParams.due_date">
                          Due: {{ editingTaskParams.due_date === todayStr ? 'Today' : editingTaskParams.due_date }}
                      </template>
                  </span>
                  
                  <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'dates' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-48 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Start Date</label>
                          <input type="date" v-model="editingTaskParams.start_date" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 mb-3 outline-none focus:ring-1 focus:ring-blue-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" />
                          
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Due Date</label>
                          <input type="date" v-model="editingTaskParams.due_date" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-blue-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" />
                      </div>
                  </div>
              </div>

              <!-- Tags -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.tags.length > 0 ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Manage Tags" @click.stop="activeDropdown = activeDropdown === 'tags' ? null : 'tags'">
                  <Tag class="w-[18px] h-[18px]" :class="editingTaskParams.tags.length > 0 ? 'text-blue-500 mr-2' : ''"/>
                  
                  <span v-if="editingTaskParams.tags.length > 0" class="text-xs font-semibold max-w-[150px] truncate">{{ editingTaskParams.tags }}</span>
                  
                  <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'tags' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-56 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Tags (comma separated)</label>
                          <input v-model="editingTaskParams.tags" placeholder="e.g. work, urgent" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-2 outline-none focus:ring-1 focus:ring-blue-500 text-[#1c1c1e] dark:text-[#f4f4f5]" />
                      </div>
                  </div>
              </div>

              <!-- Priority -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.priority ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Set Priority">
                  <Flag class="w-[18px] h-[18px]" :class="editingTaskParams.priority ? 'text-orange-500 mr-2' : ''" />
                  
                  <span v-if="editingTaskParams.priority" class="text-xs font-semibold uppercase text-orange-600 dark:text-orange-400">{{ editingTaskParams.priority }}</span>
                  
                  <select v-model="editingTaskParams.priority" class="absolute inset-0 opacity-0 cursor-pointer z-10">
                      <option value="">None</option>
                      <option value="P1">P1</option>
                      <option value="P2">P2</option>
                      <option value="P3">P3</option>
                      <option value="P4">P4</option>
                  </select>
              </div>

              <!-- Project -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.project_id ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Set Project">
                  <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :class="editingTaskParams.project_id ? 'text-indigo-500 mr-2' : ''"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                  
                  <span v-if="editingTaskParams.project_id" class="text-xs font-semibold max-w-[100px] truncate text-indigo-600 dark:text-indigo-400">
                      {{ props.projects?.find(p => p.id === editingTaskParams.project_id)?.title || 'Project' }}
                  </span>
                  
                  <select v-model="editingTaskParams.project_id" class="absolute inset-0 opacity-0 cursor-pointer z-10">
                      <option value="">No Project</option>
                      <option v-for="proj in props.projects" :key="proj.id" :value="proj.id">{{ proj.title }}</option>
                  </select>
              </div>

              <!-- Transfer -->
              <div class="relative flex items-center group">
                  <button 
                      @click.stop="editingTaskParams.is_transferred = !editingTaskParams.is_transferred; if(editingTaskParams.is_transferred) activeDropdown = 'transfer'; else activeDropdown = null;"
                      class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer flex items-center transition-colors" 
                      :class="editingTaskParams.is_transferred ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" 
                      title="Transfer Task"
                  >
                      <Send class="w-[18px] h-[18px]" :class="editingTaskParams.is_transferred ? 'text-purple-500 mr-2' : ''" />
                      <span v-if="editingTaskParams.is_transferred && editingTaskParams.transferred_to" class="text-xs font-semibold max-w-[120px] truncate text-purple-600 dark:text-purple-400">
                          {{ editingTaskParams.transferred_to }}
                      </span>
                  </button>
                  
                  <div v-if="editingTaskParams.is_transferred" class="absolute bottom-full left-1/2 -translate-x-1/2 pb-2 transition-all z-50" :class="activeDropdown === 'transfer' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-52 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-2 pointer-events-auto cursor-default items-center">
                          <label class="block text-[10px] font-semibold text-gray-400 mb-1 w-full text-left ml-1">Transfer to:</label>
                          <div class="flex items-center gap-1.5 w-full">
                              <input v-model="editingTaskParams.transferred_to" placeholder="Name..." class="flex-1 min-w-0 text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-purple-500 text-[#1c1c1e] dark:text-[#f4f4f5]" />
                              
                              <button 
                                  @click.stop="editingTaskParams.track_progress = !editingTaskParams.track_progress"
                                  class="p-1.5 rounded-md hover:opacity-80 transition-opacity shrink-0 flex items-center justify-center border"
                                  :title="editingTaskParams.track_progress ? 'Tracking Progress' : 'Not Tracking'"
                                  :class="editingTaskParams.track_progress ? 'bg-blue-50 dark:bg-blue-900/30 text-blue-500 border-blue-200 dark:border-blue-800' : 'text-gray-400 dark:text-gray-500 bg-gray-50 dark:bg-[#2a2a2a] border-gray-200 dark:border-[#2c2c2c]'"
                              >
                                  <Eye v-if="editingTaskParams.track_progress" class="w-4 h-4" />
                                  <EyeOff v-else class="w-4 h-4" />
                              </button>
                          </div>
                      </div>
                  </div>
              </div>

              <!-- Delete Button (Only when editing existing task, i.e., !props.showActions) -->
              <div v-if="!props.showActions" class="ml-auto relative flex items-center p-1.5 rounded-md hover:bg-red-50 dark:hover:bg-red-900/20 text-red-400 hover:text-red-500 cursor-pointer transition-colors" title="Delete Task" @click.stop="emit('delete')">
                  <Trash2 class="w-[18px] h-[18px]" />
              </div>
          </div>

          <!-- Bottom Actions (Only for Convert mode) -->
          <div v-if="props.showActions" class="pt-4 px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-end gap-3 shrink-0" style="padding-bottom: max(env(safe-area-inset-bottom), 16px);">
              <button @click="close" class="px-5 py-2 hover:bg-gray-200 dark:hover:bg-[#2c2c2c] text-gray-700 dark:text-gray-300 rounded-lg text-sm font-medium transition-all cursor-pointer border border-transparent">
                  Cancel
              </button>
              <button @click="save" class="px-5 py-2 bg-blue-600 hover:bg-blue-700 dark:bg-blue-500 dark:hover:bg-blue-600 text-white rounded-lg text-sm font-medium transition-all shadow-sm cursor-pointer flex items-center gap-1.5 border border-transparent active:scale-95">
                  <CheckCircle2 class="w-4 h-4" /> Create Task
              </button>
          </div>
      </div>
  </div>
</template>
