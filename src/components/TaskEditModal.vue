<script setup lang="ts">
import { ref, computed, nextTick } from 'vue';
import { CheckCircle2, Circle, ListTodo, Calendar, Tag, Flag, X, Send, Eye, EyeOff } from 'lucide-vue-next';

const props = defineProps<{
    task: any;
    showActions?: boolean;
}>();

const emit = defineEmits(['save', 'close']);

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
    checklist: props.task?.checklist ? JSON.parse(JSON.stringify(props.task.checklist)) : [],
    status: props.task?.status || 'todo'
});

const todayStr = computed(() => {
    const today = new Date();
    return today.toISOString().split('T')[0];
});

const addChecklistItem = () => {
    editingTaskParams.value.checklist.push({ content: '', completed: false });
};

const focusLastChecklistItem = () => {
    nextTick(() => {
        const inputs = document.querySelectorAll('.checklist-input');
        if (inputs.length > 0) {
            (inputs[inputs.length - 1] as HTMLInputElement).focus();
        }
    });
};

const removeChecklistItem = (index: number) => {
    editingTaskParams.value.checklist.splice(index, 1);
};

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
  <div class="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/10 dark:bg-black/40 backdrop-blur-[2px]" @mousedown.self="handleBackgroundClick">
      <div class="w-full max-w-lg bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-[0_20px_40px_rgba(0,0,0,0.1)] dark:shadow-[0_20px_40px_rgba(0,0,0,0.4)] border border-gray-100 dark:border-[#2c2c2c] overflow-hidden flex flex-col" @mousedown.stop>
          <div class="p-5 flex flex-col pt-6">
              
              <!-- Title & Checkbox -->
              <div class="flex items-start gap-4 mb-3">
                   <button @click="editingTaskParams.status = (editingTaskParams.status === 'done' ? 'todo' : 'done')" class="shrink-0 mt-0.5 cursor-pointer">
                       <div v-if="editingTaskParams.status === 'done'" class="w-5 h-5 rounded border border-gray-300 dark:border-gray-600 bg-gray-100 dark:bg-[#2c2c2c] flex items-center justify-center">
                           <div class="w-2.5 h-2.5 bg-gray-400 dark:bg-gray-500 rounded-sm"></div>
                       </div>
                       <div v-else class="w-5 h-5 rounded border-[1.5px] border-gray-300 dark:border-gray-600 hover:border-gray-400 dark:hover:border-gray-500 transition-colors"></div>
                   </button>
                   <input 
                       v-model="editingTaskParams.title" 
                       class="flex-1 bg-transparent border-none outline-none text-[1.1rem] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 focus:ring-0 p-0"
                       placeholder="New To-Do"
                   />
              </div>
              
              <!-- Notes -->
              <div class="pl-9 mb-4">
                  <textarea 
                       v-model="editingTaskParams.content" 
                       class="w-full bg-transparent border-none outline-none text-[15px] leading-relaxed text-gray-500 dark:text-gray-400 placeholder-gray-300 focus:ring-0 p-0 resize-none min-h-[40px]"
                       placeholder="Notes"
                  ></textarea>
              </div>
              
              <!-- Checklist -->
              <div class="pl-9 mb-2 space-y-2">
                  <div v-for="(item, i) in editingTaskParams.checklist" :key="i" class="flex items-start gap-3 group relative">
                      <button @click="item.completed = !item.completed" class="shrink-0 mt-[5px] cursor-pointer">
                           <div class="w-[14px] h-[14px] rounded-full border-2 transition-colors border-blue-500 flex items-center justify-center p-[2px]" v-if="item.completed">
                               <div class="w-full h-full bg-blue-500 rounded-full"></div>
                           </div>
                           <div class="w-[14px] h-[14px] rounded-full border-2 border-gray-300 dark:border-gray-600 transition-colors" v-else></div>
                      </button>
                      
                      <input 
                          v-model="item.content" 
                          class="flex-1 bg-transparent border-none outline-none text-[15px] focus:ring-0 p-0 checklist-input pb-1.5 border-b border-gray-100 dark:border-[#2c2c2c] bg-white dark:bg-[#1c1c1e]"
                          :class="item.completed ? 'text-gray-400 line-through' : 'text-[#1c1c1e] dark:text-[#f4f4f5]'"
                          placeholder=""
                          @keydown.enter.prevent="addChecklistItem(); focusLastChecklistItem()"
                          @keydown.backspace="item.content === '' ? removeChecklistItem(i) : null"
                      />
                      <button @click="removeChecklistItem(i)" class="absolute right-0 top-0 opacity-0 group-hover:opacity-100 text-gray-400 hover:text-red-500 transition-opacity p-0.5 cursor-pointer bg-white dark:bg-[#1c1c1e]">
                          <X class="w-4 h-4" />
                      </button>
                  </div>
              </div>
          </div>
          
          <!-- Footer Meta Bar -->
          <div class="px-5 py-3 border-t border-gray-50 dark:border-[#2c2c2c] bg-white dark:bg-[#1c1c1e] flex items-center justify-start gap-2 flex-wrap">
              <!-- Checklist -->
              <div v-if="editingTaskParams.checklist.length === 0" class="relative flex items-center justify-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer text-gray-400" title="Add Checklist" @click="addChecklistItem(); focusLastChecklistItem()">
                  <ListTodo class="w-[18px] h-[18px]"/>
              </div>
              
              <!-- Dates -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="(editingTaskParams.start_date || editingTaskParams.due_date) ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Set Dates">
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
                  
                  <div class="absolute bottom-full left-0 pb-2 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all z-50">
                      <div class="w-48 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Start Date</label>
                          <input type="date" v-model="editingTaskParams.start_date" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 mb-3 outline-none focus:ring-1 focus:ring-blue-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" />
                          
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Due Date</label>
                          <input type="date" v-model="editingTaskParams.due_date" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-blue-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" />
                      </div>
                  </div>
              </div>

              <!-- Tags -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.tags.length > 0 ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Manage Tags">
                  <Tag class="w-[18px] h-[18px]" :class="editingTaskParams.tags.length > 0 ? 'text-blue-500 mr-2' : ''"/>
                  
                  <span v-if="editingTaskParams.tags.length > 0" class="text-xs font-semibold max-w-[150px] truncate">{{ editingTaskParams.tags }}</span>
                  
                  <div class="absolute bottom-full left-0 pb-2 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all z-50">
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

              <!-- Transfer -->
              <div class="relative flex items-center group">
                  <button 
                      @click="editingTaskParams.is_transferred = !editingTaskParams.is_transferred"
                      class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer flex items-center transition-colors" 
                      :class="editingTaskParams.is_transferred ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" 
                      title="Transfer Task"
                  >
                      <Send class="w-[18px] h-[18px]" :class="editingTaskParams.is_transferred ? 'text-purple-500 mr-2' : ''" />
                      <span v-if="editingTaskParams.is_transferred && editingTaskParams.transferred_to" class="text-xs font-semibold max-w-[120px] truncate text-purple-600 dark:text-purple-400">
                          {{ editingTaskParams.transferred_to }}
                      </span>
                  </button>
                  
                  <div v-if="editingTaskParams.is_transferred" class="absolute bottom-full left-1/2 -translate-x-1/2 pb-2 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all z-50">
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
          </div>

          <!-- Bottom Actions (Only for Convert mode) -->
          <div v-if="props.showActions" class="py-4 px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-end gap-3 shrink-0">
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
