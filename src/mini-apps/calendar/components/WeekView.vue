<script setup lang="ts">
import { Calendar as CalendarIcon, CheckSquare } from 'lucide-vue-next';
import type { EventMetadata, TaskMetadata } from '../types';
import { dayNamesShort, formatEventTime, isAllDayOrMultiDay, hours, isSameDay } from '../helpers';

defineProps<{
    currentWeekDays: { date: Date; dateStr: string }[];
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
    <div class="w-full h-full flex flex-col border border-[#ececeb] dark:border-[#333] rounded-2xl bg-white dark:bg-[#1a1a1a] overflow-hidden select-none">
       <!-- Week Days Header & All-day row -->
       <div class="flex border-b border-[#ececeb] dark:border-[#333] shadow-sm z-10 sticky top-0 bg-white dark:bg-[#1a1a1a]">
           <div class="w-12 border-r border-[#ececeb] dark:border-[#333] flex items-center justify-center bg-gray-50/50 dark:bg-[#222]">
               <span class="text-[9px] font-bold text-gray-400 uppercase tracking-widest writing-vertical-lr mb-2">{{ $t('calendar.all_day') }}</span>
           </div>
           <!-- 7 Columns headers -->
           <div v-for="dayObj in currentWeekDays" :key="dayObj.dateStr" class="flex-1 flex flex-col border-r last:border-0 border-[#ececeb] dark:border-[#333]" @click="emit('click-day', dayObj.date)">
               <!-- Day Label -->
               <div class="text-center py-2 border-b border-[#ececeb] dark:border-[#333]" 
                    :class="isSameDay(dayObj.date, new Date()) ? 'bg-purple-50 dark:bg-purple-900/20 text-purple-700 dark:text-purple-300' : 'bg-gray-50/50 dark:bg-[#222] text-gray-500 dark:text-gray-400'">
                   <span class="text-xs uppercase font-bold tracking-wider block mb-0.5">{{ dayNamesShort[dayObj.date.getDay()] }}</span>
                   <span class="text-lg font-black" :class="{'bg-purple-600 text-white rounded-full w-7 h-7 flex items-center justify-center mx-auto': isSameDay(dayObj.date, new Date())}">{{ dayObj.date.getDate() }}</span>
               </div>
               <!-- All Day Slots -->
               <div class="p-1 min-h-[40px] flex flex-col gap-1 bg-gray-50/20 dark:bg-[#1d1d1d]" @dblclick="emit('add-event', dayObj.date)">
                   <div v-for="tk in getTasksForDate(dayObj.dateStr)" :key="'wk-tsk-'+tk.id" class="truncate px-1.5 py-0.5 rounded text-[9px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1 cursor-pointer bg-white dark:bg-[#2c2c2c] shadow-[0_1px_2px_rgba(0,0,0,0.05)] hover:brightness-95" @click.stop="emit('open-task', tk.id)">
                       <CheckSquare class="w-2.5 h-2.5 flex-shrink-0 hover:text-purple-500 transition-colors" :class="tk.status === 'done' ? 'text-green-500' : ''" @click.stop="emit('toggle-task', tk)" /> {{ tk.title }}
                   </div>
                   <div v-for="ev in getEventsForDate(dayObj.dateStr).filter(isAllDayOrMultiDay)" :key="'wk-ad-ev-'+ev.id" class="truncate px-1.5 py-0.5 rounded text-[9px] font-medium border border-blue-200 dark:border-blue-800/50 text-blue-800 dark:text-blue-200 bg-blue-50 dark:bg-blue-900/30 flex items-center gap-1 cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.05)]" @click.stop="emit('edit-event', ev, dayObj.dateStr)">
                       <CalendarIcon class="w-2.5 h-2.5 flex-shrink-0" /> {{ ev.title }}
                   </div>
               </div>
           </div>
       </div>

       <!-- 24 Hour Grids for Week -->
       <div class="flex-1 overflow-y-auto no-scrollbar relative flex bg-gray-50/10 dark:bg-[#1f1f1f]">
           <!-- Time labels col -->
           <div class="w-12 border-r border-[#ececeb] dark:border-[#333] flex flex-col flex-shrink-0 sticky left-0 z-0 bg-white dark:bg-[#1a1a1a]">
               <div v-for="hr in hours" :key="'lbl-'+hr" class="h-[60px] flex justify-center pt-2 text-[10px] font-medium text-gray-400 shrink-0 border-b border-gray-100 dark:border-[#2f2f2f]">
                    {{ formatHourAMPM(hr) }}
               </div>
           </div>
           <!-- 7 Columns Grid -->
           <div class="flex-1 flex w-full">
               <div v-for="dayObj in currentWeekDays" :key="'col-'+dayObj.dateStr" class="flex-1 flex flex-col border-r last:border-0 border-gray-100 dark:border-[#2f2f2f] hover:bg-gray-50/50 dark:hover:bg-[#252525]/30 transition-colors" @click="emit('click-day', dayObj.date)">
                   <div v-for="hr in hours" :key="'col-'+dayObj.dateStr+'-'+hr" class="h-[60px] border-b border-gray-100/50 dark:border-[#2f2f2f]/50 p-0.5 relative group cursor-pointer" @dblclick.self="emit('add-event', dayObj.date, hr)">
                       <div v-for="ev in getEventsForDateAndHour(dayObj.dateStr, hr)" :key="'ev-'+ev.id" 
                           class="w-full absolute inset-x-0.5 top-0.5 p-1 rounded bg-blue-100/90 text-blue-900 border border-blue-200/50 dark:bg-blue-900/40 dark:border-blue-800/50 dark:text-blue-200 shadow-sm cursor-pointer hover:z-10 truncate text-[10px]"
                           style="height: 56px;"
                           @click.stop="emit('edit-event', ev, dayObj.dateStr)">
                           <div class="font-bold truncate">{{ ev.title }}</div>
                           <div class="opacity-70 truncate" v-if="formatEventTime(ev)">{{ formatEventTime(ev) }}</div>
                       </div>
                   </div>
               </div>
           </div>
       </div>
    </div>
</template>
