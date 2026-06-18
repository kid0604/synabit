<script setup lang="ts">
import { Calendar as CalendarIcon, CheckSquare } from 'lucide-vue-next';
import type { EventMetadata, TaskMetadata } from '../types';
import { formatDateString, formatEventTime, isAllDayOrMultiDay, hours } from '../helpers';

defineProps<{
    currentDate: Date;
    getTasksForDate: (dateStr: string) => TaskMetadata[];
    getEventsForDate: (dateStr: string) => EventMetadata[];
    getEventsForDateAndHour: (dateStr: string, hour: number) => EventMetadata[];
}>();

const emit = defineEmits<{
    (e: 'click-day', date: Date): void;
    (e: 'add-event', date: Date, hr?: number): void;
    (e: 'edit-event', ev: EventMetadata, dateStr: string): void;
    (e: 'toggle-task', task: { id: string; status: string }): void;
    (e: 'open-task', id: string): void;
}>();

const formatHourAMPM = (hr: number): string => {
    if (hr === 0) return '12 AM';
    if (hr < 12) return hr + ' AM';
    if (hr === 12) return '12 PM';
    return (hr - 12) + ' PM';
};
</script>

<template>
    <div class="w-full h-full flex flex-col border border-[#ececeb] dark:border-[#333] rounded-2xl bg-white dark:bg-[#1a1a1a] select-none overflow-hidden">
       <!-- All day tasks header -->
       <div class="flex border-b border-[#ececeb] dark:border-[#333] bg-gray-50/50 dark:bg-[#222]">
           <div class="w-16 border-r border-[#ececeb] dark:border-[#333] flex items-center justify-center p-2">
               <span class="text-[10px] font-bold text-gray-400 uppercase tracking-widest text-center writing-vertical-lr">{{ $t('calendar.all_day') }}</span>
           </div>
           <div class="flex-1 p-2 flex flex-wrap gap-2 items-start min-h-[40px]" @dblclick="emit('add-event', currentDate)">
               <div v-for="tk in getTasksForDate(formatDateString(currentDate))" :key="'tsk-'+tk.id" class="max-w-[200px] truncate px-2 py-1 rounded text-[11px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1 cursor-pointer bg-white dark:bg-[#2c2c2c] shadow-sm hover:brightness-95" @click.stop="emit('open-task', tk.id)">
                   <CheckSquare class="w-3 h-3 flex-shrink-0 hover:text-purple-500 transition-colors" :class="tk.status === 'done' ? 'text-green-500' : ''" @click.stop="emit('toggle-task', tk)" /> {{ tk.title }}
               </div>
               <div v-for="ev in getEventsForDate(formatDateString(currentDate)).filter(isAllDayOrMultiDay)" :key="'ad-ev-'+ev.id" class="max-w-[200px] truncate px-2 py-1 rounded text-[11px] font-medium border border-blue-200 dark:border-blue-800/50 text-blue-800 dark:text-blue-200 bg-blue-50 dark:bg-blue-900/30 flex items-center gap-1 cursor-pointer shadow-sm" @click.stop="emit('edit-event', ev, formatDateString(currentDate))">
                   <CalendarIcon class="w-3 h-3 flex-shrink-0" /> {{ ev.title }}
               </div>
           </div>
       </div>
       <!-- Hour grid -->
       <div class="flex-1 overflow-y-auto no-scrollbar relative">
           <div v-for="hr in hours" :key="hr" class="flex min-h-[60px] border-b border-gray-100 dark:border-[#2f2f2f] group" @click="emit('click-day', currentDate)">
               <div class="w-16 flex justify-center pt-2 border-r border-gray-100 dark:border-[#2f2f2f] text-xs font-medium text-gray-400 shrink-0 select-none">
                   {{ formatHourAMPM(hr) }}
               </div>
               <div class="flex-1 p-1 flex gap-2 relative" @dblclick.self="emit('add-event', currentDate, hr)">
                   <!-- Events in this hour block -->
                   <div v-for="ev in getEventsForDateAndHour(formatDateString(currentDate), hr)" :key="'ev-'+ev.id" 
                       class="absolute top-1 left-1 right-1 lg:static lg:flex-1 p-2 rounded-lg bg-blue-100/80 text-blue-900 border border-blue-200 dark:bg-blue-900/30 dark:border-blue-800/50 dark:text-blue-200 shadow-sm transition-transform hover:scale-[1.01] cursor-pointer"
                       @click.stop="emit('edit-event', ev, formatDateString(currentDate))">
                       <div class="font-bold text-xs truncate">{{ ev.title }}</div>
                       <div class="flex gap-2 text-[10px] opacity-70 mt-0.5">
                           <span v-if="formatEventTime(ev)">{{ formatEventTime(ev) }}</span>
                           <span v-if="ev.location" class="truncate hidden lg:inline">{{ ev.location }}</span>
                       </div>
                   </div>
               </div>
           </div>
       </div>
    </div>
</template>
