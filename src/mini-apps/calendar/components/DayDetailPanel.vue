<script setup lang="ts">
import { Plus, X, Calendar as CalendarIcon, Clock, MapPin, Hash, CheckSquare, Trash2 } from 'lucide-vue-next';
import type { EventMetadata, TaskMetadata } from '../types';
import { formatEventTime } from '../helpers';

defineProps<{
    show: boolean;
    selectedDateDisplay: string;
    selectedEvents: EventMetadata[];
    selectedTasks: TaskMetadata[];
    selectedDateFormattedStr: string;
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'add-event'): void;
    (e: 'edit-event', ev: EventMetadata, dateStr: string): void;
    (e: 'delete-event', ev: EventMetadata, dateStr: string): void;
    (e: 'toggle-task', task: { id: string; status: string }): void;
    (e: 'open-task', id: string): void;
}>();
</script>

<template>
    <!-- Mobile Overlay -->
    <div v-if="show" class="md:hidden fixed inset-0 bg-black/20 dark:bg-black/40 z-30" @click="emit('close')"></div>
    
    <!-- Panel -->
    <div v-show="show" 
         class="fixed md:absolute z-40 flex flex-col bg-white dark:bg-[#1a1a1a] shadow-[0_-10px_40px_rgba(0,0,0,0.1)] md:shadow-2xl border-[#e6e6e6] dark:border-[#2c2c2c] bottom-0 left-0 right-0 h-[75vh] rounded-t-3xl md:rounded-none md:h-auto md:top-0 md:bottom-0 md:left-auto md:w-96 md:border-l">
        <div class="h-16 flex items-center justify-between px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c] flex-shrink-0 relative" data-tauri-drag-region>
            <div class="absolute top-2 left-1/2 -translate-x-1/2 w-10 h-1.5 bg-gray-300 dark:bg-gray-600 rounded-full md:hidden"></div>
            <h2 class="font-bold text-lg text-purple-600 dark:text-purple-400 select-none mt-2 md:mt-0">{{ selectedDateDisplay }}</h2>
            <button @click="emit('close')" class="mt-2 md:mt-0 p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-500 transition-colors" aria-label="More Options">
                <X class="w-5 h-5" />
            </button>
        </div>
        
        <div class="flex-1 overflow-y-auto p-4 space-y-6">
            <!-- Add Event Button -->
            <button @click="emit('add-event')" class="w-full py-3 border border-dashed border-gray-300 dark:border-gray-700 rounded-xl flex items-center justify-center gap-2 text-gray-500 hover:bg-gray-50 dark:hover:bg-[#2c2c2c] hover:text-black dark:hover:text-white transition-all cursor-pointer">
                <Plus class="w-4 h-4" /> <span class="text-sm font-semibold">{{ $t('calendar.new_event') }}</span>
            </button>
            
            <!-- Events Section -->
            <div>
                <h3 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-3 px-2 flex items-center gap-2">
                    <CalendarIcon class="w-3.5 h-3.5" /> Events
                </h3>
                <div v-if="selectedEvents.length === 0" class="text-sm text-center text-gray-500 py-4 italic bg-gray-50 rounded-xl dark:bg-[#1e1e1e]">{{ $t('calendar.no_events') }}</div>
                <div class="space-y-2">
                    <div v-for="ev in selectedEvents" :key="ev.id" @click="emit('edit-event', ev, selectedDateFormattedStr)" class="p-3 bg-white dark:bg-[#232323] border border-[#f0f0f0] dark:border-[#333] rounded-xl shadow-sm group cursor-pointer hover:border-purple-300 dark:hover:border-purple-500/50 transition-colors">
                        <div class="flex justify-between items-start mb-1">
                            <h4 class="font-bold text-base text-gray-900 dark:text-gray-100 line-clamp-1">{{ ev.title }}</h4>
                            <div class="flex items-center gap-1 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity">
                               <button @click.stop="emit('delete-event', ev, selectedDateFormattedStr)" class="p-1 hover:bg-red-100 dark:hover:bg-red-900/30 rounded text-red-500" aria-label="More Options"><Trash2 class="w-3 h-3"/></button>
                            </div>
                        </div>
                        <div class="flex items-center flex-wrap gap-x-3 gap-y-1 text-xs text-gray-500 dark:text-gray-400 mb-2">
                            <div class="flex items-center gap-1" v-if="formatEventTime(ev)"><Clock class="w-3 h-3" /> {{ formatEventTime(ev) }}</div>
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
                <div v-if="selectedTasks.length === 0" class="text-sm text-center text-gray-500 py-4 italic bg-gray-50 rounded-xl dark:bg-[#1e1e1e]">{{ $t('calendar.no_tasks') }}</div>
                <div class="space-y-2">
                    <div v-for="tk in selectedTasks" :key="tk.id" class="p-3 bg-white dark:bg-[#232323] border border-[#f0f0f0] dark:border-[#333] rounded-xl shadow-sm flex gap-3 cursor-pointer hover:border-purple-300 transition-colors" @click.stop="emit('open-task', tk.id)">
                        <div class="pt-1 select-none pointer-events-auto">
                            <div class="w-4 h-4 rounded border-2 flex items-center justify-center transition-colors border-gray-300 dark:border-gray-500 cursor-pointer hover:border-purple-400"
                                 :class="{'bg-purple-500 border-purple-500 dark:border-purple-500 hover:border-purple-600': tk.status === 'done'}"
                                 @click.stop="emit('toggle-task', tk)">
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
</template>
