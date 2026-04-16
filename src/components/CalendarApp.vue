<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as emitTauri } from '@tauri-apps/api/event';
import { ChevronLeft, ChevronRight, Plus, X, Calendar as CalendarIcon, Clock, MapPin, Hash, CheckSquare, Trash2, Edit2 } from 'lucide-vue-next';

const props = defineProps<{ vaultPath: string }>();

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
    event_time: string; // HH:mm
    location: string;
    tags: string[];
    content: string;
    path: string;
    created_at: string;
}

type ViewMode = 'day' | 'week' | 'month' | 'year';

// --- State ---
const viewMode = ref<ViewMode>('month');
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

onMounted(() => { loadData(); });
watch(() => props.vaultPath, () => { loadData(); });

// --- Helpers ---
const monthNames = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const monthNamesShort = monthNames.map(m => m.substring(0, 3));
const dayNamesShort = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const hours = Array.from({length: 24}, (_, i) => i); // 0 to 23

const formatDateString = (date: Date) => {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    return `${y}-${m}-${d}`;
};

const isSameDay = (d1: Date, d2: Date) => {
    return d1.getFullYear() === d2.getFullYear() && d1.getMonth() === d2.getMonth() && d1.getDate() === d2.getDate();
};

const getTasksForDate = (dateStr: string) => allTasks.value.filter(t => t.due_date === dateStr || t.start_date === dateStr);
const getEventsForDate = (dateStr: string) => allEvents.value.filter(e => e.event_date === dateStr);

const getEventsForDateAndHour = (dateStr: string, hour: number) => {
    return getEventsForDate(dateStr).filter(e => {
        if (!e.event_time) return false;
        const eHour = parseInt(e.event_time.split(':')[0]);
        return eHour === hour;
    });
};

const hasItemsOnDate = (date: Date) => {
    const ds = formatDateString(date);
    return getTasksForDate(ds).length > 0 || getEventsForDate(ds).length > 0;
};

// --- Navigation ---
const headerDisplayString = computed(() => {
    const year = currentDate.value.getFullYear();
    if (viewMode.value === 'year') return `${year}`;
    if (viewMode.value === 'month') return `${monthNames[currentDate.value.getMonth()]} ${year}`;
    if (viewMode.value === 'day') return `${currentDate.value.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric'})}, ${year}`;
    if (viewMode.value === 'week') {
        const week = currentWeekDays.value;
        const first = week[0].date;
        const last = week[6].date;
        if (first.getMonth() === last.getMonth()) {
            return `${monthNames[first.getMonth()]} ${year}`;
        } else if (first.getFullYear() === last.getFullYear()) {
            return `${monthNamesShort[first.getMonth()]} - ${monthNamesShort[last.getMonth()]} ${year}`;
        } else {
            return `${monthNamesShort[first.getMonth()]} ${first.getFullYear()} - ${monthNamesShort[last.getMonth()]} ${last.getFullYear()}`;
        }
    }
    return '';
});

const navigatePrev = () => {
    const d = new Date(currentDate.value);
    if (viewMode.value === 'month') d.setMonth(d.getMonth() - 1);
    else if (viewMode.value === 'day') d.setDate(d.getDate() - 1);
    else if (viewMode.value === 'week') d.setDate(d.getDate() - 7);
    else if (viewMode.value === 'year') d.setFullYear(d.getFullYear() - 1);
    currentDate.value = d;
};

const navigateNext = () => {
    const d = new Date(currentDate.value);
    if (viewMode.value === 'month') d.setMonth(d.getMonth() + 1);
    else if (viewMode.value === 'day') d.setDate(d.getDate() + 1);
    else if (viewMode.value === 'week') d.setDate(d.getDate() + 7);
    else if (viewMode.value === 'year') d.setFullYear(d.getFullYear() + 1);
    currentDate.value = d;
};

const goToToday = () => {
    currentDate.value = new Date();
    selectedDate.value = new Date();
    if (viewMode.value === 'year') viewMode.value = 'month'; // Jump to month mode if today clicked from year
    showRightPanel.value = false;
};

// --- Computed Modes ---

// 1. Month Mode
const calendarDays = computed(() => {
    const year = currentDate.value.getFullYear();
    const month = currentDate.value.getMonth();
    const firstDay = new Date(year, month, 1);
    const startDayOfWeek = firstDay.getDay();
    const prevMonthDays = new Date(year, month, 0).getDate();
    const lastDayOfMonth = new Date(year, month + 1, 0).getDate();
    
    const days = [];
    for (let i = startDayOfWeek - 1; i >= 0; i--) {
        days.push({ date: new Date(year, month - 1, prevMonthDays - i), inMonth: false });
    }
    for (let d = 1; d <= lastDayOfMonth; d++) {
        days.push({ date: new Date(year, month, d), inMonth: true });
    }
    let nextI = 1;
    while (days.length % 7 !== 0 || days.length < 42) {
        days.push({ date: new Date(year, month + 1, nextI++), inMonth: false });
    }
    return days;
});

// 2. Week Mode
const currentWeekDays = computed(() => {
    const d = new Date(currentDate.value);
    const day = d.getDay();
    const diff = d.getDate() - day; // Sunday is 0
    const startOfWeek = new Date(d.setDate(diff));
    const week = [];
    for (let i = 0; i < 7; i++) {
        const cur = new Date(startOfWeek);
        cur.setDate(startOfWeek.getDate() + i);
        week.push({ date: cur, dateStr: formatDateString(cur) });
    }
    return week;
});

// 3. Year Mode
const yearMonths = computed(() => {
    const year = currentDate.value.getFullYear();
    return Array.from({length: 12}, (_, i) => { // i is month index (0-11)
        const daysInMonth = new Date(year, i + 1, 0).getDate();
        const startDayOfWeek = new Date(year, i, 1).getDay();
        const days = [];
        // empty paddings
        for (let p=0; p<startDayOfWeek; p++) days.push(null);
        // real days
        for (let d=1; d<=daysInMonth; d++) {
            const dt = new Date(year, i, d);
            days.push({
                date: dt,
                hasItems: hasItemsOnDate(dt),
                isToday: isSameDay(dt, new Date())
            });
        }
        return { monthIndex: i, name: monthNames[i], days };
    });
});

const clickDay = (dateObj: Date) => {
    selectedDate.value = dateObj;
    // Auto-update currentDate to follow the selection into views
    currentDate.value = new Date(dateObj);
    showRightPanel.value = true;
};

const clickYearDay = (dt: Date) => {
    selectedDate.value = dt;
    currentDate.value = new Date(dt);
    viewMode.value = 'day';
    showRightPanel.value = true;
};

// --- Panel Computed ---
const selectedDateFormattedStr = computed(() => formatDateString(selectedDate.value));
const selectedDateDisplay = computed(() => selectedDate.value.toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' }));
const selectedTasks = computed(() => getTasksForDate(selectedDateFormattedStr.value));
const selectedEvents = computed(() => getEventsForDate(selectedDateFormattedStr.value).sort((a,b) => a.event_time.localeCompare(b.event_time)));

// --- Event Functions ---
const openAddEventModal = (defaultDate?: Date) => {
    const targetDateStr = defaultDate ? formatDateString(defaultDate) : selectedDateFormattedStr.value;
    eventForm.value = {
        isEdit: false, id: '', path: '', title: '',
        event_date: targetDateStr, event_time: '12:00',
        location: '', description: '', tagsStr: ''
    };
    showEventForm.value = true;
};

const openEditEventModal = (ev: EventMetadata) => {
    eventForm.value = {
        isEdit: true, id: ev.id, path: ev.path, title: ev.title,
        event_date: ev.event_date, event_time: ev.event_time, location: ev.location,
        description: ev.content, tagsStr: ev.tags.join(', ')
    };
    showEventForm.value = true;
};

const closeEventForm = () => { showEventForm.value = false; };

const submitEvent = async () => {
    if (!eventForm.value.title || !eventForm.value.event_date) return;
    let finalTags: string[] = [];
    if (eventForm.value.tagsStr.trim()) {
        finalTags = eventForm.value.tagsStr.split(',').map(s => s.trim().replace(/^#/, '')).filter(s => s);
    }
    const meta = {
        title: eventForm.value.title, event_date: eventForm.value.event_date,
        event_time: eventForm.value.event_time, location: eventForm.value.location, tags: finalTags
    };
    try {
        if (eventForm.value.isEdit) {
            await invoke('update_event', { path: eventForm.value.path, metadata: meta, content: eventForm.value.description });
        } else {
            await invoke('create_event', { vaultPath: props.vaultPath, metadata: meta, content: eventForm.value.description });
        }
        closeEventForm();
        await loadData();
    } catch(e) { console.error("Failed to save event:", e); }
};

import { confirm } from '@tauri-apps/plugin-dialog';

const deleteEvent = async (ev: EventMetadata) => {
    const isConfirmed = await confirm(`Delete event '${ev.title}'?`, { title: 'Delete Event', kind: 'warning' });
    if (isConfirmed) {
        try {
            await invoke('delete_event', { path: ev.path });
            await loadData();
        } catch(e) { console.error("Failed to delete event:", e); }
    }
};
</script>

<template>
  <div class="h-full flex relative text-[#1c1c1e] dark:text-[#f4f4f5] bg-[#fdfdfc] dark:bg-[#242424]">
     
     <div class="flex-1 flex flex-col h-full overflow-hidden px-6 py-4 transition-all duration-300" :class="{ 'pr-96': showRightPanel }">
         <!-- Header -->
         <header class="flex items-center justify-between mb-6 h-12 flex-shrink-0" data-tauri-drag-region>
             <div class="flex items-center gap-4">
                 <CalendarIcon class="w-6 h-6 text-purple-500" />
                 <h1 class="text-2xl font-bold tracking-tight select-none">
                     {{ headerDisplayString }}
                 </h1>
             </div>
             
             <div class="flex items-center gap-4 select-none">
                 <!-- View Switcher -->
                 <div class="flex bg-gray-100 dark:bg-[#1f1f1f] p-1 rounded-xl border border-gray-200 dark:border-[#333]">
                    <button v-for="v in (['day','week','month','year'] as ViewMode[])" :key="v"
                            @click="viewMode = v"
                            class="px-4 py-1.5 text-xs font-semibold rounded-lg capitalize transition-all"
                            :class="viewMode === v ? 'bg-white shadow-[0_1px_3px_rgba(0,0,0,0.1)] text-purple-600 dark:bg-[#333] dark:text-purple-400' : 'text-gray-500 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white'">
                        {{ v }}
                    </button>
                 </div>

                 <!-- Nav Controls -->
                 <div class="flex items-center gap-2">
                     <button @click="goToToday" class="px-3 py-1.5 text-xs font-semibold bg-gray-100 hover:bg-gray-200 dark:bg-[#2c2c2c] dark:hover:bg-[#3a3a3a] rounded-lg transition-colors border border-transparent dark:border-gray-700">
                         Today
                     </button>
                     <div class="flex bg-gray-100 dark:bg-[#2c2c2c] rounded-lg p-0.5 border border-transparent dark:border-gray-700">
                         <button @click="navigatePrev" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronLeft class="w-4 h-4" /></button>
                         <button @click="navigateNext" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronRight class="w-4 h-4" /></button>
                     </div>
                 </div>
             </div>
         </header>
         
         <div class="flex-1 min-h-0 relative">
             <!-- MONTH VIEW -->
             <div v-show="viewMode === 'month'" class="h-full flex flex-col select-none">
                 <div class="grid grid-cols-7 mb-2 flex-shrink-0 border-b border-[#e6e6e6] dark:border-[#333] pb-2">
                     <div v-for="day in dayNamesShort" :key="day" class="text-center text-xs font-bold uppercase tracking-wider text-[#8b8b8b] dark:text-[#71717a]">
                         {{ day }}
                     </div>
                 </div>
                 <div class="flex-1 grid grid-cols-7 grid-rows-6 gap-2">
                     <div v-for="(dayObj, idx) in calendarDays" :key="idx" 
                          @click="clickDay(dayObj.date)"
                          class="relative flex flex-col rounded-xl border border-[#ececeb] dark:border-[#2f2f2f] cursor-pointer transition-all duration-200 overflow-hidden group hover:border-[#d4d4d8] dark:hover:border-[#4f4f4f] hover:shadow-sm"
                          :class="[
                              dayObj.inMonth ? 'bg-white dark:bg-[#262626]' : 'bg-gray-50/50 dark:bg-[#1f1f1f]',
                              isSameDay(dayObj.date, selectedDate) ? 'ring-2 ring-purple-500 border-transparent dark:border-transparent' : '',
                              isSameDay(dayObj.date, new Date()) ? 'bg-gradient-to-br from-purple-50/50 to-transparent dark:from-purple-900/10' : ''
                          ]"
                     >
                         <div class="w-full flex justify-between items-start p-2 pointer-events-none">
                             <span class="text-sm font-medium w-6 h-6 flex items-center justify-center rounded-full"
                                   :class="[
                                       !dayObj.inMonth ? 'text-gray-400 dark:text-gray-600' : 'text-[#1c1c1e] dark:text-[#f4f4f5]',
                                       isSameDay(dayObj.date, new Date()) ? 'bg-purple-600 text-white dark:text-white' : ''
                                   ]"
                             >
                                 {{ dayObj.date.getDate() }}
                             </span>
                         </div>
                         <div class="flex-1 px-2 pb-2 overflow-y-auto w-full no-scrollbar space-y-1">
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

             <!-- DAY VIEW -->
             <div v-if="viewMode === 'day'" class="h-full flex flex-col border border-[#ececeb] dark:border-[#333] rounded-2xl bg-white dark:bg-[#1a1a1a] select-none overflow-hidden">
                <!-- All day tasks header -->
                <div class="flex border-b border-[#ececeb] dark:border-[#333] bg-gray-50/50 dark:bg-[#222]">
                    <div class="w-16 border-r border-[#ececeb] dark:border-[#333] flex items-center justify-center p-2">
                        <span class="text-[10px] font-bold text-gray-400 uppercase tracking-widest text-center writing-vertical-lr rotate-180">All Day</span>
                    </div>
                    <div class="flex-1 p-2 flex flex-wrap gap-2 items-start min-h-[40px]" @dblclick="openAddEventModal(currentDate)">
                        <div v-for="tk in getTasksForDate(formatDateString(currentDate))" :key="'tsk-'+tk.id" class="max-w-[200px] truncate px-2 py-1 rounded text-[11px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1 cursor-pointer bg-white dark:bg-[#2c2c2c] shadow-sm">
                            <CheckSquare class="w-3 h-3 flex-shrink-0" :class="tk.status === 'done' ? 'text-green-500' : ''" /> {{ tk.title }}
                        </div>
                    </div>
                </div>
                <!-- Hour grid -->
                <div class="flex-1 overflow-y-auto no-scrollbar relative">
                    <div v-for="hr in hours" :key="hr" class="flex min-h-[60px] border-b border-gray-100 dark:border-[#2f2f2f] group" @click="clickDay(currentDate)">
                        <div class="w-16 flex justify-center pt-2 border-r border-gray-100 dark:border-[#2f2f2f] text-xs font-medium text-gray-400 shrink-0 select-none">
                            {{ hr === 0 ? '12 AM' : hr < 12 ? hr + ' AM' : hr === 12 ? '12 PM' : (hr - 12) + ' PM' }}
                        </div>
                        <div class="flex-1 p-1 flex gap-2 relative">
                            <!-- Events in this hour block -->
                            <div v-for="ev in getEventsForDateAndHour(formatDateString(currentDate), hr)" :key="'ev-'+ev.id" 
                                class="absolute top-1 left-1 right-1 lg:static lg:flex-1 p-2 rounded-lg bg-blue-100/80 text-blue-900 border border-blue-200 dark:bg-blue-900/30 dark:border-blue-800/50 dark:text-blue-200 shadow-sm transition-transform hover:scale-[1.01] cursor-pointer"
                                @click.stop="openEditEventModal(ev)">
                                <div class="font-bold text-xs truncate">{{ ev.title }}</div>
                                <div class="flex gap-2 text-[10px] opacity-70 mt-0.5">
                                    <span v-if="ev.event_time">{{ ev.event_time }}</span>
                                    <span v-if="ev.location" class="truncate hidden lg:inline">{{ ev.location }}</span>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
             </div>

             <!-- WEEK VIEW -->
             <div v-if="viewMode === 'week'" class="h-full flex flex-col border border-[#ececeb] dark:border-[#333] rounded-2xl bg-white dark:bg-[#1a1a1a] overflow-hidden select-none">
                <!-- Week Days Header & All-day row -->
                <div class="flex border-b border-[#ececeb] dark:border-[#333] shadow-sm z-10 sticky top-0 bg-white dark:bg-[#1a1a1a]">
                    <div class="w-12 border-r border-[#ececeb] dark:border-[#333] flex items-center justify-center bg-gray-50/50 dark:bg-[#222]">
                        <span class="text-[9px] font-bold text-gray-400 uppercase tracking-widest writing-vertical-lr rotate-180 mb-2">All Day</span>
                    </div>
                    <!-- 7 Columns headers -->
                    <div v-for="dayObj in currentWeekDays" :key="dayObj.dateStr" class="flex-1 flex flex-col border-r last:border-0 border-[#ececeb] dark:border-[#333]" @click="clickDay(dayObj.date)">
                        <!-- Day Label -->
                        <div class="text-center py-2 border-b border-[#ececeb] dark:border-[#333]" 
                             :class="isSameDay(dayObj.date, new Date()) ? 'bg-purple-50 dark:bg-purple-900/20 text-purple-700 dark:text-purple-300' : 'bg-gray-50/50 dark:bg-[#222] text-gray-500 dark:text-gray-400'">
                            <span class="text-xs uppercase font-bold tracking-wider block mb-0.5">{{ dayNamesShort[dayObj.date.getDay()] }}</span>
                            <span class="text-lg font-black" :class="{'bg-purple-600 text-white rounded-full w-7 h-7 flex items-center justify-center mx-auto': isSameDay(dayObj.date, new Date())}">{{ dayObj.date.getDate() }}</span>
                        </div>
                        <!-- All Day Slots -->
                        <div class="p-1 min-h-[40px] flex flex-col gap-1 bg-gray-50/20 dark:bg-[#1d1d1d]" @dblclick="openAddEventModal(dayObj.date)">
                            <div v-for="tk in getTasksForDate(dayObj.dateStr)" :key="'wk-tsk-'+tk.id" class="truncate px-1.5 py-0.5 rounded text-[9px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1 cursor-pointer bg-white dark:bg-[#2c2c2c] shadow-[0_1px_2px_rgba(0,0,0,0.05)]">
                                <CheckSquare class="w-2.5 h-2.5 flex-shrink-0" :class="tk.status === 'done' ? 'text-green-500' : ''" /> {{ tk.title }}
                            </div>
                        </div>
                    </div>
                </div>

                <!-- 24 Hour Grids for Week -->
                <div class="flex-1 overflow-y-auto no-scrollbar relative flex bg-gray-50/10 dark:bg-[#1f1f1f]">
                    <!-- Time labels col -->
                    <div class="w-12 border-r border-[#ececeb] dark:border-[#333] flex flex-col flex-shrink-0 sticky left-0 z-0 bg-white dark:bg-[#1a1a1a]">
                        <div v-for="hr in hours" :key="'lbl-'+hr" class="h-[60px] flex justify-center pt-2 text-[10px] font-medium text-gray-400 shrink-0 border-b border-gray-100 dark:border-[#2f2f2f]">
                             {{ hr === 0 ? '12 AM' : hr < 12 ? hr + ' AM' : hr === 12 ? '12 PM' : (hr - 12) + ' PM' }}
                        </div>
                    </div>
                    <!-- 7 Columns Grid -->
                    <div class="flex-1 flex">
                        <div v-for="dayObj in currentWeekDays" :key="'col-'+dayObj.dateStr" class="flex-1 flex flex-col border-r last:border-0 border-gray-100 dark:border-[#2f2f2f] hover:bg-gray-50/50 dark:hover:bg-[#252525]/30 transition-colors" @click="clickDay(dayObj.date)">
                            <div v-for="hr in hours" :key="'col-'+dayObj.dateStr+'-'+hr" class="h-[60px] border-b border-gray-100/50 dark:border-[#2f2f2f]/50 p-0.5 relative group" @dblclick="openAddEventModal(dayObj.date)">
                                <div v-for="ev in getEventsForDateAndHour(dayObj.dateStr, hr)" :key="'ev-'+ev.id" 
                                    class="w-full absolute inset-x-0.5 top-0.5 p-1 rounded bg-blue-100/90 text-blue-900 border border-blue-200/50 dark:bg-blue-900/40 dark:border-blue-800/50 dark:text-blue-200 shadow-sm cursor-pointer hover:z-10 truncate text-[10px]"
                                    style="height: 56px;"
                                    @click.stop="openEditEventModal(ev)">
                                    <div class="font-bold truncate">{{ ev.title }}</div>
                                    <div class="opacity-70 truncate">{{ ev.event_time }}</div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
             </div>

             <!-- YEAR VIEW -->
             <div v-if="viewMode === 'year'" class="h-full overflow-y-auto no-scrollbar pb-6 pr-2 select-none">
                <div class="grid grid-cols-3 gap-6 lg:gap-8">
                    <div v-for="monthObj in yearMonths" :key="monthObj.monthIndex" class="bg-white dark:bg-[#232323] border border-gray-100 dark:border-[#333] rounded-2xl p-4 shadow-sm">
                        <!-- Month Title -->
                        <div class="text-sm font-bold uppercase tracking-wider text-purple-600 dark:text-purple-400 mb-3 px-1 cursor-pointer hover:underline" @click="(currentDate = new Date(currentDate.getFullYear(), monthObj.monthIndex, 1)), (viewMode='month')">
                            {{ monthObj.name }}
                        </div>
                        <!-- Mini Grid -->
                        <div class="grid grid-cols-7 gap-y-1 gap-x-0.5 justify-items-center">
                            <div v-for="d in dayNamesShort" :key="'y-'+d" class="text-[9px] font-bold text-gray-400 mb-1">
                                {{ d.substring(0,1) }}
                            </div>
                            <div v-for="(day, dIdx) in monthObj.days" :key="dIdx" class="w-6 h-6 flex flex-col items-center justify-center relative group">
                                <template v-if="day">
                                    <div @click="clickYearDay(day.date)" class="w-5 h-5 rounded hover:bg-gray-200 dark:hover:bg-[#444] cursor-pointer flex items-center justify-center transition-colors relative"
                                         :class="[day.isToday ? 'bg-purple-600 text-white rounded-full font-bold hover:bg-purple-700' : 'text-xs text-gray-700 dark:text-gray-300']">
                                        {{ day.date.getDate() }}
                                    </div>
                                    <!-- Heatmap dot -->
                                    <div v-if="day.hasItems && !day.isToday" class="w-1 h-1 rounded-full bg-purple-500 absolute bottom-0"></div>
                                </template>
                            </div>
                        </div>
                    </div>
                </div>
             </div>
         </div>
     </div>
     
     <!-- RIGHT PANEL: DAY DETAILS -->
     <div v-show="showRightPanel" class="w-96 absolute right-0 top-0 bottom-0 border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-white/95 dark:bg-[#1a1a1a]/95 backdrop-blur shadow-2xl flex flex-col transition-transform z-20">
         <div class="h-16 flex items-center justify-between px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c] flex-shrink-0" data-tauri-drag-region>
             <h2 class="font-bold text-lg text-purple-600 dark:text-purple-400 select-none">{{ selectedDateDisplay }}</h2>
             <button @click="showRightPanel = false" class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-500 transition-colors">
                 <X class="w-5 h-5" />
             </button>
         </div>
         
         <div class="flex-1 overflow-y-auto p-4 space-y-6">
             <!-- Add Event Button -->
             <button @click="openAddEventModal()" class="w-full py-3 border border-dashed border-gray-300 dark:border-gray-700 rounded-xl flex items-center justify-center gap-2 text-gray-500 hover:bg-gray-50 dark:hover:bg-[#2c2c2c] hover:text-black dark:hover:text-white transition-all cursor-pointer">
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
                             <h4 class="font-bold text-base text-gray-900 dark:text-gray-100 line-clamp-1">{{ ev.title }}</h4>
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
                             <h4 class="text-sm font-semibold" :class="tk.status === 'done' ? 'line-through text-gray-400' : 'text-gray-900 dark:text-gray-100'">{{ tk.title }}</h4>
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
            <div class="p-6 space-y-4 overflow-y-auto max-h-[70vh]">
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
