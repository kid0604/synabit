<script setup lang="ts">
defineProps<{
    show: boolean;
    action: 'edit' | 'delete';
    modelValue: 'this' | 'following' | 'all';
}>();

const emit = defineEmits<{
    (e: 'update:modelValue', v: 'this' | 'following' | 'all'): void;
    (e: 'confirm'): void;
    (e: 'cancel'): void;
}>();
</script>

<template>
    <div v-if="show" class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 backdrop-blur-sm p-4" @click.self="emit('cancel')">
       <div class="bg-white dark:bg-[#1e1e1e] w-full max-w-sm rounded-2xl shadow-2xl overflow-hidden border border-[#e6e6e6] dark:border-[#333] flex flex-col">
           <div class="px-6 py-4 border-b border-[#e6e6e6] dark:border-[#333]">
               <h3 class="font-bold text-lg text-black dark:text-white">{{ action === 'edit' ? 'Edit Recurring Event' : 'Delete Recurring Event' }}</h3>
           </div>
           <div class="p-6 space-y-3">
               <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-[#2a2a2a] transition-colors" :class="{'border-purple-500 bg-purple-50/50 dark:bg-purple-900/20': modelValue === 'this'}">
                   <input type="radio" :checked="modelValue === 'this'" @change="emit('update:modelValue', 'this')" value="this" class="w-4 h-4 text-purple-600 focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                   <span class="text-sm font-medium text-black dark:text-white">{{ $t('calendar.this_event') }}</span>
               </label>
               <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-[#2a2a2a] transition-colors" :class="{'border-purple-500 bg-purple-50/50 dark:bg-purple-900/20': modelValue === 'following'}">
                   <input type="radio" :checked="modelValue === 'following'" @change="emit('update:modelValue', 'following')" value="following" class="w-4 h-4 text-purple-600 focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                   <span class="text-sm font-medium text-black dark:text-white">{{ $t('calendar.this_and_following') }}</span>
               </label>
               <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-[#2a2a2a] transition-colors" :class="{'border-purple-500 bg-purple-50/50 dark:bg-purple-900/20': modelValue === 'all'}">
                   <input type="radio" :checked="modelValue === 'all'" @change="emit('update:modelValue', 'all')" value="all" class="w-4 h-4 text-purple-600 focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                   <span class="text-sm font-medium text-black dark:text-white">{{ $t('calendar.all_events_in_series') }}</span>
               </label>
           </div>
           <div class="px-6 py-4 bg-gray-50 dark:bg-[#1a1a1a] border-t border-[#e6e6e6] dark:border-[#333] flex justify-end gap-3 text-sm font-semibold select-none">
               <button @click="emit('cancel')" class="px-4 py-2 rounded-lg text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">{{ $t('calendar.cancel') }}</button>
               <button @click="emit('confirm')" class="px-4 py-2 rounded-lg text-white transition-colors" :class="action === 'delete' ? 'bg-red-500 hover:bg-red-600' : 'bg-black dark:bg-white dark:text-black hover:bg-purple-600 dark:hover:bg-purple-400'">{{ $t('calendar.ok') }}</button>
           </div>
       </div>
    </div>
</template>
