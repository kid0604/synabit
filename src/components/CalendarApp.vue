<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ChevronLeft, ChevronRight, Plus, X, Calendar as CalendarIcon, Clock, MapPin, Hash, CheckSquare, Trash2, Edit2, AlertCircle } from 'lucide-vue-next';

const props = defineProps<{
    vaultPath: string
}>();

// --- Data Models ---
interface TaskMetadata {
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
    custom_fields: any;
}

interface EventMetadata {
    id: string;
    title: string;
    event_date: string; // YYYY-MM-DD
    event_time: string;
    location: string;
    tags: string[];
    content: string;
    path: string;
    created_at: string;
}

// --- State ---
const currentDate = ref(new Date());
const selectedDate = ref<Date>(new Date());
const allTasks = ref<TaskMetadata[]>([]);
const allEvents = ref<EventMetadata[]>([]);
const showRightPanel = ref(false);
const showEventForm = ref(false);

const eventForm = ref({
    isEdit: false,
    id: '',
    path: '',
    title: '',
    event_date: '',
    event_time: '',
    location: '',
    description: '',
    tagsStr: ''
});

// --- Methods ---
const loadData = async () => {
    if (!props.vaultPath) return;
    try {
        allTasks.value = await invoke('scan_tasks', { vaultPath: props.vaultPath });
    } catch(e) { console.error("Error loading tasks:", e); }
    try {
        allEvents.value = await invoke('scan_events', { vaultPath: props.vaultPath });
    } catch(e) { console.error("Error loading events:", e); }
};

onMounted(() => {
    loadData();
});

watch(() => props.vaultPath, () => {
    loadData();
});

// --- Calendar Logic ---
const monthNames = ["January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December"
];
const dayNamesShort = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

const currentMonthName = computed(() => monthNames[currentDate.value.getMonth()]);
const currentYear = computed(() => currentDate.value.getFullYear());

const prevMonth = () => {
    currentDate.value = new Date(currentDate.value.getFullYear(), currentDate.value.getMonth() - 1, 1);
};

const nextMonth = () => {
    currentDate.value = new Date(currentDate.value.getFullYear(), currentDate.value.getMonth() + 1, 1);
};

const goToToday = () => {
    currentDate.value = new Date();
    selectedDate.value = new Date();
    showRightPanel.value = false;
};

// Generates the 42 cells (6 rows of 7 days) for the calendar grid
const calendarDays = computed(() => {
    const year = currentDate.value.getFullYear();
    const month = currentDate.value.getMonth();
    
    const firstDayOfMonth = new Date(year, month, 1);
    const lastDayOfMonth = new Date(year, month + 1, 0);
    
    const startingDayOfWeek = firstDayOfMonth.getDay(); // 0 for Sun
    
    // Previous month days to pad
    const prevMonthDays = new Date(year, month, 0).getDate();
    
    const days = [];
    
    // Add padded days from prev month
    for (let i = startingDayOfWeek - 1; i >= 0; i--) {
        const d = new Date(year, month - 1, prevMonthDays - i);
        days.push({ date: d, inCurrentMonth: false });
    }
    
    // Current month days
    for (let d = 1; d <= lastDayOfMonth.getDate(); d++) {
        days.push({ date: new Date(year, month, d), inCurrentMonth: true });
    }
    
    // Next month padding
    let nextMonthDay = 1;
    while (days.length % 7 !== 0 || days.length < 42) {
        days.push({ date: new Date(year, month + 1, nextMonthDay++), inCurrentMonth: false });
    }
    
    return days;
});

const formatDateString = (date: Date) => {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    return `${y}-${m}-${d}`;
};

const isSameDay = (d1: Date, d2: Date) => {
    return d1.getFullYear() === d2.getFullYear() &&
           d1.getMonth() === d2.getMonth() &&
           d1.getDate() === d2.getDate();
};

const getTasksForDate = (dateStr: string) => {
    return allTasks.value.filter(t => t.due_date === dateStr || t.start_date === dateStr);
};

const getEventsForDate = (dateStr: string) => {
    return allEvents.value.filter(e => e.event_date === dateStr);
};

const hasItemsOnDate = (date: Date) => {
    const ds = formatDateString(date);
    return getTasksForDate(ds).length > 0 || getEventsForDate(ds).length > 0;
};

const clickDay = (day: { date: Date, inCurrentMonth: boolean }) => {
    selectedDate.value = day.date;
    if (!day.inCurrentMonth) {
        currentDate.value = new Date(day.date.getFullYear(), day.date.getMonth(), 1);
    }
    showRightPanel.value = true;
};

// --- Panel Computed ---
const selectedDateFormattedStr = computed(() => formatDateString(selectedDate.value));
const selectedDateDisplay = computed(() => {
    return selectedDate.value.toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' });
});

const selectedTasks = computed(() => getTasksForDate(selectedDateFormattedStr.value));
const selectedEvents = computed(() => getEventsForDate(selectedDateFormattedStr.value).sort((a,b) => a.event_time.localeCompare(b.event_time)));

// --- Event Functions ---
const openAddEventModal = () => {
    eventForm.value = {
        isEdit: false,
        id: '',
        path: '',
        title: '',
        event_date: selectedDateFormattedStr.value,
        event_time: '12:00',
        location: '',
        description: '',
        tagsStr: ''
    };
    showEventForm.value = true;
};

const openEditEventModal = (ev: EventMetadata) => {
    eventForm.value = {
        isEdit: true,
        id: ev.id,
        path: ev.path,
        title: ev.title,
        event_date: ev.event_date,
        event_time: ev.event_time,
        location: ev.location,
        description: ev.content,
        tagsStr: ev.tags.join(', ')
    };
    showEventForm.value = true;
};

const closeEventForm = () => {
    showEventForm.value = false;
};

const submitEvent = async () => {
    if (!eventForm.value.title || !eventForm.value.event_date) return;
    
    // format tags
    let finalTags: string[] = [];
    if (eventForm.value.tagsStr.trim()) {
        finalTags = eventForm.value.tagsStr.split(',').map(s => s.trim().replace(/^#/, '')).filter(s => s);
    }
    
    const meta = {
        title: eventForm.value.title,
        event_date: eventForm.value.event_date,
        event_time: eventForm.value.event_time,
        location: eventForm.value.location,
        tags: finalTags
    };
    
    try {
        if (eventForm.value.isEdit) {
            await invoke('update_event', { path: eventForm.value.path, metadata: meta, content: eventForm.value.description });
        } else {
            await invoke('create_event', { vaultPath: props.vaultPath, metadata: meta, content: eventForm.value.description });
        }
        closeEventForm();
        await loadData();
    } catch(e) {
        console.error("Failed to save event:", e);
    }
};

const deleteEvent = async (ev: EventMetadata) => {
    if (confirm(`Delete event '${ev.title}'?`)) {
        try {
            await invoke('delete_event', { path: ev.path });
            await loadData();
        } catch(e) {
            console.error("Failed to delete event:", e);
        }
    }
};

</script>

<template>
  <div class="h-full flex relative text-[#1c1c1e] dark:text-[#f4f4f5] bg-[#fdfdfc] dark:bg-[#242424]">
     
     <!-- MAIN CALENDAR CONTAINER -->
     <div class="flex-1 flex flex-col h-full owerflow-hidden px-6 py-4 transition-all duration-300" :class="{ 'pr-96': showRightPanel }">
         
         <!-- Header -->
         <header class="flex items-center justify-between mb-6 h-12 flex-shrink-0" data-tauri-drag-region>
             <div class="flex items-center gap-4">
                 <CalendarIcon class="w-6 h-6 text-purple-500" />
                 <h1 class="text-2xl font-bold tracking-tight">{{ currentMonthName }} <span class="font-normal opacity-50">{{ currentYear }}</span></h1>
             </div>
             
             <div class="flex items-center gap-2">
                 <button @click="goToToday" class="px-3 py-1.5 text-xs font-semibold bg-gray-100 hover:bg-gray-200 dark:bg-[#2c2c2c] dark:hover:bg-[#3a3a3a] rounded-lg transition-colors border border-transparent dark:border-gray-700">
                     Today
                 </button>
                 <div class="flex bg-gray-100 dark:bg-[#2c2c2c] rounded-lg p-0.5 border border-transparent dark:border-gray-700">
                     <button @click="prevMonth" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronLeft class="w-4 h-4" /></button>
                     <button @click="nextMonth" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronRight class="w-4 h-4" /></button>
                 </div>
             </div>
         </header>
         
         <!-- Calendar Grid -->
         <div class="flex-1 min-h-0 flex flex-col pt-2 select-none">
             <!-- Days Header -->
             <div class="grid grid-cols-7 mb-2 flex-shrink-0 border-b border-[#e6e6e6] dark:border-[#333] pb-2">
                 <div v-for="day in dayNamesShort" :key="day" class="text-center text-xs font-bold uppercase tracking-wider text-[#8b8b8b] dark:text-[#71717a]">
                     {{ day }}
                 </div>
             </div>
             
             <!-- Days Grid -->
             <div class="flex-1 grid grid-cols-7 grid-rows-6 gap-2">
                 <div v-for="(dayObj, idx) in calendarDays" :key="idx" 
                      @click="clickDay(dayObj)"
                      class="relative flex flex-col rounded-xl border border-[#ececeb] dark:border-[#2f2f2f] cursor-pointer transition-all duration-200 overflow-hidden group hover:border-[#d4d4d8] dark:hover:border-[#4f4f4f] hover:shadow-sm"
                      :class="[
                          dayObj.inCurrentMonth ? 'bg-white dark:bg-[#262626]' : 'bg-gray-50/50 dark:bg-[#1f1f1f]',
                          isSameDay(dayObj.date, selectedDate) ? 'ring-2 ring-purple-500 border-transparent dark:border-transparent' : '',
                          isSameDay(dayObj.date, new Date()) ? 'bg-gradient-to-br from-purple-50/50 to-transparent dark:from-purple-900/10' : ''
                      ]"
                 >
                     <div class="w-full flex justify-between items-start p-2 pointer-events-none">
                         <span class="text-sm font-medium w-6 h-6 flex items-center justify-center rounded-full"
                               :class="[
                                   !dayObj.inCurrentMonth ? 'text-gray-400 dark:text-gray-600' : 'text-[#1c1c1e] dark:text-[#f4f4f5]',
                                   isSameDay(dayObj.date, new Date()) ? 'bg-purple-600 text-white dark:text-white' : ''
                               ]"
                         >
                             {{ dayObj.date.getDate() }}
                         </span>
                     </div>
                     
                     <div class="flex-1 px-2 pb-2 overflow-y-auto w-full no-scrollbar space-y-1">
                         <!-- Event & Task preview boxes -->
                         <div v-for="ev in getEventsForDate(formatDateString(dayObj.date))" :key="'evt-'+ev.id" class="w-full text-left truncate px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-200">
                             {{ ev.event_time }} {{ ev.title }}
                         </div>
                         <div v-for="tk in getTasksForDate(formatDateString(dayObj.date))" :key="'tsk-'+tk.id" class="w-full text-left truncate px-1.5 py-0.5 rounded text-[10px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1">
                             <CheckSquare class="w-2.5 h-2.5" :class="tk.status === 'done' ? 'text-green-500' : ''" /> {{ tk.title }}
                         </div>
                     </div>
                 </div>
             </div>
         </div>
     </div>
     
     <!-- RIGHT PANEL: DAY DETAILS -->
     <div v-show="showRightPanel" class="w-96 absolute right-0 top-0 bottom-0 border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-white dark:bg-[#1a1a1a] shadow-2xl flex flex-col transition-transform z-10">
         <div class="h-16 flex items-center justify-between px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c] flex-shrink-0" data-tauri-drag-region>
             <h2 class="font-bold text-lg text-purple-600 dark:text-purple-400 select-none">{{ selectedDateDisplay }}</h2>
             <button @click="showRightPanel = false" class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-500 transition-colors">
                 <X class="w-5 h-5" />
             </button>
         </div>
         
         <div class="flex-1 overflow-y-auto p-4 space-y-6">
             <!-- Add Event Button -->
             <button @click="openAddEventModal" class="w-full py-3 border border-dashed border-gray-300 dark:border-gray-700 rounded-xl flex items-center justify-center gap-2 text-gray-500 hover:bg-gray-50 dark:hover:bg-[#2c2c2c] hover:text-black dark:hover:text-white transition-all cursor-pointer">
                 <Plus class="w-4 h-4" /> <span class="text-sm font-semibold">New Event</span>
             </button>
             
             <!-- Events Section -->
             <div>
                 <h3 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-3 px-2 flex items-center gap-2">
                     <CalendarIcon class="w-3.5 h-3.5" /> Events
                 </h3>
                 <div v-if="selectedEvents.length === 0" class="text-sm text-center text-gray-500 py-4 italic bg-gray-50 rounded-xl dark:bg-[#1e1e1e]">No events scheduled.</div>
                 <div class="space-y-2">
                     <div v-for="ev in selectedEvents" :key="ev.id" class="p-3 bg-white dark:bg-[#232323] border border-[#f0f0f0] dark:border-[#333] rounded-xl shadow-sm group">
                         <div class="flex justify-between items-start mb-1">
                             <h4 class="font-bold text-base text-[#1c1c1e] dark:text-[#f4f4f5] line-clamp-1">{{ ev.title }}</h4>
                             <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                <button @click="openEditEventModal(ev)" class="p-1 hover:bg-gray-100 dark:hover:bg-[#333] rounded text-gray-500"><Edit2 class="w-3 h-3"/></button>
                                <button @click="deleteEvent(ev)" class="p-1 hover:bg-red-100 dark:hover:bg-red-900/30 rounded text-red-500"><Trash2 class="w-3 h-3"/></button>
                             </div>
                         </div>
                         <div class="flex items-center flex-wrap gap-x-3 gap-y-1 text-xs text-gray-500 dark:text-gray-400 mb-2">
                             <div class="flex items-center gap-1" v-if="ev.event_time"><Clock class="w-3 h-3" /> {{ ev.event_time }}</div>
                             <div class="flex items-center gap-1" v-if="ev.location"><MapPin class="w-3 h-3" /> {{ ev.location }}</div>
                         </div>
                         <p v-if="ev.content" class="text-sm text-gray-600 dark:text-gray-300 line-clamp-3 mb-2">{{ ev.content }}</p>
                         <div class="flex flex-wrap gap-1" v-if="ev.tags.length">
                             <span v-for="tag in ev.tags" :key="tag" class="text-[10px] flex items-center bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 rounded text-gray-600 dark:text-gray-300">
                                 <Hash class="w-2.5 h-2.5 opacity-50"/>{{ tag }}
                             </span>
                         </div>
                     </div>
                 </div>
             </div>
             
             <!-- Tasks Section -->
             <div>
                 <h3 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-3 px-2 flex items-center gap-2">
                     <CheckSquare class="w-3.5 h-3.5" /> Due Tasks
                 </h3>
                 <div v-if="selectedTasks.length === 0" class="text-sm text-center text-gray-500 py-4 italic bg-gray-50 rounded-xl dark:bg-[#1e1e1e]">No tasks due today.</div>
                 <div class="space-y-2">
                     <div v-for="tk in selectedTasks" :key="tk.id" class="p-3 bg-white dark:bg-[#232323] border border-[#f0f0f0] dark:border-[#333] rounded-xl shadow-sm flex gap-3">
                         <div class="pt-1 select-none pointer-events-none">
                             <div class="w-4 h-4 rounded border-2 flex items-center justify-center transition-colors border-gray-300 dark:border-gray-500"
                                  :class="{'bg-purple-500 border-purple-500 dark:border-purple-500': tk.status === 'done'}">
                             </div>
                         </div>
                         <div class="flex-1">
                             <h4 class="text-sm font-semibold" :class="tk.status === 'done' ? 'line-through text-gray-400' : 'text-[#1c1c1e] dark:text-[#f4f4f5]'">{{ tk.title }}</h4>
                             <p v-if="tk.comment" class="text-xs text-gray-500 mt-1 line-clamp-1">{{ tk.comment }}</p>
                         </div>
                     </div>
                 </div>
             </div>
         </div>
     </div>

     <!-- Event Modal Overlay -->
     <div v-if="showEventForm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="closeEventForm">
        <div class="bg-white dark:bg-[#1e1e1e] w-full max-w-md rounded-2xl shadow-2xl overflow-hidden border border-[#e6e6e6] dark:border-[#333] flex flex-col">
            <div class="flex items-center justify-between px-6 py-4 border-b border-[#e6e6e6] dark:border-[#333] select-none text-black dark:text-white">
                <h3 class="font-bold text-lg">{{ eventForm.isEdit ? 'Edit Event' : 'New Event' }}</h3>
                <button @click="closeEventForm" class="text-gray-400 hover:text-red-500"><X class="w-5 h-5"/></button>
            </div>
            <div class="p-6 space-y-4 overflow-y-auto">
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Event Title *</label>
                   <input v-model="eventForm.title" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="E.g., Team Meeting, John's Birthday">
                </div>
                <div class="grid grid-cols-2 gap-4">
                    <div>
                        <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Date</label>
                        <input v-model="eventForm.event_date" type="date" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                    </div>
                    <div>
                        <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Time</label>
                        <input v-model="eventForm.event_time" type="time" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                    </div>
                </div>
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Location</label>
                   <input v-model="eventForm.location" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="Zoom link, Office, etc.">
                </div>
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Description</label>
                   <textarea v-model="eventForm.description" rows="3" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="Event details..."></textarea>
                </div>
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Tags</label>
                   <input v-model="eventForm.tagsStr" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="meeting, urgent (comma separated)">
                </div>
            </div>
            <div class="px-6 py-4 bg-gray-50 dark:bg-[#1a1a1a] border-t border-[#e6e6e6] dark:border-[#333] flex justify-end gap-3 text-sm font-semibold select-none">
                <button @click="closeEventForm" class="px-4 py-2 rounded-lg text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">Cancel</button>
                <button @click="submitEvent" class="px-4 py-2 rounded-lg bg-black text-white dark:bg-white dark:text-black hover:bg-purple-600 dark:hover:bg-purple-400 transition-colors" :disabled="!eventForm.title">Save Event</button>
            </div>
        </div>
     </div>
  </div>
</template>
