<script setup lang="ts">
import { ChevronLeft, ChevronRight, Plus, Calendar as CalendarIcon } from 'lucide-vue-next';
import NavButtons from '../../../shared/components/NavButtons.vue';
import type { ViewMode } from '../types';

defineProps<{
    headerDisplayString: string;
    viewMode: ViewMode;
}>();

const emit = defineEmits<{
    (e: 'update:viewMode', v: ViewMode): void;
    (e: 'navigate-prev'): void;
    (e: 'navigate-next'): void;
    (e: 'go-today'): void;
    (e: 'add-event'): void;
}>();
</script>

<template>
    <header class="flex flex-col md:flex-row md:items-center justify-between mb-4 md:mb-6 flex-shrink-0 gap-3" data-tauri-drag-region>
        <div class="flex items-center justify-between w-full md:w-auto">
            <div class="flex items-center gap-3">
                <NavButtons />
                <CalendarIcon class="w-5 h-5 md:w-6 md:h-6 text-purple-500" />
                <h1 class="text-xl md:text-2xl font-bold tracking-tight select-none">
                    {{ headerDisplayString }}
                </h1>
            </div>
        </div>
        
        <div class="flex flex-wrap items-center gap-2 md:gap-4 select-none w-full md:w-auto">
            <!-- View Switcher -->
            <div class="flex w-full md:w-auto bg-gray-100 dark:bg-[#1f1f1f] p-1 rounded-xl border border-gray-200 dark:border-[#333] shrink-0">
               <button v-for="v in (['day','week','month','year'] as ViewMode[])" :key="v"
                       @click="emit('update:viewMode', v)"
                       class="flex-1 md:flex-none px-3 py-1.5 md:px-4 md:py-1.5 text-[11px] md:text-xs font-semibold rounded-lg capitalize transition-all"
                       :class="viewMode === v ? 'bg-white shadow-[0_1px_3px_rgba(0,0,0,0.1)] text-purple-600 dark:bg-[#333] dark:text-purple-400' : 'text-gray-500 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white'">
                   {{ v }}
               </button>
            </div>

            <!-- Nav Controls -->
            <div class="flex w-full md:w-auto items-center justify-between md:justify-start gap-2 shrink-0">
                <button @click="emit('go-today')" class="px-2 py-1.5 md:px-3 md:py-1.5 text-[11px] md:text-xs font-semibold bg-gray-100 hover:bg-gray-200 dark:bg-[#2c2c2c] dark:hover:bg-[#3a3a3a] rounded-lg transition-colors border border-transparent dark:border-gray-700">
                    Today
                </button>
                <div class="flex bg-gray-100 dark:bg-[#2c2c2c] rounded-lg p-0.5 border border-transparent dark:border-gray-700">
                    <button @click="emit('navigate-prev')" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronLeft class="w-4 h-4" /></button>
                    <button @click="emit('navigate-next')" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronRight class="w-4 h-4" /></button>
                </div>
                <button @click="emit('add-event')" class="flex items-center gap-1.5 px-3 py-1.5 md:px-3 md:py-1.5 text-[11px] md:text-xs font-semibold bg-purple-600 hover:bg-purple-700 text-white rounded-lg transition-colors shadow-sm ml-0.5 md:ml-1">
                    <Plus class="w-3.5 h-3.5" />
                    <span class="hidden md:inline">{{ $t('calendar.new_event') }}</span>
                    <span class="md:hidden">{{ $t('calendar.new_btn') }}</span>
                </button>
            </div>
        </div>
    </header>
</template>
