<script setup lang="ts">
import { X, Plus, Check, FileText, User, CheckSquare, Link2, Trash2, Bell } from 'lucide-vue-next';
import type { EventFormData } from '../types';
import { hourOptions } from '../helpers';

defineProps<{
    show: boolean;
    form: EventFormData;  // reactive object, mutated directly
    startAtDate: string;
    startAtHour: string;
    startAtMinute: string;
    startAtMinuteOptions: string[];
    endAtDate: string;
    endAtHour: string;
    endAtMinute: string;
    endAtMinuteOptions: string[];
    reminderPreset: string;
    customReminder: string;
    eventRelations: { id: string; title: string; node_type: string }[];
    isCreatingNote: boolean;
    newNoteTitle: string;
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'submit'): void;
    (e: 'delete'): void;
    (e: 'update:startAtDate', v: string): void;
    (e: 'update:startAtHour', v: string): void;
    (e: 'update:startAtMinute', v: string): void;
    (e: 'update:endAtDate', v: string): void;
    (e: 'update:endAtHour', v: string): void;
    (e: 'update:endAtMinute', v: string): void;
    (e: 'update:reminderPreset', v: string): void;
    (e: 'update:customReminder', v: string): void;
    (e: 'add-reminder'): void;
    (e: 'remove-reminder', idx: number): void;
    (e: 'update:isCreatingNote', v: boolean): void;
    (e: 'update:newNoteTitle', v: string): void;
    (e: 'create-note'): void;
    (e: 'delete-relation', bl: any): void;
    (e: 'open-linked-note', id: string, type: string): void;
}>();
</script>

<template>
    <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4" @click.self="emit('close')">
       <div class="bg-white dark:bg-[#1e1e1e] w-full max-w-md rounded-2xl shadow-2xl overflow-hidden border border-[#e6e6e6] dark:border-[#333] flex flex-col max-h-[90vh]">
           <div class="flex items-center justify-between px-4 md:px-6 py-4 border-b border-[#e6e6e6] dark:border-[#333] select-none text-black dark:text-white">
               <h3 class="font-bold text-lg">{{ form.isEdit ? 'Edit Event' : 'New Event' }}</h3>
               <button @click="emit('close')" class="text-gray-400 hover:text-red-500"><X class="w-5 h-5"/></button>
           </div>
           <div class="p-6 space-y-4 overflow-y-auto max-h-[70vh]">
               <div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.event_title_req') }}</label>
                  <input v-model="form.title" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.event_title_ph')">
               </div>
                   <div class="flex items-center gap-4 mb-4">
                       <label class="flex items-center gap-1.5 cursor-pointer">
                           <input type="checkbox" v-model="form.isAllDay" class="w-3.5 h-3.5 text-purple-600 rounded focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                           <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider mt-0.5">{{ $t('calendar.all_day_event') }}</span>
                       </label>
                   </div>
                   <div class="grid grid-cols-2 gap-4">
                       <div>
                           <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.start') }}</label>
                           <input v-if="form.isAllDay" v-model="form.start_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                           <div v-else class="flex flex-col gap-2">
                               <input :value="startAtDate" @input="emit('update:startAtDate', ($event.target as HTMLInputElement).value)" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                               <div class="flex items-center gap-1 w-full">
                                   <select :value="startAtHour" @change="emit('update:startAtHour', ($event.target as HTMLSelectElement).value)" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                       <option v-for="h in hourOptions" :key="h" :value="h">{{ h }}</option>
                                   </select>
                                   <span class="text-gray-400 font-bold">:</span>
                                   <select :value="startAtMinute" @change="emit('update:startAtMinute', ($event.target as HTMLSelectElement).value)" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                       <option v-for="m in startAtMinuteOptions" :key="m" :value="m">{{ m }}</option>
                                   </select>
                               </div>
                           </div>
                       </div>
                       <div>
                           <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.end') }} <span class="lowercase text-[9px] font-normal">{{ $t('calendar.optional') }}</span></label>
                           <input v-if="form.isAllDay" v-model="form.end_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                           <div v-else class="flex flex-col gap-2">
                               <input :value="endAtDate" @input="emit('update:endAtDate', ($event.target as HTMLInputElement).value)" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                               <div class="flex items-center gap-1 w-full">
                                   <select :value="endAtHour" @change="emit('update:endAtHour', ($event.target as HTMLSelectElement).value)" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                       <option v-for="h in hourOptions" :key="h" :value="h">{{ h }}</option>
                                   </select>
                                   <span class="text-gray-400 font-bold">:</span>
                                   <select :value="endAtMinute" @change="emit('update:endAtMinute', ($event.target as HTMLSelectElement).value)" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                       <option v-for="m in endAtMinuteOptions" :key="m" :value="m">{{ m }}</option>
                                   </select>
                               </div>
                           </div>
                       </div>
                   </div>
                   <div class="grid grid-cols-2 gap-4">
                       <div>
                            <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.repeat') }}</label>
                            <select v-model="form.recurrence" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white appearance-none cursor-pointer">
                                <option value="none">{{ $t('calendar.does_not_repeat') }}</option>
                                <option value="daily">{{ $t('calendar.daily') }}</option>
                                <option value="weekly">{{ $t('calendar.weekly') }}</option>
                                <option value="monthly">{{ $t('calendar.monthly') }}</option>
                                <option value="yearly">{{ $t('calendar.yearly') }}</option>
                            </select>
                       </div>
                       <div v-if="form.recurrence !== 'none'">
                            <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.ends_on') }} <span class="lowercase text-[9px] font-normal">{{ $t('calendar.optional') }}</span></label>
                            <input v-model="form.recurrence_end_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                       </div>
                   </div>
                <div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.reminders') }}</label>
                  <div class="flex flex-col gap-2">
                      <div class="flex items-center gap-2 flex-wrap">
                          <div v-for="(rem, idx) in form.reminders" :key="idx" class="flex items-center gap-1 bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 px-2 py-1 rounded-md text-xs font-medium">
                              <Bell class="w-3 h-3" />
                              {{ rem }}
                              <button @click="emit('remove-reminder', idx)" class="hover:text-purple-900 dark:hover:text-purple-100 ml-1">
                                  <X class="w-3 h-3" />
                              </button>
                          </div>
                      </div>
                      <div class="flex items-center gap-2">
                          <select :value="reminderPreset" @change="emit('update:reminderPreset', ($event.target as HTMLSelectElement).value); emit('add-reminder')" class="flex-1 bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white appearance-none cursor-pointer">
                              <option value="">{{ $t('calendar.add_reminder') }}</option>
                              <option value="5m">{{ $t('calendar.m_5_before') }}</option>
                              <option value="15m">{{ $t('calendar.m_15_before') }}</option>
                              <option value="30m">{{ $t('calendar.m_30_before') }}</option>
                              <option value="1h">{{ $t('calendar.h_1_before') }}</option>
                              <option value="1d">{{ $t('calendar.d_1_before') }}</option>
                              <option value="custom">{{ $t('calendar.custom') }}</option>
                          </select>
                          <div v-if="reminderPreset === 'custom'" class="flex items-center gap-2 flex-1">
                              <input :value="customReminder" @input="emit('update:customReminder', ($event.target as HTMLInputElement).value)" @keyup.enter="emit('add-reminder')" type="text" placeholder="e.g. 45m, 2h" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white">
                              <button @click="emit('add-reminder')" class="bg-purple-600 hover:bg-purple-700 text-white p-2 rounded-lg transition-colors">
                                  <Plus class="w-4 h-4" />
                              </button>
                          </div>
                      </div>
                  </div>
               </div>
               <div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.location') }}</label>
                  <input v-model="form.location" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.location_ph')">
               </div>
               <div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.description') }}</label>
                  <textarea v-model="form.description" rows="3" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.description_ph')"></textarea>
               </div>
               <div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.tags') }}</label>
                  <input v-model="form.tagsStr" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.tags_ph')">
               </div>

               <!-- Relations Section -->
               <div v-if="form.isEdit" class="pt-4 border-t border-gray-100 dark:border-[#333]">
                  <div class="flex items-center justify-between mb-2">
                      <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider">Relations ({{ eventRelations.length }})</label>
                      <button v-if="!isCreatingNote" @click="emit('update:isCreatingNote', true); emit('update:newNoteTitle', `Meeting Note: ${form.title}`)" class="text-[11px] font-medium text-purple-600 hover:text-purple-700 flex items-center">
                          <Plus class="w-3 h-3 mr-0.5" /> Create Note
                      </button>
                  </div>
                  
                  <div v-if="isCreatingNote" class="mb-3 flex items-center gap-2">
                      <input :value="newNoteTitle" @input="emit('update:newNoteTitle', ($event.target as HTMLInputElement).value)" type="text" class="flex-1 bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-md px-2.5 py-1.5 text-xs focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.note_title_ph')">
                      <button @click="emit('create-note')" class="p-1.5 bg-purple-600 text-white rounded-md hover:bg-purple-700 transition-colors">
                          <Check class="w-3.5 h-3.5" />
                      </button>
                      <button @click="emit('update:isCreatingNote', false)" class="p-1.5 bg-gray-200 dark:bg-[#444] text-gray-600 dark:text-gray-300 rounded-md hover:bg-gray-300 dark:hover:bg-[#555] transition-colors">
                          <X class="w-3.5 h-3.5" />
                      </button>
                  </div>
                  
                  <div v-if="eventRelations.length === 0 && !isCreatingNote" class="text-[12px] text-gray-400 italic">{{ $t('calendar.no_linked_items') }}</div>
                  <div v-else class="space-y-1.5">
                      <div v-for="bl in eventRelations" :key="bl.id" @click="emit('open-linked-note', bl.id, bl.node_type)" class="flex items-center gap-2 px-2.5 py-2 bg-gray-50 dark:bg-[#252525] rounded-md border border-gray-100 dark:border-[#333] cursor-pointer hover:bg-gray-100 dark:hover:bg-[#2f2f2f] transition-colors group">
                          <FileText v-if="bl.node_type === 'note'" class="w-3.5 h-3.5 text-blue-500 shrink-0" />
                          <User v-else-if="bl.node_type === 'person'" class="w-3.5 h-3.5 text-green-500 shrink-0" />
                          <CheckSquare v-else-if="bl.node_type === 'task'" class="w-3.5 h-3.5 text-yellow-500 shrink-0" />
                          <Link2 v-else class="w-3.5 h-3.5 text-purple-500 shrink-0" />
                          <span class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate flex-1">{{ bl.title }}</span>
                          
                          <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                              <button @click.stop="emit('delete-relation', bl)" class="p-1 hover:bg-red-100 dark:hover:bg-red-900/30 rounded text-red-500" :title="$t('calendar.delete_item')"><Trash2 class="w-3 h-3" /></button>
                          </div>
                      </div>
                  </div>
               </div>

           </div>
           <div class="px-6 py-4 bg-gray-50 dark:bg-[#1a1a1a] border-t border-[#e6e6e6] dark:border-[#333] flex items-center gap-3 text-sm font-semibold select-none" :class="form.isEdit ? 'justify-between' : 'justify-end'">
               <button v-if="form.isEdit" @click="emit('delete')" class="px-4 py-2 rounded-lg text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">{{ $t('calendar.delete') }}</button>
               <div class="flex items-center gap-3">
                   <button @click="emit('close')" class="px-4 py-2 rounded-lg text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">{{ $t('calendar.cancel') }}</button>
                   <button @click="emit('submit')" class="px-4 py-2 rounded-lg bg-black text-white dark:bg-white dark:text-black hover:bg-purple-600 dark:hover:bg-purple-400 transition-colors" :disabled="!form.title">{{ $t('calendar.save_event') }}</button>
               </div>
           </div>
       </div>
    </div>
</template>
