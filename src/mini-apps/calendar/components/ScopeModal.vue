<script setup lang="ts">
import AppDialog from '../../../shared/components/AppDialog.vue';

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
    <!-- Elevated: a drag or a delete can ask this with the event form still open. -->
    <AppDialog :show="show" labelledby="scope-modal-title" size="sm" elevated unstyled
               panel-class="bg-white dark:bg-surface-dark rounded-2xl shadow-2xl overflow-hidden border border-border dark:border-[#333] flex flex-col"
               @close="emit('cancel')">
           <div class="px-6 py-4 border-b border-border dark:border-[#333]">
               <h3 id="scope-modal-title" class="font-bold text-lg text-black dark:text-white">{{ action === 'edit' ? $t('calendar.edit_recurring') : $t('calendar.delete_recurring') }}</h3>
           </div>
           <div class="p-6 space-y-3">
               <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-surface-hover-dark transition-colors" :class="{'border-accent bg-accent/10 dark:bg-accent/15': modelValue === 'this'}">
                   <input type="radio" :checked="modelValue === 'this'" @change="emit('update:modelValue', 'this')" value="this" class="w-4 h-4 text-accent focus:ring-accent bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                   <span class="text-sm font-medium text-black dark:text-white">{{ $t('calendar.this_event') }}</span>
               </label>
               <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-surface-hover-dark transition-colors" :class="{'border-accent bg-accent/10 dark:bg-accent/15': modelValue === 'following'}">
                   <input type="radio" :checked="modelValue === 'following'" @change="emit('update:modelValue', 'following')" value="following" class="w-4 h-4 text-accent focus:ring-accent bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                   <span class="text-sm font-medium text-black dark:text-white">{{ $t('calendar.this_and_following') }}</span>
               </label>
               <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-surface-hover-dark transition-colors" :class="{'border-accent bg-accent/10 dark:bg-accent/15': modelValue === 'all'}">
                   <input type="radio" :checked="modelValue === 'all'" @change="emit('update:modelValue', 'all')" value="all" class="w-4 h-4 text-accent focus:ring-accent bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                   <span class="text-sm font-medium text-black dark:text-white">{{ $t('calendar.all_events_in_series') }}</span>
               </label>
           </div>
           <div class="px-6 py-4 bg-gray-50 dark:bg-[#1a1a1a] border-t border-border dark:border-[#333] flex justify-end gap-3 text-sm font-semibold select-none">
               <button @click="emit('cancel')" class="px-4 py-2 rounded-lg text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">{{ $t('calendar.cancel') }}</button>
               <button @click="emit('confirm')" class="px-4 py-2 rounded-lg text-white transition-colors" :class="action === 'delete' ? 'bg-red-500 hover:bg-red-600' : 'bg-accent hover:bg-accent/90'">{{ $t('calendar.ok') }}</button>
           </div>
    </AppDialog>
</template>
