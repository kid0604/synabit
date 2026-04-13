<script setup lang="ts">
import { ref, onMounted, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { confirm } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, Circle, Plus, Trash2, Tag, CalendarDays, List, Trello, Table2, Search, X, Info, Target, Inbox, Sun, Calendar, Coffee, Send, Flag, ListTodo } from 'lucide-vue-next';
import TaskEditModal from './TaskEditModal.vue';

const props = defineProps<{
  vaultPath: string;
}>();

export interface ChecklistItem {
    content: string;
    completed: boolean;
}

export interface TaskMetadata {
    id: string;
    title: string;
    status: string;
    is_transferred: boolean;
    priority: string;
    start_date: string;
    due_date: string;
    comment: string;
    source_link: string;
    tags: string[];
    checklist: ChecklistItem[];
    content: string;
    path: string;
    created_at: string;
    updated_at: string;
    custom_fields: Record<string, any>;
    isNew?: boolean;
}

const tasks = ref<TaskMetadata[]>([]);
const searchQuery = ref('');

const activeCategory = ref<'all' | 'today' | 'upcoming' | 'someday' | 'transferred'>('today');

const searchedTasks = computed(() => {
    let result = tasks.value;
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

const categoryCounts = computed(() => {
    const now = new Date();
    const offset = now.getTimezoneOffset() * 60000;
    const localNow = new Date(now.getTime() - offset);
    const todayStr = localNow.toISOString().split('T')[0];
    
    let all = 0, today = 0, upcoming = 0, someday = 0, transferred = 0;
    
    searchedTasks.value.forEach(t => {
        if (t.status === 'done') return;
        all++;
        if (t.is_transferred) {
            transferred++;
            return;
        }
        
        let isToday = false;
        if (t.due_date === todayStr) isToday = true;
        else if (!t.due_date && t.start_date && t.start_date <= todayStr) isToday = true;
        else if (t.due_date && t.due_date < todayStr) isToday = true;
        
        if (isToday) {
            today++;
            return;
        }
        
        if (t.due_date && t.due_date > todayStr) upcoming++;
        else if (t.start_date && t.start_date > todayStr) upcoming++;
        else someday++;
    });
    
    return { all, today, upcoming, someday, transferred };
});

const todayStr = computed(() => {
    const now = new Date();
    const offset = now.getTimezoneOffset() * 60000;
    const localNow = new Date(now.getTime() - offset);
    return localNow.toISOString().split('T')[0];
});

const activeCategoryTasks = computed(() => {
    const today = todayStr.value;
    
    return searchedTasks.value.filter(t => {
        if (activeCategory.value === 'all') return true;

        if (activeCategory.value === 'transferred') return t.is_transferred;
        if (t.is_transferred) return false; 
        
        const isToday = (t.due_date === today) || 
                        (!t.due_date && t.start_date && t.start_date <= today) ||
                        (t.due_date && t.due_date < today);

        if (activeCategory.value === 'today') return isToday;
        
        if (isToday) return false; 
        
        const isUpcoming = (t.due_date && t.due_date > today) || (t.start_date && t.start_date > today);
        if (activeCategory.value === 'upcoming') return isUpcoming;
        
        if (activeCategory.value === 'someday') return !isUpcoming;
        
        return false;
    });
});

const viewMode = ref<'list' | 'board' | 'table' | 'gtd'>(localStorage.getItem('synabitTaskViewMode') as 'list' | 'board' | 'table' | 'gtd' || 'list');

watch(viewMode, (newVal) => {
    localStorage.setItem('synabitTaskViewMode', newVal);
});

const BOARD_COLUMNS = [
  { id: 'todo', name: 'TO DO', class: 'border-t-2 border-gray-300 dark:border-gray-600' },
  { id: 'in_progress', name: 'IN PROGRESS', class: 'border-t-2 border-blue-400 dark:border-blue-500' },
  { id: 'done', name: 'DONE', class: 'border-t-2 border-green-400 dark:border-green-500' }
];

const getPriorityClass = (priority: string) => {
    switch (priority) {
        case 'P1': return 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400';
        case 'P2': return 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400';
        case 'P3': return 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400';
        case 'P4': return 'bg-slate-100 text-slate-700 dark:bg-slate-800/50 dark:text-slate-400';
        default: return '';
    }
};

const getOrderValueForDrop = (t: TaskMetadata) => {
    if (t.custom_fields && t.custom_fields['order'] !== undefined) {
        return Number(t.custom_fields['order']);
    }
    return -new Date(t.created_at).getTime();
};

const tasksByStatus = computed(() => {
    const sorted: Record<string, TaskMetadata[]> = { todo: [], in_progress: [], done: [] };
    activeCategoryTasks.value.forEach(t => {
        if (sorted[t.status]) {
            sorted[t.status].push(t);
        } else {
            sorted.todo.push(t);
        }
    });

    for (const key in sorted) {
        sorted[key].sort((a, b) => getOrderValueForDrop(a) - getOrderValueForDrop(b));
    }
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
    if (!task) return;
    
    const columnElement = (e.currentTarget as HTMLElement);
    const columnContent = columnElement.querySelector('.column-content');
    let insertAfterTaskIdx = -1;
    
    if (columnContent) {
        const cards = Array.from(columnContent.querySelectorAll('.task-card'));
        let filteredCardIndex = -1;
        for (let i = 0; i < cards.length; i++) {
            const card = cards[i] as HTMLElement;
            if (card.getAttribute('data-task-id') === taskId) continue;
            
            filteredCardIndex++;
            const rect = card.getBoundingClientRect();
            const cardMiddleY = rect.top + rect.height / 2;
            if (e.clientY > cardMiddleY) {
                insertAfterTaskIdx = filteredCardIndex;
            } else {
                break;
            }
        }
    }
    
    const tasksInCol = tasksByStatus.value[newStatus].filter(t => t.id !== taskId);
    let newOrder = 0;
    
    if (tasksInCol.length === 0) {
        newOrder = new Date().getTime();
    } else if (insertAfterTaskIdx === -1) {
        newOrder = getOrderValueForDrop(tasksInCol[0]) - 100000;
    } else if (insertAfterTaskIdx >= tasksInCol.length - 1) {
        newOrder = getOrderValueForDrop(tasksInCol[tasksInCol.length - 1]) + 100000;
    } else {
        const prevOrder = getOrderValueForDrop(tasksInCol[insertAfterTaskIdx]);
        const nextOrder = getOrderValueForDrop(tasksInCol[insertAfterTaskIdx + 1]);
        newOrder = (prevOrder + nextOrder) / 2;
    }
    
    const prevStatus = task.status;
    const prevOrderFromCustomFields = task.custom_fields?.['order'];
    // Avoid API call if no change in status and order position (virtually)
    if (prevStatus === newStatus && Number(prevOrderFromCustomFields) === newOrder) return;
    
    if (!task.custom_fields) task.custom_fields = {};
    task.custom_fields['order'] = newOrder;
    task.status = newStatus;
    
    try {
        await invoke('update_task', {
            path: task.path,
            metadata: {
                title: task.title,
                status: newStatus,
                is_transferred: task.is_transferred,
                priority: task.priority,
                start_date: task.start_date,
                due_date: task.due_date,
                comment: task.comment,
                source_link: task.source_link,
                tags: task.tags,
                ...task.custom_fields
            },
            content: task.content
        });
    } catch (err) {
        console.error("Drag update failed", err);
    }
};

const editingTask = ref<TaskMetadata | null>(null);
const editingTaskParams = ref({
    title: '',
    content: '',
    is_transferred: false,
    priority: '',
    start_date: '',
    due_date: '',
    comment: '',
    tags: '',
    checklist: [] as ChecklistItem[],
});
const customFields = ref<{k: string, v: string}[]>([]);

const openEditModal = (task: TaskMetadata) => {
    editingTask.value = task;
    editingTaskParams.value = {
        title: task.title,
        content: task.content,
        is_transferred: task.is_transferred || false,
        priority: task.priority || '',
        start_date: task.start_date,
        due_date: task.due_date,
        comment: task.comment,
        tags: task.tags.join(', '),
        checklist: JSON.parse(JSON.stringify(task.checklist || []))
    };
    customFields.value = Object.entries(task.custom_fields || {})
        .filter(([k, _]) => k.trim() !== 'order')
        .map(([k, v]) => ({ k, v: String(v) }));
};

const openCreateModal = () => {
    editingTask.value = {
        id: '',
        title: '',
        status: 'todo',
        is_transferred: false,
        priority: '',
        start_date: '',
        due_date: '',
        comment: '',
        source_link: '',
        tags: [],
        checklist: [],
        content: '',
        path: '',
        created_at: '',
        updated_at: '',
        custom_fields: {},
        isNew: true
    };
    editingTaskParams.value = {
        title: '',
        content: '',
        is_transferred: false,
        priority: '',
        start_date: '',
        due_date: '',
        comment: '',
        tags: '',
        checklist: []
    };
    customFields.value = [];
};

const addChecklistItem = () => {
    editingTaskParams.value.checklist.push({ content: '', completed: false });
};

const removeChecklistItem = (index: number) => {
    editingTaskParams.value.checklist.splice(index, 1);
};

const focusLastChecklistItem = () => {
    const inputs = document.querySelectorAll('.checklist-input');
    if (inputs.length > 0) {
        (inputs[inputs.length - 1] as HTMLInputElement).focus();
    }
};

const handleModalSave = async (payload: any) => {
    editingTaskParams.value = payload;
    if (editingTask.value) {
        editingTask.value.status = payload.status;
    }
    await saveTask();
    editingTask.value = null;
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
        
        if (editingTask.value.custom_fields && editingTask.value.custom_fields['order'] !== undefined) {
             updatedCustomFields['order'] = editingTask.value.custom_fields['order'] as string;
        }
        
        if (editingTask.value.isNew) {
            const newTask = await invoke<TaskMetadata>('create_task', {
                vaultPath: props.vaultPath,
                metadata: {
                    title: editingTaskParams.value.title || 'Untitled',
                    status: editingTask.value.status || 'todo',
                    is_transferred: editingTaskParams.value.is_transferred,
                    priority: editingTaskParams.value.priority,
                    start_date: editingTaskParams.value.start_date,
                    due_date: editingTaskParams.value.due_date,
                    comment: editingTaskParams.value.comment,
                    source_link: '',
                    tags: tagArray,
                    checklist: editingTaskParams.value.checklist,
                    ...updatedCustomFields
                },
                content: editingTaskParams.value.content
            });
            tasks.value.unshift(newTask);
        } else if (editingTask.value.path) {
            await invoke('update_task', {
                path: editingTask.value.path,
                metadata: {
                    title: editingTaskParams.value.title,
                    status: editingTask.value.status,
                    is_transferred: editingTaskParams.value.is_transferred,
                    priority: editingTaskParams.value.priority,
                    start_date: editingTaskParams.value.start_date,
                    due_date: editingTaskParams.value.due_date,
                    comment: editingTaskParams.value.comment,
                    source_link: editingTask.value.source_link,
                    tags: tagArray,
                    checklist: editingTaskParams.value.checklist,
                    ...updatedCustomFields
                },
                content: editingTaskParams.value.content
            });
            
            editingTask.value.title = editingTaskParams.value.title;
            editingTask.value.content = editingTaskParams.value.content;
            editingTask.value.is_transferred = editingTaskParams.value.is_transferred;
            editingTask.value.priority = editingTaskParams.value.priority;
            editingTask.value.start_date = editingTaskParams.value.start_date;
            editingTask.value.due_date = editingTaskParams.value.due_date;
            editingTask.value.comment = editingTaskParams.value.comment;
            editingTask.value.tags = tagArray;
            editingTask.value.checklist = editingTaskParams.value.checklist;
            editingTask.value.custom_fields = updatedCustomFields;
        }
        
        closeEditModal();
    } catch (e) {
        console.error("Failed to update/create task", e);
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

const toggleTaskStatus = async (task: TaskMetadata) => {
    const newStatus = task.status === 'done' ? 'todo' : 'done';
    
    try {
        await invoke('update_task', {
            path: task.path,
            metadata: {
                title: task.title,
                status: newStatus,
                is_transferred: task.is_transferred,
                priority: task.priority,
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

const deleteTask = async (task: TaskMetadata) => {
    const isConfirmed = await confirm('Xoá công việc này?', { title: 'Xoá Task', kind: 'warning' });
    if (!isConfirmed) return;
    
    try {
        await invoke('delete_task', { path: task.path });
        const idx = tasks.value.findIndex(t => t.id === task.id);
        if (idx !== -1) tasks.value.splice(idx, 1);
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
  <div class="h-full flex bg-[#fdfdfc] dark:bg-[#242424] w-full overflow-hidden">
      <!-- SIDEBAR -->
      <div class="w-64 border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-gray-50/50 dark:bg-[#1a1a1a]/50 flex flex-col pt-10 shrink-0 hidden md:flex">
          <div class="px-6 mb-6">
              <h2 class="text-xs font-bold uppercase tracking-wider text-gray-500">Navigation</h2>
          </div>
          <div class="flex flex-col px-3 space-y-1">
              <button @click="activeCategory = 'all'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'all' ? 'bg-white dark:bg-[#2c2c2c] text-black dark:text-white shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Inbox class="w-4 h-4 mr-3" />All Tasks</div>
                  <span class="text-xs bg-gray-200 dark:bg-[#333] px-1.5 py-0.5 rounded-full text-gray-600 dark:text-gray-400" v-if="categoryCounts.all">{{ categoryCounts.all }}</span>
              </button>
              <button @click="activeCategory = 'today'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'today' ? 'bg-white dark:bg-[#2c2c2c] text-blue-600 dark:text-blue-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Sun class="w-4 h-4 mr-3" />Today</div>
                  <span class="text-xs bg-blue-100 dark:bg-blue-900/30 px-1.5 py-0.5 rounded-full text-blue-600 dark:text-blue-400" v-if="categoryCounts.today">{{ categoryCounts.today }}</span>
              </button>
              <button @click="activeCategory = 'upcoming'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'upcoming' ? 'bg-white dark:bg-[#2c2c2c] text-red-600 dark:text-red-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Calendar class="w-4 h-4 mr-3" />Upcoming</div>
                  <span class="text-xs bg-red-100 dark:bg-red-900/30 px-1.5 py-0.5 rounded-full text-red-600 dark:text-red-400" v-if="categoryCounts.upcoming">{{ categoryCounts.upcoming }}</span>
              </button>
              <button @click="activeCategory = 'someday'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'someday' ? 'bg-white dark:bg-[#2c2c2c] text-yellow-600 dark:text-yellow-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Coffee class="w-4 h-4 mr-3" />Someday</div>
                  <span class="text-xs bg-yellow-100 dark:bg-yellow-900/30 px-1.5 py-0.5 rounded-full text-yellow-600 dark:text-yellow-400" v-if="categoryCounts.someday">{{ categoryCounts.someday }}</span>
              </button>
              <button @click="activeCategory = 'transferred'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'transferred' ? 'bg-white dark:bg-[#2c2c2c] text-slate-600 dark:text-slate-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Send class="w-4 h-4 mr-3" />Transferred</div>
                  <span class="text-xs bg-slate-200 dark:bg-slate-700 px-1.5 py-0.5 rounded-full text-slate-600 dark:text-slate-400" v-if="categoryCounts.transferred">{{ categoryCounts.transferred }}</span>
              </button>
          </div>
      </div>

      <!-- MAIN CONTENT -->
      <div class="flex-1 flex flex-col h-full overflow-hidden">
          <!-- Header -->
          <div class="px-8 pt-10 pb-4 shrink-0 border-b border-transparent">
              <div class="flex items-center justify-between mb-6">
                  <h1 class="text-3xl font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] tracking-tight capitalize">
                      {{ activeCategory === 'all' ? 'All Tasks' : activeCategory }}
                  </h1>
                  
                  <div class="flex items-center gap-3">
                      <!-- New Task Button -->
                      <button 
                          @click="openCreateModal"
                          class="flex items-center px-3 py-1.5 bg-blue-500 hover:bg-blue-600 text-white rounded-lg shadow-[0_2px_10px_rgba(59,130,246,0.3)] hover:shadow-[0_4px_14px_rgba(59,130,246,0.4)] transition-all cursor-pointer text-sm font-medium"
                      >
                          <Plus class="w-4 h-4 mr-1.5"/>
                          New
                      </button>

                      <div class="flex bg-gray-100 dark:bg-[#1a1a1a] p-1 rounded-xl">
                          <button @click="viewMode = 'list'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'list' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" title="List View">
                              <List class="w-4 h-4"/>
                          </button>
                          <button @click="viewMode = 'board'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'board' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" title="Board View">
                              <Trello class="w-4 h-4"/>
                          </button>
                          <button @click="viewMode = 'table'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'table' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" title="Table View">
                              <Table2 class="w-4 h-4"/>
                          </button>
                      </div>
                  </div>
              </div>

              <!-- Filter Bar (Search only) -->
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
              </div>
          </div>

      <!-- Main Content -->
      <div class="flex-1 overflow-y-auto px-8 pb-16">
          <div v-if="activeCategoryTasks.length === 0" class="flex flex-col items-center justify-center h-full opacity-40">
              <CheckCircle2 class="w-16 h-16 mb-4"/>
              <p>You're all caught up!</p>
          </div>
          
          <div v-else class="h-full">
              <!-- LIST VIEW -->
              <div v-if="viewMode === 'list'" class="space-y-2 mt-4 max-w-4xl mx-auto">
                  <div v-for="task in activeCategoryTasks" :key="task.id" 
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
                              
                              <span v-if="task.priority" class="text-[10px] px-2 py-0.5 rounded-full font-bold tracking-wider" :class="getPriorityClass(task.priority)">
                                  {{ task.priority }}
                              </span>
                              
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
                          <button @click.stop="deleteTask(task)" class="p-1.5 text-gray-400 hover:text-red-500 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer">
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
                      <div class="flex-1 overflow-y-auto space-y-3 pb-4 column-content">
                          <div v-for="task in tasksByStatus[col.id]" :key="task.id"
                               draggable="true"
                               @dragstart="onDragStart($event, task)"
                               @click="openEditModal(task)"
                               :data-task-id="task.id"
                               class="task-card bg-white dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] hover:shadow-md transition-shadow cursor-grab active:cursor-grabbing group relative"
                          >
                             <p class="text-sm font-medium text-[#1c1c1e] dark:text-[#f4f4f5] leading-snug mb-3">{{ task.title }}</p>
                             <div class="flex items-center justify-between mt-auto pt-2 border-t border-gray-50 dark:border-[#2c2c2c]">
                                 <div class="flex gap-2 items-center flex-wrap">
                                     <span v-if="task.priority" class="text-[10px] px-1.5 py-0.5 rounded font-bold" :class="getPriorityClass(task.priority)">
                                         {{ task.priority }}
                                     </span>
                                     <span v-if="task.start_date || task.due_date" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded flex items-center">
                                         <CalendarDays class="w-3 h-3 mr-1" /> {{ task.start_date ? task.start_date.substring(5) : '--' }} - {{ task.due_date ? task.due_date.substring(5) : '--' }}
                                     </span>
                                     <div v-if="task.tags.length" class="flex flex-wrap gap-1">
                                         <span v-for="tag in task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">
                                             {{ tag }}
                                         </span>
                                     </div>
                                 </div>
                                 <button @click.stop="deleteTask(task)" class="text-gray-300 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer">
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
                         <tr v-for="task in activeCategoryTasks" :key="task.id" class="hover:bg-gray-50 dark:hover:bg-[#252525] group cursor-pointer" @click="openEditModal(task)">
                             <td class="px-6 py-3">
                                 <button @click.stop="toggleTaskStatus(task)" class="transition-colors cursor-pointer block mt-1">
                                      <CheckCircle2 v-if="task.status === 'done'" class="w-5 h-5 text-green-500" />
                                      <Circle v-else class="w-5 h-5 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
                                  </button>
                             </td>
                             <td class="px-6 py-3 font-medium text-[#1c1c1e] dark:text-[#f4f4f5]" :class="task.status === 'done' ? 'line-through text-gray-400' : ''">
                                 <span v-if="task.priority" class="mr-2 text-[10px] px-1.5 py-0.5 rounded font-bold" :class="getPriorityClass(task.priority)">{{ task.priority }}</span>
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
                                 <button @click.stop="deleteTask(task)" class="p-1 text-gray-400 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer">
                                     <Trash2 class="w-4 h-4" />
                                 </button>
                             </td>
                         </tr>
                     </tbody>
                 </table>
              </div>
          </div>
      </div>
  </div>

  <!-- Edit Task Modal -->
  <TaskEditModal 
      v-if="editingTask" 
      :task="editingTaskParams" 
      @save="handleModalSave" 
      @close="editingTask = null" 
  />
  </div>
</template>
