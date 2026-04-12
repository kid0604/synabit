<script setup lang="ts">
import { ref, onMounted, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { confirm } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, Circle, AlertCircle, Plus, Trash2, Tag, CalendarDays, List, Trello, Table2, MoreHorizontal, Search, X } from 'lucide-vue-next';

const props = defineProps<{
  vaultPath: string;
}>();

export interface TaskMetadata {
    id: string;
    title: string;
    status: string;
    start_date: string;
    due_date: string;
    comment: string;
    source_link: string;
    tags: string[];
    content: string;
    path: string;
    created_at: string;
    updated_at: string;
    custom_fields: Record<string, any>;
}

const tasks = ref<TaskMetadata[]>([]);
const newTaskTitle = ref('');
const isSubmitting = ref(false);
const searchQuery = ref('');
const timeFilter = ref<'all' | 'today' | 'this_week' | 'this_month' | 'overdue' | 'custom'>('all');
const customDateFilter = ref('');

const filteredTasks = computed(() => {
    let result = tasks.value;
    
    // 1. Time Filter
    if (timeFilter.value !== 'all') {
        const now = new Date();
        // Convert local strictly
        const offset = now.getTimezoneOffset() * 60000;
        const localNow = new Date(now.getTime() - offset);
        const todayStr = localNow.toISOString().split('T')[0];
        
        // Start of week (Monday as day 1)
        const dayOfWeek = localNow.getDay() || 7; 
        const startOfWeek = new Date(localNow);
        startOfWeek.setDate(localNow.getDate() - dayOfWeek + 1);
        const startOfWeekStr = startOfWeek.toISOString().split('T')[0];
        
        // End of week (Sunday)
        const endOfWeek = new Date(startOfWeek);
        endOfWeek.setDate(startOfWeek.getDate() + 6);
        const endOfWeekStr = endOfWeek.toISOString().split('T')[0];
        
        // Month prefix YYYY-MM
        const monthStr = todayStr.substring(0, 7);
        
        result = result.filter(t => {
            const dateToCompare = t.due_date || t.start_date;
            if (!dateToCompare) return false;
            
            switch (timeFilter.value) {
                case 'today':
                    return dateToCompare === todayStr;
                case 'this_week':
                    return dateToCompare >= startOfWeekStr && dateToCompare <= endOfWeekStr;
                case 'this_month':
                    return dateToCompare.startsWith(monthStr);
                case 'overdue':
                    return dateToCompare < todayStr && t.status !== 'done';
                case 'custom':
                    return customDateFilter.value ? dateToCompare === customDateFilter.value : true;
                default:
                    return true;
            }
        });
    }

    // 2. Text/Tag Filter
    if (searchQuery.value.trim()) {
        const query = searchQuery.value.toLowerCase();
        if (query.startsWith('#') && query.length > 1) {
            const tagSearch = query.substring(1).trim();
            result = result.filter(t => t.tags.some(tag => tag.toLowerCase().includes(tagSearch)));
        } else {
            result = result.filter(t => 
                t.title.toLowerCase().includes(query) || 
                t.content.toLowerCase().includes(query) ||
                t.tags.some(tag => tag.toLowerCase().includes(query))
            );
        }
    }
    
    return result;
});

const viewMode = ref<'list' | 'board' | 'table'>(localStorage.getItem('synabitTaskViewMode') as 'list' | 'board' | 'table' || 'list');

watch(viewMode, (newVal) => {
    localStorage.setItem('synabitTaskViewMode', newVal);
});

const BOARD_COLUMNS = [
  { id: 'todo', name: 'TO DO', class: 'border-t-2 border-gray-300 dark:border-gray-600' },
  { id: 'in_progress', name: 'IN PROGRESS', class: 'border-t-2 border-blue-400 dark:border-blue-500' },
  { id: 'done', name: 'DONE', class: 'border-t-2 border-green-400 dark:border-green-500' }
];

const tasksByStatus = computed(() => {
    const sorted: Record<string, TaskMetadata[]> = { todo: [], in_progress: [], done: [] };
    filteredTasks.value.forEach(t => {
        if (sorted[t.status]) {
            sorted[t.status].push(t);
        } else {
            sorted.todo.push(t);
        }
    });
    return sorted;
});

const onDragStart = (e: DragEvent, task: TaskMetadata) => {
    if (e.dataTransfer) {
        e.dataTransfer.setData('taskId', task.id);
        e.dataTransfer.effectAllowed = 'move';
    }
};

const onDrop = async (e: DragEvent, newStatus: string) => {
    const taskId = e.dataTransfer?.getData('taskId');
    if (!taskId) return;
    
    const task = tasks.value.find(t => t.id === taskId);
    if (!task || task.status === newStatus) return;
    
    try {
        await invoke('update_task', {
            path: task.path,
            metadata: {
                title: task.title,
                status: newStatus,
                start_date: task.start_date,
                due_date: task.due_date,
                comment: task.comment,
                source_link: task.source_link,
                tags: task.tags,
                ...task.custom_fields
            },
            content: task.content
        });
        task.status = newStatus;
    } catch (err) {
        console.error("Drag update failed", err);
    }
};

const editingTask = ref<TaskMetadata | null>(null);
const editingTaskParams = ref({
    title: '',
    content: '',
    start_date: '',
    due_date: '',
    comment: '',
    tags: '',
});
const customFields = ref<{k: string, v: string}[]>([]);

const openEditModal = (task: TaskMetadata) => {
    editingTask.value = task;
    editingTaskParams.value = {
        title: task.title,
        content: task.content,
        start_date: task.start_date,
        due_date: task.due_date,
        comment: task.comment,
        tags: task.tags.join(', ')
    };
    customFields.value = Object.entries(task.custom_fields || {}).map(([k, v]) => ({
        k, v: String(v)
    }));
};

const closeEditModal = () => {
    editingTask.value = null;
};

const saveTask = async () => {
    if (!editingTask.value) return;
    try {
        const tagArray = editingTaskParams.value.tags.split(',').map(t => t.trim()).filter(t => t !== '');
        const updatedCustomFields: Record<string, string> = {};
        
        customFields.value.forEach(field => {
            if (field.k.trim()) {
                updatedCustomFields[field.k.trim()] = field.v;
            }
        });
        
        await invoke('update_task', {
            path: editingTask.value.path,
            metadata: {
                title: editingTaskParams.value.title,
                status: editingTask.value.status,
                start_date: editingTaskParams.value.start_date,
                due_date: editingTaskParams.value.due_date,
                comment: editingTaskParams.value.comment,
                source_link: editingTask.value.source_link,
                tags: tagArray,
                ...updatedCustomFields
            },
            content: editingTaskParams.value.content
        });
        
        editingTask.value.title = editingTaskParams.value.title;
        editingTask.value.content = editingTaskParams.value.content;
        editingTask.value.start_date = editingTaskParams.value.start_date;
        editingTask.value.due_date = editingTaskParams.value.due_date;
        editingTask.value.comment = editingTaskParams.value.comment;
        editingTask.value.tags = tagArray;
        editingTask.value.custom_fields = updatedCustomFields;
        
        closeEditModal();
    } catch (e) {
        console.error("Failed to update task", e);
    }
};

const loadTasks = async () => {
    if (!props.vaultPath) return;
    try {
        tasks.value = await invoke('scan_tasks', { vaultPath: props.vaultPath });
    } catch (e) {
        console.error("Failed to load tasks", e);
    }
};

const submitTask = async () => {
    if (!newTaskTitle.value.trim() || !props.vaultPath) return;
    
    isSubmitting.value = true;
    try {
        const newTask = await invoke<TaskMetadata>('create_task', {
            vaultPath: props.vaultPath,
            metadata: {
                title: newTaskTitle.value.trim(),
                status: 'todo',
                start_date: '',
                due_date: '',
                comment: '',
                source_link: '',
                tags: []
            },
            content: ''
        });
        
        tasks.value.unshift(newTask);
        newTaskTitle.value = '';
    } catch (e) {
        console.error("Failed to create task", e);
    } finally {
        isSubmitting.value = false;
    }
};

const toggleTaskStatus = async (task: TaskMetadata) => {
    const newStatus = task.status === 'done' ? 'todo' : 'done';
    
    try {
        await invoke('update_task', {
            path: task.path,
            metadata: {
                title: task.title,
                status: newStatus,
                start_date: task.start_date,
                due_date: task.due_date,
                comment: task.comment,
                source_link: task.source_link,
                tags: task.tags,
                ...task.custom_fields
            },
            content: task.content
        });
        task.status = newStatus;
    } catch (e) {
        console.error("Failed to update task", e);
    }
};

const deleteTask = async (task: TaskMetadata, index: number) => {
    const isConfirmed = await confirm('Xoá công việc này?', { title: 'Xoá Task', kind: 'warning' });
    if (!isConfirmed) return;
    
    try {
        await invoke('delete_task', { path: task.path });
        tasks.value.splice(index, 1);
    } catch (e) {
        console.error("Failed to delete task", e);
    }
};

onMounted(() => {
    loadTasks();
});

watch(() => props.vaultPath, () => {
    loadTasks();
});
</script>

<template>
  <div class="h-full flex flex-col bg-[#fdfdfc] dark:bg-[#242424] w-full overflow-hidden">
      <!-- Header -->
      <div class="px-8 pt-10 pb-4 shrink-0 border-b border-transparent">
          <div class="flex items-center justify-between mb-6">
              <h1 class="text-3xl font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] tracking-tight">Tasks</h1>
              <div class="flex bg-gray-100 dark:bg-[#1a1a1a] p-1 rounded-xl">
                  <button @click="viewMode = 'list'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'list' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'">
                      <List class="w-4 h-4"/>
                  </button>
                  <button @click="viewMode = 'board'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'board' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'">
                      <Trello class="w-4 h-4"/>
                  </button>
                  <button @click="viewMode = 'table'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'table' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'">
                      <Table2 class="w-4 h-4"/>
                  </button>
              </div>
          </div>
          
          <!-- Quick Add Input -->
          <div class="w-full bg-white dark:bg-[#1e1e1e] rounded-xl shadow-sm border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden focus-within:ring-1 focus-within:ring-black dark:focus-within:ring-white transition-all flex items-center px-4 py-3">
              <div class="mr-3 text-gray-400">
                  <Plus class="w-5 h-5"/>
              </div>
              <input 
                  v-model="newTaskTitle" 
                  @keydown.enter="submitTask"
                  class="flex-1 bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] font-medium"
                  placeholder="What needs to be done? (Press Enter)"
                  :disabled="isSubmitting"
              />
          </div>

          <!-- Filter Bar -->
          <div class="mt-4 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
              <div class="relative w-full sm:max-w-xs group">
                  <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none">
                      <Search class="h-4 w-4 text-gray-400 group-focus-within:text-blue-500 transition-colors" />
                  </div>
                  <input 
                      v-model="searchQuery" 
                      type="text" 
                      class="block w-full pl-10 pr-3 py-2 border border-gray-200 dark:border-[#2c2c2c] rounded-full leading-5 bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-black/5 dark:focus:ring-white/10 sm:text-sm transition-all shadow-[0_2px_8px_rgba(0,0,0,0.02)]" 
                      placeholder="Search tasks or #tag..." 
                  />
                  <button v-if="searchQuery" @click="searchQuery = ''" class="absolute inset-y-0 right-0 pr-3 flex items-center cursor-pointer">
                      <X class="h-4 w-4 text-gray-400 hover:text-gray-600 transition-colors" />
                  </button>
              </div>

              <div class="flex items-center gap-2 w-full sm:w-auto">
                  <select v-model="timeFilter" class="bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm rounded-lg focus:ring-black dark:focus:ring-white focus:border-black dark:focus:border-white w-full sm:w-auto p-2 outline-none shadow-sm cursor-pointer">
                      <option value="all">All Time</option>
                      <option value="today">Today</option>
                      <option value="this_week">This Week</option>
                      <option value="this_month">This Month</option>
                      <option value="overdue">Overdue</option>
                      <option value="custom">Custom Date...</option>
                  </select>
                  <input 
                      v-if="timeFilter === 'custom'" 
                      type="date" 
                      v-model="customDateFilter" 
                      class="bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm rounded-lg focus:ring-black dark:focus:ring-white focus:border-black dark:focus:border-white p-2 outline-none shadow-sm cursor-pointer [color-scheme:light] dark:[color-scheme:dark]"
                  />
              </div>
          </div>
      </div>

      <!-- Main Content -->
      <div class="flex-1 overflow-y-auto px-8 pb-16">
          <div v-if="filteredTasks.length === 0" class="flex flex-col items-center justify-center h-full opacity-40">
              <CheckCircle2 class="w-16 h-16 mb-4"/>
              <p>You're all caught up!</p>
          </div>
          
          <div v-else class="h-full">
              <!-- LIST VIEW -->
              <div v-if="viewMode === 'list'" class="space-y-2 mt-4 max-w-4xl mx-auto">
                  <div v-for="(task, index) in filteredTasks" :key="task.id" 
                      class="group flex items-center p-3 rounded-xl hover:bg-gray-50 dark:hover:bg-[#1a1a1a] border border-transparent hover:border-gray-100 dark:hover:border-gray-800 transition-colors cursor-pointer"
                      :class="{'opacity-50': task.status === 'done'}"
                      @click="openEditModal(task)"
                  >
                      <!-- Checkbox -->
                      <button @click.stop="toggleTaskStatus(task)" class="shrink-0 mr-4 transition-colors cursor-pointer">
                          <CheckCircle2 v-if="task.status === 'done'" class="w-6 h-6 text-green-500 fill-green-50 dark:fill-green-900/30" />
                          <Circle v-else class="w-6 h-6 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
                      </button>
                      
                      <!-- Title & Meta -->
                      <div class="flex-1 min-w-0 flex items-center justify-between">
                          <p class="text-[15px] font-medium truncate transition-all duration-300" :class="task.status === 'done' ? 'text-gray-400 line-through' : 'text-[#1c1c1e] dark:text-[#f4f4f5]'">
                              {{ task.title }}
                          </p>
                          <div class="flex items-center gap-3 overflow-hidden ml-4 shrink-0">
                              <span v-if="task.status === 'in_progress'" class="text-[10px] px-2 py-0.5 rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300 font-bold tracking-wider">DOING</span>
                              
                              <span v-if="task.due_date" class="text-xs flex items-center text-red-500 font-medium">
                                  <CalendarDays class="w-3 h-3 mr-1" />
                                  {{ task.due_date }}
                              </span>
                              
                              <span v-if="task.start_date" class="text-xs flex items-center text-blue-500 font-medium">
                                  <CalendarDays class="w-3 h-3 mr-1" />
                                  {{ task.start_date }}
                              </span>
                              
                              <span v-if="task.tags.length > 0" class="text-xs flex items-center text-gray-500 max-w-[150px] truncate">
                                  <Tag class="w-3 h-3 mr-1 shrink-0" />
                                  {{ task.tags.join(', ') }}
                              </span>
                          </div>
                      </div>
                      
                      <!-- Actions -->
                      <div class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1 ml-4 w-[60px] justify-end">
                          <button @click.stop="deleteTask(task, tasks.findIndex(t => t.id === task.id))" class="p-1.5 text-gray-400 hover:text-red-500 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer">
                              <Trash2 class="w-4 h-4" />
                          </button>
                      </div>
                  </div>
              </div>

              <!-- BOARD VIEW -->
              <div v-else-if="viewMode === 'board'" class="flex gap-6 h-full mt-6 pb-8 overflow-x-auto">
                  <div v-for="col in BOARD_COLUMNS" :key="col.id" 
                       class="flex-1 min-w-[280px] max-w-[350px] flex flex-col bg-gray-50/50 dark:bg-[#161616] rounded-2xl p-4 border border-[#e6e6e6] dark:border-[#2c2c2c]"
                       @dragover.prevent 
                       @drop="onDrop($event, col.id)"
                  >
                      <div class="flex items-center justify-between mb-4 px-1" :class="col.class">
                          <h3 class="text-xs font-bold text-gray-500 pt-3">{{ col.name }} <span class="bg-gray-200 dark:bg-[#2a2a2a] text-gray-600 dark:text-gray-300 ml-2 px-2 py-0.5 rounded-full">{{ tasksByStatus[col.id].length }}</span></h3>
                          <button class="text-gray-400 hover:text-black dark:hover:text-white pt-3"><Plus class="w-4 h-4"/></button>
                      </div>
                      <div class="flex-1 overflow-y-auto space-y-3 pb-4">
                          <div v-for="task in tasksByStatus[col.id]" :key="task.id"
                               draggable="true"
                               @dragstart="onDragStart($event, task)"
                               @click="openEditModal(task)"
                               class="bg-white dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] hover:shadow-md transition-shadow cursor-grab active:cursor-grabbing group"
                          >
                             <p class="text-sm font-medium text-[#1c1c1e] dark:text-[#f4f4f5] leading-snug mb-3">{{ task.title }}</p>
                             <div class="flex items-center justify-between mt-auto pt-2 border-t border-gray-50 dark:border-[#2c2c2c]">
                                 <div class="flex gap-2">
                                     <span v-if="task.start_date || task.due_date" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded flex items-center">
                                         <CalendarDays class="w-3 h-3 mr-1" /> {{ task.start_date ? task.start_date.substring(5) : '--' }} - {{ task.due_date ? task.due_date.substring(5) : '--' }}
                                     </span>
                                     <div v-if="task.tags.length" class="flex flex-wrap gap-1">
                                         <span v-for="tag in task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">
                                             {{ tag }}
                                         </span>
                                     </div>
                                 </div>
                                 <button @click.stop="deleteTask(task, tasks.findIndex(t => t.id === task.id))" class="text-gray-300 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer">
                                     <Trash2 class="w-3.5 h-3.5" />
                                 </button>
                             </div>
                          </div>
                      </div>
                  </div>
              </div>

              <!-- TABLE VIEW -->
              <div v-else-if="viewMode === 'table'" class="mt-6 border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl overflow-hidden bg-white dark:bg-[#1e1e1e]">
                 <table class="w-full text-left text-sm">
                     <thead class="bg-gray-50 dark:bg-[#1a1a1a] text-gray-500 dark:text-gray-400 text-xs uppercase font-semibold">
                         <tr>
                             <th class="px-6 py-3 w-8">Status</th>
                             <th class="px-6 py-3">Title</th>
                             <th class="px-6 py-3 w-32">Start Date</th>
                             <th class="px-6 py-3 w-32">Due Date</th>
                             <th class="px-6 py-3 w-48">Tags</th>
                             <th class="px-6 py-3 w-16"></th>
                         </tr>
                     </thead>
                     <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#2c2c2c]">
                         <tr v-for="(task, index) in filteredTasks" :key="task.id" class="hover:bg-gray-50 dark:hover:bg-[#252525] group cursor-pointer" @click="openEditModal(task)">
                             <td class="px-6 py-3">
                                 <button @click.stop="toggleTaskStatus(task)" class="transition-colors cursor-pointer block mt-1">
                                      <CheckCircle2 v-if="task.status === 'done'" class="w-5 h-5 text-green-500" />
                                      <Circle v-else class="w-5 h-5 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
                                  </button>
                             </td>
                             <td class="px-6 py-3 font-medium text-[#1c1c1e] dark:text-[#f4f4f5]" :class="task.status === 'done' ? 'line-through text-gray-400' : ''">
                                 {{ task.title }}
                             </td>
                             <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                                 {{ task.start_date || '--/--/----' }}
                             </td>
                             <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                                 {{ task.due_date || '--/--/----' }}
                             </td>
                             <td class="px-6 py-3">
                                 <div class="flex flex-wrap gap-1">
                                     <span v-for="tag in task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">
                                         {{ tag }}
                                     </span>
                                 </div>
                             </td>
                             <td class="px-6 py-3">
                                 <button @click.stop="deleteTask(task, index)" class="p-1 text-gray-400 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer">
                                     <Trash2 class="w-4 h-4" />
                                 </button>
                             </td>
                         </tr>
                     </tbody>
                 </table>
              </div>
          </div>
      </div>

      <!-- Edit Task Modal -->
      <div v-if="editingTask" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @click="closeEditModal">
          <div class="w-full max-w-lg rounded-2xl shadow-xl flex flex-col border border-[#e6e6e6] dark:border-[#2c2c2c] bg-white dark:bg-[#1e1e1e] overflow-hidden" @click.stop>
              <div class="p-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-between">
                  <h3 class="text-xl font-bold text-[#1c1c1e] dark:text-[#f4f4f5]">Edit Task</h3>
              </div>
              <div class="p-6 overflow-y-auto max-h-[70vh] flex flex-col gap-4">
                  <div>
                      <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Title</label>
                      <input 
                          v-model="editingTaskParams.title" 
                          class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                      />
                  </div>
                  <div>
                      <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Description</label>
                      <textarea 
                          v-model="editingTaskParams.content" 
                          class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 min-h-[100px] outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                      ></textarea>
                  </div>
                  <div class="flex gap-4">
                      <div class="flex-1">
                          <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Start Date</label>
                          <input 
                              type="date"
                              v-model="editingTaskParams.start_date" 
                              class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5] [color-scheme:light] dark:[color-scheme:dark]"
                          />
                      </div>
                      <div class="flex-1">
                          <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Due Date</label>
                          <input 
                              type="date"
                              v-model="editingTaskParams.due_date" 
                              class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5] [color-scheme:light] dark:[color-scheme:dark]"
                          />
                      </div>
                  </div>
                  <div>
                      <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Tags (comma separated)</label>
                      <input 
                          v-model="editingTaskParams.tags" 
                          placeholder="work, urgent, finance"
                          class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                      />
                  </div>
                  <div>
                      <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Add Comment</label>
                      <textarea 
                          v-model="editingTaskParams.comment"
                          placeholder="Add your note or comment..."
                          class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 min-h-[60px] outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                      ></textarea>
                  </div>
                  <div class="pt-2 border-t border-gray-200 dark:border-gray-700">
                      <div class="flex justify-between items-center mb-2">
                          <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500">Custom Properties</label>
                          <button @click="customFields.push({k: '', v: ''})" class="text-xs text-blue-500 hover:text-blue-600 font-bold flex items-center cursor-pointer">
                              <Plus class="w-3 h-3 mr-1" /> Add Property
                          </button>
                      </div>
                      <div class="space-y-2">
                          <div v-for="(field, i) in customFields" :key="i" class="flex gap-2 items-center">
                              <input v-model="field.k" placeholder="Property" class="w-1/3 bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-2 text-sm outline-none text-[#1c1c1e] dark:text-[#f4f4f5]" />
                              <input v-model="field.v" placeholder="Value" class="flex-1 bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-2 text-sm outline-none text-[#1c1c1e] dark:text-[#f4f4f5]" />
                              <button @click="customFields.splice(i, 1)" class="text-red-400 hover:text-red-500 cursor-pointer">
                                  <Trash2 class="w-4 h-4" />
                              </button>
                          </div>
                          <p v-if="customFields.length === 0" class="text-xs text-gray-400 italic">No custom properties.</p>
                      </div>
                  </div>

              </div>
              <div class="py-4 px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex justify-end gap-3 rounded-b-2xl">
                  <button @click="closeEditModal" class="px-5 py-2 hover:bg-gray-200 dark:hover:bg-[#2c2c2c] text-gray-700 dark:text-gray-300 rounded-lg text-sm font-medium transition-all cursor-pointer">
                      Cancel
                  </button>
                  <button @click="saveTask" class="px-5 py-2 bg-black dark:bg-white text-white dark:text-black rounded-lg text-sm font-semibold hover:scale-95 transition-all shadow-sm cursor-pointer">
                      Save Changes
                  </button>
              </div>
          </div>
      </div>
  </div>
</template>
