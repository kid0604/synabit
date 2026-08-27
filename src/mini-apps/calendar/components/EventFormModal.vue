<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue';
import type { NoteCheckbox } from '../checkboxes';
import { X, Plus, Check, FileText, User, UserPlus, CheckSquare, Link2, Trash2, Bell, History } from 'lucide-vue-next';
import type { EventFormData } from '../types';
import { hourOptions } from '../helpers';
import type { RecurrenceFields } from '../rrule';
import { knownTimeZones, shortZoneName, zoneOffsetLabel, localTimeZone } from '../timezone';
import RecurrenceEditor from './RecurrenceEditor.vue';
import ModalDialog from './ModalDialog.vue';
import { EVENT_COLOURS, paletteFor } from '../subscriptions';

const props = defineProps<{
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
    reminderError: string;
    formError: string;
    showErrors: boolean;
    eventRelations: { id: string; title: string; node_type: string }[];
    isCreatingNote: boolean;
    newNoteTitle: string;
    /**
     * Somebody else's calendar. Every control is disabled and the footer
     * offers only a way out — the next refresh would overwrite an edit, so
     * offering one would be a lie about what the app can do.
     */
    readOnly: boolean;
    /** The calendar it came from, for the banner. */
    sourceName: string;
    eventPeople: { id: string; title: string; node_type: string }[];
    peopleQuery: string;
    peopleMatches: { id: string; title: string }[];
    isAddingPerson: boolean;
    noteActions: { noteId: string; noteTitle: string; box: NoteCheckbox }[];
    isMakingTasks: boolean;
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
    (e: 'update:peopleQuery', v: string): void;
    (e: 'update:isAddingPerson', v: boolean): void;
    (e: 'search-people'): void;
    (e: 'add-person', person: { id: string; title: string }): void;
    (e: 'remove-person', id: string): void;
    (e: 'see-meetings-with', id: string, name: string): void;
    (e: 'load-note-actions'): void;
    (e: 'make-tasks', chosen: number[]): void;
}>();

/**
 * Focus the title when the dialog opens. It is what a keyboard user needs
 * first, and it also puts focus inside the dialog so Escape reaches the
 * handler on the wrapper.
 */
const titleInput = ref<HTMLInputElement | null>(null);

/** Which of the note's open boxes to turn into tasks. All of them, by default. */
const chosenActions = ref<Set<number>>(new Set());
const toggleAction = (index: number) => {
    const next = new Set(chosenActions.value);
    if (next.has(index)) next.delete(index); else next.add(index);
    chosenActions.value = next;
};
watch(() => props.noteActions, (actions) => {
    chosenActions.value = new Set(actions.map((_, i) => i));
});

/**
 * The editor works in the event's own zone, the way every calendar does: you
 * edit "09:00 Tokyo", not the local time it happens to land on. Nothing here
 * converts anything — the grid already showed the converted time, and this is
 * the one place the stored value is meant to be visible.
 */
const zones = computed(() => knownTimeZones());
const zoneOffset = computed(() =>
    props.form.tzid ? zoneOffsetLabel(props.form.tzid, props.form.start_at) : '');
watch(() => props.show, async (open) => {
    if (!open) return;
    await nextTick();
    titleInput.value?.focus();
});
</script>

<template>
    <ModalDialog :show="show" labelled-by="event-form-title" card-class="max-w-md max-h-[90vh]"
                 @close="emit('close')">
           <div class="flex items-center justify-between px-4 md:px-6 py-4 border-b border-[#e6e6e6] dark:border-[#333] select-none text-black dark:text-white">
               <h3 id="event-form-title" class="font-bold text-lg">{{ form.isEdit ? $t('calendar.edit_event') : $t('calendar.new_event') }}</h3>
               <button @click="emit('close')" class="text-gray-400 hover:text-red-500" :aria-label="$t('calendar.a11y_close')"><X class="w-5 h-5"/></button>
           </div>
           <p v-if="readOnly" class="mx-6 mt-4 px-3 py-2 rounded-lg text-[12px] font-medium
                     bg-gray-100 dark:bg-[#2a2a2a] text-gray-600 dark:text-gray-300">
               {{ $t('calendar.subscribe_readonly', { name: sourceName }) }}
           </p>
           <fieldset :disabled="readOnly" class="p-6 space-y-4 overflow-y-auto max-h-[70vh] disabled:opacity-70">
               <div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.event_title_req') }}</label>
                  <input ref="titleInput" v-model="form.title" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.event_title_ph')">
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
                           <input v-if="form.isAllDay" v-model="form.start_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :style="{ colorScheme: 'light dark' }">
                           <div v-else class="flex flex-col gap-2">
                               <input :value="startAtDate" @input="emit('update:startAtDate', ($event.target as HTMLInputElement).value)" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :style="{ colorScheme: 'light dark' }">
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
                           <input v-if="form.isAllDay" v-model="form.end_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :style="{ colorScheme: 'light dark' }">
                           <div v-else class="flex flex-col gap-2">
                               <input :value="endAtDate" @input="emit('update:endAtDate', ($event.target as HTMLInputElement).value)" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :style="{ colorScheme: 'light dark' }">
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
                   <div>
                       <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5" for="event-tz">
                           {{ $t('calendar.timezone') }}
                       </label>
                       <div class="flex items-center gap-2">
                           <select id="event-tz" v-model="form.tzid"
                                   class="flex-1 min-w-0 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white appearance-none cursor-pointer">
                               <option value="">{{ $t('calendar.timezone_local') }}</option>
                               <option v-for="z in zones" :key="z" :value="z">{{ shortZoneName(z) }} — {{ z }}</option>
                           </select>
                           <span v-if="zoneOffset" class="text-[11px] font-mono text-gray-500 dark:text-gray-400 shrink-0">{{ zoneOffset }}</span>
                       </div>
                       <p v-if="form.tzid && form.tzid !== localTimeZone()" class="mt-1 text-[11px] text-gray-500 dark:text-gray-400">
                           {{ $t('calendar.timezone_hint') }}
                       </p>
                   </div>

                   <RecurrenceEditor
                       :model-value="form.recurrence"
                       :start-at="form.start_at"
                       @update:model-value="(v: RecurrenceFields) => form.recurrence = v"
                   />
                <div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.reminders') }}</label>
                  <div class="flex flex-col gap-2">
                      <div class="flex items-center gap-2 flex-wrap">
                          <div v-for="(rem, idx) in form.reminders" :key="idx" class="flex items-center gap-1 bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 px-2 py-1 rounded-md text-xs font-medium">
                              <Bell class="w-3 h-3" />
                              {{ rem }}
                              <button @click="emit('remove-reminder', idx)" class="hover:text-purple-900 dark:hover:text-purple-100 ml-1" :aria-label="$t('calendar.a11y_remove_reminder')">
                                  <X class="w-3 h-3" />
                              </button>
                          </div>
                      </div>
                      <div class="flex items-center gap-2">
                          <select :value="reminderPreset" @change="emit('update:reminderPreset', ($event.target as HTMLSelectElement).value); emit('add-reminder')" :aria-label="$t('calendar.add_reminder')" class="flex-1 bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white appearance-none cursor-pointer">
                              <option value="">{{ $t('calendar.add_reminder') }}</option>
                              <option value="5m">{{ $t('calendar.m_5_before') }}</option>
                              <option value="15m">{{ $t('calendar.m_15_before') }}</option>
                              <option value="30m">{{ $t('calendar.m_30_before') }}</option>
                              <option value="1h">{{ $t('calendar.h_1_before') }}</option>
                              <option value="1d">{{ $t('calendar.d_1_before') }}</option>
                              <option value="custom">{{ $t('calendar.custom') }}</option>
                          </select>
                          <div v-if="reminderPreset === 'custom'" class="flex items-center gap-2 flex-1">
                              <input :value="customReminder" @input="emit('update:customReminder', ($event.target as HTMLInputElement).value)" @keyup.enter="emit('add-reminder')" type="text" :placeholder="$t('calendar.custom_reminder_ph')" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white">
                              <button @click="emit('add-reminder')" class="bg-purple-600 hover:bg-purple-700 text-white p-2 rounded-lg transition-colors" :aria-label="$t('calendar.a11y_add_reminder')">
                                  <Plus class="w-4 h-4" />
                              </button>
                          </div>
                      </div>
                      <p v-if="reminderError" class="text-[11px] text-red-500 font-medium">{{ reminderError }}</p>
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
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.colour') }}</label>
                  <div class="flex flex-wrap gap-1.5 mb-4">
                      <button v-for="c in EVENT_COLOURS" :key="c" type="button"
                              :aria-pressed="(form.colour || 'blue') === c"
                              :aria-label="c === 'blue' ? $t('calendar.colour_default') : c"
                              :title="c === 'blue' ? $t('calendar.colour_default') : c"
                              @click="form.colour = c === 'blue' ? '' : c"
                              class="w-7 h-7 rounded-full border-2 transition-transform"
                              :class="[
                                  paletteFor(c === 'blue' ? '' : c).swatch,
                                  (form.colour || 'blue') === c
                                      ? 'border-[#1c1c1e] dark:border-white scale-110'
                                      : 'border-transparent hover:scale-105',
                              ]"></button>
                  </div>
                  <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">{{ $t('calendar.tags') }}</label>
                  <input v-model="form.tagsStr" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.tags_ph')">
               </div>

               <!-- Who was there -->
               <div v-if="form.isEdit" class="pt-4 border-t border-gray-100 dark:border-[#333]">
                   <div class="flex items-center justify-between mb-2">
                       <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider">
                           {{ $t('calendar.people') }} ({{ eventPeople.length }})
                       </label>
                       <button v-if="!isAddingPerson && !readOnly" type="button"
                               @click="emit('update:isAddingPerson', true)"
                               class="text-[11px] font-medium text-purple-600 hover:text-purple-700 flex items-center">
                           <UserPlus class="w-3 h-3 mr-0.5" /> {{ $t('calendar.people_add') }}
                       </button>
                   </div>

                   <div v-if="isAddingPerson" class="mb-2">
                       <input :value="peopleQuery" type="search"
                              :placeholder="$t('calendar.people_search_ph')"
                              :aria-label="$t('calendar.people_search_ph')"
                              class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-md px-2.5 py-1.5 text-xs focus:outline-none focus:border-purple-500 text-black dark:text-white"
                              @input="emit('update:peopleQuery', ($event.target as HTMLInputElement).value); emit('search-people')"
                              @keydown.esc="emit('update:isAddingPerson', false)">
                       <ul v-if="peopleMatches.length" class="mt-1 rounded-md border border-gray-100 dark:border-[#333] overflow-hidden">
                           <li v-for="p in peopleMatches" :key="p.id">
                               <button type="button" @click="emit('add-person', p)"
                                       class="w-full flex items-center gap-2 px-2.5 py-1.5 text-left text-[12px] hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors">
                                   <User class="w-3 h-3 text-green-500 shrink-0" />{{ p.title }}
                               </button>
                           </li>
                       </ul>
                   </div>

                   <p v-if="eventPeople.length === 0 && !isAddingPerson" class="text-[12px] text-gray-400 italic">
                       {{ $t('calendar.people_none') }}
                   </p>
                   <div v-else class="flex flex-wrap gap-1.5">
                       <span v-for="p in eventPeople" :key="p.id"
                             class="group flex items-center gap-1 pl-2 pr-1 py-1 rounded-full bg-green-50 dark:bg-green-900/25 border border-green-200 dark:border-green-800/50 text-[12px] text-green-800 dark:text-green-200">
                           <User class="w-3 h-3 shrink-0" />{{ p.title }}
                           <button type="button" @click="emit('see-meetings-with', p.id, p.title)"
                                   :title="$t('calendar.people_see_meetings', { name: p.title })"
                                   :aria-label="$t('calendar.people_see_meetings', { name: p.title })"
                                   class="p-0.5 rounded-full hover:bg-green-200 dark:hover:bg-green-800/50 transition-colors">
                               <History class="w-3 h-3" />
                           </button>
                           <button v-if="!readOnly" type="button" @click="emit('remove-person', p.id)"
                                   :aria-label="$t('calendar.subscribe_remove')"
                                   class="p-0.5 rounded-full hover:bg-green-200 dark:hover:bg-green-800/50 transition-colors">
                               <X class="w-3 h-3" />
                           </button>
                       </span>
                   </div>
               </div>

               <!-- What came out of the meeting -->
               <div v-if="form.isEdit && !readOnly" class="pt-4 border-t border-gray-100 dark:border-[#333]">
                   <div class="flex items-center justify-between mb-2">
                       <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider">
                           {{ $t('calendar.note_actions') }}
                       </label>
                       <button type="button" @click="emit('load-note-actions')"
                               class="text-[11px] font-medium text-purple-600 hover:text-purple-700">
                           <History class="w-3 h-3 inline mr-0.5" />{{ $t('calendar.note_actions_scan') }}
                       </button>
                   </div>

                   <p v-if="noteActions.length === 0" class="text-[12px] text-gray-400 italic">
                       {{ $t('calendar.note_actions_none') }}
                   </p>
                   <div v-else class="flex flex-col gap-1.5">
                       <label v-for="(item, i) in noteActions" :key="item.noteId + ':' + item.box.line"
                              class="flex items-start gap-2 px-2.5 py-1.5 rounded-md bg-gray-50 dark:bg-[#252525] border border-gray-100 dark:border-[#333] cursor-pointer">
                           <input type="checkbox" :checked="chosenActions.has(i)" @change="toggleAction(i)"
                                  class="mt-0.5 w-3.5 h-3.5 text-purple-600 rounded focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                           <span class="min-w-0 flex-1">
                               <span class="block text-[12px] text-[#1c1c1e] dark:text-[#f4f4f5]">{{ item.box.text }}</span>
                               <span class="block text-[10px] text-gray-400 truncate">{{ item.noteTitle }}</span>
                           </span>
                       </label>
                       <button type="button" :disabled="isMakingTasks || chosenActions.size === 0"
                               @click="emit('make-tasks', [...chosenActions])"
                               class="self-start mt-1 px-3 py-1.5 rounded-lg bg-purple-600 hover:bg-purple-700 disabled:opacity-40 text-white text-[11px] font-semibold transition-colors">
                           {{ $t('calendar.note_actions_make') }} ({{ chosenActions.size }})
                       </button>
                   </div>
               </div>

               <!-- Relations Section -->
               <div v-if="form.isEdit" class="pt-4 border-t border-gray-100 dark:border-[#333]">
                  <div class="flex items-center justify-between mb-2">
                      <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider">{{ $t('calendar.relations') }} ({{ eventRelations.filter(r => r.node_type !== 'person').length }})</label>
                      <button v-if="!isCreatingNote" @click="emit('update:isCreatingNote', true); emit('update:newNoteTitle', `Meeting Note: ${form.title}`)" class="text-[11px] font-medium text-purple-600 hover:text-purple-700 flex items-center">
                          <Plus class="w-3 h-3 mr-0.5" /> {{ $t('calendar.create_note') }}
                      </button>
                  </div>
                  
                  <div v-if="isCreatingNote" class="mb-3 flex items-center gap-2">
                      <input :value="newNoteTitle" @input="emit('update:newNoteTitle', ($event.target as HTMLInputElement).value)" type="text" class="flex-1 bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-md px-2.5 py-1.5 text-xs focus:outline-none focus:border-purple-500 text-black dark:text-white" :placeholder="$t('calendar.note_title_ph')">
                      <button @click="emit('create-note')" class="p-1.5 bg-purple-600 text-white rounded-md hover:bg-purple-700 transition-colors" :aria-label="$t('calendar.a11y_confirm_note')">
                          <Check class="w-3.5 h-3.5" />
                      </button>
                      <button @click="emit('update:isCreatingNote', false)" class="p-1.5 bg-gray-200 dark:bg-[#444] text-gray-600 dark:text-gray-300 rounded-md hover:bg-gray-300 dark:hover:bg-[#555] transition-colors" :aria-label="$t('calendar.a11y_cancel_note')">
                          <X class="w-3.5 h-3.5" />
                      </button>
                  </div>
                  
                  <div v-if="eventRelations.filter(r => r.node_type !== 'person').length === 0 && !isCreatingNote" class="text-[12px] text-gray-400 italic">{{ $t('calendar.no_linked_items') }}</div>
                  <div v-else class="space-y-1.5">
                      <div v-for="bl in eventRelations.filter(r => r.node_type !== 'person')" :key="bl.id" @click="emit('open-linked-note', bl.id, bl.node_type)" class="flex items-center gap-2 px-2.5 py-2 bg-gray-50 dark:bg-[#252525] rounded-md border border-gray-100 dark:border-[#333] cursor-pointer hover:bg-gray-100 dark:hover:bg-[#2f2f2f] transition-colors group">
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

           </fieldset>
           <p v-if="showErrors && formError" role="alert" class="px-6 pt-3 -mb-1 text-[12px] font-medium text-red-500">{{ formError }}</p>
           <div class="px-6 py-4 bg-gray-50 dark:bg-[#1a1a1a] border-t border-[#e6e6e6] dark:border-[#333] flex items-center gap-3 text-sm font-semibold select-none" :class="form.isEdit ? 'justify-between' : 'justify-end'">
               <button v-if="form.isEdit && !readOnly" @click="emit('delete')" class="px-4 py-2 rounded-lg text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">{{ $t('calendar.delete') }}</button>
               <div class="flex items-center gap-3">
                   <button @click="emit('close')" class="px-4 py-2 rounded-lg text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">{{ readOnly ? $t('calendar.a11y_close') : $t('calendar.cancel') }}</button>
                   <button v-if="!readOnly" @click="emit('submit')" class="px-4 py-2 rounded-lg bg-black text-white dark:bg-white dark:text-black hover:bg-purple-600 dark:hover:bg-purple-400 transition-colors">{{ $t('calendar.save_event') }}</button>
               </div>
           </div>
    </ModalDialog>
</template>
