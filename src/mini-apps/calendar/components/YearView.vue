<script setup lang="ts">
import { dayNamesShort } from '../helpers';

defineProps<{
    yearMonths: { monthIndex: number; name: string; days: (null | { date: Date; hasItems: boolean; isToday: boolean })[] }[];
    currentDate: Date;
}>();

const emit = defineEmits<{
    (e: 'click-year-day', date: Date): void;
    (e: 'go-to-month', monthIndex: number): void;
}>();
</script>

<template>
    <div class="w-full h-full overflow-y-auto no-scrollbar pb-6 pr-2 select-none">
       <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 md:gap-6 lg:gap-8">
           <div v-for="monthObj in yearMonths" :key="monthObj.monthIndex" class="bg-white dark:bg-[#232323] border border-gray-100 dark:border-[#333] rounded-2xl p-4 shadow-sm">
               <!-- Month Title -->
               <div class="text-sm font-bold uppercase tracking-wider text-purple-600 dark:text-purple-400 mb-3 px-1 cursor-pointer hover:underline" @click="emit('go-to-month', monthObj.monthIndex)">
                   {{ monthObj.name }}
               </div>
               <!-- Mini Grid -->
               <div class="grid grid-cols-7 gap-y-1 gap-x-0.5 justify-items-center">
                   <div v-for="d in dayNamesShort" :key="'y-'+d" class="text-[9px] font-bold text-gray-400 mb-1">
                       {{ d.substring(0,1) }}
                   </div>
                   <div v-for="(day, dIdx) in monthObj.days" :key="dIdx" class="w-6 h-6 flex flex-col items-center justify-center relative group">
                       <template v-if="day">
                           <div @click="emit('click-year-day', day.date)" class="w-5 h-5 rounded hover:bg-gray-200 dark:hover:bg-[#444] cursor-pointer flex items-center justify-center transition-colors relative"
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
</template>
